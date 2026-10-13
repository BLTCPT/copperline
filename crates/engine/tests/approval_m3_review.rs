//! Adversarial public-command tests for frozen sequential quote approval.
use copperline_engine::{Command, Engine, Request, export_quote};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
type Corruption = (&'static str, fn(&mut Quote));
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-approval-review-{}-{}",
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
fn line() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date()?,
        quantity: Quantity(1000),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
    })
}
fn demo() -> Result<Engine> {
    let mut engine = Engine::memory();
    engine.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    Ok(engine)
}
fn quote<'a>(engine: &'a Engine, id: &str) -> Result<&'a Quote> {
    engine
        .state()
        .quotes
        .get(id)
        .ok_or_else(|| error("Missing quote"))
}
fn cycle<'a>(engine: &'a Engine, id: &str) -> Result<&'a QuoteApprovalCycle> {
    quote(engine, id)?
        .governance
        .as_ref()
        .and_then(|g| g.cycles.last())
        .ok_or_else(|| error("Missing cycle"))
}
fn create(engine: &mut Engine, id: &str, request: PriceRequest) -> Result<()> {
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: id.into(),
            title: "Synthetic approval review".into(),
            customer_id: request.customer_id.clone(),
            date: request.date.clone(),
        },
    )?;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: id.into(),
            version: 1,
            request,
        },
    )?;
    Ok(())
}
fn submit(engine: &mut Engine, id: &str) -> Result<()> {
    let version = quote(engine, id)?.version;
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: id.into(),
            version,
        },
    )?;
    Ok(())
}
fn decision(id: &str, version: u64, approve: bool) -> Command {
    Command::QuoteDecide {
        id: id.into(),
        version,
        approve,
        note: "Independent synthetic review".into(),
    }
}
fn decide(engine: &mut Engine, id: &str, name: &str, role: Role, approve: bool) -> Result<()> {
    let version = quote(engine, id)?.version;
    engine.run(actor(name, role), decision(id, version, approve))?;
    Ok(())
}
fn thresholds() -> ApprovalThresholds {
    ApprovalThresholds {
        max_discount: Bps(10_000),
        min_margin: Bps(0),
        usd_value: Money(1_000_000_000_000),
        eur_value: Money(1_000_000_000_000),
        gbp_value: Money(1_000_000_000_000),
    }
}
fn set_policy(engine: &mut Engine, thresholds: ApprovalThresholds) -> Result<()> {
    let expected_version = engine
        .state()
        .quote_approval_policies
        .last()
        .map_or(0, |p| p.version);
    engine.run(
        actor("admin", Role::Admin),
        Command::QuoteApprovalPolicySet {
            expected_version,
            thresholds,
        },
    )?;
    Ok(())
}
fn force_finance(engine: &mut Engine) -> Result<()> {
    let mut t = thresholds();
    t.usd_value = Money(1);
    set_policy(engine, t)
}
fn rejected_without_change(engine: &mut Engine, who: Actor, command: Command) {
    let before = engine.state().clone();
    assert!(engine.run(who, command).is_err());
    assert_eq!(engine.state(), &before);
}

#[test]
fn ownership_has_no_admin_edit_submit_or_revise_bypass() -> TestResult {
    let mut engine = demo()?;
    create(&mut engine, "Q", line()?)?;
    for role in [Role::Sales, Role::Admin] {
        for command in [
            Command::QuoteAddLine {
                id: "Q".into(),
                version: 2,
                request: line()?,
            },
            Command::QuoteRemoveLine {
                id: "Q".into(),
                version: 2,
                index: 0,
            },
            Command::QuoteReprice {
                id: "Q".into(),
                version: 2,
            },
            Command::QuoteSubmit {
                id: "Q".into(),
                version: 2,
            },
        ] {
            rejected_without_change(&mut engine, actor("outsider", role), command);
        }
    }
    submit(&mut engine, "Q")?;
    for role in [Role::Approver, Role::Finance, Role::Admin] {
        rejected_without_change(&mut engine, actor("seller", role), decision("Q", 3, true));
    }
    decide(&mut engine, "Q", "reviewer", Role::Approver, true)?;
    rejected_without_change(
        &mut engine,
        actor("outsider", Role::Admin),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 4,
        },
    );
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 4,
        },
    )?;
    assert_eq!(quote(&engine, "Q")?.status, Status::Draft);
    assert!(export_quote(quote(&engine, "Q")?).is_err());
    Ok(())
}

#[test]
fn sequential_route_is_frozen_role_separated_and_not_exportable_midway() -> TestResult {
    let mut engine = demo()?;
    force_finance(&mut engine)?;
    create(&mut engine, "Q", line()?)?;
    submit(&mut engine, "Q")?;
    let frozen = cycle(&engine, "Q")?.clone();
    assert_eq!(
        frozen.stages.iter().map(|s| s.role).collect::<Vec<_>>(),
        vec![Role::Approver, Role::Finance]
    );
    set_policy(&mut engine, thresholds())?;
    assert_eq!(cycle(&engine, "Q")?, &frozen);
    for role in [Role::Finance, Role::Admin] {
        rejected_without_change(
            &mut engine,
            actor("wrong-first", role),
            decision("Q", 3, true),
        );
    }
    decide(&mut engine, "Q", "commercial", Role::Approver, true)?;
    assert_eq!(quote(&engine, "Q")?.status, Status::Pending);
    assert!(quote(&engine, "Q")?.decisions.is_empty());
    assert!(export_quote(quote(&engine, "Q")?).is_err());
    for (name, role) in [
        ("commercial", Role::Finance),
        ("admin", Role::Admin),
        ("other-commercial", Role::Approver),
    ] {
        rejected_without_change(&mut engine, actor(name, role), decision("Q", 4, true));
    }
    rejected_without_change(
        &mut engine,
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 4,
        },
    );
    decide(&mut engine, "Q", "finance", Role::Finance, true)?;
    assert_eq!(quote(&engine, "Q")?.status, Status::Approved);
    assert_eq!(quote(&engine, "Q")?.decisions.len(), 1);
    assert!(export_quote(quote(&engine, "Q")?)?.contains("100.00"));
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 5,
        },
    )?;
    submit(&mut engine, "Q")?;
    assert_eq!(
        cycle(&engine, "Q")?.policy.as_ref().map(|p| p.version),
        Some(2)
    );
    assert_eq!(cycle(&engine, "Q")?.stages.len(), 1);
    assert_eq!(
        quote(&engine, "Q")?
            .governance
            .as_ref()
            .map(|g| g.cycles.len()),
        Some(2)
    );
    Ok(())
}

#[test]
fn rejection_finishes_a_cycle_and_revision_keeps_its_history() -> TestResult {
    let mut engine = demo()?;
    force_finance(&mut engine)?;
    create(&mut engine, "Q", line()?)?;
    submit(&mut engine, "Q")?;
    decide(&mut engine, "Q", "commercial", Role::Approver, false)?;
    let rejected = cycle(&engine, "Q")?.clone();
    assert_eq!(rejected.status(), Status::Rejected);
    assert_eq!(rejected.reviews.len(), 1);
    rejected_without_change(
        &mut engine,
        actor("finance", Role::Finance),
        decision("Q", 4, true),
    );
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
    submit(&mut engine, "Q")?;
    let g = quote(&engine, "Q")?
        .governance
        .as_ref()
        .ok_or("Missing governance")?;
    assert_eq!(g.cycles.first(), Some(&rejected));
    assert_eq!(g.cycles.len(), 2);
    assert!(cycle(&engine, "Q")?.reviews.is_empty());
    assert!(export_quote(quote(&engine, "Q")?).is_err());
    decide(&mut engine, "Q", "commercial", Role::Approver, true)?;
    decide(&mut engine, "Q", "finance", Role::Finance, true)?;
    assert_eq!(quote(&engine, "Q")?.decisions.len(), 2);
    engine.state().validate()?;
    Ok(())
}

#[test]
fn stale_versions_and_exact_review_retries_survive_reopen() -> TestResult {
    let workspace = Workspace::new()?;
    let mut engine = Engine::open(&workspace.0)?;
    engine.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    force_finance(&mut engine)?;
    create(&mut engine, "Q", line()?)?;
    submit(&mut engine, "Q")?;
    let request = Request {
        request_id: "commercial-review".into(),
        expected_revision: engine.state().revision,
        actor: actor("commercial", Role::Approver),
        action: decision("Q", 3, true),
    };
    let first = engine.execute(request.clone())?;
    rejected_without_change(
        &mut engine,
        actor("finance", Role::Finance),
        decision("Q", 3, true),
    );
    rejected_without_change(
        &mut engine,
        actor("admin", Role::Admin),
        Command::QuoteApprovalPolicySet {
            expected_version: 0,
            thresholds: thresholds(),
        },
    );
    decide(&mut engine, "Q", "finance", Role::Finance, true)?;
    let state = engine.state().clone();
    drop(engine);
    let mut engine = Engine::open(&workspace.0)?;
    let replay = engine.execute(request.clone())?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, first.revision);
    assert_eq!(engine.state(), &state);
    let mut altered = request;
    altered.actor = actor("different-person", Role::Approver);
    assert!(engine.execute(altered).is_err());
    assert_eq!(engine.state(), &state);
    Ok(())
}

fn product_quote(
    list: i64,
    cost: i64,
    currency: Currency,
    discount: i32,
    rebate: i32,
    t: ApprovalThresholds,
) -> Result<Engine> {
    let mut engine = demo()?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::ProductPut(Product {
            id: "BOUNDARY".into(),
            name: "Synthetic threshold product".into(),
            family: "Test".into(),
            unit: "each".into(),
            currency,
            list_price: Money(list),
        }),
    )?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "BOUNDARY".into(),
            version: 1,
            effective_from: date()?,
            amount: Money(cost),
            currency,
            unit: "each".into(),
        }),
    )?;
    set_policy(&mut engine, t)?;
    let mut request = line()?;
    request.product_id = "BOUNDARY".into();
    request.discount = Bps(discount);
    request.rebate = Bps(rebate);
    create(&mut engine, "Q", request)?;
    submit(&mut engine, "Q")?;
    Ok(engine)
}

#[test]
fn routing_compares_exact_effective_discount_and_weighted_margin() -> TestResult {
    let mut t = thresholds();
    t.max_discount = Bps(1500);
    for (discount, stages) in [(1499, 1), (1500, 1), (1501, 2)] {
        let engine = product_quote(100_000, 40_000, Currency::USD, discount, 0, t.clone())?;
        assert_eq!(
            cycle(&engine, "Q")?.stages.len(),
            stages,
            "discount {discount}"
        );
    }
    // A rule's concession is part of effective invoice discount even with zero negotiated discount.
    let mut engine = demo()?;
    set_policy(&mut engine, t)?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::RuleAdd(Rule {
            id: "TEST-FIXED".into(),
            version: 1,
            name: "Synthetic effective discount".into(),
            priority: 100,
            product_id: Some("VLV-100".into()),
            customer_id: None,
            segment: None,
            effective_from: date()?,
            effective_to: None,
            min_quantity: Quantity(1),
            action: RuleAction::Fixed(Money(8_499)),
            currency: Currency::USD,
        }),
    )?;
    create(&mut engine, "Q", line()?)?;
    submit(&mut engine, "Q")?;
    assert_eq!(cycle(&engine, "Q")?.stages.len(), 2);
    assert!(
        cycle(&engine, "Q")?.stages[1]
            .reason
            .contains("effective invoice discount")
    );
    let mut t = thresholds();
    t.min_margin = Bps(2500);
    for (cost, stages) in [(67_499, 1), (67_500, 1), (67_501, 2)] {
        let engine = product_quote(90_000, cost, Currency::USD, 0, 0, t.clone())?;
        assert_eq!(cycle(&engine, "Q")?.stages.len(), stages, "cost {cost}");
        // Both sides of the true boundary round to the same displayed margin.
        assert_eq!(quote(&engine, "Q")?.lines[0].margin_bps, Some(2500));
    }
    Ok(())
}

#[test]
fn routing_value_threshold_uses_pocket_and_correct_currency() -> TestResult {
    let mut t = thresholds();
    t.usd_value = Money(10_000);
    t.eur_value = Money(20_000);
    t.gbp_value = Money(30_000);
    for (currency, boundary) in [
        (Currency::USD, 10_000),
        (Currency::EUR, 20_000),
        (Currency::GBP, 30_000),
    ] {
        for (value, stages) in [(boundary - 1, 1), (boundary, 2), (boundary + 1, 2)] {
            let engine = product_quote(value, 1, currency, 0, 0, t.clone())?;
            assert_eq!(
                cycle(&engine, "Q")?.stages.len(),
                stages,
                "{currency:?} value {value}"
            );
        }
    }
    let engine = product_quote(10_000, 1, Currency::USD, 0, 1, t)?;
    assert_eq!(quote(&engine, "Q")?.lines[0].pocket_total, Money(9_999));
    assert_eq!(cycle(&engine, "Q")?.stages.len(), 1);
    Ok(())
}

#[test]
fn margin_route_weights_line_revenue_instead_of_averaging_percentages() -> TestResult {
    let mut engine = demo()?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::ProductPut(Product {
            id: "LARGE".into(),
            name: "Synthetic high-value line".into(),
            family: "Test".into(),
            unit: "each".into(),
            currency: Currency::USD,
            list_price: Money(90_000),
        }),
    )?;
    engine.run(
        actor("analyst", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "LARGE".into(),
            version: 1,
            effective_from: date()?,
            amount: Money(80_000),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    let mut t = thresholds();
    t.min_margin = Bps(2500);
    set_policy(&mut engine, t)?;
    create(&mut engine, "Q", line()?)?;
    let mut request = line()?;
    request.product_id = "LARGE".into();
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 2,
            request,
        },
    )?;
    submit(&mut engine, "Q")?;
    // Line margins are 40% and 11.11%; their mean exceeds 25%, while the true
    // total margin is 14% because most revenue comes from the low-margin line.
    assert_eq!(cycle(&engine, "Q")?.stages.len(), 2);
    assert!(
        cycle(&engine, "Q")?.stages[1]
            .reason
            .contains("weighted margin")
    );
    Ok(())
}

#[test]
fn approval_policy_updates_require_admin_and_valid_bounded_thresholds() -> TestResult {
    let mut engine = demo()?;
    for role in [Role::Analyst, Role::Sales, Role::Approver, Role::Finance] {
        rejected_without_change(
            &mut engine,
            actor("outsider", role),
            Command::QuoteApprovalPolicySet {
                expected_version: 0,
                thresholds: thresholds(),
            },
        );
    }
    let mut invalid = vec![];
    let mut t = thresholds();
    t.usd_value = Money(0);
    invalid.push(t);
    let mut t = thresholds();
    t.eur_value = Money(-1);
    invalid.push(t);
    let mut t = thresholds();
    t.gbp_value = Money(1_000_000_000_001);
    invalid.push(t);
    let mut t = thresholds();
    t.max_discount = Bps(10_001);
    invalid.push(t);
    let mut t = thresholds();
    t.min_margin = Bps(-1);
    invalid.push(t);
    for t in invalid {
        rejected_without_change(
            &mut engine,
            actor("admin", Role::Admin),
            Command::QuoteApprovalPolicySet {
                expected_version: 0,
                thresholds: t,
            },
        );
    }
    set_policy(&mut engine, thresholds())?;
    assert_eq!(engine.state().quote_approval_policies.len(), 1);
    Ok(())
}

#[test]
fn approval_provenance_cannot_be_erased_or_redirected() -> TestResult {
    let mut engine = demo()?;
    force_finance(&mut engine)?;
    create(&mut engine, "Q", line()?)?;
    submit(&mut engine, "Q")?;
    decide(&mut engine, "Q", "commercial", Role::Approver, true)?;
    let state = engine.state().clone();
    let cases: &[Corruption] = &[
        ("governance deletion", |q| q.governance = None),
        ("cycle deletion", |q| {
            if let Some(g) = &mut q.governance {
                g.cycles.clear();
            }
        }),
        ("review deletion", |q| {
            if let Some(g) = &mut q.governance
                && let Some(c) = g.cycles.last_mut()
            {
                c.reviews.clear();
                q.version -= 1;
            }
        }),
        ("contributor deletion", |q| {
            if let Some(g) = &mut q.governance {
                g.contributors.clear();
            }
        }),
        ("finance route removal", |q| {
            if let Some(g) = &mut q.governance
                && let Some(c) = g.cycles.last_mut()
            {
                c.stages.pop();
            }
        }),
        ("pending cycle retained in draft", |q| {
            q.status = Status::Draft;
            q.submitted_by = None;
        }),
        ("frozen policy removal", |q| {
            if let Some(g) = &mut q.governance
                && let Some(c) = g.cycles.last_mut()
            {
                c.policy = None;
            }
        }),
        ("owner substitution", |q| q.owner = "commercial".into()),
    ];
    for (name, mutate) in cases {
        let mut bad = state.clone();
        mutate(bad.quotes.get_mut("Q").ok_or("Missing quote")?);
        assert!(bad.validate().is_err(), "accepted {name}");
    }
    let mut bad = state.clone();
    let review_revision = cycle(&engine, "Q")?.reviews[0].revision;
    bad.audit
        .iter_mut()
        .find(|a| a.revision == review_revision)
        .ok_or("Missing review audit")?
        .target = None;
    assert!(bad.validate().is_err());
    let mut bad = state;
    bad.quote_approval_policies.clear();
    assert!(bad.validate().is_err());
    Ok(())
}

#[test]
fn cross_quote_review_cannot_supply_another_quotes_approval() -> TestResult {
    let mut engine = demo()?;
    for id in ["Q1", "Q2"] {
        create(&mut engine, id, line()?)?;
        submit(&mut engine, id)?;
    }
    decide(&mut engine, "Q1", "reviewer", Role::Approver, true)?;
    let q1 = quote(&engine, "Q1")?.clone();
    let mut bad = engine.state().clone();
    let q2 = bad.quotes.get_mut("Q2").ok_or("Missing Q2")?;
    q2.status = Status::Approved;
    q2.version += 1;
    q2.decisions = q1.decisions.clone();
    q2.governance
        .as_mut()
        .and_then(|g| g.cycles.last_mut())
        .ok_or("Missing Q2 cycle")?
        .reviews = q1
        .governance
        .as_ref()
        .and_then(|g| g.cycles.last())
        .ok_or("Missing Q1 cycle")?
        .reviews
        .clone();
    assert!(bad.validate().is_err());
    // Even deleting the route does not turn the new quote into a legacy one-step document.
    q2_cycle_clear(&mut bad)?;
    assert!(bad.validate().is_err());
    Ok(())
}
fn q2_cycle_clear(state: &mut State) -> Result<()> {
    let g = state
        .quotes
        .get_mut("Q2")
        .and_then(|q| q.governance.as_mut())
        .ok_or_else(|| error("Missing governance"))?;
    g.cycles.clear();
    Ok(())
}

// Captured from the published M2 engine, before M3 was compiled, for the exact
// command sequence below. Matching each chained hash certifies actual old bytes.
const M2_HASHES: [&str; 5] = [
    "20597816b453a303526ee556442cb375e3f37710b520fe97a03d1dd5e229950c",
    "eafb9912ca4936ba91e287b92e77a7af681ed64c81260f886e3e5cacb693c2b2",
    "b7c0092276bdafebd82578a399500afff03ba1d7860d9a8d37ba121e4bf86d4d",
    "931ba1f1599e2ad63f801fb52e90cf5fc2aaa60989fad3a5c97680a81bdabee7",
    "ed058fd7908a9b39bb4ebb318c7e9da22fc2b7918e0c229990cce897aa2d95c8",
];
fn legacy_edit() -> Result<Request> {
    Ok(Request {
        request_id: "local-3-quote_add_line".into(),
        expected_revision: 2,
        actor: actor("editor", Role::Admin),
        action: Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: line()?,
        },
    })
}
fn write_legacy_pending(workspace: &Workspace) -> TestResult {
    let mut engine = Engine::memory();
    let mut states = vec![engine.state().clone()];
    for command in [
        Command::SeedDemo,
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Legacy synthetic quote".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
        },
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: line()?,
        },
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    ] {
        let who = if matches!(command, Command::SeedDemo) {
            actor("admin", Role::Admin)
        } else {
            actor("seller", Role::Sales)
        };
        engine.run(who, command)?;
        states.push(engine.state().clone());
    }
    let mut previous = String::new();
    for (i, mut state) in states.into_iter().enumerate() {
        if let Some(q) = state.quotes.get_mut("Q") {
            q.governance = None;
        }
        for a in &mut state.audit {
            a.target = None;
            if a.command == "quote_add_line" {
                a.actor = actor("editor", Role::Admin);
            }
            if a.command == "quote_submit" {
                a.detail = "Submitted Q for approval".into();
            }
        }
        if let Some(r) = state.receipts.iter_mut().find(|r| r.revision == 3) {
            r.fingerprint = copperline_storage::fingerprint(&serde_json::to_vec(&legacy_edit()?)?);
        }
        state.validate()?;
        let mut bytes = previous.as_bytes().to_vec();
        bytes.extend(serde_json::to_vec(&state)?);
        let hash = copperline_storage::fingerprint(&bytes);
        assert_eq!(hash, M2_HASHES[i], "M2 revision {i} serialization changed");
        fs::write(
            workspace.0.join(format!("{i:020}.json")),
            serde_json::to_vec(
                &serde_json::json!({"previous_hash":previous,"hash":hash,"state":state}),
            )?,
        )?;
        previous = hash;
    }
    Ok(())
}

#[test]
fn genuine_m2_snapshot_reopens_replays_and_retains_legacy_contributors() -> TestResult {
    let workspace = Workspace::new()?;
    write_legacy_pending(&workspace)?;
    let mut engine = Engine::open(&workspace.0)?;
    assert_eq!(engine.state().revision, 4);
    let original = engine.state().clone();
    let replay = engine.execute(legacy_edit()?)?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, 3);
    assert_eq!(engine.state(), &original);
    for role in [Role::Approver, Role::Admin] {
        rejected_without_change(&mut engine, actor("editor", role), decision("Q", 3, true));
    }
    force_finance(&mut engine)?;
    // An already pending legacy quote keeps its original one-step route.
    decide(&mut engine, "Q", "independent", Role::Approver, true)?;
    assert_eq!(quote(&engine, "Q")?.status, Status::Approved);
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 4,
        },
    )?;
    let g = quote(&engine, "Q")?
        .governance
        .as_ref()
        .ok_or("Missing governance")?;
    assert!(g.contributors.contains("editor"));
    assert!(g.contributors.contains("seller"));
    // A former editor is already a contributor; ownership still cannot be moved to them.
    let mut bad = engine.state().clone();
    bad.quotes.get_mut("Q").ok_or("Missing quote")?.owner = "editor".into();
    assert!(bad.validate().is_err());
    engine.run(
        actor("seller", Role::Sales),
        Command::QuoteReprice {
            id: "Q".into(),
            version: 5,
        },
    )?;
    submit(&mut engine, "Q")?;
    assert_eq!(cycle(&engine, "Q")?.stages.len(), 2);
    rejected_without_change(
        &mut engine,
        actor("editor", Role::Approver),
        decision("Q", 7, true),
    );
    decide(&mut engine, "Q", "commercial", Role::Approver, true)?;
    rejected_without_change(
        &mut engine,
        actor("editor", Role::Finance),
        decision("Q", 8, true),
    );
    decide(&mut engine, "Q", "finance", Role::Finance, true)?;
    let state = engine.state().clone();
    drop(engine);
    let engine = Engine::open(&workspace.0)?;
    assert_eq!(engine.state(), &state);
    Ok(())
}

#[test]
fn externally_claimed_local_request_id_does_not_block_run() -> TestResult {
    let mut engine = Engine::memory();
    engine.execute(Request {
        request_id: "local-2-product_put".into(),
        expected_revision: 0,
        actor: actor("admin", Role::Admin),
        action: Command::SeedDemo,
    })?;
    let mut product = engine
        .state()
        .products
        .get("VLV-100")
        .ok_or("Missing product")?
        .clone();
    product.name = "Synthetic renamed valve".into();
    engine.run(
        actor("analyst", Role::Analyst),
        Command::ProductPut(product),
    )?;
    assert_eq!(engine.state().revision, 2);
    assert_eq!(
        engine
            .state()
            .receipts
            .last()
            .map(|r| r.request_id.as_str()),
        Some("local-2-product_put-1")
    );
    engine.state().validate()?;
    Ok(())
}

#[test]
fn rejected_review_cannot_be_reinterpreted_as_an_unfinished_approval() -> TestResult {
    let mut engine = demo()?;
    force_finance(&mut engine)?;
    create(&mut engine, "Q", line()?)?;
    submit(&mut engine, "Q")?;
    decide(&mut engine, "Q", "commercial", Role::Approver, false)?;
    let mut bad = engine.state().clone();
    let q = bad.quotes.get_mut("Q").ok_or("Missing quote")?;
    q.status = Status::Pending;
    q.decisions.clear();
    let review = q
        .governance
        .as_mut()
        .and_then(|g| g.cycles.last_mut())
        .and_then(|c| c.reviews.last_mut())
        .ok_or("Missing review")?;
    review.approved = true;
    // The audit still records Rejected at this review revision. Outcome binding
    // must prevent deleting the terminal decision and reopening the same route.
    assert!(bad.validate().is_err());
    Ok(())
}
