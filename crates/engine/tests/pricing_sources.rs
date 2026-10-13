//! Regression coverage for explicit approved prices, frozen evidence and safe repricing.
use copperline_engine::{Command, Engine, export_quote};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
use copperline_pricing::evaluate;
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
fn request() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: Date::parse("2026-10-13")?,
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
    e.run(
        actor("pricing", Role::Analyst),
        Command::RuleAdd(Rule {
            id: "SOURCE-FIXTURE".into(),
            version: 1,
            name: "Synthetic source price fixture".into(),
            priority: 100,
            product_id: Some("VLV-100".into()),
            customer_id: Some("C-ATLAS".into()),
            segment: None,
            effective_from: Date::parse("2026-01-01")?,
            effective_to: None,
            min_quantity: Quantity(1_000),
            action: RuleAction::Discount(Bps(1_000)),
            currency: Currency::USD,
        }),
    )?;
    Ok(e)
}
fn calculate_grid(e: &mut Engine, discount: Bps, rebate: Bps) -> Result<()> {
    let version = e.state().grid.get("G").map(|g| g.version).unwrap_or(0);
    let mut price = request()?;
    price.discount = discount;
    price.rebate = rebate;
    e.run(
        actor("pricing", Role::Analyst),
        Command::GridCalculate {
            id: "G".into(),
            version,
            request: price,
        },
    )?;
    Ok(())
}
fn approve_grid(e: &mut Engine) -> Result<PriceSource> {
    let version = e
        .state()
        .grid
        .get("G")
        .ok_or_else(|| error("Missing grid"))?
        .version;
    e.run(
        actor("pricing", Role::Analyst),
        Command::GridSubmit {
            id: "G".into(),
            version,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::GridDecide {
            id: "G".into(),
            version: version + 1,
            approve: true,
            note: "Independent approval".into(),
        },
    )?;
    Ok(PriceSource::Grid {
        id: "G".into(),
        approval_revision: e.state().revision,
    })
}
fn calculate_list(e: &mut Engine) -> Result<()> {
    e.run(
        actor("pricing", Role::Analyst),
        Command::ListCalculate {
            id: "L".into(),
            name: "Synthetic October list".into(),
            customer_id: "C-ATLAS".into(),
            date: request()?.date,
            quantity: Quantity(2_000),
            currency: Currency::USD,
        },
    )?;
    Ok(())
}
fn publish_list(e: &mut Engine) -> Result<PriceSource> {
    e.run(
        actor("pricing", Role::Analyst),
        Command::ListSubmit {
            id: "L".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::ListDecide {
            id: "L".into(),
            version: 2,
            approve: true,
            note: "Independent approval".into(),
        },
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::ListPublish {
            id: "L".into(),
            version: 3,
        },
    )?;
    Ok(PriceSource::PriceList {
        id: "L".into(),
        version: 4,
    })
}
fn add_quote(e: &mut Engine, source: PriceSource) -> Result<()> {
    let mut price = request()?;
    price.source = Some(source);
    e.run(
        actor("sales", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic approved-source quote".into(),
            customer_id: price.customer_id.clone(),
            date: price.date.clone(),
        },
    )?;
    e.run(
        actor("sales", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: price,
        },
    )?;
    Ok(())
}

#[test]
fn grid_is_explicit_and_current_candidate_never_overrides_active_price() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(1_000), Bps(500))?;
    let source = approve_grid(&mut e)?;
    calculate_grid(&mut e, Bps(2_000), Bps(0))?;
    let live = evaluate(e.state(), &request()?)?;
    assert_eq!(live.invoice_unit, Money(9_000));
    assert!(live.source.is_none());
    let mut price = request()?;
    price.source = Some(source);
    price.discount = Bps(1_000);
    price.rebate = Bps(200);
    let sourced = evaluate(e.state(), &price)?;
    // Live rule 100 -> 90. Approved grid's own 10% -> 81. New 10% -> 72.90.
    assert_eq!(sourced.rule_unit, Money(8_100));
    assert_eq!(sourced.invoice_unit, Money(7_290));
    assert_eq!(sourced.rebate_total, Money(146));
    assert_eq!(sourced.pocket_total, Money(7_144));
    assert!(sourced.rule.is_none());
    assert_eq!(
        sourced.source.as_ref().and_then(|s| s.document_version),
        Some(3)
    );
    sourced.validate_snapshot()?;
    Ok(())
}

#[test]
fn source_date_customer_product_quantity_and_units_are_enforced() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let mut price = request()?;
    price.source = Some(approve_grid(&mut e)?);
    let original = price.clone();
    price.date = Date::parse("2026-10-14")?;
    assert!(evaluate(e.state(), &price).is_err());
    price = original.clone();
    price.quantity = Quantity(999);
    assert!(evaluate(e.state(), &price).is_err());
    price = original.clone();
    price.customer_id = "C-NORTH".into();
    assert!(evaluate(e.state(), &price).is_err());
    price = original.clone();
    price.product_id = "FLT-020".into();
    assert!(evaluate(e.state(), &price).is_err());
    price = original;
    price.quantity = Quantity(2_000);
    assert_eq!(evaluate(e.state(), &price)?.invoice_total, Money(18_000));
    let mut state = e.state().clone();
    state
        .grid
        .get_mut("G")
        .and_then(|g| g.last_approved.as_mut())
        .ok_or("Missing grid")?
        .evaluation
        .unit = "kg".into();
    assert!(evaluate(&state, &price).is_err());
    Ok(())
}

#[test]
fn unpublished_list_and_unapproved_grid_never_fall_back_to_live_rules() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let mut price = request()?;
    price.source = Some(PriceSource::Grid {
        id: "G".into(),
        approval_revision: 3,
    });
    assert!(evaluate(e.state(), &price).is_err());
    calculate_list(&mut e)?;
    price.quantity = Quantity(2_000);
    price.source = Some(PriceSource::PriceList {
        id: "L".into(),
        version: 1,
    });
    assert!(evaluate(e.state(), &price).is_err());
    e.run(
        actor("pricing", Role::Analyst),
        Command::ListSubmit {
            id: "L".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::ListDecide {
            id: "L".into(),
            version: 2,
            approve: true,
            note: "Independent approval".into(),
        },
    )?;
    price.source = Some(PriceSource::PriceList {
        id: "L".into(),
        version: 3,
    });
    assert!(evaluate(e.state(), &price).is_err());
    Ok(())
}

#[test]
fn published_list_pins_exact_unique_item_and_publication_revision() -> TestResult {
    let mut e = demo()?;
    calculate_list(&mut e)?;
    let mut price = request()?;
    price.source = Some(publish_list(&mut e)?);
    price.quantity = Quantity(2_000);
    price.discount = Bps(1_000);
    let evaluation = evaluate(e.state(), &price)?;
    assert_eq!(evaluation.invoice_unit, Money(8_100));
    let evidence = evaluation.source.as_ref().ok_or("Missing evidence")?;
    assert_eq!(evidence.publication_revision, Some(e.state().revision));
    assert_eq!(evidence.document_version, Some(4));
    let mut state = e.state().clone();
    let list = state.price_lists.get_mut("L").ok_or("Missing list")?;
    let duplicate = list
        .lines
        .iter()
        .find(|l| l.request.product_id == price.product_id)
        .cloned()
        .ok_or("Missing item")?;
    list.lines.push(duplicate);
    assert!(evaluate(&state, &price).is_err());
    price.source = Some(PriceSource::PriceList {
        id: "L".into(),
        version: 3,
    });
    assert!(evaluate(e.state(), &price).is_err());
    Ok(())
}

#[test]
fn published_list_flows_through_quote_approval_and_commercial_export() -> TestResult {
    let mut e = demo()?;
    calculate_list(&mut e)?;
    let mut price = request()?;
    price.source = Some(publish_list(&mut e)?);
    price.quantity = Quantity(2_000);
    price.discount = Bps(500);
    e.run(
        actor("sales", Role::Sales),
        Command::QuoteCreate {
            id: "QL".into(),
            title: "Published price list quote".into(),
            customer_id: price.customer_id.clone(),
            date: price.date.clone(),
        },
    )?;
    e.run(
        actor("sales", Role::Sales),
        Command::QuoteAddLine {
            id: "QL".into(),
            version: 1,
            request: price,
        },
    )?;
    e.run(
        actor("sales", Role::Sales),
        Command::QuoteSubmit {
            id: "QL".into(),
            version: 2,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "QL".into(),
            version: 3,
            approve: true,
            note: "Approved published price basis".into(),
        },
    )?;
    let quote = e.state().quotes.get("QL").ok_or("Missing quote")?;
    assert_eq!(quote.status, Status::Approved);
    assert_eq!(
        quote.lines.first().map(|l| l.invoice_total),
        Some(Money(17_100))
    );
    let csv = export_quote(quote)?;
    assert!(csv.contains("List L · v4"));
    assert!(csv.contains("Explicit approved source"));
    e.state().validate()?;
    Ok(())
}

#[test]
fn grid_reapproval_archives_price_without_mutating_approved_quote_or_export() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let source = approve_grid(&mut e)?;
    add_quote(&mut e, source.clone())?;
    e.run(
        actor("sales", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "Commercial review".into(),
        },
    )?;
    let quote = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let before_csv = export_quote(&quote)?;
    calculate_grid(&mut e, Bps(500), Bps(0))?;
    let replacement = approve_grid(&mut e)?;
    assert_eq!(e.state().quotes.get("Q"), Some(&quote));
    assert_eq!(
        export_quote(e.state().quotes.get("Q").ok_or("Missing quote")?)?,
        before_csv
    );
    assert!(before_csv.contains("pricing_source_approval_revision"));
    assert!(before_csv.contains("Grid G"));
    e.state().validate()?;
    let mut price = request()?;
    price.source = Some(source);
    assert!(evaluate(e.state(), &price).is_err());
    price.source = Some(replacement);
    assert_eq!(evaluate(e.state(), &price)?.invoice_unit, Money(8_550));
    assert_eq!(
        e.state()
            .grid
            .get("G")
            .ok_or("Missing grid")?
            .approved_history
            .len(),
        1
    );
    Ok(())
}

#[test]
fn stale_source_reprice_fails_atomically_and_history_cannot_be_forged() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let source = approve_grid(&mut e)?;
    add_quote(&mut e, source)?;
    calculate_grid(&mut e, Bps(500), Bps(0))?;
    approve_grid(&mut e)?;
    let before = e.state().clone();
    assert!(
        e.run(
            actor("sales", Role::Sales),
            Command::QuoteReprice {
                id: "Q".into(),
                version: 2
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    let mut tampered = before.clone();
    let evidence = tampered
        .quotes
        .get_mut("Q")
        .and_then(|q| q.lines.first_mut())
        .and_then(|l| l.source.as_mut())
        .ok_or("Missing evidence")?;
    evidence.pricing_revision += 1;
    assert!(tampered.validate().is_err());
    let mut tampered = before.clone();
    let latest = tampered.revision;
    let quote = tampered.quotes.get_mut("Q").ok_or("Missing quote")?;
    quote.source_revision = latest;
    quote
        .lines
        .first_mut()
        .and_then(|l| l.source.as_mut())
        .ok_or("Missing evidence")?
        .captured_revision = latest;
    assert!(
        tampered.validate().is_err(),
        "Superseded price cannot be captured after its replacement approval"
    );
    let mut tampered = before;
    tampered
        .grid
        .get_mut("G")
        .ok_or("Missing grid")?
        .approved_history
        .clear();
    assert!(tampered.validate().is_err());
    Ok(())
}

#[test]
fn frozen_source_cannot_predate_approval_or_omit_price_evidence() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let source = approve_grid(&mut e)?;
    add_quote(&mut e, source)?;
    let mut tampered = e.state().clone();
    tampered
        .quotes
        .get_mut("Q")
        .ok_or("Missing quote")?
        .source_revision = 1;
    assert!(tampered.validate().is_err());
    let mut line = e
        .state()
        .quotes
        .get("Q")
        .and_then(|q| q.lines.first())
        .cloned()
        .ok_or("Missing line")?;
    line.source = None;
    assert!(line.validate_snapshot().is_err());
    Ok(())
}

#[test]
fn legacy_requests_omit_source_and_legacy_grid_approval_remains_selectable() -> TestResult {
    let mut e = demo()?;
    let encoded = serde_json::to_string(&request()?)?;
    assert!(!encoded.contains("source"));
    assert_eq!(serde_json::from_str::<PriceRequest>(&encoded)?, request()?);
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let source = approve_grid(&mut e)?;
    let mut state = e.state().clone();
    let grid = state.grid.get_mut("G").ok_or("Missing grid")?;
    grid.last_approved
        .as_mut()
        .ok_or("Missing active")?
        .document_version = None;
    let encoded = serde_json::to_string(&state)?;
    assert!(!encoded.contains("document_version"));
    assert!(!encoded.contains("approved_history"));
    let mut price = request()?;
    price.source = Some(source);
    let evaluation = evaluate(&state, &price)?;
    assert_eq!(
        evaluation.source.as_ref().and_then(|s| s.document_version),
        None
    );
    state.validate()?;
    Ok(())
}

#[test]
fn derived_grid_cannot_chain_source_discounts() -> TestResult {
    let mut e = demo()?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let mut price = request()?;
    price.source = Some(approve_grid(&mut e)?);
    let before = e.state().clone();
    assert!(
        e.run(
            actor("pricing", Role::Analyst),
            Command::GridCalculate {
                id: "CHAIN".into(),
                version: 0,
                request: price
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-sources-{}-{}",
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
fn approved_source_history_survives_persistence_and_reopen() -> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    calculate_grid(&mut e, Bps(0), Bps(0))?;
    let source = approve_grid(&mut e)?;
    add_quote(&mut e, source)?;
    calculate_grid(&mut e, Bps(500), Bps(0))?;
    approve_grid(&mut e)?;
    let before = e.state().clone();
    drop(e);
    let reopened = Engine::open(&dir.0)?;
    assert_eq!(reopened.state(), &before);
    Ok(())
}
