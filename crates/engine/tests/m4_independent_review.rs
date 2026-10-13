//! Independent adversarial review of invoice corrections and local settlement true-ups.
use copperline_engine::{Command, Engine, analytics};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};

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
fn seeded() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("administrator", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn agreement(
    e: &mut Engine,
    id: &str,
    customer: &str,
    currency: Currency,
    start: &str,
    end: &str,
) -> Result<()> {
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementCreate {
            terms: AgreementTerms {
                id: id.into(),
                name: "Synthetic retrospective review agreement".into(),
                customer_id: customer.into(),
                currency,
                start: date(start)?,
                end: date(end)?,
                tiers: vec![
                    RebateTier {
                        threshold: Money(0),
                        rate: Bps(200),
                    },
                    RebateTier {
                        threshold: Money(100_000),
                        rate: Bps(500),
                    },
                    RebateTier {
                        threshold: Money(200_000),
                        rate: Bps(1_000),
                    },
                ],
            },
        },
    )?;
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("commercial-reviewer", Role::Approver),
        Command::AgreementDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Reviewed synthetic terms".into(),
        },
    )?;
    Ok(())
}
fn usual() -> Result<Engine> {
    let mut e = seeded()?;
    agreement(
        &mut e,
        "A",
        "C-ATLAS",
        Currency::USD,
        "2026-01-01",
        "2027-01-01",
    )?;
    Ok(e)
}
fn invoice(id: &str, amount: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "independent-synthetic-fixture".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(1_000),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(amount),
        cost_total: Money(amount / 2),
    })
}
fn add(e: &mut Engine, transaction: Transaction) -> Result<()> {
    e.run(
        actor("invoice-author", Role::Analyst),
        Command::TransactionAdd(transaction),
    )?;
    Ok(())
}
fn accrue(e: &mut Engine, id: &str, agreement: &str) -> Result<()> {
    e.run(
        actor("accrual-author", Role::Analyst),
        Command::RebateAccrue {
            id: id.into(),
            agreement_id: agreement.into(),
            version: 3,
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
        actor("correction-author", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: id.into(),
                original_transaction_id: original.into(),
                effective_date: date("2027-12-31")?,
                reason: "Synthetic independent correction review".into(),
                replacement,
            },
        },
    )?;
    e.run(
        actor("correction-author", Role::Analyst),
        Command::CorrectionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("finance-reviewer", Role::Finance),
        Command::CorrectionDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Reviewed immutable reversal and replacement".into(),
        },
    )?;
    Ok(())
}
fn settlement(e: &mut Engine, id: &str, accrual: &str) -> Result<()> {
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementCreate {
            id: id.into(),
            accrual_id: accrual.into(),
            reason: "Synthetic local recognition only".into(),
        },
    )?;
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    Ok(())
}
fn decide(e: &mut Engine, id: &str, approve: bool) -> Result<()> {
    e.run(
        actor("finance-reviewer", Role::Finance),
        Command::SettlementDecide {
            id: id.into(),
            version: 2,
            approve,
            note: "Independent local settlement review".into(),
        },
    )?;
    Ok(())
}

#[test]
fn correction_record_date_does_not_move_reversal_period_or_convert_replacement_context()
-> TestResult {
    let mut e = usual()?;
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::ProductPut(Product {
            id: "EUR-P".into(),
            name: "Synthetic euro valve".into(),
            family: "Flow".into(),
            unit: "each".into(),
            currency: Currency::EUR,
            list_price: Money(100_000),
        }),
    )?;
    agreement(
        &mut e,
        "B",
        "C-NORTH",
        Currency::EUR,
        "2027-01-01",
        "2028-01-01",
    )?;
    let original = invoice("ORIGINAL", 100_000)?;
    add(&mut e, original.clone())?;
    accrue(&mut e, "OLD", "A")?;
    let old = e
        .state()
        .accruals
        .get("OLD")
        .cloned()
        .ok_or("Missing old accrual")?;
    settlement(&mut e, "BEFORE", "OLD")?;
    decide(&mut e, "BEFORE", true)?;
    let mut replacement = invoice("REPLACEMENT", 80_000)?;
    replacement.date = date("2027-03-04")?;
    replacement.customer_id = "C-NORTH".into();
    replacement.currency = Currency::EUR;
    replacement.product_id = "EUR-P".into();
    correction(&mut e, "MOVE", "ORIGINAL", Some(replacement.clone()))?;
    assert_eq!(e.state().transactions.get("ORIGINAL"), Some(&original));
    assert_eq!(
        e.state().transactions.get("REPLACEMENT"),
        Some(&replacement)
    );
    assert_eq!(e.state().accruals.get("OLD"), Some(&old));
    accrue(&mut e, "A-ZERO", "A")?;
    accrue(&mut e, "B-NEW", "B")?;
    let a = e
        .state()
        .accruals
        .get("A-ZERO")
        .ok_or("Missing reversal accrual")?;
    assert_eq!(a.transaction_ids, vec!["ORIGINAL"]);
    assert_eq!(a.correction_ids, vec!["MOVE"]);
    assert_eq!((a.eligible_invoice, a.rebate), (Money(0), Money(0)));
    let b = e
        .state()
        .accruals
        .get("B-NEW")
        .ok_or("Missing replacement accrual")?;
    assert_eq!(b.transaction_ids, vec!["REPLACEMENT"]);
    assert!(b.correction_ids.is_empty());
    assert_eq!(
        (b.eligible_invoice, b.rebate),
        (Money(80_000), Money(1_600))
    );
    settlement(&mut e, "REFUND", "A-ZERO")?;
    assert_eq!(
        e.state().settlements.get("REFUND").map(|s| s.adjustment),
        Some(Money(-5_000))
    );
    decide(&mut e, "REFUND", true)?;
    settlement(&mut e, "B-RECOGNITION", "B-NEW")?;
    let b_settlement = e
        .state()
        .settlements
        .get("B-RECOGNITION")
        .ok_or("Missing replacement settlement")?;
    assert!(b_settlement.contributors.contains("correction-author"));
    assert!(
        !b_settlement.contributors.contains("finance-reviewer"),
        "Approving replacement ingestion must not misclassify its independent reviewer as an author"
    );
    decide(&mut e, "B-RECOGNITION", true)?;
    let groups = analytics(e.state())?;
    let a_group = groups
        .iter()
        .find(|g| g.customer_id == "C-ATLAS")
        .ok_or("Missing original analytics")?;
    assert_eq!(
        (a_group.invoice, a_group.cost, a_group.margin_bps),
        (Money(0), Money(0), None)
    );
    let b_group = groups
        .iter()
        .find(|g| g.customer_id == "C-NORTH")
        .ok_or("Missing replacement analytics")?;
    assert_eq!(b_group.currency, Currency::EUR);
    assert_eq!(b_group.invoice, Money(80_000));
    e.state().validate()?;
    Ok(())
}

#[test]
fn equal_amount_replacement_stales_provenance_but_pending_record_can_be_rejected() -> TestResult {
    let mut e = usual()?;
    add(&mut e, invoice("ORIGINAL", 100_000)?)?;
    accrue(&mut e, "OLD", "A")?;
    settlement(&mut e, "STALE", "OLD")?;
    correction(
        &mut e,
        "SAME-AMOUNT",
        "ORIGINAL",
        Some(invoice("REPLACEMENT", 100_000)?),
    )?;
    assert_eq!(
        e.state()
            .rebate_position("A", e.state().revision)?
            .target_rebate,
        Money(5_000)
    );
    let before = e.state().clone();
    assert!(decide(&mut e, "STALE", true).is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "STALE", false)?;
    assert_eq!(
        e.state().settlements.get("STALE").map(|s| s.status),
        Some(RecordStatus::Rejected)
    );
    accrue(&mut e, "FRESH", "A")?;
    settlement(&mut e, "CURRENT", "FRESH")?;
    decide(&mut e, "CURRENT", true)?;
    assert_eq!(
        e.state().settlement_baseline("A", e.state().revision)?.1,
        Money(5_000)
    );
    Ok(())
}

#[test]
fn stale_draft_can_withdraw_and_unrelated_catalog_write_does_not_stale_current_source() -> TestResult
{
    let mut e = usual()?;
    add(&mut e, invoice("ORIGINAL", 100_000)?)?;
    accrue(&mut e, "OLD", "A")?;
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementCreate {
            id: "DRAFT".into(),
            accrual_id: "OLD".into(),
            reason: "Local proposal".into(),
        },
    )?;
    correction(&mut e, "REVERSE", "ORIGINAL", None)?;
    let before = e.state().clone();
    assert!(
        e.run(
            actor("settlement-author", Role::Analyst),
            Command::SettlementSubmit {
                id: "DRAFT".into(),
                version: 1
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementWithdraw {
            id: "DRAFT".into(),
            version: 1,
        },
    )?;
    add(&mut e, invoice("NEW", 100_000)?)?;
    accrue(&mut e, "FRESH", "A")?;
    settlement(&mut e, "LIVE", "FRESH")?;
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::CustomerPut(Customer {
            id: "UNRELATED".into(),
            name: "Synthetic unrelated customer".into(),
            segment: "Research".into(),
        }),
    )?;
    decide(&mut e, "LIVE", true)?;
    Ok(())
}

#[test]
fn competing_proposals_cannot_recognize_same_rebate_twice() -> TestResult {
    let mut e = usual()?;
    add(&mut e, invoice("I", 100_000)?)?;
    accrue(&mut e, "R", "A")?;
    settlement(&mut e, "FIRST", "R")?;
    settlement(&mut e, "SECOND", "R")?;
    decide(&mut e, "FIRST", true)?;
    let before = e.state().clone();
    assert!(decide(&mut e, "SECOND", true).is_err());
    assert_eq!(e.state(), &before);
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementWithdraw {
            id: "SECOND".into(),
            version: 2,
        },
    )?;
    assert_eq!(
        e.state().settlement_baseline("A", e.state().revision)?.1,
        Money(5_000)
    );
    let before = e.state().clone();
    assert!(
        e.run(
            actor("settlement-author", Role::Analyst),
            Command::SettlementCreate {
                id: "ZERO".into(),
                accrual_id: "R".into(),
                reason: "No difference".into()
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    Ok(())
}

#[test]
fn tier_drop_refunds_whole_period_difference_and_offsetting_prior_ids_cannot_disappear()
-> TestResult {
    let mut e = usual()?;
    add(&mut e, invoice("I-1", 100_000)?)?;
    accrue(&mut e, "R-1", "A")?;
    settlement(&mut e, "S-1", "R-1")?;
    decide(&mut e, "S-1", true)?;
    add(&mut e, invoice("I-2", 100_000)?)?;
    accrue(&mut e, "R-2", "A")?;
    settlement(&mut e, "S-2", "R-2")?;
    assert_eq!(
        e.state().settlements.get("S-2").map(|s| s.adjustment),
        Some(Money(15_000))
    );
    decide(&mut e, "S-2", true)?;
    correction(&mut e, "REV-2", "I-2", None)?;
    accrue(&mut e, "R-3", "A")?;
    settlement(&mut e, "S-3", "R-3")?;
    // Net drops from 2,000 @ 10% to 1,000 @ 5%: refund 150, not reversed 1,000 @ 10% = 100.
    assert_eq!(
        e.state().settlements.get("S-3").map(|s| s.adjustment),
        Some(Money(-15_000))
    );
    decide(&mut e, "S-3", true)?;
    assert_eq!(
        e.state().settlement_baseline("A", e.state().revision)?.1,
        Money(5_000)
    );
    add(&mut e, invoice("I-3", 50_000)?)?;
    accrue(&mut e, "R-4", "A")?;
    settlement(&mut e, "S-4", "R-4")?;
    let before = e.state().clone();
    let mut tampered = before.clone();
    let record = tampered
        .settlements
        .get_mut("S-4")
        .ok_or("Missing settlement")?;
    assert_eq!(record.prior_settlement_ids, vec!["S-1", "S-2", "S-3"]);
    record.prior_settlement_ids = vec!["S-1".into()];
    assert!(
        tampered.validate().is_err(),
        "Offsetting adjustments remain essential provenance even when sum is unchanged"
    );
    let mut tampered = before;
    tampered
        .accruals
        .get_mut("R-4")
        .ok_or("Missing accrual")?
        .correction_ids
        .clear();
    assert!(tampered.validate().is_err());
    Ok(())
}

#[test]
fn correction_and_settlement_snapshot_authority_and_chronology_are_not_editable() -> TestResult {
    let mut e = usual()?;
    add(&mut e, invoice("I", 100_000)?)?;
    accrue(&mut e, "R", "A")?;
    settlement(&mut e, "S", "R")?;
    decide(&mut e, "S", true)?;
    correction(&mut e, "C", "I", Some(invoice("RPL", 90_000)?))?;
    let original = e.state().clone();
    let mut tampered = original.clone();
    tampered
        .corrections
        .get_mut("C")
        .ok_or("Missing correction")?
        .original
        .invoice_total = Money(90_000);
    assert!(tampered.validate().is_err());
    let mut tampered = original.clone();
    tampered.transaction_revisions.insert("RPL".into(), 1);
    assert!(tampered.validate().is_err());
    let mut tampered = original.clone();
    tampered
        .settlements
        .get_mut("S")
        .and_then(|s| s.review.as_mut())
        .ok_or("Missing review")?
        .actor
        .role = Role::Admin;
    assert!(tampered.validate().is_err());
    let mut tampered = original.clone();
    tampered
        .settlements
        .get_mut("S")
        .ok_or("Missing settlement")?
        .contributors
        .remove("invoice-author");
    assert!(tampered.validate().is_err());
    let mut tampered = original;
    tampered.corrections.remove("C");
    assert!(tampered.validate().is_err());
    Ok(())
}

#[test]
fn accrual_identity_and_creation_event_cannot_be_replaced_with_an_unrelated_audit() -> TestResult {
    let mut e = usual()?;
    add(&mut e, invoice("I", 100_000)?)?;
    e.run(
        actor("unrelated-author", Role::Analyst),
        Command::CustomerPut(Customer {
            id: "OTHER".into(),
            name: "Unrelated synthetic customer".into(),
            segment: "Research".into(),
        }),
    )?;
    let unrelated_revision = e.state().revision;
    accrue(&mut e, "R", "A")?;
    let mut tampered = e.state().clone();
    tampered
        .accruals
        .get_mut("R")
        .ok_or("Missing accrual")?
        .source_revision = unrelated_revision - 1;
    assert!(
        tampered.validate().is_err(),
        "Accrual creator must be its own audited rebate_accrue event, not an arbitrary same-basis event"
    );
    let mut tampered = e.state().clone();
    let mut accrual = tampered.accruals.remove("R").ok_or("Missing accrual")?;
    accrual.id = "FORGED".into();
    tampered.accruals.insert("FORGED".into(), accrual);
    assert!(
        tampered.validate().is_err(),
        "Accrual identity must be bound to its command provenance"
    );
    Ok(())
}
