//! Nominal closes freeze exact invoice evidence and gate actions at historical revisions.
use copperline_engine::{Command, Engine, Request};
use copperline_model::*;
use copperline_money::{Money, Quantity};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
fn actor(name: &str, role: Role) -> Actor {
    Actor {
        name: name.into(),
        role,
    }
}
fn date(value: &str) -> Result<Date> {
    Date::parse(value)
}
fn seed(e: &mut Engine) -> Result<()> {
    for (id, currency) in [("P", Currency::USD), ("E", Currency::EUR)] {
        e.run(
            actor("Catalog", Role::Analyst),
            Command::ProductPut(Product {
                id: id.into(),
                name: "Synthetic valve".into(),
                family: "Industrial".into(),
                unit: "each".into(),
                currency,
                list_price: Money(100),
            }),
        )?;
    }
    e.run(
        actor("Catalog", Role::Analyst),
        Command::CustomerPut(Customer {
            id: "C".into(),
            name: "Synthetic customer".into(),
            segment: "OEM".into(),
        }),
    )?;
    Ok(())
}
fn engine() -> Result<Engine> {
    let mut e = Engine::memory();
    seed(&mut e)?;
    Ok(e)
}
fn invoice(id: &str, when: &str, currency: Currency, amount: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "Synthetic period test".into(),
        product_id: if currency == Currency::USD { "P" } else { "E" }.into(),
        customer_id: "C".into(),
        date: date(when)?,
        quantity: Quantity(3000),
        unit: "each".into(),
        currency,
        invoice_total: Money(amount),
        cost_total: Money(amount / 2),
    })
}
fn add(e: &mut Engine, id: &str, when: &str, currency: Currency, amount: i64) -> Result<()> {
    e.run(
        actor("Ingest", Role::Analyst),
        Command::TransactionAdd(invoice(id, when, currency, amount)?),
    )?;
    Ok(())
}
fn terms(id: &str, currency: Currency) -> Result<PeriodCloseTerms> {
    Ok(PeriodCloseTerms {
        id: id.into(),
        start: date("2026-10-01")?,
        end: date("2026-11-01")?,
        currency,
        reason: "Synthetic nominal invoice close".into(),
    })
}
fn create(e: &mut Engine, id: &str, currency: Currency) -> Result<()> {
    e.run(
        actor("Closer", Role::Analyst),
        Command::PeriodCloseCreate {
            terms: terms(id, currency)?,
        },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("Closer", Role::Analyst),
        Command::PeriodCloseSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    Ok(())
}
fn decision(id: &str, approve: bool) -> Command {
    Command::PeriodCloseDecide {
        id: id.into(),
        version: 2,
        approve,
        note: "Independent Finance review".into(),
    }
}
fn approve(e: &mut Engine, id: &str) -> Result<()> {
    submit(e, id)?;
    e.run(actor("Finance", Role::Finance), decision(id, true))?;
    Ok(())
}
fn reject(e: &mut Engine, actor: Actor, command: Command) {
    let original = e.state().clone();
    assert!(e.run(actor, command).is_err());
    assert_eq!(e.state(), &original);
}
fn credit(e: &mut Engine, id: &str, original: &str) -> Result<()> {
    e.run(
        actor("Credit author", Role::Analyst),
        Command::CreditCreate {
            proposal: CreditProposal {
                id: id.into(),
                original_transaction_id: original.into(),
                effective_date: date("2026-12-01")?,
                reason: "Synthetic return".into(),
                quantity: Quantity(1000),
            },
        },
    )?;
    Ok(())
}
fn approve_credit(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("Credit author", Role::Analyst),
        Command::CreditSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("Credit reviewer", Role::Finance),
        Command::CreditDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Independent return review".into(),
        },
    )?;
    Ok(())
}
fn correction(
    e: &mut Engine,
    id: &str,
    original: &str,
    replacement: Option<Transaction>,
) -> Result<()> {
    e.run(
        actor("Correction author", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: id.into(),
                original_transaction_id: original.into(),
                effective_date: date("2026-12-01")?,
                reason: "Synthetic full correction".into(),
                replacement,
            },
        },
    )?;
    Ok(())
}

#[test]
fn close_has_half_open_recorded_currency_boundaries_and_preserves_earlier_ingestion() -> TestResult
{
    let mut e = engine()?;
    add(&mut e, "I", "2026-10-01", Currency::USD, 100)?;
    let ingestion_revision = e.state().revision;
    add(&mut e, "OTHER", "2026-10-13", Currency::EUR, 70)?;
    create(&mut e, "OCT", Currency::USD)?;
    assert_eq!(
        e.state().period_closes["OCT"].basis.transaction_ids,
        vec!["I"]
    );
    approve(&mut e, "OCT")?;
    let closed = e.state().period_closes["OCT"].clone();
    e.state()
        .ensure_invoice_open_at(&e.state().transactions["I"], ingestion_revision - 1)?;
    for when in ["2026-10-01", "2026-10-31"] {
        reject(
            &mut e,
            actor("Ingest", Role::Analyst),
            Command::TransactionAdd(invoice("BLOCKED", when, Currency::USD, 1)?),
        );
    }
    add(&mut e, "BEFORE", "2026-09-30", Currency::USD, 1)?;
    add(&mut e, "AFTER", "2026-11-01", Currency::USD, 1)?;
    add(&mut e, "EUR", "2026-10-01", Currency::EUR, 1)?;
    assert_eq!(e.state().period_closes["OCT"], closed);
    assert_eq!(
        e.state().period_basis(&closed.terms, e.state().revision)?,
        closed.basis
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn owner_and_source_authors_cannot_close_and_empty_adjacent_scopes_can_close() -> TestResult {
    let mut e = engine()?;
    add(&mut e, "I", "2026-10-10", Currency::USD, 100)?;
    create(&mut e, "OCT", Currency::USD)?;
    reject(
        &mut e,
        actor("Other", Role::Admin),
        Command::PeriodCloseSubmit {
            id: "OCT".into(),
            version: 1,
        },
    );
    submit(&mut e, "OCT")?;
    for who in [
        actor("Closer", Role::Finance),
        actor("Ingest", Role::Finance),
        actor("Admin", Role::Admin),
    ] {
        reject(&mut e, who, decision("OCT", true));
    }
    e.run(actor("Finance", Role::Finance), decision("OCT", true))?;
    reject(
        &mut e,
        actor("Closer", Role::Analyst),
        Command::PeriodCloseCreate {
            terms: terms("OVERLAP", Currency::USD)?,
        },
    );
    let mut adjacent = terms(&"A".repeat(64), Currency::USD)?;
    adjacent.start = date("2026-11-01")?;
    adjacent.end = date("2026-12-01")?;
    e.run(
        actor("Closer", Role::Analyst),
        Command::PeriodCloseCreate {
            terms: adjacent.clone(),
        },
    )?;
    approve(&mut e, &adjacent.id)?;
    assert!(
        e.state().period_closes[&adjacent.id]
            .basis
            .transaction_ids
            .is_empty()
    );
    assert!(e.state().audit.iter().all(|a| a.detail.len() <= 160));
    create(&mut e, "EURO", Currency::EUR)?;
    approve(&mut e, "EURO")?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn zero_value_ingestion_stales_exact_basis_but_rejection_and_withdrawal_remain_available()
-> TestResult {
    let mut e = engine()?;
    create(&mut e, "PENDING", Currency::USD)?;
    submit(&mut e, "PENDING")?;
    create(&mut e, "DRAFT", Currency::USD)?;
    add(&mut e, "ZERO", "2026-10-15", Currency::USD, 0)?;
    reject(
        &mut e,
        actor("Finance", Role::Finance),
        decision("PENDING", true),
    );
    reject(
        &mut e,
        actor("Closer", Role::Analyst),
        Command::PeriodCloseSubmit {
            id: "DRAFT".into(),
            version: 1,
        },
    );
    e.run(actor("Finance", Role::Finance), decision("PENDING", false))?;
    e.run(
        actor("Closer", Role::Analyst),
        Command::PeriodCloseWithdraw {
            id: "DRAFT".into(),
            version: 1,
        },
    )?;
    create(&mut e, "FRESH", Currency::USD)?;
    approve(&mut e, "FRESH")?;
    assert_eq!(
        e.state().period_closes["FRESH"].basis.transaction_ids,
        vec!["ZERO"]
    );
    assert_eq!(e.state().period_closes["FRESH"].basis.invoice, Money(0));
    e.state().validate()?;
    Ok(())
}

#[test]
fn competing_close_approval_is_rechecked_and_stale_loser_can_be_rejected() -> TestResult {
    let mut e = engine()?;
    for id in ["FIRST", "SECOND"] {
        create(&mut e, id, Currency::USD)?;
        submit(&mut e, id)?;
    }
    e.run(actor("Finance", Role::Finance), decision("FIRST", true))?;
    reject(
        &mut e,
        actor("Other Finance", Role::Finance),
        decision("SECOND", true),
    );
    e.run(
        actor("Other Finance", Role::Finance),
        decision("SECOND", false),
    )?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn later_release_cannot_hide_credit_or_replacement_proposal_at_an_earlier_close_check() -> TestResult
{
    let mut e = engine()?;
    add(&mut e, "OUTSIDE", "2026-09-15", Currency::USD, 90)?;
    create(&mut e, "EUR", Currency::EUR)?;
    submit(&mut e, "EUR")?;
    correction(
        &mut e,
        "MOVE",
        "OUTSIDE",
        Some(invoice("REPLACEMENT", "2026-10-15", Currency::EUR, 80)?),
    )?;
    let unresolved = e.state().revision;
    reject(
        &mut e,
        actor("Finance", Role::Finance),
        decision("EUR", true),
    );
    reject(
        &mut e,
        actor("Closer", Role::Analyst),
        Command::PeriodCloseCreate {
            terms: terms("OTHER-EUR", Currency::EUR)?,
        },
    );
    e.run(
        actor("Correction author", Role::Analyst),
        Command::CorrectionWithdraw {
            id: "MOVE".into(),
            version: 1,
        },
    )?;
    assert!(
        e.state()
            .ensure_period_close_current(&e.state().period_closes["EUR"], unresolved)
            .is_err()
    );
    e.run(actor("Finance", Role::Finance), decision("EUR", true))?;
    add(&mut e, "INSIDE", "2026-10-15", Currency::USD, 100)?;
    create(&mut e, "USD", Currency::USD)?;
    submit(&mut e, "USD")?;
    credit(&mut e, "RETURN", "INSIDE")?;
    let unresolved = e.state().revision;
    reject(
        &mut e,
        actor("Finance", Role::Finance),
        decision("USD", true),
    );
    e.run(
        actor("Credit author", Role::Analyst),
        Command::CreditWithdraw {
            id: "RETURN".into(),
            version: 1,
        },
    )?;
    assert!(
        e.state()
            .ensure_period_close_current(&e.state().period_closes["USD"], unresolved)
            .is_err()
    );
    e.run(actor("Finance", Role::Finance), decision("USD", true))?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn frozen_basis_includes_approved_credits_and_both_legs_of_full_corrections() -> TestResult {
    let mut e = engine()?;
    add(&mut e, "CREDITED", "2026-10-15", Currency::USD, 100)?;
    let before_credit = e.state().revision;
    credit(&mut e, "PART", "CREDITED")?;
    approve_credit(&mut e, "PART")?;
    add(&mut e, "ORIGINAL", "2026-09-15", Currency::EUR, 120)?;
    correction(
        &mut e,
        "MOVE",
        "ORIGINAL",
        Some(invoice("REPLACEMENT", "2026-10-15", Currency::USD, 80)?),
    )?;
    e.run(
        actor("Correction author", Role::Analyst),
        Command::CorrectionSubmit {
            id: "MOVE".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("Correction reviewer", Role::Finance),
        Command::CorrectionDecide {
            id: "MOVE".into(),
            version: 2,
            approve: true,
            note: "Independent replacement review".into(),
        },
    )?;
    let old = e
        .state()
        .period_basis(&terms("VIEW", Currency::USD)?, before_credit)?;
    assert!(old.credit_ids.is_empty() && old.correction_ids.is_empty());
    assert_eq!(old.invoice, Money(100));
    create(&mut e, "OCT", Currency::USD)?;
    let close = &e.state().period_closes["OCT"];
    assert_eq!(close.basis.transaction_ids, vec!["CREDITED", "REPLACEMENT"]);
    assert_eq!(close.basis.credit_ids, vec!["PART"]);
    assert_eq!(close.basis.correction_ids, vec!["MOVE"]);
    assert_eq!(close.basis.gross_invoice, Money(180));
    assert_eq!(close.basis.reversed_invoice, Money(0));
    assert_eq!(close.basis.credited_invoice, Money(33));
    assert_eq!(close.basis.invoice, Money(147));
    assert!(close.contributors.contains("Correction author"));
    assert!(!close.contributors.contains("Correction reviewer"));
    submit(&mut e, "OCT")?;
    for name in ["Credit author", "Correction author"] {
        reject(&mut e, actor(name, Role::Finance), decision("OCT", true));
    }
    e.run(actor("Finance", Role::Finance), decision("OCT", true))?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn closed_period_rejects_mixed_import_atomically_and_later_record_dates_do_not_bypass_it()
-> TestResult {
    let mut e = engine()?;
    add(&mut e, "I", "2026-10-15", Currency::USD, 100)?;
    create(&mut e, "OCT", Currency::USD)?;
    approve(&mut e, "OCT")?;
    reject(&mut e, actor("Ingest", Role::Analyst), Command::TransactionsImport { csv:
        "id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\nOUT,synthetic,P,C,2026-11-02,1,each,USD,1.00,0.50\nIN,synthetic,P,C,2026-10-15,1,each,USD,1.00,0.50\n".into() });
    assert!(!e.state().transactions.contains_key("OUT"));
    assert!(credit(&mut e, "TOO-LATE", "I").is_err());
    assert!(correction(&mut e, "TOO-LATE", "I", None).is_err());
    e.state().validate()?;
    Ok(())
}

fn corrupt(state: &State, change: impl FnOnce(&mut Value)) -> TestResult {
    let mut value = serde_json::to_value(state)?;
    change(&mut value);
    let changed: State = serde_json::from_value(value)?;
    assert!(changed.validate().is_err());
    Ok(())
}
#[test]
fn period_evidence_review_and_audit_cannot_be_erased_or_detached() -> TestResult {
    let mut e = engine()?;
    add(&mut e, "I", "2026-10-15", Currency::USD, 100)?;
    create(&mut e, "OCT", Currency::USD)?;
    approve(&mut e, "OCT")?;
    for (field, value) in [
        ("owner", json!("Forged")),
        ("source_revision", json!(0)),
        ("contributors", json!([])),
    ] {
        corrupt(e.state(), |s| s["period_closes"]["OCT"][field] = value)?;
    }
    corrupt(e.state(), |s| {
        s["period_closes"]["OCT"]["basis"]["transaction_ids"] = json!([])
    })?;
    corrupt(e.state(), |s| {
        s["period_closes"]["OCT"]["terms"]["end"] = json!("2026-12-01")
    })?;
    corrupt(e.state(), |s| {
        s["period_closes"]["OCT"]["review"]["note"] = json!("Altered")
    })?;
    corrupt(e.state(), |s| s["period_closes"] = json!({}))?;
    let last = e.state().audit.len() - 1;
    corrupt(e.state(), |s| {
        s["audit"][last]["command"] = json!("fixture")
    })?;
    corrupt(e.state(), |s| s["audit"][last]["target"] = Value::Null)?;
    Ok(())
}

#[test]
fn combined_frozen_basis_limit_rejects_new_close_without_mutating_existing_proposals() -> TestResult
{
    let mut e = engine()?;
    let mut csv = String::from(
        "id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\n",
    );
    for index in 0..1000 {
        csv.push_str(&format!(
            "I{index:04},synthetic,P,C,2026-10-15,1,each,USD,0.00,0.00\n"
        ));
    }
    e.run(
        actor("Ingest", Role::Analyst),
        Command::TransactionsImport { csv },
    )?;
    for index in 0..100 {
        create(&mut e, &format!("PERIOD{index:03}"), Currency::USD)?;
    }
    assert_eq!(
        e.state()
            .period_closes
            .values()
            .map(|p| p.basis.transaction_ids.len())
            .sum::<usize>(),
        100_000
    );
    let original = e.state().clone();
    let problem = e
        .run(
            actor("Closer", Role::Analyst),
            Command::PeriodCloseCreate {
                terms: terms("TOO-MANY", Currency::USD)?,
            },
        )
        .err()
        .ok_or("Accepted too many frozen basis IDs")?;
    assert!(problem.to_string().contains("100,000"));
    assert_eq!(e.state(), &original);
    let mut tampered = original;
    let close = tampered
        .period_closes
        .values()
        .next()
        .ok_or("Missing close")?
        .clone();
    tampered.period_closes.insert("EXTRA".into(), close);
    let problem = tampered
        .validate()
        .err()
        .ok_or("Validation accepted too many frozen basis IDs")?;
    assert!(problem.to_string().contains("100,000"));
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-m9-periods-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn close_failed_commit_exact_retry_and_reopen_preserve_original_receipt() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    seed(&mut e)?;
    create(&mut e, "OCT", Currency::USD)?;
    submit(&mut e, "OCT")?;
    let before = e.state().clone();
    let request = Request {
        request_id: "close-once".into(),
        expected_revision: before.revision,
        actor: actor("Finance", Role::Finance),
        action: decision("OCT", true),
    };
    let pending = workspace
        .0
        .join(format!("{:020}.pending", before.revision + 1));
    fs::write(&pending, b"owned failed-commit fixture")?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert_eq!(fs::read(&pending)?, b"owned failed-commit fixture");
    fs::remove_file(pending)?;
    let result = e.execute(request.clone())?;
    assert!(!result.replayed);
    let expected = e.state().clone();
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &expected);
    let replay = e.execute(request)?;
    assert!(replay.replayed && replay.revision == result.revision);
    assert_eq!(e.state(), &expected);
    Ok(())
}
