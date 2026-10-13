//! Quantity-credit allocation, authorization, frozen evidence and transactional regressions.
use copperline_engine::{Command, Engine, Request, analytics};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity, Rational};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-m9-credit-{}-{}",
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
    e.run(actor("seed", Role::Admin), Command::SeedDemo)?;
    Ok(())
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    seed(&mut e)?;
    Ok(e)
}
fn invoice(id: &str, quantity: i64, amount: i64, cost: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "Synthetic credit fixture".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(quantity),
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
fn proposal(id: &str, original: &str, quantity: i64) -> Result<CreditProposal> {
    Ok(CreditProposal {
        id: id.into(),
        original_transaction_id: original.into(),
        effective_date: date("2027-02-01")?,
        reason: "Synthetic quantity return".into(),
        quantity: Quantity(quantity),
    })
}
fn create(e: &mut Engine, id: &str, original: &str, quantity: i64, owner: &str) -> Result<()> {
    e.run(
        actor(owner, Role::Analyst),
        Command::CreditCreate {
            proposal: proposal(id, original, quantity)?,
        },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str, owner: &str) -> Result<()> {
    e.run(
        actor(owner, Role::Analyst),
        Command::CreditSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    Ok(())
}
fn decision(id: &str, approve: bool) -> Command {
    Command::CreditDecide {
        id: id.into(),
        version: 2,
        approve,
        note: "Independent synthetic Finance review".into(),
    }
}
fn approve(e: &mut Engine, id: &str, original: &str, quantity: i64, owner: &str) -> Result<()> {
    create(e, id, original, quantity, owner)?;
    submit(e, id, owner)?;
    e.run(actor("finance", Role::Finance), decision(id, true))?;
    Ok(())
}
fn credit<'a>(e: &'a Engine, id: &str) -> Result<&'a Credit> {
    e.state()
        .credits
        .get(id)
        .ok_or_else(|| error("Missing credit"))
}
fn reject(e: &mut Engine, actor: Actor, command: Command) {
    let before = e.state().clone();
    assert!(e.run(actor, command).is_err());
    assert_eq!(e.state(), &before);
}
fn correction(id: &str, original: &str, replacement: Option<Transaction>) -> Result<Command> {
    Ok(Command::CorrectionCreate {
        proposal: CorrectionProposal {
            id: id.into(),
            original_transaction_id: original.into(),
            effective_date: date("2027-02-01")?,
            reason: "Synthetic full correction".into(),
            replacement,
        },
    })
}
fn rates(e: &mut Engine, entries: Vec<ConversionEntry>) -> Result<()> {
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: "RATES".into(),
                name: "Synthetic exact rates".into(),
                entries,
            },
        },
    )?;
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionSubmit {
            id: "RATES".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("rate-reviewer", Role::Approver),
        Command::ConversionDecide {
            id: "RATES".into(),
            version: 2,
            approve: true,
            note: "Checked synthetic ratios".into(),
        },
    )?;
    Ok(())
}
fn rate(id: &str, conversion: ConversionKind, n: i64, d: i64) -> Result<ConversionEntry> {
    Ok(ConversionEntry {
        id: id.into(),
        conversion,
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
        ratio: Rational {
            numerator: n,
            denominator: d,
        },
    })
}
fn quote_version(e: &Engine) -> Result<u64> {
    e.state()
        .quotes
        .get("Q")
        .map(|q| q.version)
        .ok_or_else(|| error("Missing quote"))
}
fn book_native_fraction(e: &mut Engine) -> Result<()> {
    rates(
        e,
        vec![rate(
            "third",
            ConversionKind::Uom {
                product_id: "VLV-100".into(),
                from_unit: "third".into(),
                to_unit: "each".into(),
            },
            1,
            3,
        )?],
    )?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic native precision".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: quote_version(e)?,
            request: PriceRequest {
                product_id: "VLV-100".into(),
                customer_id: "C-ATLAS".into(),
                date: date("2026-10-13")?,
                quantity: Quantity(3000),
                discount: Bps(0),
                rebate: Bps(0),
                source: None,
                conversion: Some(ConversionRequest {
                    currency: Currency::USD,
                    unit: "third".into(),
                }),
            },
        },
    )?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: quote_version(e)?,
        },
    )?;
    e.run(
        actor("commercial", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: quote_version(e)?,
            approve: true,
            note: "Checked quote".into(),
        },
    )?;
    if e.state()
        .quotes
        .get("Q")
        .is_some_and(|q| q.status == Status::Pending)
    {
        e.run(
            actor("quote-finance", Role::Finance),
            Command::QuoteDecide {
                id: "Q".into(),
                version: quote_version(e)?,
                approve: true,
                note: "Checked escalation".into(),
            },
        )?;
    }
    e.run(
        actor("booker", Role::Analyst),
        Command::QuoteBook {
            request: BookingRequest {
                id: "B".into(),
                quote_id: "Q".into(),
                quote_version: quote_version(e)?,
                invoice_date: date("2026-10-13")?,
                source: "synthetic-booking-credit".into(),
            },
        },
    )?;
    Ok(())
}
fn rehash_credit(state: &mut State, id: &str) -> Result<()> {
    let c = state
        .credits
        .get(id)
        .ok_or_else(|| error("Missing credit"))?;
    let created_revision = c.created_revision;
    let creation = credit_created_detail(c)?;
    let review = c
        .review
        .as_ref()
        .map(|r| Ok((r.revision, credit_decision_detail(c, r)?)))
        .transpose()?;
    state
        .audit
        .iter_mut()
        .find(|a| a.revision == created_revision)
        .ok_or_else(|| error("Missing creation audit"))?
        .detail = creation;
    if let Some((revision, detail)) = review {
        state
            .audit
            .iter_mut()
            .find(|a| a.revision == revision)
            .ok_or_else(|| error("Missing review audit"))?
            .detail = detail;
    }
    Ok(())
}

#[test]
fn cumulative_allocation_is_nonnegative_partition_invariant_and_clears_remainder() -> TestResult {
    for total in 0..32 {
        for quantity in 1..16 {
            let mut allocated = Money(0);
            for prior in 0..quantity {
                let piece = credit_allocation(
                    Money(total),
                    Quantity(quantity),
                    Quantity(prior),
                    Quantity(1),
                )?;
                assert!(piece.0 >= 0 && piece.0 <= total);
                allocated = allocated.add(piece)?;
                let expected = (i128::from(total) * i128::from(prior + 1)
                    + i128::from(quantity / 2))
                    / i128::from(quantity);
                assert_eq!(i128::from(allocated.0), expected);
                for next in 1..=quantity - prior {
                    let span = credit_allocation(
                        Money(total),
                        Quantity(quantity),
                        Quantity(prior),
                        Quantity(next),
                    )?;
                    let prefix = copperline_money::ratio(total, prior, quantity)?;
                    assert_eq!(
                        span.0 + prefix,
                        copperline_money::ratio(total, prior + next, quantity)?
                    );
                }
            }
            assert_eq!(allocated, Money(total));
        }
    }
    assert_eq!(
        credit_allocation(
            Money(i64::MAX),
            Quantity(1_000_000_000),
            Quantity(999_999_999),
            Quantity(1)
        )?,
        Money(i64::MAX - copperline_money::ratio(i64::MAX, 999_999_999, 1_000_000_000)?)
    );
    for (total, original, prior, quantity) in [
        (1, 0, 0, 1),
        (-1, 2, 0, 1),
        (1, 2, -1, 1),
        (1, 2, 0, 0),
        (1, 2, 1, 2),
        (1, 2, i64::MAX, 1),
    ] {
        assert!(
            credit_allocation(
                Money(total),
                Quantity(original),
                Quantity(prior),
                Quantity(quantity)
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn thirds_use_approval_order_and_zero_cent_credit_finishes_exactly() -> TestResult {
    let mut e = demo()?;
    let original = invoice("I", 3000, 2, 1)?;
    record(&mut e, original.clone())?;
    let initial_revision = e.state().revision;
    for (index, (id, expected_invoice, expected_cost)) in [("Z", 1, 0), ("A", 0, 1), ("M", 1, 0)]
        .into_iter()
        .enumerate()
    {
        let before = e.state().clone();
        let preview = e
            .state()
            .credit_preview(&proposal(id, "I", 1000)?, "owner")?;
        assert_eq!(e.state(), &before);
        assert_eq!(
            (preview.invoice, preview.cost),
            (Money(expected_invoice), Money(expected_cost))
        );
        approve(&mut e, id, "I", 1000, "owner")?;
        assert_eq!(
            credit(&e, id)?.prior_quantity,
            Quantity(i64::try_from(index)? * 1000)
        );
        assert_eq!(
            (credit(&e, id)?.invoice, credit(&e, id)?.cost),
            (preview.invoice, preview.cost)
        );
        let groups = analytics(e.state())?;
        assert_eq!(groups[0].credit_count, index + 1);
        assert_eq!(groups[0].transaction_count, usize::from(index < 2));
    }
    assert_eq!(credit(&e, "M")?.prior_credit_ids, vec!["Z", "A"]);
    let approved: Vec<&Credit> = e.state().approved_credits_for("I", e.state().revision);
    assert_eq!(
        approved
            .iter()
            .map(|c| c.proposal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Z", "A", "M"]
    );
    assert!(
        e.state()
            .approved_credits_for("I", initial_revision)
            .is_empty()
    );
    assert_eq!(e.state().transactions.get("I"), Some(&original));
    let group = analytics(e.state())?.remove(0);
    assert_eq!(
        (group.invoice, group.cost, group.fully_credited_count),
        (Money(0), Money(0), 1)
    );
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CreditCreate {
            proposal: proposal("OVER", "I", 1)?,
        },
    );
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        correction("FULL", "I", None)?,
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn owners_roles_reservations_and_terminal_release_are_enforced() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    for role in [Role::Sales, Role::Approver, Role::Finance] {
        reject(
            &mut e,
            actor("owner", role),
            Command::CreditCreate {
                proposal: proposal("NO", "I", 1000)?,
            },
        );
    }
    create(&mut e, "C", "I", 1000, "owner")?;
    let created = credit(&e, "C")?.created_revision;
    assert!(credit(&e, "C")?.live_at(created));
    reject(
        &mut e,
        actor("other", Role::Admin),
        Command::CreditSubmit {
            id: "C".into(),
            version: 1,
        },
    );
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CreditCreate {
            proposal: proposal("CONCURRENT", "I", 1000)?,
        },
    );
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        correction("FULL", "I", None)?,
    );
    submit(&mut e, "C", "owner")?;
    for (name, role) in [
        ("owner", Role::Finance),
        ("importer", Role::Finance),
        ("other", Role::Admin),
        ("other", Role::Approver),
    ] {
        reject(&mut e, actor(name, role), decision("C", true));
        reject(&mut e, actor(name, role), decision("C", false));
    }
    e.run(actor("finance", Role::Finance), decision("C", false))?;
    assert!(credit(&e, "C")?.live_at(created));
    assert!(!credit(&e, "C")?.live_at(e.state().revision));
    create(&mut e, "D", "I", 1000, "second-owner")?;
    e.run(
        actor("second-owner", Role::Admin),
        Command::CreditWithdraw {
            id: "D".into(),
            version: 1,
        },
    )?;
    create(&mut e, "E", "I", 1000, "second-owner")?;
    submit(&mut e, "E", "second-owner")?;
    e.run(
        actor("second-owner", Role::Analyst),
        Command::CreditWithdraw {
            id: "E".into(),
            version: 2,
        },
    )?;
    approve(&mut e, "F", "I", 1000, "first-author")?;
    create(&mut e, "G", "I", 1000, "next-author")?;
    submit(&mut e, "G", "next-author")?;
    assert!(credit(&e, "G")?.contributors.contains("first-author"));
    reject(
        &mut e,
        actor("first-author", Role::Finance),
        decision("G", true),
    );
    e.run(actor("finance", Role::Finance), decision("G", true))?;
    reject(&mut e, actor("finance", Role::Finance), decision("G", true));
    e.state().validate()?;
    Ok(())
}

#[test]
fn replacement_proposer_and_ingestion_reviewer_are_both_excluded() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("I", 3000, 6, 3)?)?;
    e.run(
        actor("repair-author", Role::Analyst),
        correction("REPAIR", "I", Some(invoice("R", 3000, 2, 1)?))?,
    )?;
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CreditCreate {
            proposal: proposal("BLOCKED", "I", 1000)?,
        },
    );
    e.run(
        actor("repair-author", Role::Analyst),
        Command::CorrectionSubmit {
            id: "REPAIR".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("repair-reviewer", Role::Finance),
        Command::CorrectionDecide {
            id: "REPAIR".into(),
            version: 2,
            approve: true,
            note: "Checked replacement".into(),
        },
    )?;
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CreditCreate {
            proposal: proposal("ALSO-BLOCKED", "I", 1000)?,
        },
    );
    create(&mut e, "C", "R", 1000, "owner")?;
    submit(&mut e, "C", "owner")?;
    for name in ["repair-author", "repair-reviewer"] {
        assert!(credit(&e, "C")?.contributors.contains(name));
        reject(&mut e, actor(name, Role::Finance), decision("C", true));
    }
    e.run(actor("finance", Role::Finance), decision("C", true))?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn booked_native_quantity_is_exact_and_authors_remain_frozen_after_quote_revision() -> TestResult {
    let mut e = demo()?;
    book_native_fraction(&mut e)?;
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CreditCreate {
            proposal: proposal("SUB-TICK", "B-L001", 1)?,
        },
    );
    create(&mut e, "C", "B-L001", 3, "owner")?;
    let frozen = credit(&e, "C")?.clone();
    let evidence = frozen.booking.as_ref().ok_or("Missing native evidence")?;
    assert_eq!(
        (
            frozen.original.quantity,
            evidence.native_basis.quantity,
            evidence.native_quantity
        ),
        (Quantity(3000), Quantity(1000), Quantity(1))
    );
    let booking = e.state().quote_bookings.get("B").ok_or("Missing booking")?;
    assert_eq!(evidence.booking_fingerprint, record_fingerprint(booking)?);
    assert_eq!(
        evidence.native_invoice,
        credit_allocation(
            evidence.native_basis.invoice_total,
            Quantity(3000),
            Quantity(0),
            Quantity(3)
        )?
    );
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: quote_version(&e)?,
        },
    )?;
    assert_eq!(credit(&e, "C")?, &frozen);
    submit(&mut e, "C", "owner")?;
    for name in ["seller", "booker", "owner"] {
        reject(&mut e, actor(name, Role::Finance), decision("C", true));
    }
    e.run(actor("finance", Role::Finance), decision("C", true))?;
    approve(&mut e, "REST", "B-L001", 2997, "owner")?;
    let rest = credit(&e, "REST")?
        .booking
        .as_ref()
        .ok_or("Missing remaining native evidence")?;
    assert_eq!(evidence.native_quantity.0 + rest.native_quantity.0, 1000);
    assert_eq!(
        evidence.native_invoice.add(rest.native_invoice)?,
        evidence.native_basis.invoice_total
    );
    assert_eq!(
        evidence.native_cost.add(rest.native_cost)?,
        evidence.native_basis.cost_total
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn recomputed_audit_hashes_cannot_change_generated_amounts_baseline_or_contributors() -> TestResult
{
    let mut e = demo()?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    approve(&mut e, "Z", "I", 1000, "first")?;
    approve(&mut e, "A", "I", 1000, "second")?;
    // A changed but arithmetically equivalent input must still match its immutable audit anchor.
    let mut changed_quantity = e.state().clone();
    changed_quantity
        .credits
        .get_mut("A")
        .ok_or("Missing credit")?
        .proposal
        .quantity = Quantity(1001);
    assert!(changed_quantity.validate().is_err());
    for field in [
        "invoice",
        "cost",
        "quantity",
        "prior_quantity",
        "prior_ids",
        "contributors",
        "original_revision",
        "original",
    ] {
        let mut state = e.state().clone();
        let c = state.credits.get_mut("A").ok_or("Missing credit")?;
        match field {
            "invoice" => c.invoice = Money(1),
            "cost" => c.cost = Money(0),
            "quantity" => c.proposal.quantity = Quantity(3000),
            "prior_quantity" => c.prior_quantity = Quantity(0),
            "prior_ids" => c.prior_credit_ids.clear(),
            "contributors" => {
                c.contributors.remove("first");
            }
            "original_revision" => c.original_revision = 1,
            "original" => c.original.invoice_total = Money(3),
            _ => return Err("Unknown mutation".into()),
        }
        rehash_credit(&mut state, "A")?;
        assert!(state.validate().is_err(), "Accepted forged {field}");
    }
    for target in [None, Some("credit:OTHER".into())] {
        let mut state = e.state().clone();
        let revision = credit(&e, "A")?.created_revision;
        state
            .audit
            .iter_mut()
            .find(|a| a.revision == revision)
            .ok_or("Missing audit")?
            .target = target;
        assert!(state.validate().is_err());
    }
    let mut erased = e.state().clone();
    erased.credits.remove("A");
    assert!(erased.validate().is_err());
    Ok(())
}

#[test]
fn later_withdrawal_cannot_conceal_overlapping_credit_reservations() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    create(&mut e, "FIRST", "I", 1000, "owner")?;
    e.run(
        actor("owner", Role::Analyst),
        Command::CustomerPut(Customer {
            id: "C-UNRELATED".into(),
            name: "Synthetic chronology marker".into(),
            segment: "OEM".into(),
        }),
    )?;
    let marker = e.state().revision;
    e.run(
        actor("owner", Role::Analyst),
        Command::CreditWithdraw {
            id: "FIRST".into(),
            version: 1,
        },
    )?;
    create(&mut e, "SECOND", "I", 1000, "owner")?;
    let mut state = e.state().clone();
    let old_revision = state
        .credits
        .get("SECOND")
        .ok_or("Missing second credit")?
        .created_revision;
    state
        .credits
        .get_mut("SECOND")
        .ok_or("Missing second credit")?
        .created_revision = marker;
    let old = state
        .audit
        .iter_mut()
        .find(|a| a.revision == old_revision)
        .ok_or("Missing old audit")?;
    old.command = "fixture".into();
    old.target = None;
    old.detail = "Synthetic unrelated event".into();
    let at_marker = state
        .audit
        .iter_mut()
        .find(|a| a.revision == marker)
        .ok_or("Missing marker")?;
    at_marker.command = "credit_create".into();
    at_marker.target = Some(credit_target("SECOND"));
    rehash_credit(&mut state, "SECOND")?;
    assert!(state.validate().is_err());
    Ok(())
}

#[test]
fn reporting_allocates_whole_converted_invoice_and_preserves_earlier_snapshot() -> TestResult {
    let mut e = demo()?;
    rates(
        &mut e,
        vec![rate(
            "fx",
            ConversionKind::Fx {
                from: Currency::USD,
                to: Currency::EUR,
            },
            2,
            3,
        )?],
    )?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    let request = ReportingRequest {
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
        currency: Currency::EUR,
    };
    e.run(
        actor("reporter", Role::Analyst),
        Command::ReportConvert {
            id: "BEFORE".into(),
            request: request.clone(),
        },
    )?;
    let old = e
        .state()
        .conversion_reports
        .get("BEFORE")
        .ok_or("Missing old report")?
        .clone();
    for id in ["Z", "A", "M"] {
        approve(&mut e, id, "I", 1000, "owner")?;
    }
    let report = e.state().converted_actuals(&request, e.state().revision)?;
    assert_eq!(report.rows[0].invoice, Money(1));
    assert_eq!(
        report.rows[0]
            .credits
            .iter()
            .map(|c| c.invoice.0)
            .collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!(
        (report.groups[0].invoice, report.groups[0].cost),
        (Money(0), Money(0))
    );
    assert_eq!(e.state().conversion_reports.get("BEFORE"), Some(&old));
    assert!(
        e.state()
            .converted_actuals(&request, old.calculation.source_revision)?
            .rows[0]
            .credits
            .is_empty()
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn failed_credit_approval_commit_retries_once_and_reopens_with_original_rows() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    seed(&mut e)?;
    let original = invoice("I", 3000, 2, 1)?;
    record(&mut e, original.clone())?;
    create(&mut e, "C", "I", 1000, "owner")?;
    submit(&mut e, "C", "owner")?;
    let before = e.state().clone();
    let request = Request {
        request_id: "approve-credit-once".into(),
        expected_revision: before.revision,
        actor: actor("finance", Role::Finance),
        action: decision("C", true),
    };
    let pending = workspace
        .0
        .join(format!("{:020}.pending", before.revision + 1));
    fs::write(&pending, b"retain synthetic interrupted write")?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert_eq!(fs::read(&pending)?, b"retain synthetic interrupted write");
    assert!(
        !workspace
            .0
            .join(format!("{:020}.json", before.revision + 1))
            .exists()
    );
    fs::remove_file(&pending)?;
    let committed = e.execute(request.clone())?;
    assert!(!committed.replayed);
    let state = e.state().clone();
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &state);
    assert_eq!(e.state().transactions.get("I"), Some(&original));
    let retry = e.execute(request.clone())?;
    assert!(retry.replayed);
    assert_eq!(retry.revision, committed.revision);
    assert_eq!(e.state(), &state);
    let mut changed = request;
    changed.action = decision("C", false);
    assert!(e.execute(changed).is_err());
    assert_eq!(e.state(), &state);
    Ok(())
}

#[test]
fn closed_original_period_blocks_later_record_date_credit_and_maximum_ids_fit() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    let id = "C".repeat(64);
    create(&mut e, &id, "I", 1000, "owner")?;
    submit(&mut e, &id, "owner")?;
    e.run(actor("finance", Role::Finance), decision(&id, true))?;
    assert!(e.state().audit.iter().all(|a| a.detail.len() <= 160));
    e.run(
        actor("closer", Role::Analyst),
        Command::PeriodCloseCreate {
            terms: PeriodCloseTerms {
                id: "CLOSED".into(),
                start: date("2026-10-01")?,
                end: date("2026-11-01")?,
                currency: Currency::USD,
                reason: "Synthetic closed nominal basis".into(),
            },
        },
    )?;
    e.run(
        actor("closer", Role::Analyst),
        Command::PeriodCloseSubmit {
            id: "CLOSED".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("closing-finance", Role::Finance),
        Command::PeriodCloseDecide {
            id: "CLOSED".into(),
            version: 2,
            approve: true,
            note: "Checked closed interval".into(),
        },
    )?;
    reject(
        &mut e,
        actor("owner", Role::Analyst),
        Command::CreditCreate {
            proposal: proposal("LATE", "I", 1000)?,
        },
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn zero_cent_credit_stales_settlement_by_ids_without_changing_rebate_amount() -> TestResult {
    let mut e = demo()?;
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementCreate {
            terms: AgreementTerms {
                id: "AG".into(),
                name: "Synthetic credit rebate".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::USD,
                start: date("2026-01-01")?,
                end: date("2027-01-01")?,
                tiers: vec![RebateTier {
                    threshold: Money(0),
                    rate: Bps(5000),
                }],
            },
        },
    )?;
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementSubmit {
            id: "AG".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("agreement-reviewer", Role::Approver),
        Command::AgreementDecide {
            id: "AG".into(),
            version: 2,
            approve: true,
            note: "Synthetic agreement review".into(),
        },
    )?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    approve(&mut e, "Z", "I", 1000, "credit-author")?;
    e.run(
        actor("accruer", Role::Analyst),
        Command::RebateAccrue {
            id: "BEFORE".into(),
            agreement_id: "AG".into(),
            version: 3,
        },
    )?;
    let before = e
        .state()
        .accruals
        .get("BEFORE")
        .ok_or("Missing accrual")?
        .clone();
    assert_eq!(
        (before.eligible_invoice, before.rebate),
        (Money(1), Money(1))
    );
    assert_eq!(before.credit_ids, vec!["Z"]);
    e.run(
        actor("reconciler", Role::Analyst),
        Command::SettlementCreate {
            id: "S".into(),
            accrual_id: "BEFORE".into(),
            reason: "Synthetic credit recognition".into(),
        },
    )?;
    e.run(
        actor("reconciler", Role::Analyst),
        Command::SettlementSubmit {
            id: "S".into(),
            version: 1,
        },
    )?;
    approve(&mut e, "A", "I", 1000, "zero-credit-author")?;
    assert_eq!(credit(&e, "A")?.invoice, Money(0));
    let position = e.state().rebate_position("AG", e.state().revision)?;
    assert_eq!(position.target_rebate, Money(1));
    assert_eq!(position.basis.credit_ids, vec!["A", "Z"]);
    assert_eq!(e.state().accruals.get("BEFORE"), Some(&before));
    reject(
        &mut e,
        actor("settlement-finance", Role::Finance),
        Command::SettlementDecide {
            id: "S".into(),
            version: 2,
            approve: true,
            note: "Must reject stale IDs".into(),
        },
    );
    e.run(
        actor("settlement-finance", Role::Finance),
        Command::SettlementDecide {
            id: "S".into(),
            version: 2,
            approve: false,
            note: "Source IDs changed".into(),
        },
    )?;
    e.run(
        actor("accruer", Role::Analyst),
        Command::RebateAccrue {
            id: "AFTER".into(),
            agreement_id: "AG".into(),
            version: 3,
        },
    )?;
    e.run(
        actor("reconciler", Role::Analyst),
        Command::SettlementCreate {
            id: "FRESH".into(),
            accrual_id: "AFTER".into(),
            reason: "Refreshed exact source IDs".into(),
        },
    )?;
    e.run(
        actor("reconciler", Role::Analyst),
        Command::SettlementSubmit {
            id: "FRESH".into(),
            version: 1,
        },
    )?;
    reject(
        &mut e,
        actor("zero-credit-author", Role::Finance),
        Command::SettlementDecide {
            id: "FRESH".into(),
            version: 2,
            approve: true,
            note: "Author cannot review".into(),
        },
    );
    e.run(
        actor("settlement-finance", Role::Finance),
        Command::SettlementDecide {
            id: "FRESH".into(),
            version: 2,
            approve: true,
            note: "Fresh independent review".into(),
        },
    )?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn booked_credit_native_evidence_and_frozen_authors_cannot_be_retargeted() -> TestResult {
    let mut e = demo()?;
    book_native_fraction(&mut e)?;
    approve(&mut e, "C", "B-L001", 3, "owner")?;
    for field in [
        "fingerprint",
        "line",
        "native_quantity",
        "native_amount",
        "basis",
        "booker",
        "seller",
    ] {
        let mut state = e.state().clone();
        let c = state.credits.get_mut("C").ok_or("Missing credit")?;
        let evidence = c.booking.as_mut().ok_or("Missing booking evidence")?;
        match field {
            "fingerprint" => evidence.booking_fingerprint = "0".repeat(64),
            "line" => evidence.line_index = 1,
            "native_quantity" => evidence.native_quantity = Quantity(2),
            "native_amount" => evidence.native_invoice = evidence.native_invoice.add(Money(1))?,
            "basis" => {
                evidence.native_basis.invoice_total =
                    evidence.native_basis.invoice_total.add(Money(1))?
            }
            "booker" => {
                c.contributors.remove("booker");
            }
            "seller" => {
                c.contributors.remove("seller");
            }
            _ => return Err("Unknown evidence mutation".into()),
        }
        rehash_credit(&mut state, "C")?;
        assert!(state.validate().is_err(), "Accepted forged booked {field}");
    }
    let mut erased = e.state().clone();
    erased.credits.get_mut("C").ok_or("Missing credit")?.booking = None;
    rehash_credit(&mut erased, "C")?;
    assert!(erased.validate().is_err());
    Ok(())
}

#[test]
fn hostile_proposal_inputs_fail_without_record_or_receipt() -> TestResult {
    let mut e = demo()?;
    record(&mut e, invoice("I", 3000, 2, 1)?)?;
    for field in [
        "zero", "negative", "over", "maximum", "id", "missing", "date", "reason",
    ] {
        let mut p = proposal("BAD", "I", 1000)?;
        match field {
            "zero" => p.quantity = Quantity(0),
            "negative" => p.quantity = Quantity(-1),
            "over" => p.quantity = Quantity(3001),
            "maximum" => p.quantity = Quantity(i64::MAX),
            "id" => p.id = "C".repeat(65),
            "missing" => p.original_transaction_id = "MISSING".into(),
            "date" => p.effective_date = date("2025-01-01")?,
            "reason" => p.reason = "bad\nreason".into(),
            _ => return Err("Unknown input mutation".into()),
        }
        reject(
            &mut e,
            actor("owner", Role::Analyst),
            Command::CreditCreate { proposal: p },
        );
    }
    Ok(())
}

#[test]
fn closed_period_multiline_booking_rolls_back_disk_and_capacity_then_adjacent_date_succeeds()
-> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    seed(&mut e)?;
    e.run(
        actor("closer", Role::Analyst),
        Command::PeriodCloseCreate {
            terms: PeriodCloseTerms {
                id: "OCTOBER".into(),
                start: date("2026-10-01")?,
                end: date("2026-11-01")?,
                currency: Currency::USD,
                reason: "Synthetic empty-period close".into(),
            },
        },
    )?;
    e.run(
        actor("closer", Role::Analyst),
        Command::PeriodCloseSubmit {
            id: "OCTOBER".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("closing-finance", Role::Finance),
        Command::PeriodCloseDecide {
            id: "OCTOBER".into(),
            version: 2,
            approve: true,
            note: "Independent empty basis review".into(),
        },
    )?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic two-line boundary booking".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    for quantity in [1000, 2000] {
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteAddLine {
                id: "Q".into(),
                version: quote_version(&e)?,
                request: PriceRequest {
                    product_id: "VLV-100".into(),
                    customer_id: "C-ATLAS".into(),
                    date: date("2026-10-13")?,
                    quantity: Quantity(quantity),
                    discount: Bps(0),
                    rebate: Bps(0),
                    source: None,
                    conversion: None,
                },
            },
        )?;
    }
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: quote_version(&e)?,
        },
    )?;
    e.run(
        actor("commercial", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: quote_version(&e)?,
            approve: true,
            note: "Reviewed two complete lines".into(),
        },
    )?;
    if e.state()
        .quotes
        .get("Q")
        .is_some_and(|q| q.status == Status::Pending)
    {
        e.run(
            actor("quote-finance", Role::Finance),
            Command::QuoteDecide {
                id: "Q".into(),
                version: quote_version(&e)?,
                approve: true,
                note: "Independent escalation review".into(),
            },
        )?;
    }
    let snapshots = || -> std::io::Result<Vec<(String, Vec<u8>)>> {
        let mut files = vec![];
        for entry in fs::read_dir(&workspace.0)? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                files.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read(entry.path())?,
                ));
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    };
    let before = e.state().clone();
    let files_before = snapshots()?;
    let mut request = BookingRequest {
        id: "B".into(),
        quote_id: "Q".into(),
        quote_version: quote_version(&e)?,
        invoice_date: date("2026-10-31")?,
        source: "synthetic-closed-boundary".into(),
    };
    assert!(e.state().preview_quote_booking(&request).is_err());
    reject(
        &mut e,
        actor("booker", Role::Analyst),
        Command::QuoteBook {
            request: request.clone(),
        },
    );
    assert_eq!(e.state(), &before);
    assert_eq!(snapshots()?, files_before);
    assert!(!e.state().quote_bookings.contains_key("B"));
    assert!(e.state().transactions.is_empty());
    assert!(e.state().transaction_revisions.is_empty());
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &before);
    request.invoice_date = date("2026-11-01")?;
    assert_eq!(e.state().preview_quote_booking(&request)?.len(), 2);
    e.run(
        actor("booker", Role::Analyst),
        Command::QuoteBook { request },
    )?;
    assert_eq!(
        e.state().quote_bookings.get("B").map(|b| b.lines.len()),
        Some(2)
    );
    assert_eq!(
        e.state()
            .transactions
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["B-L001", "B-L002"]
    );
    assert_eq!(e.state().transaction_revisions.len(), 2);
    e.state().validate()?;
    Ok(())
}
