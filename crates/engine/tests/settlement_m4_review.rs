//! Independent adversarial coverage for local recognition records, never payments.
use copperline_engine::{Command, Engine};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
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
fn invoice(id: &str, revenue: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "synthetic-independent-review".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: Date::parse("2026-10-13")?,
        quantity: Quantity(10000),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(revenue),
        cost_total: Money(revenue / 2),
    })
}
fn initialize(e: &mut Engine) -> Result<()> {
    e.run(actor("Admin", Role::Admin), Command::SeedDemo)?;
    e.run(
        actor("Agreement writer", Role::Analyst),
        Command::AgreementCreate {
            terms: AgreementTerms {
                id: "A".into(),
                name: "Synthetic annual agreement".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::USD,
                start: Date::parse("2026-01-01")?,
                end: Date::parse("2027-01-01")?,
                tiers: vec![RebateTier {
                    threshold: Money(0),
                    rate: Bps(500),
                }],
            },
        },
    )?;
    e.run(
        actor("Agreement writer", Role::Analyst),
        Command::AgreementSubmit {
            id: "A".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("Agreement reviewer", Role::Approver),
        Command::AgreementDecide {
            id: "A".into(),
            version: 2,
            approve: true,
            note: "Synthetic independent agreement review".into(),
        },
    )?;
    e.run(
        actor("Invoice writer", Role::Analyst),
        Command::TransactionAdd(invoice("T1", 100_000)?),
    )?;
    accrue(e, "R1", "Accrual writer")?;
    Ok(())
}
fn fixture() -> Result<Engine> {
    let mut e = Engine::memory();
    initialize(&mut e)?;
    Ok(e)
}
fn accrue(e: &mut Engine, id: &str, name: &str) -> Result<()> {
    e.run(
        actor(name, Role::Analyst),
        Command::RebateAccrue {
            id: id.into(),
            agreement_id: "A".into(),
            version: 3,
        },
    )?;
    Ok(())
}
fn create(e: &mut Engine, id: &str, accrual_id: &str) -> Result<()> {
    e.run(
        actor("Settlement writer", Role::Analyst),
        Command::SettlementCreate {
            id: id.into(),
            accrual_id: accrual_id.into(),
            reason: "Synthetic reconciliation".into(),
        },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("Settlement writer", Role::Analyst),
        Command::SettlementSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    Ok(())
}
fn decide(e: &mut Engine, id: &str, approve: bool, name: &str) -> Result<()> {
    e.run(
        actor(name, Role::Finance),
        Command::SettlementDecide {
            id: id.into(),
            version: 2,
            approve,
            note: "Independent local recognition review".into(),
        },
    )?;
    Ok(())
}
fn record<'a>(e: &'a Engine, id: &str) -> Result<&'a Settlement> {
    e.state()
        .settlements
        .get(id)
        .ok_or_else(|| error("Missing settlement fixture"))
}
fn correct(
    e: &mut Engine,
    id: &str,
    original: &str,
    replacement: Option<Transaction>,
) -> Result<()> {
    e.run(
        actor("Correction writer", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: id.into(),
                original_transaction_id: original.into(),
                effective_date: Date::parse("2026-10-14")?,
                reason: "Synthetic full correction".into(),
                replacement,
            },
        },
    )?;
    e.run(
        actor("Correction writer", Role::Analyst),
        Command::CorrectionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("Correction reviewer", Role::Finance),
        Command::CorrectionDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Independent correction review".into(),
        },
    )?;
    Ok(())
}

#[test]
fn recognition_is_signed_delta_and_corrections_do_not_rewrite_prior_approvals() -> TestResult {
    let mut e = fixture()?;
    create(&mut e, "S1", "R1")?;
    submit(&mut e, "S1")?;
    decide(&mut e, "S1", true, "Settlement reviewer")?;
    let original = record(&e, "S1")?.clone();
    assert_eq!(original.adjustment, Money(5000));
    let before = e.state().clone();
    assert!(create(&mut e, "NO-DOUBLE-COUNT", "R1").is_err());
    assert_eq!(e.state(), &before);
    correct(&mut e, "C1", "T1", Some(invoice("T2", 60_000)?))?;
    accrue(&mut e, "R2", "Accrual writer")?;
    create(&mut e, "S2", "R2")?;
    assert_eq!(record(&e, "S2")?.adjustment, Money(-2000));
    assert_eq!(record(&e, "S2")?.prior_settlement_ids, ["S1"]);
    submit(&mut e, "S2")?;
    decide(&mut e, "S2", true, "Settlement reviewer")?;
    assert_eq!(record(&e, "S1")?, &original);
    let position = e.state().rebate_position("A", e.state().revision)?;
    assert_eq!(position.recognized_rebate, Money(3000));
    assert_eq!(position.remaining_adjustment, Money(0));
    correct(&mut e, "C2", "T2", None)?;
    accrue(&mut e, "R3", "Accrual writer")?;
    create(&mut e, "S3", "R3")?;
    assert_eq!(record(&e, "S3")?.adjustment, Money(-3000));
    submit(&mut e, "S3")?;
    decide(&mut e, "S3", true, "Settlement reviewer")?;
    assert_eq!(
        e.state()
            .rebate_position("A", e.state().revision)?
            .recognized_rebate,
        Money(0)
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn owner_and_all_source_authors_cannot_substitute_for_independent_finance() -> TestResult {
    let mut e = fixture()?;
    create(&mut e, "S1", "R1")?;
    let before = e.state().clone();
    for role in [Role::Analyst, Role::Admin] {
        assert!(
            e.run(
                actor("Unrelated writer", role),
                Command::SettlementSubmit {
                    id: "S1".into(),
                    version: 1
                }
            )
            .is_err()
        );
        assert!(
            e.run(
                actor("Unrelated writer", role),
                Command::SettlementWithdraw {
                    id: "S1".into(),
                    version: 1
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    submit(&mut e, "S1")?;
    let before = e.state().clone();
    for name in [
        "Settlement writer",
        "Agreement writer",
        "Invoice writer",
        "Accrual writer",
    ] {
        assert!(
            decide(&mut e, "S1", true, name).is_err(),
            "Contributor {name} was allowed"
        );
        assert_eq!(e.state(), &before);
    }
    for role in [Role::Admin, Role::Approver, Role::Analyst, Role::Sales] {
        assert!(
            e.run(
                actor("Independent wrong role", role),
                Command::SettlementDecide {
                    id: "S1".into(),
                    version: 2,
                    approve: true,
                    note: "Wrong role".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    decide(&mut e, "S1", true, "Independent Finance")?;
    assert_eq!(record(&e, "S1")?.status, RecordStatus::Approved);
    Ok(())
}

#[test]
fn replacement_author_is_excluded_but_correction_reviewer_can_review_settlement() -> TestResult {
    let mut e = fixture()?;
    correct(&mut e, "C1", "T1", Some(invoice("T2", 60_000)?))?;
    accrue(&mut e, "R2", "Accrual writer")?;
    create(&mut e, "S1", "R2")?;
    let contributors = &record(&e, "S1")?.contributors;
    assert!(contributors.contains("Correction writer"));
    assert!(contributors.contains("Invoice writer"));
    assert!(!contributors.contains("Correction reviewer"));
    submit(&mut e, "S1")?;
    let before = e.state().clone();
    assert!(decide(&mut e, "S1", true, "Correction writer").is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "S1", true, "Correction reviewer")?;
    Ok(())
}

#[test]
fn same_total_new_invoice_makes_pending_and_draft_sources_stale_but_releasable() -> TestResult {
    let mut e = fixture()?;
    create(&mut e, "PENDING", "R1")?;
    submit(&mut e, "PENDING")?;
    create(&mut e, "DRAFT", "R1")?;
    e.run(
        actor("Invoice writer", Role::Analyst),
        Command::TransactionAdd(invoice("ZERO", 0)?),
    )?;
    let before = e.state().clone();
    assert!(decide(&mut e, "PENDING", true, "Independent Finance").is_err());
    assert!(submit(&mut e, "DRAFT").is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "PENDING", false, "Independent Finance")?;
    e.run(
        actor("Settlement writer", Role::Analyst),
        Command::SettlementWithdraw {
            id: "DRAFT".into(),
            version: 1,
        },
    )?;
    assert_eq!(record(&e, "PENDING")?.status, RecordStatus::Rejected);
    assert_eq!(record(&e, "DRAFT")?.status, RecordStatus::Withdrawn);
    Ok(())
}

#[test]
fn same_total_new_correction_invalidates_approval_and_approved_baseline_cannot_double_count()
-> TestResult {
    let mut e = fixture()?;
    create(&mut e, "PENDING", "R1")?;
    submit(&mut e, "PENDING")?;
    correct(&mut e, "C1", "T1", Some(invoice("T2", 100_000)?))?;
    let before = e.state().clone();
    assert!(decide(&mut e, "PENDING", true, "Independent Finance").is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "PENDING", false, "Independent Finance")?;
    accrue(&mut e, "R2", "Accrual writer")?;
    create(&mut e, "FIRST", "R2")?;
    create(&mut e, "SECOND", "R2")?;
    submit(&mut e, "FIRST")?;
    submit(&mut e, "SECOND")?;
    decide(&mut e, "FIRST", true, "Independent Finance")?;
    let before = e.state().clone();
    assert!(decide(&mut e, "SECOND", true, "Independent Finance").is_err());
    assert_eq!(e.state(), &before);
    e.run(
        actor("Settlement writer", Role::Analyst),
        Command::SettlementWithdraw {
            id: "SECOND".into(),
            version: 2,
        },
    )?;
    assert_eq!(
        e.state()
            .rebate_position("A", e.state().revision)?
            .recognized_rebate,
        Money(5000)
    );
    Ok(())
}

#[test]
fn persisted_settlement_arithmetic_status_history_and_actor_tampering_are_rejected() -> TestResult {
    let mut e = fixture()?;
    create(&mut e, "S1", "R1")?;
    submit(&mut e, "S1")?;
    decide(&mut e, "S1", true, "Independent Finance")?;
    for mutation in 0..13 {
        let mut invalid = e.state().clone();
        let s = invalid
            .settlements
            .get_mut("S1")
            .ok_or("Missing settlement")?;
        match mutation {
            0 => s.adjustment = Money(5001),
            1 => s.status = RecordStatus::Draft,
            2 => s.version = 4,
            3 => s.contributors.clear(),
            4 => s.prior_settlement_ids.push("S1".into()),
            5 => s.previously_recognized = Money(5000),
            6 => s.submitted_revision = None,
            7 => s.review.as_mut().ok_or("Missing review")?.actor.name = "Invoice writer".into(),
            8 => s.review.as_mut().ok_or("Missing review")?.approved = false,
            9 => s.source_revision = s.created_revision,
            10 => {
                let revision = s.review.as_ref().ok_or("Missing review")?.revision;
                invalid
                    .audit
                    .iter_mut()
                    .find(|a| a.revision == revision)
                    .ok_or("Missing audit")?
                    .target = Some("settlement:OTHER".into());
            }
            11 => s.reason = "Changed justification after independent review".into(),
            _ => s.review.as_mut().ok_or("Missing review")?.note = "Forged Finance note".into(),
        }
        assert!(
            invalid.validate().is_err(),
            "Accepted settlement corruption case {mutation}"
        );
    }
    let mut missing = e.state().clone();
    missing.settlements.clear();
    assert!(missing.validate().is_err());
    Ok(())
}

#[test]
fn longest_valid_settlement_id_and_reason_fit_audit_evidence_limits() -> TestResult {
    let mut e = fixture()?;
    let id = "S".repeat(64);
    e.run(
        actor("Settlement writer", Role::Analyst),
        Command::SettlementCreate {
            id: id.clone(),
            accrual_id: "R1".into(),
            reason: "R".repeat(160),
        },
    )?;
    submit(&mut e, &id)?;
    decide(&mut e, &id, true, "Independent Finance")?;
    assert_eq!(record(&e, &id)?.status, RecordStatus::Approved);
    e.state().validate()?;
    Ok(())
}

#[test]
fn settlement_cannot_switch_to_a_different_authors_equivalent_accrual() -> TestResult {
    let mut e = fixture()?;
    accrue(&mut e, "R2", "Other accrual writer")?;
    create(&mut e, "S1", "R1")?;
    let mut invalid = e.state().clone();
    let other = invalid
        .accruals
        .get("R2")
        .cloned()
        .ok_or("Missing accrual")?;
    let replacement_contributors = invalid.settlement_contributors(&other, "Settlement writer")?;
    let record = invalid
        .settlements
        .get_mut("S1")
        .ok_or("Missing settlement")?;
    record.accrual_id = "R2".into();
    record.contributors = replacement_contributors;
    assert!(
        invalid.validate().is_err(),
        "A frozen settlement source and its contributor exclusion were retargeted without changing audit"
    );
    Ok(())
}

#[test]
fn settlement_accrual_author_cannot_be_rebound_to_an_unrelated_audit_event() -> TestResult {
    let mut e = fixture()?;
    e.run(
        actor("Unrelated customer editor", Role::Analyst),
        Command::CustomerPut(Customer {
            id: "C-ATLAS".into(),
            name: "Synthetic unchanged customer context".into(),
            segment: "OEM".into(),
        }),
    )?;
    create(&mut e, "S1", "R1")?;
    let mut invalid = e.state().clone();
    let source = invalid.accruals.get_mut("R1").ok_or("Missing accrual")?;
    source.source_revision += 1;
    let other = source.clone();
    let contributors = invalid.settlement_contributors(&other, "Settlement writer")?;
    invalid
        .settlements
        .get_mut("S1")
        .ok_or("Missing settlement")?
        .contributors = contributors;
    assert!(
        invalid.validate().is_err(),
        "Accrual creator was taken from an unrelated customer-edit audit"
    );
    Ok(())
}

#[test]
fn settlement_target_audit_cannot_outlive_a_deleted_record_even_if_command_is_corrupt() -> TestResult
{
    let mut e = fixture()?;
    create(&mut e, "S1", "R1")?;
    let mut invalid = e.state().clone();
    invalid.settlements.clear();
    for event in &mut invalid.audit {
        if event.target.as_deref() == Some("settlement:S1") {
            event.command = "customer_put".into();
        }
    }
    assert!(
        invalid.validate().is_err(),
        "Persisted settlement-target audit points to an erased record"
    );
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-settlement-review-{}-{}",
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
fn settlement_adjustments_and_historical_accruals_survive_reopen() -> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    initialize(&mut e)?;
    assert!(!serde_json::to_string(e.state())?.contains("\"settlements\""));
    create(&mut e, "S1", "R1")?;
    submit(&mut e, "S1")?;
    decide(&mut e, "S1", true, "Independent Finance")?;
    correct(&mut e, "C1", "T1", None)?;
    accrue(&mut e, "R2", "Accrual writer")?;
    create(&mut e, "S2", "R2")?;
    submit(&mut e, "S2")?;
    decide(&mut e, "S2", true, "Independent Finance")?;
    let before = e.state().clone();
    assert_eq!(
        before
            .accruals
            .get("R1")
            .ok_or("Missing historical accrual")?
            .rebate,
        Money(5000)
    );
    drop(e);
    let reopened = Engine::open(&dir.0)?;
    assert_eq!(reopened.state(), &before);
    Ok(())
}

#[test]
fn m3_shaped_accrual_chain_opens_and_supports_first_settlement_without_rewriting_history()
-> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    initialize(&mut e)?;
    let revision = e.state().revision;
    drop(e);
    // M3 had no correction/settlement fields and no target on rebate_accrue audits.
    // Rebuild the semantic checksum chain in that exact legacy shape.
    let mut previous = String::new();
    let mut originals = Vec::new();
    for n in 0..=revision {
        let path = dir.0.join(format!("{n:020}.json"));
        let envelope: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        let mut state: State =
            serde_json::from_value(envelope.get("state").cloned().ok_or("Missing state")?)?;
        for audit in &mut state.audit {
            if audit.command == "rebate_accrue" {
                audit.target = None;
            }
        }
        let encoded = serde_json::to_vec(&state)?;
        let json = std::str::from_utf8(&encoded)?;
        assert!(
            !json.contains("\"corrections\"")
                && !json.contains("\"settlements\"")
                && !json.contains("\"correction_ids\"")
        );
        let mut bytes = previous.as_bytes().to_vec();
        bytes.extend(encoded);
        let hash = copperline_storage::fingerprint(&bytes);
        let serialized = serde_json::to_vec(
            &serde_json::json!({"previous_hash": previous, "hash": hash, "state": state}),
        )?;
        fs::write(&path, &serialized)?;
        originals.push((path, serialized));
        previous = hash;
    }
    let mut reopened = Engine::open(&dir.0)?;
    create(&mut reopened, "FIRST-M4", "R1")?;
    submit(&mut reopened, "FIRST-M4")?;
    decide(&mut reopened, "FIRST-M4", true, "Independent Finance")?;
    for (path, bytes) in originals {
        assert_eq!(fs::read(path)?, bytes);
    }
    let before = reopened.state().clone();
    drop(reopened);
    assert_eq!(Engine::open(&dir.0)?.state(), &before);
    Ok(())
}
