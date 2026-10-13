//! Independent checks of approved pricing evidence against frozen quote review routes.
use copperline_engine::{Command, Engine, export_quote};
use copperline_model::*;
use copperline_money::{Bps, Quantity};

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
        quantity: Quantity(1000),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
    })
}
fn activate_grid(engine: &mut Engine, discount: Bps) -> Result<PriceSource> {
    let version = engine.state().grid.get("G").map(|g| g.version).unwrap_or(0);
    let mut price = request()?;
    price.discount = discount;
    engine.run(
        actor("Pricing", Role::Analyst),
        Command::GridCalculate {
            id: "G".into(),
            version,
            request: price,
        },
    )?;
    engine.run(
        actor("Pricing", Role::Analyst),
        Command::GridSubmit {
            id: "G".into(),
            version: version + 1,
        },
    )?;
    engine.run(
        actor("Grid reviewer", Role::Approver),
        Command::GridDecide {
            id: "G".into(),
            version: version + 2,
            approve: true,
            note: "Synthetic grid approval".into(),
        },
    )?;
    Ok(PriceSource::Grid {
        id: "G".into(),
        approval_revision: engine.state().revision,
    })
}
fn submitted_source_quote() -> Result<(Engine, PriceSource)> {
    let mut engine = Engine::memory();
    engine.run(actor("Administrator", Role::Admin), Command::SeedDemo)?;
    engine.run(
        actor("Administrator", Role::Admin),
        Command::QuoteApprovalPolicySet {
            expected_version: 0,
            thresholds: ApprovalThresholds::default(),
        },
    )?;
    let source = activate_grid(&mut engine, Bps(2500))?;
    engine.run(
        actor("Seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic approved-source route".into(),
            customer_id: "C-ATLAS".into(),
            date: request()?.date,
        },
    )?;
    let mut price = request()?;
    price.source = Some(source.clone());
    engine.run(
        actor("Seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: price,
        },
    )?;
    engine.run(
        actor("Seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    Ok((engine, source))
}

#[test]
fn embedded_source_discount_requires_finance_and_survives_source_replacement() -> TestResult {
    let (mut engine, source) = submitted_source_quote()?;
    let quote = engine.state().quotes.get("Q").ok_or("Missing quote")?;
    let cycle = quote
        .governance
        .as_ref()
        .and_then(|g| g.cycles.last())
        .ok_or("Missing cycle")?;
    assert_eq!(quote.lines[0].request.discount, Bps(0));
    assert_eq!(
        cycle.stages.iter().map(|s| s.role).collect::<Vec<_>>(),
        vec![Role::Approver, Role::Finance]
    );
    assert!(
        cycle.stages[1]
            .reason
            .contains("effective invoice discount")
    );
    engine.run(
        actor("Commercial reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "First stage".into(),
        },
    )?;
    let before = engine
        .state()
        .quotes
        .get("Q")
        .cloned()
        .ok_or("Missing quote")?;
    assert_eq!(before.status, Status::Pending);
    assert!(export_quote(&before).is_err());
    activate_grid(&mut engine, Bps(0))?;
    engine.run(
        actor("Administrator", Role::Admin),
        Command::QuoteApprovalPolicySet {
            expected_version: 1,
            thresholds: ApprovalThresholds {
                max_discount: Bps(10000),
                min_margin: Bps(0),
                ..ApprovalThresholds::default()
            },
        },
    )?;
    assert_eq!(engine.state().quotes.get("Q"), Some(&before));
    let stable = engine.state().clone();
    assert!(
        engine
            .run(
                actor("Commercial reviewer", Role::Finance),
                Command::QuoteDecide {
                    id: "Q".into(),
                    version: 4,
                    approve: true,
                    note: "Same person cannot cover finance".into()
                }
            )
            .is_err()
    );
    assert_eq!(engine.state(), &stable);
    engine.run(
        actor("Finance reviewer", Role::Finance),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 4,
            approve: true,
            note: "Independent finance review".into(),
        },
    )?;
    let quote = engine.state().quotes.get("Q").ok_or("Missing quote")?;
    assert_eq!(quote.status, Status::Approved);
    assert_eq!(quote.lines, before.lines);
    assert_eq!(quote.lines[0].request.source, Some(source));
    assert!(export_quote(quote)?.contains("Grid G"));
    engine.state().validate()?;
    Ok(())
}

#[test]
fn corrupted_submission_version_returns_error_instead_of_panicking() -> TestResult {
    let (mut engine, _) = submitted_source_quote()?;
    engine.run(
        actor("Commercial reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "First stage".into(),
        },
    )?;
    let mut invalid = engine.state().clone();
    let quote = invalid.quotes.get_mut("Q").ok_or("Missing quote")?;
    quote
        .governance
        .as_mut()
        .and_then(|g| g.cycles.last_mut())
        .ok_or("Missing cycle")?
        .submitted_version = u64::MAX;
    let validation = std::panic::catch_unwind(|| invalid.validate());
    assert!(
        validation.is_ok(),
        "Malformed persisted versions must not panic during validation"
    );
    assert!(validation.map_err(|_| "Validation panicked")?.is_err());
    Ok(())
}
