//! Independent adversarial review of atomic quote line replacement.
use copperline_engine::{Command, Engine, Request, evaluate, export_quote};
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
            "copperline-replace-review-{}-{}",
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
fn price() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date()?,
        quantity: Quantity(1_000),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
        conversion: None,
    })
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn quote(e: &Engine) -> Result<&Quote> {
    e.state()
        .quotes
        .get("Q")
        .ok_or_else(|| error("Missing quote"))
}
fn create(e: &mut Engine) -> Result<()> {
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic replacement review".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
        },
    )?;
    Ok(())
}
fn add(e: &mut Engine, request: PriceRequest) -> Result<()> {
    let version = quote(e)?.version;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version,
            request,
        },
    )?;
    Ok(())
}
fn replace(version: u64, index: usize, request: PriceRequest) -> Command {
    Command::QuoteReplaceLine {
        id: "Q".into(),
        version,
        index,
        request,
    }
}
fn reject_atomic(e: &mut Engine, who: Actor, command: Command) {
    let before = e.state().clone();
    assert!(e.run(who, command).is_err());
    assert_eq!(e.state(), &before);
}
fn activate_grid(e: &mut Engine, id: &str, discount: Bps) -> Result<PriceSource> {
    let version = e.state().grid.get(id).map_or(0, |g| g.version);
    let mut request = price()?;
    request.discount = discount;
    e.run(
        actor("pricing", Role::Analyst),
        Command::GridCalculate {
            id: id.into(),
            version,
            request,
        },
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::GridSubmit {
            id: id.into(),
            version: version + 1,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::GridDecide {
            id: id.into(),
            version: version + 2,
            approve: true,
            note: "Independent synthetic price review".into(),
        },
    )?;
    Ok(PriceSource::Grid {
        id: id.into(),
        approval_revision: e.state().revision,
    })
}
fn selected(source: PriceSource) -> Result<PriceRequest> {
    let mut request = price()?;
    request.source = Some(source);
    Ok(request)
}

#[test]
fn replacement_requires_owner_current_draft_version_and_existing_index() -> TestResult {
    let mut e = demo()?;
    create(&mut e)?;
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        replace(1, 0, price()?),
    );
    add(&mut e, price()?)?;
    for role in [Role::Sales, Role::Admin] {
        reject_atomic(&mut e, actor("outsider", role), replace(2, 0, price()?));
    }
    for role in [Role::Analyst, Role::Approver, Role::Finance] {
        reject_atomic(&mut e, actor("seller", role), replace(2, 0, price()?));
    }
    for version in [0, 1, 3, u64::MAX] {
        reject_atomic(
            &mut e,
            actor("seller", Role::Sales),
            replace(version, 0, price()?),
        );
    }
    for index in [1, usize::MAX] {
        reject_atomic(
            &mut e,
            actor("seller", Role::Sales),
            replace(2, index, price()?),
        );
    }
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        Command::QuoteReplaceLine {
            id: "MISSING".into(),
            version: 2,
            index: 0,
            request: price()?,
        },
    );
    e.run(actor("seller", Role::Admin), replace(2, 0, price()?))?;
    assert_eq!(quote(&e)?.version, 3);
    assert_eq!(quote(&e)?.lines.len(), 1);
    let event = e.state().audit.last().ok_or("Missing audit")?;
    assert_eq!(event.command, "quote_replace_line");
    assert_eq!(event.target.as_deref(), Some("quote:Q"));
    assert_eq!(event.actor, actor("seller", Role::Admin));
    Ok(())
}

#[test]
fn invalid_customer_date_product_amounts_and_sources_leave_all_state_unchanged() -> TestResult {
    let mut e = demo()?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    let mut cases = vec![];
    let mut p = price()?;
    p.customer_id = "C-NORTH".into();
    cases.push(p);
    let mut p = price()?;
    p.date = Date::parse("2026-10-14")?;
    cases.push(p);
    let mut p = price()?;
    p.product_id = "MISSING".into();
    cases.push(p);
    let mut p = price()?;
    p.quantity = Quantity(0);
    cases.push(p);
    let mut p = price()?;
    p.discount = Bps(10_001);
    cases.push(p);
    let mut p = price()?;
    p.rebate = Bps(-1);
    cases.push(p);
    let mut p = price()?;
    p.source = Some(PriceSource::Grid {
        id: "MISSING".into(),
        approval_revision: 1,
    });
    cases.push(p);
    let mut p = price()?;
    p.source = Some(PriceSource::PriceList {
        id: "MISSING".into(),
        version: 1,
    });
    cases.push(p);
    for p in cases {
        reject_atomic(&mut e, actor("seller", Role::Sales), replace(2, 0, p));
    }
    // Every evaluated line may be valid alone while the mixed document is invalid.
    add(&mut e, price()?)?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::ProductPut(Product {
            id: "EUR".into(),
            name: "Synthetic euro item".into(),
            family: "Test".into(),
            unit: "each".into(),
            currency: Currency::EUR,
            list_price: Money(10_000),
        }),
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "EUR".into(),
            version: 1,
            effective_from: date()?,
            amount: Money(6_000),
            currency: Currency::EUR,
            unit: "each".into(),
        }),
    )?;
    let mut p = price()?;
    p.product_id = "EUR".into();
    reject_atomic(&mut e, actor("seller", Role::Sales), replace(3, 0, p));
    Ok(())
}

#[test]
fn stale_selected_source_can_be_replaced_by_current_source_or_explicit_live_rules() -> TestResult {
    let mut e = demo()?;
    let old = activate_grid(&mut e, "G", Bps(1_000))?;
    create(&mut e)?;
    add(&mut e, selected(old.clone())?)?;
    let current = activate_grid(&mut e, "G", Bps(1_500))?;
    assert!(evaluate(e.state(), &selected(old.clone())?).is_err());
    let before = e.state().revision;
    e.run(
        actor("seller", Role::Sales),
        replace(2, 0, selected(current.clone())?),
    )?;
    let q = quote(&e)?;
    assert_eq!(q.version, 3);
    assert_eq!(q.source_revision, before);
    assert_eq!(q.lines[0].request.source, Some(current.clone()));
    assert_eq!(q.lines[0].invoice_unit, Money(8_500));
    assert_eq!(
        q.lines[0].source.as_ref().map(|s| s.captured_revision),
        Some(before)
    );
    activate_grid(&mut e, "G", Bps(500))?;
    assert!(evaluate(e.state(), &selected(current)?).is_err());
    e.run(actor("seller", Role::Sales), replace(3, 0, price()?))?;
    assert!(quote(&e)?.lines[0].source.is_none());
    assert!(quote(&e)?.lines[0].request.source.is_none());
    assert_eq!(quote(&e)?.lines[0].invoice_unit, Money(10_000));
    e.state().validate()?;
    Ok(())
}

#[test]
fn stale_retained_source_rolls_back_even_when_replacement_itself_is_valid() -> TestResult {
    let mut e = demo()?;
    let first = activate_grid(&mut e, "G1", Bps(1_000))?;
    let second = activate_grid(&mut e, "G2", Bps(500))?;
    create(&mut e)?;
    add(&mut e, selected(first)?)?;
    add(&mut e, selected(second)?)?;
    let current_first = activate_grid(&mut e, "G1", Bps(1_500))?;
    activate_grid(&mut e, "G2", Bps(1_000))?;
    let request = selected(current_first)?;
    assert!(evaluate(e.state(), &request).is_ok());
    assert!(evaluate(e.state(), &quote(&e)?.lines[1].request).is_err());
    reject_atomic(&mut e, actor("seller", Role::Sales), replace(3, 0, request));
    // It must not re-use the untouched line's historical evidence as fresh pricing.
    assert_eq!(quote(&e)?.lines[0].invoice_unit, Money(9_000));
    assert_eq!(quote(&e)?.lines[1].invoice_unit, Money(9_500));
    Ok(())
}

#[test]
fn replacement_refreshes_every_line_cost_policy_and_source_capture_at_one_revision() -> TestResult {
    let mut e = demo()?;
    let source = activate_grid(&mut e, "G", Bps(0))?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    add(&mut e, selected(source)?)?;
    let old = quote(&e)?.clone();
    let mut product = e
        .state()
        .products
        .get("VLV-100")
        .ok_or("Missing product")?
        .clone();
    product.list_price = Money(12_000);
    e.run(
        actor("pricing", Role::Analyst),
        Command::ProductPut(product),
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "VLV-100".into(),
            version: 2,
            effective_from: date()?,
            amount: Money(6_500),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::RuleAdd(Rule {
            id: "M5-FIXED".into(),
            version: 1,
            name: "Synthetic new rule".into(),
            priority: 100,
            product_id: Some("VLV-100".into()),
            customer_id: None,
            segment: None,
            effective_from: date()?,
            effective_to: None,
            min_quantity: Quantity(1),
            action: RuleAction::Fixed(Money(11_000)),
            currency: Currency::USD,
        }),
    )?;
    let policy = Policy {
        min_margin: Bps(4_000),
        max_discount: Bps(1_000),
        hard_min_margin: Bps(0),
    };
    e.run(
        actor("admin", Role::Admin),
        Command::PolicySet(policy.clone()),
    )?;
    let mut request = price()?;
    request.quantity = Quantity(2_000);
    request.discount = Bps(1_000);
    request.rebate = Bps(500);
    let before = e.state().clone();
    let expected = vec![
        evaluate(&before, &request)?,
        evaluate(&before, &old.lines[1].request)?,
    ];
    e.run(actor("seller", Role::Sales), replace(3, 0, request))?;
    let q = quote(&e)?;
    assert_eq!(q.lines, expected);
    assert_eq!(q.version, 4);
    assert_eq!(q.source_revision, before.revision);
    assert_eq!(q.policy, policy);
    assert_eq!(q.lines[1].request, old.lines[1].request);
    assert_eq!(q.lines[1].cost_version, 2);
    assert_eq!(q.lines[1].cost_unit, Money(6_500));
    assert_eq!(
        q.lines[1].source.as_ref().map(|s| s.captured_revision),
        Some(before.revision)
    );
    assert_ne!(q.lines[1], old.lines[1]);
    assert_eq!(q.lines[0].pocket_total, Money(18_810));
    assert_eq!(q.lines[1].invoice_unit, Money(10_000));
    e.state().validate()?;
    Ok(())
}

#[test]
fn replacement_cannot_modify_pending_or_approved_cycles_and_revision_preserves_history()
-> TestResult {
    let mut e = demo()?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        replace(3, 0, price()?),
    );
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "Independent quote review".into(),
        },
    )?;
    let approved = quote(&e)?.clone();
    let csv = export_quote(&approved)?;
    reject_atomic(
        &mut e,
        actor("seller", Role::Admin),
        replace(4, 0, price()?),
    );
    assert_eq!(export_quote(quote(&e)?)?, csv);
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 4,
        },
    )?;
    let mut changed = price()?;
    changed.discount = Bps(500);
    e.run(actor("seller", Role::Sales), replace(5, 0, changed))?;
    assert_eq!(quote(&e)?.status, Status::Draft);
    assert!(export_quote(quote(&e)?).is_err());
    assert_eq!(quote(&e)?.decisions, approved.decisions);
    assert_eq!(
        quote(&e)?.governance.as_ref().map(|g| &g.cycles),
        approved.governance.as_ref().map(|g| &g.cycles)
    );
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 6,
        },
    )?;
    for role in [Role::Approver, Role::Admin, Role::Finance] {
        reject_atomic(
            &mut e,
            actor("seller", role),
            Command::QuoteDecide {
                id: "Q".into(),
                version: 7,
                approve: true,
                note: "Owner cannot approve replacement".into(),
            },
        );
    }
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 7,
            approve: true,
            note: "Reviewed replacement".into(),
        },
    )?;
    assert_eq!(
        quote(&e)?.governance.as_ref().map(|g| g.cycles.len()),
        Some(2)
    );
    assert!(export_quote(quote(&e)?)?.contains("95.00"));
    Ok(())
}

#[test]
fn failed_disk_commit_does_not_replace_and_json_retry_is_exact_after_reopen() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    let before = e.state().clone();
    let mut changed = price()?;
    changed.quantity = Quantity(3_000);
    changed.discount = Bps(500);
    let request = Request {
        request_id: "replace-with-retry".into(),
        expected_revision: before.revision,
        actor: actor("seller", Role::Sales),
        action: replace(2, 0, changed),
    };
    let encoded = serde_json::to_string(&request)?;
    assert!(encoded.contains("quote_replace_line"));
    let request: Request = serde_json::from_str(&encoded)?;
    let pending = workspace
        .0
        .join(format!("{:020}.pending", before.revision + 1));
    fs::write(&pending, b"synthetic pending blocker")?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert!(
        !workspace
            .0
            .join(format!("{:020}.json", before.revision + 1))
            .exists()
    );
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &before);
    fs::remove_file(&pending)?;
    let accepted = e.execute(request.clone())?;
    assert!(!accepted.replayed);
    assert_eq!(quote(&e)?.lines[0].invoice_total, Money(28_500));
    let after = e.state().clone();
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    let retry = e.execute(request.clone())?;
    assert!(retry.replayed);
    assert_eq!(retry.revision, accepted.revision);
    assert_eq!(e.state(), &after);
    let mut altered = request;
    if let Command::QuoteReplaceLine { index, .. } = &mut altered.action {
        *index = 1;
    }
    assert!(e.execute(altered).is_err());
    assert_eq!(e.state(), &after);
    Ok(())
}

#[test]
fn refreshing_retained_lines_cannot_overflow_the_document_atomically() -> TestResult {
    let mut e = demo()?;
    create(&mut e)?;
    let mut large = price()?;
    large.quantity = Quantity(1_000_000_000);
    for _ in 0..12 {
        add(&mut e, large.clone())?;
    }
    let mut product = e
        .state()
        .products
        .get("VLV-100")
        .ok_or("Missing product")?
        .clone();
    product.list_price = Money(1_000_000_000_000);
    e.run(
        actor("pricing", Role::Analyst),
        Command::ProductPut(product),
    )?;
    // The new line is tiny and valid; eleven retained lines overflow after repricing.
    assert!(evaluate(e.state(), &price()?).is_ok());
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        replace(13, 0, price()?),
    );
    assert_eq!(quote(&e)?.lines.len(), 12);
    Ok(())
}

#[test]
fn replacing_legacy_contents_retains_every_historical_contributor() -> TestResult {
    // The first four chained hashes were captured from the genuine published M2
    // binary. Default omission must still reproduce those exact historical bytes.
    let hashes = [
        "20597816b453a303526ee556442cb375e3f37710b520fe97a03d1dd5e229950c",
        "eafb9912ca4936ba91e287b92e77a7af681ed64c81260f886e3e5cacb693c2b2",
        "b7c0092276bdafebd82578a399500afff03ba1d7860d9a8d37ba121e4bf86d4d",
        "931ba1f1599e2ad63f801fb52e90cf5fc2aaa60989fad3a5c97680a81bdabee7",
    ];
    let mut e = Engine::memory();
    let mut states = vec![e.state().clone()];
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    states.push(e.state().clone());
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Legacy synthetic quote".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
        },
    )?;
    states.push(e.state().clone());
    add(&mut e, price()?)?;
    states.push(e.state().clone());
    let old_edit = Request {
        request_id: "local-3-quote_add_line".into(),
        expected_revision: 2,
        actor: actor("editor", Role::Admin),
        action: Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: price()?,
        },
    };
    let workspace = Workspace::new()?;
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
        }
        if let Some(r) = state.receipts.iter_mut().find(|r| r.revision == 3) {
            r.fingerprint = copperline_storage::fingerprint(&serde_json::to_vec(&old_edit)?);
        }
        state.validate()?;
        let mut bytes = previous.as_bytes().to_vec();
        bytes.extend(serde_json::to_vec(&state)?);
        let hash = copperline_storage::fingerprint(&bytes);
        assert_eq!(hash, hashes[i]);
        fs::write(
            workspace.0.join(format!("{i:020}.json")),
            serde_json::to_vec(
                &serde_json::json!({"previous_hash":previous,"hash":hash,"state":state}),
            )?,
        )?;
        previous = hash;
    }
    let mut e = Engine::open(&workspace.0)?;
    reject_atomic(
        &mut e,
        actor("editor", Role::Admin),
        replace(2, 0, price()?),
    );
    let mut request = price()?;
    request.discount = Bps(500);
    e.run(actor("seller", Role::Sales), replace(2, 0, request))?;
    let g = quote(&e)?.governance.as_ref().ok_or("Missing governance")?;
    assert!(g.contributors.contains("seller"));
    assert!(g.contributors.contains("editor"));
    assert_eq!(e.state().quote_contributors(quote(&e)?), g.contributors);
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 3,
        },
    )?;
    reject_atomic(
        &mut e,
        actor("editor", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 4,
            approve: true,
            note: "Prior author is still excluded".into(),
        },
    );
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 4,
            approve: true,
            note: "Independent replacement approval".into(),
        },
    )?;
    let state = e.state().clone();
    drop(e);
    let e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &state);
    Ok(())
}
