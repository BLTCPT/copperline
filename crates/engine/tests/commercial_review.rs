//! Independent review regressions for live prices, invoice actuals and rebate accruals.
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
            "copperline-commercial-review-{}-{}",
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
fn date(s: &str) -> Result<Date> {
    Date::parse(s)
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn price_request() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(1_000),
        discount: Bps(0),
        rebate: Bps(0),
    })
}
fn approved_grid(e: &mut Engine) -> Result<()> {
    e.run(
        actor("analyst", Role::Analyst),
        Command::GridCalculate {
            id: "GRID".into(),
            version: 0,
            request: price_request()?,
        },
    )?;
    e.run(
        actor("submitter", Role::Analyst),
        Command::GridSubmit {
            id: "GRID".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::GridDecide {
            id: "GRID".into(),
            version: 2,
            approve: true,
            note: "Independent price review".into(),
        },
    )?;
    Ok(())
}
fn terms(id: &str) -> Result<AgreementTerms> {
    Ok(AgreementTerms {
        id: id.into(),
        name: "Synthetic retrospective rebate".into(),
        customer_id: "C-ATLAS".into(),
        currency: Currency::USD,
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
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
    })
}
fn approve_agreement(e: &mut Engine, terms: AgreementTerms) -> Result<()> {
    let id = terms.id.clone();
    e.run(
        actor("analyst", Role::Analyst),
        Command::AgreementCreate { terms },
    )?;
    e.run(
        actor("submitter", Role::Analyst),
        Command::AgreementSubmit {
            id: id.clone(),
            version: 1,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::AgreementDecide {
            id,
            version: 2,
            approve: true,
            note: "Independent terms review".into(),
        },
    )?;
    Ok(())
}
fn invoice(id: &str, day: &str, invoice: i64, cost: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "Synthetic invoice fixture".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date(day)?,
        quantity: Quantity(1_000),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(invoice),
        cost_total: Money(cost),
    })
}
fn accrue(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("analyst", Role::Analyst),
        Command::RebateAccrue {
            id: id.into(),
            agreement_id: "AGREEMENT".into(),
            version: 3,
        },
    )?;
    Ok(())
}

#[test]
fn m1_workspace_hash_survives_first_m2_mutation_and_reopen() -> TestResult {
    // Exact M1 State::default field order from public baseline 6083f03, with no M2 keys.
    let m1_state = r#"{"schema_version":1,"revision":0,"products":{},"customers":{},"costs":[],"rules":[],"quotes":{},"price_lists":{},"policy":{"min_margin":2500,"max_discount":1500,"hard_min_margin":0},"audit":[],"receipts":[]}"#;
    let original = format!(
        r#"{{"previous_hash":"","hash":"{}","state":{m1_state}}}"#,
        copperline_storage::fingerprint(m1_state.as_bytes())
    );
    let workspace = Workspace::new()?;
    let first = workspace.0.join("00000000000000000000.json");
    fs::write(&first, &original)?;
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &State::default());
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    approved_grid(&mut e)?;
    let expected = e.state().clone();
    drop(e);
    let reopened = Engine::open(&workspace.0)?;
    assert_eq!(reopened.state(), &expected);
    assert_eq!(fs::read_to_string(first)?, original);
    Ok(())
}

#[test]
fn grid_reauthoring_retains_history_and_requires_fresh_independent_approval() -> TestResult {
    let mut e = demo()?;
    approved_grid(&mut e)?;
    let active = e
        .state()
        .grid
        .get("GRID")
        .and_then(|g| g.last_approved.clone())
        .ok_or("missing active price")?;
    let mut request = price_request()?;
    request.discount = Bps(1_000);
    // A former reviewer may author a new candidate, but cannot approve that new candidate.
    e.run(
        actor("reviewer", Role::Admin),
        Command::GridCalculate {
            id: "GRID".into(),
            version: 3,
            request: request.clone(),
        },
    )?;
    assert_eq!(
        e.state()
            .grid
            .get("GRID")
            .and_then(|g| g.last_approved.clone()),
        Some(active.clone())
    );
    e.run(
        actor("submitter", Role::Analyst),
        Command::GridSubmit {
            id: "GRID".into(),
            version: 4,
        },
    )?;
    for name in ["reviewer", "submitter"] {
        let before = e.state().clone();
        assert!(
            e.run(
                actor(name, Role::Admin),
                Command::GridDecide {
                    id: "GRID".into(),
                    version: 5,
                    approve: true,
                    note: "Not independent".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    let before = e.state().clone();
    assert!(
        e.run(
            actor("analyst", Role::Analyst),
            Command::GridCalculate {
                id: "GRID".into(),
                version: 5,
                request
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    e.run(
        actor("second-reviewer", Role::Approver),
        Command::GridDecide {
            id: "GRID".into(),
            version: 5,
            approve: false,
            note: "Retain active price".into(),
        },
    )?;
    assert_eq!(
        e.state()
            .grid
            .get("GRID")
            .and_then(|g| g.last_approved.clone()),
        Some(active)
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn grid_snapshot_cannot_lose_active_price_or_reference_missing_customer() -> TestResult {
    let mut e = demo()?;
    approved_grid(&mut e)?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::GridCalculate {
            id: "GRID".into(),
            version: 3,
            request: price_request()?,
        },
    )?;
    let mut corrupted = e.state().clone();
    corrupted
        .grid
        .get_mut("GRID")
        .ok_or("missing grid")?
        .last_approved = None;
    assert!(
        corrupted.validate().is_err(),
        "approved history must retain its active price"
    );
    let mut corrupted = e.state().clone();
    let item = corrupted.grid.get_mut("GRID").ok_or("missing grid")?;
    item.candidate.request.customer_id = "MISSING".into();
    if let Some(active) = &mut item.last_approved {
        active.evaluation.request.customer_id = "MISSING".into();
    }
    assert!(corrupted.validate().is_err(), "grid customer must exist");
    Ok(())
}

#[test]
fn grid_hard_block_and_stale_version_preserve_last_approved() -> TestResult {
    let mut e = demo()?;
    approved_grid(&mut e)?;
    let active = e
        .state()
        .grid
        .get("GRID")
        .and_then(|g| g.last_approved.clone())
        .ok_or("missing active")?;
    let mut request = price_request()?;
    request.discount = Bps(10_000);
    e.run(
        actor("analyst", Role::Analyst),
        Command::GridCalculate {
            id: "GRID".into(),
            version: 3,
            request,
        },
    )?;
    let before = e.state().clone();
    assert!(
        e.run(
            actor("analyst", Role::Analyst),
            Command::GridSubmit {
                id: "GRID".into(),
                version: 4
            }
        )
        .is_err()
    );
    assert!(
        e.run(
            actor("analyst", Role::Analyst),
            Command::GridCalculate {
                id: "GRID".into(),
                version: 3,
                request: price_request()?
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    assert_eq!(
        e.state()
            .grid
            .get("GRID")
            .and_then(|g| g.last_approved.clone()),
        Some(active)
    );
    Ok(())
}

#[test]
fn agreement_overlap_is_atomic_but_adjacent_period_is_approvable() -> TestResult {
    let mut e = demo()?;
    approve_agreement(&mut e, terms("AGREEMENT")?)?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::AgreementCreate {
            terms: terms("OVERLAP")?,
        },
    )?;
    e.run(
        actor("submitter", Role::Analyst),
        Command::AgreementSubmit {
            id: "OVERLAP".into(),
            version: 1,
        },
    )?;
    let before = e.state().clone();
    assert!(
        e.run(
            actor("reviewer", Role::Approver),
            Command::AgreementDecide {
                id: "OVERLAP".into(),
                version: 2,
                approve: true,
                note: "Overlapping period".into()
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    assert_eq!(
        e.state().agreements.get("OVERLAP").map(|a| a.status),
        Some(Status::Pending)
    );
    e.run(
        actor("reviewer", Role::Approver),
        Command::AgreementDecide {
            id: "OVERLAP".into(),
            version: 2,
            approve: false,
            note: "Rejected overlapping period".into(),
        },
    )?;
    let mut next = terms("ADJACENT")?;
    next.start = date("2027-01-01")?;
    next.end = date("2028-01-01")?;
    approve_agreement(&mut e, next)?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn agreement_requires_independent_approval_and_valid_tiers() -> TestResult {
    let mut e = demo()?;
    for tiers in [
        vec![],
        vec![RebateTier {
            threshold: Money(1),
            rate: Bps(200),
        }],
        vec![
            RebateTier {
                threshold: Money(0),
                rate: Bps(200),
            },
            RebateTier {
                threshold: Money(0),
                rate: Bps(500),
            },
        ],
        vec![
            RebateTier {
                threshold: Money(0),
                rate: Bps(500),
            },
            RebateTier {
                threshold: Money(100),
                rate: Bps(200),
            },
        ],
    ] {
        let mut invalid = terms("BAD")?;
        invalid.tiers = tiers;
        let before = e.state().clone();
        assert!(
            e.run(
                actor("analyst", Role::Analyst),
                Command::AgreementCreate { terms: invalid }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    e.run(
        actor("analyst", Role::Analyst),
        Command::AgreementCreate {
            terms: terms("AGREEMENT")?,
        },
    )?;
    e.run(
        actor("submitter", Role::Analyst),
        Command::AgreementSubmit {
            id: "AGREEMENT".into(),
            version: 1,
        },
    )?;
    for name in ["analyst", "submitter"] {
        let before = e.state().clone();
        assert!(
            e.run(
                actor(name, Role::Admin),
                Command::AgreementDecide {
                    id: "AGREEMENT".into(),
                    version: 2,
                    approve: true,
                    note: "Not independent".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    assert!(accrue(&mut e, "NOT-APPROVED").is_err());
    Ok(())
}

#[test]
fn accrual_eligibility_and_analytics_exclude_projections_and_other_currencies() -> TestResult {
    let mut e = demo()?;
    approve_agreement(&mut e, terms("AGREEMENT")?)?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::ProductPut(Product {
            id: "EUR-P".into(),
            name: "Synthetic euro valve".into(),
            family: "Flow".into(),
            unit: "each".into(),
            currency: Currency::EUR,
            list_price: Money(10_000),
        }),
    )?;
    for t in [
        invoice("START", "2026-01-01", 90_000, 60_000)?,
        invoice("INSIDE", "2026-12-31", 10_000, 5_000)?,
        invoice("BEFORE", "2025-12-31", 50_000, 10_000)?,
        invoice("END", "2027-01-01", 50_000, 10_000)?,
    ] {
        e.run(actor("analyst", Role::Analyst), Command::TransactionAdd(t))?;
    }
    let mut other_customer = invoice("OTHER-C", "2026-10-13", 99_000, 10_000)?;
    other_customer.customer_id = "C-NORTH".into();
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(other_customer),
    )?;
    let mut euro = invoice("OTHER-CURRENCY", "2026-10-13", 200_000, 90_000)?;
    euro.product_id = "EUR-P".into();
    euro.currency = Currency::EUR;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(euro),
    )?;
    let before = serde_json::to_value(analytics(e.state())?)?;
    approved_grid(&mut e)?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "PROJECTION".into(),
            title: "Projection only".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "PROJECTION".into(),
            version: 1,
            request: price_request()?,
        },
    )?;
    accrue(&mut e, "ACCRUAL")?;
    assert_eq!(serde_json::to_value(analytics(e.state())?)?, before);
    let accrual = e.state().accruals.get("ACCRUAL").ok_or("missing accrual")?;
    assert_eq!(accrual.transaction_ids, vec!["INSIDE", "START"]);
    assert_eq!(
        (accrual.eligible_invoice, accrual.rate, accrual.rebate),
        (Money(100_000), Bps(500), Money(5_000))
    );
    let groups = analytics(e.state())?;
    let usd = groups
        .iter()
        .find(|g| g.customer_id == "C-ATLAS" && g.currency == Currency::USD)
        .ok_or("missing USD group")?;
    assert_eq!(
        (usd.invoice, usd.cost, usd.margin_bps),
        (Money(200_000), Money(85_000), Some(5_750))
    );
    assert_eq!(groups.len(), 3);
    Ok(())
}

#[test]
fn accrual_is_frozen_and_cannot_predate_agreement_approval() -> TestResult {
    let mut e = demo()?;
    approve_agreement(&mut e, terms("AGREEMENT")?)?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("T1", "2026-10-13", 90_000, 60_000)?),
    )?;
    accrue(&mut e, "FIRST")?;
    let first = e
        .state()
        .accruals
        .get("FIRST")
        .ok_or("missing first")?
        .clone();
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("T2", "2026-10-13", 10_000, 5_000)?),
    )?;
    accrue(&mut e, "SECOND")?;
    assert_eq!(e.state().accruals.get("FIRST"), Some(&first));
    assert_eq!(first.rebate, Money(1_800));
    assert_eq!(
        e.state().accruals.get("SECOND").map(|a| a.rebate),
        Some(Money(5_000))
    );
    let mut invalid = e.state().clone();
    invalid
        .accruals
        .get_mut("FIRST")
        .ok_or("missing first")?
        .source_revision = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = e.state().clone();
    invalid
        .accruals
        .get_mut("FIRST")
        .ok_or("missing first")?
        .transaction_ids
        .push("T1".into());
    assert!(invalid.validate().is_err());
    Ok(())
}

#[test]
fn transaction_csv_late_failures_leave_no_partial_import_or_audit() -> TestResult {
    let mut e = demo()?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("EXISTING", "2026-10-13", 100, 60)?),
    )?;
    let header =
        "id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\n";
    let first = "NEW,synthetic,VLV-100,C-ATLAS,2026-10-13,1.000,each,USD,100.00,60.00\n";
    for bad in [
        "EXISTING,synthetic,VLV-100,C-ATLAS,2026-10-13,1,each,USD,10,6",
        "NEW,synthetic,VLV-100,C-ATLAS,2026-10-13,1,each,USD,10,6",
        "BAD,synthetic,VLV-100,C-ATLAS,2026-02-30,1,each,USD,10,6",
        "BAD,synthetic,VLV-100,C-ATLAS,2026-10-13,1,each,USD,-10,6",
        "BAD,synthetic,VLV-100,C-ATLAS,2026-10-13,1,kg,USD,10,6",
    ] {
        let before = e.state().clone();
        assert!(
            e.run(
                actor("analyst", Role::Analyst),
                Command::TransactionsImport {
                    csv: format!("{header}{first}{bad}")
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    e.run(actor("analyst", Role::Analyst), Command::TransactionsImport { csv: format!("{header}QUOTED,\"Synthetic, CSV\",VLV-100,C-ATLAS,2026-10-13,1,each,USD,100.00,60.00") })?;
    assert_eq!(
        e.state()
            .transactions
            .get("QUOTED")
            .map(|t| t.source.as_str()),
        Some("Synthetic, CSV")
    );
    Ok(())
}

#[test]
fn invoice_aggregate_overflow_is_atomic_and_add_is_idempotent() -> TestResult {
    let mut e = demo()?;
    let request = Request {
        request_id: "invoice-idempotency".into(),
        expected_revision: e.state().revision,
        actor: actor("analyst", Role::Analyst),
        action: Command::TransactionAdd(invoice("MAX", "2026-10-13", i64::MAX, 0)?),
    };
    e.execute(request.clone())?;
    let before = e.state().clone();
    assert!(e.execute(request)?.replayed);
    assert_eq!(e.state(), &before);
    assert!(
        e.run(
            actor("analyst", Role::Analyst),
            Command::TransactionAdd(invoice("ONE-MORE", "2026-10-13", 1, 0)?)
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    Ok(())
}

#[test]
fn invoice_snapshot_validation_rejects_aggregate_overflow() -> TestResult {
    let mut e = demo()?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("MAX", "2026-10-13", i64::MAX - 1, 0)?),
    )?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("ONE-MORE", "2026-10-13", 1, 0)?),
    )?;
    let mut invalid = e.state().clone();
    invalid
        .transactions
        .get_mut("MAX")
        .ok_or("missing invoice")?
        .invoice_total = Money(i64::MAX);
    assert!(analytics(&invalid).is_err());
    let failure = invalid
        .validate()
        .err()
        .ok_or("overflowed snapshot was accepted")?;
    assert!(failure.to_string().contains("Amount overflow"));
    Ok(())
}

#[test]
fn accrual_snapshot_contains_all_and_only_invoices_known_at_source_revision() -> TestResult {
    let mut e = demo()?;
    approve_agreement(&mut e, terms("AGREEMENT")?)?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("T1", "2026-10-13", 90_000, 60_000)?),
    )?;
    accrue(&mut e, "FIRST")?;
    e.run(
        actor("analyst", Role::Analyst),
        Command::TransactionAdd(invoice("T2", "2026-10-13", 10_000, 5_000)?),
    )?;
    accrue(&mut e, "SECOND")?;

    // Arithmetic can be internally consistent yet use an invoice recorded after the source.
    let mut invalid = e.state().clone();
    let first = invalid.accruals.get_mut("FIRST").ok_or("missing first")?;
    first.transaction_ids.push("T2".into());
    first.eligible_invoice = Money(100_000);
    first.rate = Bps(500);
    first.rebate = Money(5_000);
    assert!(
        invalid.validate().is_err(),
        "future-recorded invoice changed a historical accrual"
    );

    // Likewise, silently omitting a then-eligible invoice must not lower a frozen tier.
    let mut invalid = e.state().clone();
    let second = invalid.accruals.get_mut("SECOND").ok_or("missing second")?;
    second.transaction_ids.retain(|id| id != "T2");
    second.eligible_invoice = Money(90_000);
    second.rate = Bps(200);
    second.rebate = Money(1_800);
    assert!(
        invalid.validate().is_err(),
        "omitting eligible invoice changed a historical accrual"
    );
    Ok(())
}
