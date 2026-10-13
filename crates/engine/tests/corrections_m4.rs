//! Full-invoice correction lifecycle, audit provenance and transactional regressions.
use copperline_engine::{Command, Engine, Request, analytics};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-correction-review-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
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
fn actor(name: &str, role: Role) -> Actor {
    Actor {
        name: name.into(),
        role,
    }
}
fn date() -> Result<Date> {
    Date::parse("2026-10-13")
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn invoice(id: &str, amount: i64, cost: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "Synthetic corrections fixture".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date()?,
        quantity: Quantity(1_000),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(amount),
        cost_total: Money(cost),
    })
}
fn record(e: &mut Engine, t: Transaction) -> Result<()> {
    e.run(actor("importer", Role::Analyst), Command::TransactionAdd(t))?;
    Ok(())
}
fn proposal(
    id: &str,
    original: &str,
    replacement: Option<Transaction>,
) -> Result<CorrectionProposal> {
    Ok(CorrectionProposal {
        id: id.into(),
        original_transaction_id: original.into(),
        effective_date: date()?,
        reason: "Synthetic full correction review".into(),
        replacement,
    })
}
fn create(e: &mut Engine, p: CorrectionProposal) -> Result<()> {
    e.run(
        actor("owner", Role::Analyst),
        Command::CorrectionCreate { proposal: p },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("owner", Role::Analyst),
        Command::CorrectionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    Ok(())
}
fn decision(id: &str, approve: bool) -> Command {
    Command::CorrectionDecide {
        id: id.into(),
        version: 2,
        approve,
        note: "Independent Finance review".into(),
    }
}
fn decide(e: &mut Engine, id: &str, approve: bool) -> Result<()> {
    e.run(actor("finance", Role::Finance), decision(id, approve))?;
    Ok(())
}
fn reject_atomic(e: &mut Engine, who: Actor, command: Command) {
    let before = e.state().clone();
    assert!(e.run(who, command).is_err());
    assert_eq!(e.state(), &before);
}
fn agreement(e: &mut Engine) -> Result<()> {
    e.run(
        actor("pricing", Role::Analyst),
        Command::AgreementCreate {
            terms: AgreementTerms {
                id: "A".into(),
                name: "Synthetic annual rebate".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::USD,
                start: Date::parse("2026-01-01")?,
                end: Date::parse("2027-01-01")?,
                tiers: vec![
                    RebateTier {
                        threshold: Money(0),
                        rate: Bps(200),
                    },
                    RebateTier {
                        threshold: Money(100_000),
                        rate: Bps(500),
                    },
                ],
            },
        },
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::AgreementSubmit {
            id: "A".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("commercial", Role::Approver),
        Command::AgreementDecide {
            id: "A".into(),
            version: 2,
            approve: true,
            note: "Synthetic agreement review".into(),
        },
    )?;
    Ok(())
}
fn accrue(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("accruer", Role::Analyst),
        Command::RebateAccrue {
            id: id.into(),
            agreement_id: "A".into(),
            version: 3,
        },
    )?;
    Ok(())
}

#[test]
fn full_reversal_is_effective_only_on_finance_approval_and_old_accrual_is_frozen() -> TestResult {
    let mut e = demo()?;
    agreement(&mut e)?;
    let original = invoice("ORIGINAL", 100_000, 60_000)?;
    record(&mut e, original.clone())?;
    accrue(&mut e, "BEFORE")?;
    let old = e
        .state()
        .accruals
        .get("BEFORE")
        .ok_or("Missing accrual")?
        .clone();
    let old_revision = e.state().revision;
    create(&mut e, proposal("C", "ORIGINAL", None)?)?;
    assert_eq!(analytics(e.state())?[0].invoice, Money(100_000));
    submit(&mut e, "C")?;
    accrue(&mut e, "PENDING")?;
    assert_eq!(
        e.state().accruals.get("PENDING").map(|a| a.rebate),
        Some(Money(5_000))
    );
    decide(&mut e, "C", true)?;
    assert_eq!(e.state().transactions.get("ORIGINAL"), Some(&original));
    assert_eq!(e.state().accruals.get("BEFORE"), Some(&old));
    let historic = e.state().accrual_basis("A", old_revision)?;
    assert!(historic.correction_ids.is_empty());
    assert_eq!(historic.eligible_invoice, Money(100_000));
    let basis = e.state().accrual_basis("A", e.state().revision)?;
    assert_eq!(basis.transaction_ids, vec!["ORIGINAL"]);
    assert_eq!(basis.correction_ids, vec!["C"]);
    assert_eq!(
        (
            basis.gross_invoice,
            basis.reversed_invoice,
            basis.eligible_invoice
        ),
        (Money(100_000), Money(100_000), Money(0))
    );
    accrue(&mut e, "AFTER")?;
    let after = e.state().accruals.get("AFTER").ok_or("Missing accrual")?;
    assert_eq!((after.eligible_invoice, after.rebate), (Money(0), Money(0)));
    assert_eq!(after.correction_ids, vec!["C"]);
    let g = analytics(e.state())?.remove(0);
    assert_eq!(
        (
            g.gross_transaction_count,
            g.reversal_count,
            g.transaction_count
        ),
        (1, 1, 0)
    );
    assert_eq!(
        (
            g.gross_invoice,
            g.reversed_invoice,
            g.invoice,
            g.cost,
            g.margin_bps
        ),
        (Money(100_000), Money(100_000), Money(0), Money(0), None)
    );
    reject_atomic(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CorrectionCreate {
            proposal: proposal("AGAIN", "ORIGINAL", None)?,
        },
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn reversal_and_replacement_approval_is_atomic_persistent_and_replayable() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    record(&mut e, invoice("ORIGINAL", 100_000, 60_000)?)?;
    let replacement = invoice("REPLACEMENT", 90_000, 50_000)?;
    create(
        &mut e,
        proposal("C", "ORIGINAL", Some(replacement.clone()))?,
    )?;
    submit(&mut e, "C")?;
    let before = e.state().clone();
    let request = Request {
        request_id: "approve-correction".into(),
        expected_revision: before.revision,
        actor: actor("finance", Role::Finance),
        action: decision("C", true),
    };
    let pending = workspace
        .0
        .join(format!("{:020}.pending", before.revision + 1));
    fs::write(&pending, b"synthetic write blocker")?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert!(!e.state().transactions.contains_key("REPLACEMENT"));
    fs::remove_file(&pending)?;
    let approved = e.execute(request.clone())?;
    assert_eq!(
        e.state().transactions.get("REPLACEMENT"),
        Some(&replacement)
    );
    assert_eq!(
        e.state().transaction_revisions.get("REPLACEMENT"),
        Some(&approved.revision)
    );
    let g = analytics(e.state())?.remove(0);
    assert_eq!(
        (g.gross_invoice, g.reversed_invoice, g.invoice, g.cost),
        (Money(190_000), Money(100_000), Money(90_000), Money(50_000))
    );
    let state = e.state().clone();
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &state);
    let retry = e.execute(request.clone())?;
    assert!(retry.replayed);
    assert_eq!(retry.revision, approved.revision);
    assert_eq!(e.state(), &state);
    let mut altered = request;
    altered.actor = actor("other-finance", Role::Finance);
    assert!(e.execute(altered).is_err());
    assert_eq!(e.state(), &state);
    Ok(())
}

#[test]
fn correction_roles_ownership_versions_and_terminal_states_are_enforced() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", 10_000, 6_000)?)?;
    for role in [Role::Sales, Role::Approver, Role::Finance] {
        reject_atomic(
            &mut e,
            actor("outsider", role),
            Command::CorrectionCreate {
                proposal: proposal("C", "ORIGINAL", None)?,
            },
        );
    }
    create(&mut e, proposal("C", "ORIGINAL", None)?)?;
    for role in [Role::Analyst, Role::Admin] {
        reject_atomic(
            &mut e,
            actor("outsider", role),
            Command::CorrectionSubmit {
                id: "C".into(),
                version: 1,
            },
        );
        reject_atomic(
            &mut e,
            actor("outsider", role),
            Command::CorrectionWithdraw {
                id: "C".into(),
                version: 1,
            },
        );
    }
    reject_atomic(&mut e, actor("finance", Role::Finance), decision("C", true));
    reject_atomic(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CorrectionSubmit {
            id: "C".into(),
            version: 2,
        },
    );
    submit(&mut e, "C")?;
    for role in [Role::Analyst, Role::Sales, Role::Approver, Role::Admin] {
        reject_atomic(&mut e, actor("outsider", role), decision("C", true));
    }
    reject_atomic(&mut e, actor("owner", Role::Finance), decision("C", true));
    decide(&mut e, "C", true)?;
    reject_atomic(
        &mut e,
        actor("owner", Role::Admin),
        Command::CorrectionWithdraw {
            id: "C".into(),
            version: 3,
        },
    );
    reject_atomic(
        &mut e,
        actor("finance", Role::Finance),
        decision("C", false),
    );
    Ok(())
}

#[test]
fn reserved_replacement_ids_block_add_and_atomic_csv_then_release_on_withdrawal() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", 10_000, 6_000)?)?;
    let replacement = invoice("RESERVED", 9_000, 6_000)?;
    create(
        &mut e,
        proposal("C", "ORIGINAL", Some(replacement.clone()))?,
    )?;
    reject_atomic(
        &mut e,
        actor("importer", Role::Analyst),
        Command::TransactionAdd(replacement.clone()),
    );
    let csv = "id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\nNEW,synthetic,VLV-100,C-ATLAS,2026-10-13,1,each,USD,10,6\nRESERVED,synthetic,VLV-100,C-ATLAS,2026-10-13,1,each,USD,90,60";
    reject_atomic(
        &mut e,
        actor("importer", Role::Analyst),
        Command::TransactionsImport { csv: csv.into() },
    );
    assert!(!e.state().transactions.contains_key("NEW"));
    submit(&mut e, "C")?;
    reject_atomic(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CorrectionCreate {
            proposal: proposal("CONCURRENT", "ORIGINAL", None)?,
        },
    );
    e.run(
        actor("owner", Role::Analyst),
        Command::CorrectionWithdraw {
            id: "C".into(),
            version: 2,
        },
    )?;
    record(&mut e, replacement)?;
    create(&mut e, proposal("FRESH", "ORIGINAL", None)?)?;
    submit(&mut e, "FRESH")?;
    decide(&mut e, "FRESH", true)?;
    assert_eq!(analytics(e.state())?[0].invoice, Money(9_000));
    e.state().validate()?;
    Ok(())
}

#[test]
fn rejected_and_draft_withdrawn_proposals_release_the_original_without_erasing_history()
-> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", 10_000, 6_000)?)?;
    create(&mut e, proposal("DRAFT", "ORIGINAL", None)?)?;
    e.run(
        actor("owner", Role::Analyst),
        Command::CorrectionWithdraw {
            id: "DRAFT".into(),
            version: 1,
        },
    )?;
    create(
        &mut e,
        proposal(
            "REJECTED",
            "ORIGINAL",
            Some(invoice("REPLACEMENT", 9_000, 5_000)?),
        )?,
    )?;
    submit(&mut e, "REJECTED")?;
    decide(&mut e, "REJECTED", false)?;
    assert!(!e.state().transactions.contains_key("REPLACEMENT"));
    assert_eq!(analytics(e.state())?[0].invoice, Money(10_000));
    create(
        &mut e,
        proposal(
            "APPROVED",
            "ORIGINAL",
            Some(invoice("REPLACEMENT", 8_000, 5_000)?),
        )?,
    )?;
    submit(&mut e, "APPROVED")?;
    decide(&mut e, "APPROVED", true)?;
    assert_eq!(e.state().corrections.len(), 3);
    assert_eq!(analytics(e.state())?[0].invoice, Money(8_000));
    e.state().validate()?;
    Ok(())
}

#[test]
fn replacement_chains_are_chronological_and_cannot_reverse_an_original_twice() -> TestResult {
    let mut e = demo()?;
    agreement(&mut e)?;
    record(&mut e, invoice("T1", 100_000, 60_000)?)?;
    let mut p = proposal("C1", "T1", Some(invoice("T2", 90_000, 50_000)?))?;
    p.effective_date = Date::parse("2026-10-15")?;
    create(&mut e, p)?;
    submit(&mut e, "C1")?;
    decide(&mut e, "C1", true)?;
    let mut early = proposal("EARLY", "T2", None)?;
    early.effective_date = Date::parse("2026-10-14")?;
    reject_atomic(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CorrectionCreate { proposal: early },
    );
    let mut cycle = proposal("CYCLE", "T2", Some(invoice("T1", 80_000, 40_000)?))?;
    cycle.effective_date = Date::parse("2026-10-16")?;
    reject_atomic(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CorrectionCreate { proposal: cycle },
    );
    let mut p = proposal("C2", "T2", Some(invoice("T3", 80_000, 40_000)?))?;
    p.effective_date = Date::parse("2026-10-16")?;
    create(&mut e, p)?;
    submit(&mut e, "C2")?;
    decide(&mut e, "C2", true)?;
    accrue(&mut e, "NET")?;
    let a = e.state().accruals.get("NET").ok_or("Missing accrual")?;
    assert_eq!(a.transaction_ids, vec!["T1", "T2", "T3"]);
    assert_eq!(a.correction_ids, vec!["C1", "C2"]);
    assert_eq!(
        (a.eligible_invoice, a.rebate),
        (Money(80_000), Money(1_600))
    );
    let g = analytics(e.state())?.remove(0);
    assert_eq!(
        (
            g.gross_transaction_count,
            g.reversal_count,
            g.transaction_count
        ),
        (3, 2, 1)
    );
    assert_eq!((g.invoice, g.cost), (Money(80_000), Money(40_000)));
    Ok(())
}

#[test]
fn invalid_proposal_context_dates_and_existing_ids_fail_atomically() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", 10_000, 6_000)?)?;
    let mut cases = vec![];
    let mut p = proposal("C", "ORIGINAL", None)?;
    p.reason.clear();
    cases.push(p);
    let mut p = proposal("C", "ORIGINAL", None)?;
    p.effective_date = Date::parse("2026-10-12")?;
    cases.push(p);
    cases.push(proposal("C", "MISSING", None)?);
    cases.push(proposal(
        "C",
        "ORIGINAL",
        Some(invoice("ORIGINAL", 9_000, 5_000)?),
    )?);
    let mut r = invoice("NEW", -1, 0)?;
    cases.push(proposal("C", "ORIGINAL", Some(r.clone()))?);
    r.invoice_total = Money(9_000);
    r.date = Date::parse("2026-10-14")?;
    cases.push(proposal("C", "ORIGINAL", Some(r.clone()))?);
    r.date = date()?;
    r.currency = Currency::EUR;
    cases.push(proposal("C", "ORIGINAL", Some(r.clone()))?);
    r.currency = Currency::USD;
    r.unit = "kg".into();
    cases.push(proposal("C", "ORIGINAL", Some(r))?);
    for p in cases {
        reject_atomic(
            &mut e,
            actor("owner", Role::Analyst),
            Command::CorrectionCreate { proposal: p },
        );
    }
    Ok(())
}

#[test]
fn approval_overflow_leaves_original_and_proposal_untouched() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", i64::MAX, 0)?)?;
    create(
        &mut e,
        proposal("C", "ORIGINAL", Some(invoice("REPLACEMENT", 1, 0)?))?,
    )?;
    submit(&mut e, "C")?;
    reject_atomic(&mut e, actor("finance", Role::Finance), decision("C", true));
    assert!(!e.state().transactions.contains_key("REPLACEMENT"));
    assert_eq!(
        e.state().corrections.get("C").map(|c| c.status),
        Some(RecordStatus::Pending)
    );
    // Net margin itself can overflow its display representation after a reversal.
    let mut e = demo()?;
    record(&mut e, invoice("HIGH", 1_000_000, 0)?)?;
    record(&mut e, invoice("LOW", 1, 1_000_000)?)?;
    create(&mut e, proposal("C", "HIGH", None)?)?;
    submit(&mut e, "C")?;
    reject_atomic(&mut e, actor("finance", Role::Finance), decision("C", true));
    Ok(())
}

#[test]
fn snapshots_bind_original_review_outcome_and_replacement_ingestion() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", 10_000, 6_000)?)?;
    create(
        &mut e,
        proposal("C", "ORIGINAL", Some(invoice("REPLACEMENT", 9_000, 5_000)?))?,
    )?;
    submit(&mut e, "C")?;
    decide(&mut e, "C", true)?;
    let state = e.state().clone();
    let mut bad = state.clone();
    bad.corrections
        .get_mut("C")
        .ok_or("Missing correction")?
        .original
        .invoice_total = Money(20_000);
    assert!(bad.validate().is_err());
    let mut bad = state.clone();
    bad.corrections
        .get_mut("C")
        .ok_or("Missing correction")?
        .owner = "finance".into();
    assert!(bad.validate().is_err());
    let mut bad = state.clone();
    bad.transactions
        .get_mut("REPLACEMENT")
        .ok_or("Missing replacement")?
        .invoice_total = Money(20_000);
    assert!(bad.validate().is_err());
    let mut bad = state.clone();
    bad.transaction_revisions.insert("REPLACEMENT".into(), 2);
    assert!(bad.validate().is_err());
    let mut bad = state.clone();
    bad.corrections.clear();
    assert!(bad.validate().is_err());
    let mut bad = state.clone();
    let c = bad.corrections.get_mut("C").ok_or("Missing correction")?;
    c.review.as_mut().ok_or("Missing review")?.actor.role = Role::Admin;
    assert!(bad.validate().is_err());
    let mut bad = state;
    let c = bad.corrections.get_mut("C").ok_or("Missing correction")?;
    c.review.as_mut().ok_or("Missing review")?.approved = false;
    c.status = RecordStatus::Rejected;
    bad.transactions.remove("REPLACEMENT");
    bad.transaction_revisions.remove("REPLACEMENT");
    assert!(
        bad.validate().is_err(),
        "Audit still records approval, not rejection"
    );
    Ok(())
}

#[test]
fn immutable_correction_payload_and_review_note_are_bound_to_audit_evidence() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("ORIGINAL", 10_000, 6_000)?)?;
    let key = "C".repeat(64);
    let mut p = proposal(
        &key,
        "ORIGINAL",
        Some(invoice("REPLACEMENT", 9_000, 5_000)?),
    )?;
    p.reason = "R".repeat(160);
    create(&mut e, p)?;
    assert!(
        e.state()
            .audit
            .last()
            .is_some_and(|a| a.detail.len() <= 160)
    );
    let mut bad = e.state().clone();
    bad.corrections
        .get_mut(&key)
        .ok_or("Missing correction")?
        .proposal
        .reason = "Different reason".into();
    assert!(bad.validate().is_err());
    submit(&mut e, &key)?;
    decide(&mut e, &key, true)?;
    assert!(
        e.state()
            .audit
            .last()
            .is_some_and(|a| a.detail.len() <= 160)
    );
    let mut bad = e.state().clone();
    bad.corrections
        .get_mut(&key)
        .and_then(|c| c.review.as_mut())
        .ok_or("Missing review")?
        .note = "Different review".into();
    assert!(bad.validate().is_err());
    let mut bad = e.state().clone();
    bad.corrections
        .get_mut(&key)
        .and_then(|c| c.proposal.replacement.as_mut())
        .ok_or("Missing proposed replacement")?
        .invoice_total = Money(8_000);
    bad.transactions
        .get_mut("REPLACEMENT")
        .ok_or("Missing replacement")?
        .invoice_total = Money(8_000);
    assert!(
        bad.validate().is_err(),
        "Matching ledger arithmetic must not permit rewriting immutable proposal terms"
    );
    Ok(())
}
