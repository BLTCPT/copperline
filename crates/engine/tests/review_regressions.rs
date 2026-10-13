//! Independent public-API regressions for pricing, approval and persistence boundaries.
use copperline_engine::{Command, Engine, Request, export_quote};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
type Corruption<T> = (&'static str, fn(&mut T));
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-engine-review-{}-{}",
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
    let mut engine = Engine::memory();
    engine.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    Ok(engine)
}
fn line() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date()?,
        quantity: Quantity(10_000),
        discount: Bps(1_000),
        rebate: Bps(500),
        source: None,
    })
}
fn create_quote(engine: &mut Engine) -> Result<()> {
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic review fixture".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
        },
    )?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: line()?,
        },
    )?;
    Ok(())
}
fn approved_fixture() -> Result<State> {
    let mut engine = demo()?;
    create_quote(&mut engine)?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    engine.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "Synthetic review complete".into(),
        },
    )?;
    engine.state().validate()?;
    Ok(engine.state().clone())
}

#[test]
fn mixed_catalog_produces_separate_publishable_lists() -> TestResult {
    let mut engine = demo()?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::ProductPut(Product {
            id: "EUR-1".into(),
            name: "Synthetic euro valve".into(),
            family: "Flow".into(),
            unit: "each".into(),
            currency: Currency::EUR,
            list_price: Money(10_000),
        }),
    )?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "EUR-1".into(),
            version: 1,
            effective_from: date()?,
            amount: Money(6_000),
            currency: Currency::EUR,
            unit: "each".into(),
        }),
    )?;
    for (id, currency, count) in [
        ("USD-LIST", Currency::USD, 6),
        ("EUR-LIST", Currency::EUR, 1),
    ] {
        engine.run(
            actor("analyst", Role::Analyst),
            Command::ListCalculate {
                id: id.into(),
                name: "Synthetic currency list".into(),
                customer_id: "C-ATLAS".into(),
                date: date()?,
                quantity: Quantity(1_000),
                currency,
            },
        )?;
        let list = engine.state().price_lists.get(id).ok_or("missing list")?;
        assert_eq!(list.lines.len(), count);
        assert!(list.lines.iter().all(|line| line.currency == currency));
        engine.run(
            actor("analyst", Role::Analyst),
            Command::ListSubmit {
                id: id.into(),
                version: 1,
            },
        )?;
        engine.run(
            actor("reviewer", Role::Approver),
            Command::ListDecide {
                id: id.into(),
                version: 2,
                approve: true,
                note: "Currency-specific list reviewed".into(),
            },
        )?;
        engine.run(
            actor("analyst", Role::Analyst),
            Command::ListPublish {
                id: id.into(),
                version: 3,
            },
        )?;
        assert_eq!(
            engine.state().price_lists.get(id).map(|list| list.status),
            Some(Status::Published)
        );
    }
    let before = engine.state().clone();
    assert!(
        engine
            .run(
                actor("analyst", Role::Analyst),
                Command::ListCalculate {
                    id: "GBP-EMPTY".into(),
                    name: "Empty selection".into(),
                    customer_id: "C-ATLAS".into(),
                    date: date()?,
                    quantity: Quantity(1_000),
                    currency: Currency::GBP
                }
            )
            .is_err()
    );
    assert_eq!(engine.state(), &before);
    Ok(())
}

#[test]
fn failed_durable_commit_preserves_memory_and_request_can_retry() -> TestResult {
    let workspace = Workspace::new()?;
    let mut engine = Engine::open(&workspace.0)?;
    engine.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    let before = engine.state().clone();
    let previous_bytes = fs::read(workspace.0.join("00000000000000000001.json"))?;
    let pending = workspace.0.join("00000000000000000002.pending");
    fs::write(&pending, b"interrupted write must be preserved")?;
    let request = Request {
        request_id: "retry-after-storage-error".into(),
        expected_revision: before.revision,
        actor: actor("admin", Role::Admin),
        action: Command::PolicySet(Policy {
            max_discount: Bps(1_000),
            ..Policy::default()
        }),
    };
    assert!(engine.execute(request.clone()).is_err());
    assert_eq!(engine.state(), &before);
    assert_eq!(fs::read(&pending)?, b"interrupted write must be preserved");
    assert_eq!(
        fs::read(workspace.0.join("00000000000000000001.json"))?,
        previous_bytes
    );
    assert!(!workspace.0.join("00000000000000000002.json").exists());
    fs::remove_file(pending)?;
    let outcome = engine.execute(request.clone())?;
    assert!(!outcome.replayed);
    assert_eq!(outcome.revision, 2);
    let committed = engine.state().clone();
    assert!(engine.execute(request.clone())?.replayed);
    assert_eq!(engine.state(), &committed);
    drop(engine);
    let mut reopened = Engine::open(&workspace.0)?;
    assert_eq!(reopened.state(), &committed);
    assert!(reopened.execute(request)?.replayed);
    Ok(())
}

#[test]
fn owner_and_distinct_submitter_cannot_approve() -> TestResult {
    let mut engine = demo()?;
    create_quote(&mut engine)?;
    let before = engine.state().clone();
    for role in [Role::Sales, Role::Admin] {
        assert!(
            engine
                .run(
                    actor("submitter", role),
                    Command::QuoteSubmit {
                        id: "Q".into(),
                        version: 2,
                    }
                )
                .is_err()
        );
        assert_eq!(engine.state(), &before);
    }
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    for role in [Role::Approver, Role::Admin] {
        let before = engine.state().clone();
        assert!(
            engine
                .run(
                    actor("seller", role),
                    Command::QuoteDecide {
                        id: "Q".into(),
                        version: 3,
                        approve: true,
                        note: "Must fail separation of duties".into()
                    }
                )
                .is_err()
        );
        assert_eq!(engine.state(), &before);
    }
    engine.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "Independent review".into(),
        },
    )?;
    let quote = engine.state().quotes.get("Q").ok_or("missing quote")?;
    assert!(export_quote(quote)?.contains("855.00"));
    Ok(())
}

#[test]
fn frozen_document_validation_rejects_independent_corruptions() -> TestResult {
    let state = approved_fixture()?;
    let cases: &[Corruption<Quote>] = &[
        ("zero version", |q| q.version = 0),
        ("future source", |q| q.source_revision = u64::MAX),
        ("missing decision", |q| q.decisions.clear()),
        ("missing submitter", |q| q.submitted_by = None),
        ("empty approved document", |q| q.lines.clear()),
        ("invalid policy", |q| q.policy.min_margin = Bps(-1)),
        ("wrong status", |q| q.status = Status::Rejected),
        ("wrong map key", |q| q.id = "OTHER".into()),
        ("wrong arithmetic", |q| {
            if let Some(l) = q.lines.first_mut() {
                l.margin_total = Money(l.margin_total.0 + 1);
            }
        }),
        ("wrong hard flag", |q| {
            if let Some(l) = q.lines.first_mut() {
                l.hard_block = true;
            }
        }),
    ];
    for (name, corrupt) in cases {
        let mut candidate = state.clone();
        corrupt(candidate.quotes.get_mut("Q").ok_or("missing quote")?);
        assert!(candidate.validate().is_err(), "accepted {name}");
    }
    Ok(())
}

#[test]
fn frozen_lines_match_immutable_product_currency_unit_and_cost() -> TestResult {
    let state = approved_fixture()?;
    let cases: &[Corruption<Evaluation>] = &[
        ("currency", |l| l.currency = Currency::EUR),
        ("unit", |l| l.unit = "kg".into()),
        ("nonexistent cost version", |l| l.cost_version = 99),
    ];
    for (name, corrupt) in cases {
        let mut candidate = state.clone();
        let quote = candidate.quotes.get_mut("Q").ok_or("missing quote")?;
        corrupt(quote.lines.first_mut().ok_or("missing line")?);
        assert!(
            candidate.validate().is_err(),
            "accepted changed immutable {name}"
        );
    }
    Ok(())
}

#[test]
fn decision_chronology_and_audit_identity_cannot_be_rewritten() -> TestResult {
    let state = approved_fixture()?;
    let cases: &[Corruption<State>] = &[
        ("decision before source", |s| {
            if let Some(q) = s.quotes.get_mut("Q")
                && let Some(d) = q.decisions.last_mut()
            {
                d.revision = 1;
            }
        }),
        ("source at decision revision", |s| {
            if let Some(q) = s.quotes.get_mut("Q")
                && let Some(d) = q.decisions.last()
            {
                q.source_revision = d.revision;
            }
        }),
        ("duplicate decision revision", |s| {
            if let Some(q) = s.quotes.get_mut("Q")
                && let Some(d) = q.decisions.last().cloned()
            {
                q.decisions.push(d);
            }
        }),
        ("unrecorded decision actor", |s| {
            if let Some(q) = s.quotes.get_mut("Q")
                && let Some(d) = q.decisions.last_mut()
            {
                d.actor = "not-the-audited-reviewer".into();
            }
        }),
        ("non-approver audit role", |s| {
            if let Some(a) = s.audit.last_mut() {
                a.actor.role = Role::Sales;
            }
        }),
    ];
    for (name, corrupt) in cases {
        let mut candidate = state.clone();
        corrupt(&mut candidate);
        assert!(candidate.validate().is_err(), "accepted {name}");
    }
    Ok(())
}

#[test]
fn historical_decision_survives_repricing_but_new_approval_is_required() -> TestResult {
    let mut engine = demo()?;
    create_quote(&mut engine)?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    engine.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "First review".into(),
        },
    )?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 4,
        },
    )?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteReprice {
            id: "Q".into(),
            version: 5,
        },
    )?;
    let quote = engine.state().quotes.get("Q").ok_or("missing quote")?;
    assert!(
        quote
            .decisions
            .first()
            .ok_or("missing historical decision")?
            .revision
            < quote.source_revision
    );
    assert_eq!(quote.status, Status::Draft);
    assert!(export_quote(quote).is_err());
    engine.state().validate()?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 6,
        },
    )?;
    engine.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 7,
            approve: true,
            note: "Second independent review".into(),
        },
    )?;
    let quote = engine.state().quotes.get("Q").ok_or("missing quote")?;
    assert_eq!(quote.decisions.len(), 2);
    assert!(
        quote
            .decisions
            .last()
            .ok_or("missing current decision")?
            .revision
            > quote.source_revision
    );
    assert!(export_quote(quote).is_ok());
    engine.state().validate()?;
    Ok(())
}

#[test]
fn frozen_list_policy_and_arithmetic_are_validated() -> TestResult {
    let mut engine = demo()?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::ListCalculate {
            id: "L".into(),
            name: "Frozen list".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
            quantity: Quantity(1_000),
            currency: Currency::USD,
        },
    )?;
    let before = engine
        .state()
        .price_lists
        .get("L")
        .ok_or("missing list")?
        .clone();
    engine.run(
        actor("admin", Role::Admin),
        Command::PolicySet(Policy {
            hard_min_margin: Bps(2_000),
            ..Policy::default()
        }),
    )?;
    assert_eq!(engine.state().price_lists.get("L"), Some(&before));
    let mut invalid = engine.state().clone();
    let list = invalid.price_lists.get_mut("L").ok_or("missing list")?;
    list.lines
        .first_mut()
        .ok_or("missing list line")?
        .pocket_total = Money(1);
    assert!(invalid.validate().is_err());
    let mut invalid = engine.state().clone();
    invalid
        .price_lists
        .get_mut("L")
        .ok_or("missing list")?
        .policy
        .hard_min_margin = Bps(10_001);
    assert!(invalid.validate().is_err());
    Ok(())
}

fn max_markup_quote() -> Result<Engine> {
    let mut engine = demo()?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "VLV-100".into(),
            version: 2,
            effective_from: date()?,
            amount: Money(1_000_000_000_000),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::RuleAdd(Rule {
            id: "MAX-MARKUP".into(),
            version: 1,
            name: "Boundary markup fixture".into(),
            priority: 100,
            product_id: Some("VLV-100".into()),
            customer_id: None,
            segment: None,
            effective_from: date()?,
            effective_to: None,
            min_quantity: Quantity(1_000),
            action: RuleAction::Markup(Bps(10_000)),
            currency: Currency::USD,
        }),
    )?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "MAX-Q".into(),
            title: "Boundary fixture".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
        },
    )?;
    let request = PriceRequest {
        quantity: Quantity(1_000),
        discount: Bps(0),
        rebate: Bps(0),
        ..line()?
    };
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "MAX-Q".into(),
            version: 1,
            request,
        },
    )?;
    engine.state().validate()?;
    Ok(engine)
}

#[test]
fn supported_max_cost_with_markup_can_be_saved_as_quote() -> TestResult {
    let engine = max_markup_quote()?;
    let quote = engine.state().quotes.get("MAX-Q").ok_or("missing quote")?;
    assert_eq!(
        quote.lines.first().ok_or("missing line")?.invoice_unit,
        Money(2_000_000_000_000)
    );
    Ok(())
}

#[test]
fn document_aggregate_overflow_is_rejected_by_command_and_snapshot_validation() -> TestResult {
    let mut engine = max_markup_quote()?;
    let request = PriceRequest {
        quantity: Quantity(1_000_000_000),
        discount: Bps(0),
        rebate: Bps(0),
        ..line()?
    };
    let evaluation = copperline_engine::evaluate(engine.state(), &request)?;
    assert_eq!(evaluation.invoice_total, Money(2_000_000_000_000_000_000));
    evaluation.validate_snapshot()?;
    let mut invalid = engine.state().clone();
    let quote = invalid.quotes.get_mut("MAX-Q").ok_or("missing quote")?;
    quote.lines = vec![evaluation; 5];
    assert!(copperline_engine::totals(&quote.lines).is_err());
    assert!(invalid.validate().is_err());

    // Four large lines fit; the fifth must fail without changing the document or audit.
    for version in 2..6 {
        engine.run(
            actor("seller", Role::Sales),
            Command::QuoteAddLine {
                id: "MAX-Q".into(),
                version,
                request: request.clone(),
            },
        )?;
    }
    let before = engine.state().clone();
    assert!(
        engine
            .run(
                actor("seller", Role::Sales),
                Command::QuoteAddLine {
                    id: "MAX-Q".into(),
                    version: 6,
                    request
                }
            )
            .is_err()
    );
    assert_eq!(engine.state(), &before);
    Ok(())
}
