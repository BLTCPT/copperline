//! Independent boundary, provenance and atomicity review of customer price contracts.
use copperline_engine::{Command, Engine, Request, export_quote};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
use copperline_pricing::evaluate;

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
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("seed-admin", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn row(id: &str, product: &str, min: i64, max: Option<i64>, price: i64) -> ContractRow {
    ContractRow {
        id: id.into(),
        product_id: product.into(),
        unit: "each".into(),
        min_quantity: Quantity(min),
        max_quantity: max.map(Quantity),
        basis: ContractBasis::Fixed {
            invoice_unit: Money(price),
        },
    }
}
fn terms(id: &str, edition: u64, start: &str, end: &str, priority: i32) -> Result<ContractTerms> {
    Ok(ContractTerms {
        id: id.into(),
        edition,
        name: "Synthetic independent contract".into(),
        customer_id: "C-ATLAS".into(),
        currency: Currency::USD,
        start: date(start)?,
        end: date(end)?,
        priority,
        rows: vec![row("VLV-ROW", "VLV-100", 1_000, None, 8_000)],
    })
}
fn create(e: &mut Engine, terms: ContractTerms) -> Result<()> {
    e.run(
        actor("contract-author", Role::Analyst),
        Command::ContractCreate { terms },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str, edition: u64) -> Result<()> {
    e.run(
        actor("contract-author", Role::Analyst),
        Command::ContractSubmit {
            id: id.into(),
            edition,
            version: 1,
        },
    )?;
    Ok(())
}
fn decide(e: &mut Engine, id: &str, edition: u64, approve: bool) -> Result<()> {
    e.run(
        actor("contract-reviewer", Role::Approver),
        Command::ContractDecide {
            id: id.into(),
            edition,
            version: 2,
            approve,
            note: "Independent synthetic contract review".into(),
        },
    )?;
    Ok(())
}
fn approve(e: &mut Engine, terms: ContractTerms) -> Result<()> {
    let (id, edition) = (terms.id.clone(), terms.edition);
    create(e, terms)?;
    submit(e, &id, edition)?;
    decide(e, &id, edition, true)
}
fn request(
    day: &str,
    product: &str,
    quantity: i64,
    source: Option<PriceSource>,
) -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: product.into(),
        customer_id: "C-ATLAS".into(),
        date: date(day)?,
        quantity: Quantity(quantity),
        discount: Bps(0),
        rebate: Bps(0),
        source,
    })
}
fn exact(id: &str, edition: u64) -> Option<PriceSource> {
    Some(PriceSource::Contract {
        id: id.into(),
        edition,
    })
}
fn automatic() -> Option<PriceSource> {
    Some(PriceSource::CustomerContract)
}
fn unit(
    e: &Engine,
    day: &str,
    product: &str,
    quantity: i64,
    source: Option<PriceSource>,
) -> Result<Money> {
    evaluate(e.state(), &request(day, product, quantity, source)?).map(|v| v.invoice_unit)
}
fn quote(e: &mut Engine, id: &str, request: PriceRequest) -> Result<()> {
    e.run(
        actor("sales-author", Role::Sales),
        Command::QuoteCreate {
            id: id.into(),
            title: "Synthetic contract quote".into(),
            customer_id: request.customer_id.clone(),
            date: request.date.clone(),
        },
    )?;
    e.run(
        actor("sales-author", Role::Sales),
        Command::QuoteAddLine {
            id: id.into(),
            version: 1,
            request,
        },
    )?;
    Ok(())
}

#[test]
fn contract_dates_and_quantity_bands_are_half_open_and_opt_in() -> TestResult {
    let mut e = demo()?;
    let mut c = terms("C", 1, "2026-06-01", "2026-07-01", 10)?;
    c.rows = vec![
        row("SMALL", "VLV-100", 1_000, Some(2_000), 9_000),
        row("LARGE", "VLV-100", 2_000, Some(3_000), 8_500),
    ];
    approve(&mut e, c)?;
    assert_eq!(
        unit(&e, "2026-06-01", "VLV-100", 1_000, exact("C", 1))?,
        Money(9_000)
    );
    assert_eq!(
        unit(&e, "2026-06-30", "VLV-100", 1_999, automatic())?,
        Money(9_000)
    );
    assert_eq!(
        unit(&e, "2026-06-15", "VLV-100", 2_000, automatic())?,
        Money(8_500)
    );
    assert_eq!(
        unit(&e, "2026-06-15", "VLV-100", 2_999, automatic())?,
        Money(8_500)
    );
    for (day, quantity) in [
        ("2026-05-31", 1_000),
        ("2026-07-01", 1_000),
        ("2026-06-01", 999),
        ("2026-06-01", 3_000),
    ] {
        assert!(unit(&e, day, "VLV-100", quantity, automatic()).is_err());
        assert!(unit(&e, day, "VLV-100", quantity, exact("C", 1)).is_err());
    }
    assert_eq!(
        unit(&e, "2026-06-15", "VLV-100", 1_000, None)?,
        Money(10_000),
        "Absent selector must preserve live rules"
    );
    let mut p = request("2026-06-15", "VLV-100", 1_001, exact("C", 1))?;
    p.discount = Bps(125);
    p.rebate = Bps(250);
    let v = evaluate(e.state(), &p)?;
    assert_eq!(v.invoice_unit, Money(8_887)); // 112.5 cents discount rounds away from zero.
    assert_eq!(v.invoice_total, Money(8_896));
    assert_eq!(v.rebate_total, Money(222));
    assert_eq!(v.pocket_total, Money(8_674));
    Ok(())
}

#[test]
fn same_start_replacement_removes_products_without_resurrection_and_preserves_approved_quote()
-> TestResult {
    let mut e = demo()?;
    approve(&mut e, terms("C", 1, "2026-01-01", "2027-01-01", 1)?)?;
    quote(
        &mut e,
        "Q",
        request("2026-02-01", "VLV-100", 1_000, automatic())?,
    )?;
    e.run(
        actor("sales-author", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    e.run(
        actor("quote-reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "Reviewed contract quote".into(),
        },
    )?;
    let frozen = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let before_export = export_quote(&frozen)?;
    let mut replacement = terms("C", 2, "2026-01-01", "2026-03-01", 1)?;
    replacement.rows = vec![row("FILTER", "FLT-020", 1_000, None, 7_000)];
    approve(&mut e, replacement)?;
    assert!(unit(&e, "2026-02-01", "VLV-100", 1_000, automatic()).is_err());
    assert!(unit(&e, "2026-02-01", "VLV-100", 1_000, exact("C", 1)).is_err());
    assert_eq!(
        unit(&e, "2026-02-01", "FLT-020", 1_000, automatic())?,
        Money(7_000)
    );
    assert!(unit(&e, "2026-04-01", "VLV-100", 1_000, automatic()).is_err());
    assert!(unit(&e, "2026-04-01", "FLT-020", 1_000, automatic()).is_err());
    assert_eq!(e.state().quotes.get("Q"), Some(&frozen));
    assert_eq!(
        export_quote(e.state().quotes.get("Q").ok_or("Missing quote")?)?,
        before_export
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn future_edition_quantity_gap_and_expiry_do_not_restore_older_coverage() -> TestResult {
    let mut e = demo()?;
    approve(&mut e, terms("C", 1, "2026-01-01", "2027-01-01", 1)?)?;
    let mut next = terms("C", 2, "2026-06-01", "2026-08-01", 1)?;
    next.rows = vec![row("BULK", "VLV-100", 5_000, None, 7_500)];
    approve(&mut e, next)?;
    assert_eq!(
        unit(&e, "2026-05-31", "VLV-100", 1_000, exact("C", 1))?,
        Money(8_000)
    );
    assert!(unit(&e, "2026-06-01", "VLV-100", 1_000, automatic()).is_err());
    assert_eq!(
        unit(&e, "2026-06-01", "VLV-100", 5_000, automatic())?,
        Money(7_500)
    );
    assert!(unit(&e, "2026-08-01", "VLV-100", 5_000, automatic()).is_err());
    Ok(())
}

#[test]
fn priority_is_explicit_equal_priority_overlap_rejected_and_adjacent_bands_allowed() -> TestResult {
    let mut e = demo()?;
    approve(&mut e, terms("LOW", 1, "2026-01-01", "2027-01-01", 5)?)?;
    let mut high = terms("HIGH", 1, "2026-01-01", "2027-01-01", 10)?;
    high.rows = vec![row("SMALL", "VLV-100", 1_000, Some(2_000), 9_000)];
    approve(&mut e, high)?;
    assert_eq!(
        unit(&e, "2026-10-13", "VLV-100", 1_000, automatic())?,
        Money(9_000),
        "Priority wins, not the cheapest price"
    );
    assert_eq!(
        unit(&e, "2026-10-13", "VLV-100", 1_000, exact("LOW", 1))?,
        Money(8_000)
    );
    create(
        &mut e,
        terms("CONFLICT", 1, "2026-01-01", "2027-01-01", 10)?,
    )?;
    submit(&mut e, "CONFLICT", 1)?;
    let before = e.state().clone();
    assert!(decide(&mut e, "CONFLICT", 1, true).is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "CONFLICT", 1, false)?;
    let mut adjacent = terms("ADJACENT", 1, "2026-01-01", "2027-01-01", 10)?;
    adjacent.rows = vec![row("LARGE", "VLV-100", 2_000, None, 8_700)];
    approve(&mut e, adjacent)?;
    assert_eq!(
        unit(&e, "2026-10-13", "VLV-100", 2_000, automatic())?,
        Money(8_700)
    );
    Ok(())
}

#[test]
fn obsolete_edition_raw_end_does_not_block_new_family_after_effective_cutoff() -> TestResult {
    let mut e = demo()?;
    approve(&mut e, terms("A", 1, "2026-01-01", "2027-01-01", 10)?)?;
    let mut next = terms("A", 2, "2026-06-01", "2027-01-01", 10)?;
    next.rows = vec![row("FILTER", "FLT-020", 1_000, None, 7_000)];
    approve(&mut e, next)?;
    approve(&mut e, terms("B", 1, "2026-06-01", "2027-01-01", 10)?)?;
    assert_eq!(
        unit(&e, "2026-05-31", "VLV-100", 1_000, automatic())?,
        Money(8_000)
    );
    let p = evaluate(
        e.state(),
        &request("2026-06-01", "VLV-100", 1_000, automatic())?,
    )?;
    assert_eq!(p.source.map(|s| s.source), exact("B", 1));
    e.state().validate()?;
    Ok(())
}

#[test]
fn older_pending_and_backward_start_approvals_fail_but_can_be_rejected_or_withdrawn() -> TestResult
{
    let mut e = demo()?;
    create(&mut e, terms("C", 1, "2026-01-01", "2027-01-01", 1)?)?;
    submit(&mut e, "C", 1)?;
    approve(&mut e, terms("C", 2, "2026-06-01", "2027-01-01", 1)?)?;
    let before = e.state().clone();
    assert!(decide(&mut e, "C", 1, true).is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "C", 1, false)?;
    create(&mut e, terms("C", 3, "2026-05-01", "2027-01-01", 1)?)?;
    submit(&mut e, "C", 3)?;
    let before = e.state().clone();
    assert!(decide(&mut e, "C", 3, true).is_err());
    assert_eq!(e.state(), &before);
    e.run(
        actor("contract-author", Role::Analyst),
        Command::ContractWithdraw {
            id: "C".into(),
            edition: 3,
            version: 2,
        },
    )?;
    approve(&mut e, terms("C", 4, "2026-06-01", "2027-01-01", 1)?)?;
    assert_eq!(
        unit(&e, "2026-07-01", "VLV-100", 1_000, exact("C", 4))?,
        Money(8_000)
    );
    Ok(())
}

fn approve_grid(e: &mut Engine, discount: i32) -> Result<PriceRequest> {
    let version = e.state().grid.get("G").map(|g| g.version).unwrap_or(0);
    let mut p = request("2026-10-13", "VLV-100", 2_000, None)?;
    p.discount = Bps(discount);
    e.run(
        actor("origin-author", Role::Analyst),
        Command::GridCalculate {
            id: "G".into(),
            version,
            request: p,
        },
    )?;
    e.run(
        actor("origin-author", Role::Analyst),
        Command::GridSubmit {
            id: "G".into(),
            version: version + 1,
        },
    )?;
    e.run(
        actor("origin-reviewer", Role::Approver),
        Command::GridDecide {
            id: "G".into(),
            version: version + 2,
            approve: true,
            note: "Independent origin review".into(),
        },
    )?;
    request(
        "2026-10-13",
        "VLV-100",
        2_000,
        Some(PriceSource::Grid {
            id: "G".into(),
            approval_revision: e.state().revision,
        }),
    )
}

#[test]
fn frozen_grid_origin_survives_replacement_before_contract_review_and_excludes_origin_author()
-> TestResult {
    let mut e = demo()?;
    let origin = approve_grid(&mut e, 1_000)?;
    let mut c = terms("C", 1, "2026-11-01", "2027-02-01", 10)?;
    c.rows = vec![ContractRow {
        basis: ContractBasis::ApprovedSource {
            request: origin.clone(),
        },
        ..row("APPROVED", "VLV-100", 2_000, None, 0)
    }];
    create(&mut e, c)?;
    approve_grid(&mut e, 2_000)?;
    submit(&mut e, "C", 1)?;
    for (name, role) in [
        ("origin-author", Role::Approver),
        ("contract-author", Role::Approver),
        ("contract-reviewer", Role::Admin),
        ("contract-reviewer", Role::Finance),
    ] {
        let before = e.state().clone();
        assert!(
            e.run(
                actor(name, role),
                Command::ContractDecide {
                    id: "C".into(),
                    edition: 1,
                    version: 2,
                    approve: true,
                    note: "Unauthorized review".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    decide(&mut e, "C", 1, true)?;
    assert_eq!(
        unit(&e, "2026-11-01", "VLV-100", 2_000, exact("C", 1))?,
        Money(9_000)
    );
    assert!(
        evaluate(e.state(), &origin).is_err(),
        "Raw old grid source is stale, but captured contract origin remains valid"
    );
    let mut stale = terms("STALE", 1, "2026-11-01", "2027-02-01", 20)?;
    stale.rows = vec![ContractRow {
        basis: ContractBasis::ApprovedSource { request: origin },
        ..row("APPROVED", "VLV-100", 2_000, None, 0)
    }];
    let before = e.state().clone();
    assert!(create(&mut e, stale).is_err());
    assert_eq!(e.state(), &before);
    e.state().validate()?;
    Ok(())
}

#[test]
fn published_list_origin_freezes_price_and_rejects_underqualified_or_chained_origins() -> TestResult
{
    let mut e = demo()?;
    e.run(
        actor("origin-author", Role::Analyst),
        Command::ListCalculate {
            id: "L".into(),
            name: "Synthetic source list".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
            quantity: Quantity(2_000),
            currency: Currency::USD,
        },
    )?;
    e.run(
        actor("origin-author", Role::Analyst),
        Command::ListSubmit {
            id: "L".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("origin-reviewer", Role::Approver),
        Command::ListDecide {
            id: "L".into(),
            version: 2,
            approve: true,
            note: "List review".into(),
        },
    )?;
    e.run(
        actor("origin-author", Role::Analyst),
        Command::ListPublish {
            id: "L".into(),
            version: 3,
        },
    )?;
    let origin = request(
        "2026-10-13",
        "VLV-100",
        2_000,
        Some(PriceSource::PriceList {
            id: "L".into(),
            version: 4,
        }),
    )?;
    let mut c = terms("C", 1, "2026-11-01", "2027-01-01", 1)?;
    c.rows = vec![ContractRow {
        basis: ContractBasis::ApprovedSource {
            request: origin.clone(),
        },
        ..row("ROW", "VLV-100", 2_000, None, 0)
    }];
    approve(&mut e, c.clone())?;
    let mut product = e
        .state()
        .products
        .get("VLV-100")
        .cloned()
        .ok_or("Missing product")?;
    product.list_price = Money(20_000);
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::ProductPut(product),
    )?;
    assert_eq!(
        unit(&e, "2026-11-01", "VLV-100", 2_000, exact("C", 1))?,
        Money(10_000)
    );
    for variation in 0..5 {
        let mut invalid = c.clone();
        invalid.id = format!("INVALID-{variation}");
        let r = invalid.rows.first_mut().ok_or("Missing row")?;
        if variation == 0 {
            r.min_quantity = Quantity(1_999);
        }
        if let ContractBasis::ApprovedSource { request } = &mut r.basis {
            match variation {
                1 => request.discount = Bps(1),
                2 => request.date = date("2026-10-14")?,
                3 => request.customer_id = "C-NORTH".into(),
                4 => request.source = exact("C", 1),
                _ => {}
            }
        }
        let before = e.state().clone();
        assert!(create(&mut e, invalid).is_err());
        assert_eq!(e.state(), &before);
    }
    Ok(())
}

#[test]
fn contract_scope_band_and_money_validation_rejects_ambiguous_or_changed_identity() -> TestResult {
    let mut e = demo()?;
    let base = terms("C", 1, "2026-01-01", "2027-01-01", 1)?;
    for variant in 0..6 {
        let mut invalid = base.clone();
        invalid.id = format!("BAD-{variant}");
        match variant {
            0 => invalid
                .rows
                .push(row("OVERLAP", "VLV-100", 2_000, None, 8_000)),
            1 => invalid
                .rows
                .push(row("VLV-ROW", "FLT-020", 1_000, None, 8_000)),
            2 => {
                invalid.rows.first_mut().ok_or("Missing row")?.basis = ContractBasis::Fixed {
                    invoice_unit: Money(-1),
                }
            }
            3 => invalid.rows.first_mut().ok_or("Missing row")?.unit = "kg".into(),
            4 => invalid.currency = Currency::EUR,
            5 => {
                invalid.rows.first_mut().ok_or("Missing row")?.max_quantity = Some(Quantity(1_000))
            }
            _ => {}
        }
        let before = e.state().clone();
        assert!(create(&mut e, invalid).is_err());
        assert_eq!(e.state(), &before);
    }
    approve(&mut e, base)?;
    let mut next = terms("C", 2, "2026-06-01", "2027-01-01", 1)?;
    next.customer_id = "C-NORTH".into();
    assert!(create(&mut e, next).is_err());
    assert!(
        e.run(
            actor("other-admin", Role::Admin),
            Command::ContractCreate {
                terms: terms("C", 2, "2026-06-01", "2027-01-01", 1)?
            }
        )
        .is_err()
    );
    let mut wrong_customer = request("2026-10-13", "VLV-100", 1_000, exact("C", 1))?;
    wrong_customer.customer_id = "C-NORTH".into();
    assert!(evaluate(e.state(), &wrong_customer).is_err());
    Ok(())
}

#[test]
fn fingerprints_reject_consistent_price_edits_review_changes_and_frozen_selector_tampering()
-> TestResult {
    let mut e = demo()?;
    approve(&mut e, terms("C", 1, "2026-01-01", "2027-01-01", 1)?)?;
    quote(
        &mut e,
        "Q",
        request("2026-10-13", "VLV-100", 1_000, automatic())?,
    )?;
    let baseline = e.state().clone();
    let mut tampered = baseline.clone();
    let contract = tampered
        .price_contracts
        .get_mut(&contract_key("C", 1))
        .ok_or("Missing contract")?;
    contract.terms.rows.first_mut().ok_or("Missing row")?.basis = ContractBasis::Fixed {
        invoice_unit: Money(8_100),
    };
    contract
        .rows
        .first_mut()
        .ok_or("Missing evidence")?
        .invoice_unit = Money(8_100);
    assert!(tampered.validate().is_err());
    let mut tampered = baseline.clone();
    tampered
        .price_contracts
        .get_mut(&contract_key("C", 1))
        .and_then(|c| c.review.as_mut())
        .ok_or("Missing review")?
        .note = "Rewritten review narrative".into();
    assert!(tampered.validate().is_err());
    let mut tampered = baseline.clone();
    tampered
        .quotes
        .get_mut("Q")
        .and_then(|q| q.lines.first_mut())
        .and_then(|l| l.source.as_mut())
        .and_then(|s| s.contract.as_mut())
        .ok_or("Missing frozen proof")?
        .terms_fingerprint = "0".repeat(64);
    assert!(tampered.validate().is_err());
    let mut tampered = baseline;
    tampered.price_contracts.remove(&contract_key("C", 1));
    assert!(tampered.validate().is_err());
    Ok(())
}

#[test]
fn replacing_stale_quote_source_is_atomic_and_revalidates_other_retained_lines() -> TestResult {
    let mut e = demo()?;
    approve(&mut e, terms("C", 1, "2026-01-01", "2027-01-01", 1)?)?;
    quote(
        &mut e,
        "Q",
        request("2026-10-13", "VLV-100", 1_000, exact("C", 1))?,
    )?;
    e.run(
        actor("sales-author", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 2,
            request: request("2026-10-13", "FLT-020", 1_000, None)?,
        },
    )?;
    let mut next = terms("C", 2, "2026-01-01", "2027-01-01", 1)?;
    next.rows.first_mut().ok_or("Missing row")?.basis = ContractBasis::Fixed {
        invoice_unit: Money(8_250),
    };
    approve(&mut e, next)?;
    let before = e.state().clone();
    assert!(
        e.run(
            actor("sales-author", Role::Sales),
            Command::QuoteReplaceLine {
                id: "Q".into(),
                version: 3,
                index: 1,
                request: request("2026-10-13", "FLT-020", 2_000, None)?
            }
        )
        .is_err(),
        "Retained stale source must fail whole edit"
    );
    assert_eq!(e.state(), &before);
    assert!(
        e.run(
            actor("other-admin", Role::Admin),
            Command::QuoteReplaceLine {
                id: "Q".into(),
                version: 3,
                index: 0,
                request: request("2026-10-13", "VLV-100", 1_000, exact("C", 2))?
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    let command = Request {
        request_id: "replace-idempotent".into(),
        expected_revision: e.state().revision,
        actor: actor("sales-author", Role::Sales),
        action: Command::QuoteReplaceLine {
            id: "Q".into(),
            version: 3,
            index: 0,
            request: request("2026-10-13", "VLV-100", 1_000, exact("C", 2))?,
        },
    };
    e.execute(command.clone())?;
    let after = e.state().clone();
    let quote = after.quotes.get("Q").ok_or("Missing quote")?;
    assert_eq!(quote.version, 4);
    assert_eq!(quote.lines.len(), 2);
    assert_eq!(
        quote.lines.first().map(|l| l.invoice_unit),
        Some(Money(8_250))
    );
    assert!(e.execute(command)?.replayed);
    assert_eq!(e.state(), &after);
    e.state().validate()?;
    Ok(())
}
