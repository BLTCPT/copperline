//! Independent financial boundary review for explicit dated FX and product UOM conversion.
use copperline_engine::{Command, Engine};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity, Rational};
use copperline_pricing::{compare, evaluate, totals};

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
fn ratio(numerator: i64, denominator: i64) -> Rational {
    Rational {
        numerator,
        denominator,
    }
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("seed-author", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn fx(
    id: &str,
    from: Currency,
    to: Currency,
    n: i64,
    d: i64,
    start: &str,
    end: &str,
) -> Result<ConversionEntry> {
    Ok(ConversionEntry {
        id: id.into(),
        conversion: ConversionKind::Fx { from, to },
        start: date(start)?,
        end: date(end)?,
        ratio: ratio(n, d),
    })
}
fn uom(id: &str, product: &str, unit: &str, n: i64, d: i64) -> Result<ConversionEntry> {
    Ok(ConversionEntry {
        id: id.into(),
        conversion: ConversionKind::Uom {
            product_id: product.into(),
            from_unit: unit.into(),
            to_unit: "each".into(),
        },
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
        ratio: ratio(n, d),
    })
}
fn book(e: &mut Engine, id: &str, entries: Vec<ConversionEntry>) -> Result<()> {
    e.run(
        actor("conversion-author", Role::Analyst),
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: id.into(),
                name: "Independent synthetic conversion schedule".into(),
                entries,
            },
        },
    )?;
    e.run(
        actor("conversion-author", Role::Analyst),
        Command::ConversionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("conversion-reviewer", Role::Approver),
        Command::ConversionDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Reviewed direct rational rates and dates".into(),
        },
    )?;
    Ok(())
}
fn request(
    day: &str,
    product: &str,
    quantity: i64,
    currency: Currency,
    unit: &str,
) -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: product.into(),
        customer_id: "C-ATLAS".into(),
        date: date(day)?,
        quantity: Quantity(quantity),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
        conversion: Some(ConversionRequest {
            currency,
            unit: unit.into(),
        }),
    })
}
fn native(day: &str, product: &str, quantity: i64) -> Result<PriceRequest> {
    let mut r = request(day, product, quantity, Currency::USD, "each")?;
    r.conversion = None;
    Ok(r)
}
fn product(
    e: &mut Engine,
    id: &str,
    currency: Currency,
    unit: &str,
    list: i64,
    cost: i64,
) -> Result<()> {
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::ProductPut(Product {
            id: id.into(),
            name: "Synthetic arithmetic fixture".into(),
            family: "Review".into(),
            unit: unit.into(),
            currency,
            list_price: Money(list),
        }),
    )?;
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: id.into(),
            version: 1,
            effective_from: date("2026-01-01")?,
            amount: Money(cost),
            currency,
            unit: unit.into(),
        }),
    )?;
    Ok(())
}
fn contract(e: &mut Engine) -> Result<()> {
    e.run(
        actor("contract-author", Role::Analyst),
        Command::ContractCreate {
            terms: ContractTerms {
                id: "CONTRACT".into(),
                edition: 1,
                name: "Native quantity bands".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::USD,
                start: date("2026-01-01")?,
                end: date("2027-01-01")?,
                priority: 10,
                rows: vec![
                    ContractRow {
                        id: "SMALL".into(),
                        product_id: "VLV-100".into(),
                        unit: "each".into(),
                        min_quantity: Quantity(12_000),
                        max_quantity: Some(Quantity(24_000)),
                        basis: ContractBasis::Fixed {
                            invoice_unit: Money(9_000),
                        },
                    },
                    ContractRow {
                        id: "BULK".into(),
                        product_id: "VLV-100".into(),
                        unit: "each".into(),
                        min_quantity: Quantity(24_000),
                        max_quantity: Some(Quantity(36_000)),
                        basis: ContractBasis::Fixed {
                            invoice_unit: Money(8_000),
                        },
                    },
                ],
            },
        },
    )?;
    e.run(
        actor("contract-author", Role::Analyst),
        Command::ContractSubmit {
            id: "CONTRACT".into(),
            edition: 1,
            version: 1,
        },
    )?;
    e.run(
        actor("contract-reviewer", Role::Approver),
        Command::ContractDecide {
            id: "CONTRACT".into(),
            edition: 1,
            version: 2,
            approve: true,
            note: "Reviewed native unit bands".into(),
        },
    )?;
    Ok(())
}
fn add_quote(e: &mut Engine, id: &str, request: PriceRequest) -> Result<()> {
    e.run(
        actor("sales-author", Role::Sales),
        Command::QuoteCreate {
            id: id.into(),
            title: "Conversion review".into(),
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
fn invoice(
    id: &str,
    day: &str,
    product: &str,
    customer: &str,
    currency: Currency,
    amount: i64,
    cost: i64,
) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "independent-synthetic-review".into(),
        product_id: product.into(),
        customer_id: customer.into(),
        date: date(day)?,
        quantity: Quantity(1_234),
        unit: "each".into(),
        currency,
        invoice_total: Money(amount),
        cost_total: Money(cost),
    })
}
fn add_invoice(e: &mut Engine, transaction: Transaction) -> Result<()> {
    e.run(
        actor("invoice-author", Role::Analyst),
        Command::TransactionAdd(transaction),
    )?;
    Ok(())
}
fn reporting(start: &str, end: &str, currency: Currency) -> Result<ReportingRequest> {
    Ok(ReportingRequest {
        start: date(start)?,
        end: date(end)?,
        currency,
    })
}
fn correction(e: &mut Engine, original: &str, replacement: Option<Transaction>) -> Result<()> {
    e.run(
        actor("correction-author", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: "CORRECTION".into(),
                original_transaction_id: original.into(),
                effective_date: date("2027-12-31")?,
                reason: "Full original reversal with independent replacement context".into(),
                replacement,
            },
        },
    )?;
    e.run(
        actor("correction-author", Role::Analyst),
        Command::CorrectionSubmit {
            id: "CORRECTION".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("finance-reviewer", Role::Finance),
        Command::CorrectionDecide {
            id: "CORRECTION".into(),
            version: 2,
            approve: true,
            note: "Reviewed exact original and replacement".into(),
        },
    )?;
    Ok(())
}

#[test]
fn rules_and_contract_bands_use_exact_native_quantity_not_requested_quantity() -> TestResult {
    let mut e = demo()?;
    book(&mut e, "BOXES", vec![uom("BOX", "VLV-100", "box", 12, 1)?])?;
    let below = evaluate(
        e.state(),
        &request("2026-10-13", "VLV-100", 1_666, Currency::USD, "box")?,
    )?;
    let above = evaluate(
        e.state(),
        &request("2026-10-13", "VLV-100", 1_667, Currency::USD, "box")?,
    )?;
    assert_eq!(below.rule_unit, Money(120_000));
    assert_eq!(above.rule_unit, Money(110_400));
    assert_eq!(
        above
            .conversion
            .as_ref()
            .ok_or("Missing conversion")?
            .native_quantity,
        Quantity(20_004)
    );
    assert!(
        above
            .rule
            .as_deref()
            .is_some_and(|r| r.starts_with("OEM-VOLUME"))
    );
    contract(&mut e)?;
    let mut p = request("2026-10-13", "VLV-100", 1_999, Currency::USD, "box")?;
    p.source = Some(PriceSource::CustomerContract);
    assert_eq!(evaluate(e.state(), &p)?.rule_unit, Money(108_000));
    p.quantity = Quantity(2_000);
    let bulk = evaluate(e.state(), &p)?;
    assert_eq!(bulk.rule_unit, Money(96_000));
    let source = bulk.source.as_ref().ok_or("Missing contract evidence")?;
    assert_eq!(source.invoice_unit, Money(8_000));
    assert_eq!(source.currency, Currency::USD);
    assert_eq!(source.unit, "each");
    assert_eq!(
        source
            .contract
            .as_ref()
            .ok_or("Missing row evidence")?
            .row_id,
        "BULK"
    );
    p.quantity = Quantity(3_000);
    assert!(
        evaluate(e.state(), &p).is_err(),
        "Native upper band edge remains exclusive"
    );
    p.quantity = Quantity(999);
    assert!(
        evaluate(e.state(), &p).is_err(),
        "Source minimum is expressed in native units"
    );
    add_quote(&mut e, "Q", bulk.request)?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn both_conversion_legs_round_once_before_discount_extension_and_rebate() -> TestResult {
    let mut e = demo()?;
    product(&mut e, "PENNY", Currency::USD, "each", 1, 0)?;
    book(
        &mut e,
        "HALF-DOUBLE",
        vec![
            uom("HALF", "PENNY", "half", 1, 2)?,
            fx(
                "DOUBLE",
                Currency::USD,
                Currency::EUR,
                2,
                1,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    let p = evaluate(
        e.state(),
        &request("2026-10-13", "PENNY", 2_000, Currency::EUR, "half")?,
    )?;
    assert_eq!(
        (p.list_unit, p.rule_unit, p.invoice_unit, p.invoice_total),
        (Money(1), Money(1), Money(1), Money(2))
    );
    assert_eq!(
        p.conversion
            .as_ref()
            .ok_or("Missing conversion")?
            .native_quantity,
        Quantity(1_000)
    );
    assert_eq!(
        ratio(2, 1).money(ratio(1, 2).money(Money(1))?)?,
        Money(2),
        "Rounding each leg would double this unit price"
    );
    let mut e = demo()?;
    book(
        &mut e,
        "FRACTIONAL",
        vec![
            uom("ONE-HALF", "VLV-100", "bundle", 3, 2)?,
            fx(
                "SEVEN-NINTHS",
                Currency::USD,
                Currency::EUR,
                7,
                9,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    let mut r = request("2026-10-13", "VLV-100", 2_002, Currency::EUR, "bundle")?;
    r.discount = Bps(125);
    r.rebate = Bps(250);
    let p = evaluate(e.state(), &r)?;
    assert_eq!(
        (p.list_unit, p.rule_unit, p.cost_unit),
        (Money(11_667), Money(11_667), Money(7_000))
    );
    assert_eq!(
        (p.invoice_unit, p.invoice_total, p.rebate_total),
        (Money(11_521), Money(23_065), Money(577))
    );
    assert_eq!(
        (p.pocket_total, p.cost_total, p.margin_total),
        (Money(22_488), Money(14_014), Money(8_474))
    );
    assert_eq!(p.pocket_total.add(p.rebate_total)?, p.invoice_total);
    assert_eq!(p.margin_total.add(p.cost_total)?, p.pocket_total);
    add_quote(&mut e, "Q", r)?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn source_discounts_are_already_in_native_price_and_rebates_are_not_replayed() -> TestResult {
    let mut e = demo()?;
    let mut origin = native("2026-10-13", "VLV-100", 12_000)?;
    origin.discount = Bps(1_000);
    origin.rebate = Bps(500);
    e.run(
        actor("grid-author", Role::Analyst),
        Command::GridCalculate {
            id: "GRID".into(),
            version: 0,
            request: origin,
        },
    )?;
    e.run(
        actor("grid-author", Role::Analyst),
        Command::GridSubmit {
            id: "GRID".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("grid-reviewer", Role::Approver),
        Command::GridDecide {
            id: "GRID".into(),
            version: 2,
            approve: true,
            note: "Reviewed native source".into(),
        },
    )?;
    let approval_revision = e.state().revision;
    book(
        &mut e,
        "CONVERT",
        vec![
            uom("BOX", "VLV-100", "box", 12, 1)?,
            fx(
                "HALF",
                Currency::USD,
                Currency::EUR,
                1,
                2,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    let mut r = request("2026-10-13", "VLV-100", 1_000, Currency::EUR, "box")?;
    r.source = Some(PriceSource::Grid {
        id: "GRID".into(),
        approval_revision,
    });
    r.discount = Bps(1_000);
    r.rebate = Bps(200);
    let p = evaluate(e.state(), &r)?;
    assert_eq!(
        p.source.as_ref().ok_or("Missing source")?.invoice_unit,
        Money(9_000)
    );
    assert_eq!(
        (p.rule_unit, p.invoice_unit, p.invoice_total, p.rebate_total),
        (Money(54_000), Money(48_600), Money(48_600), Money(972))
    );
    add_quote(&mut e, "Q", r.clone())?;
    let old_quote = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let version = e.state().grid.get("GRID").ok_or("Missing grid")?.version;
    e.run(
        actor("grid-author", Role::Analyst),
        Command::GridCalculate {
            id: "GRID".into(),
            version,
            request: native("2026-10-13", "VLV-100", 12_000)?,
        },
    )?;
    e.run(
        actor("grid-author", Role::Analyst),
        Command::GridSubmit {
            id: "GRID".into(),
            version: version + 1,
        },
    )?;
    e.run(
        actor("grid-reviewer", Role::Approver),
        Command::GridDecide {
            id: "GRID".into(),
            version: version + 2,
            approve: true,
            note: "Reviewed replacement native source".into(),
        },
    )?;
    assert!(
        evaluate(e.state(), &r).is_err(),
        "New pricing cannot reuse a stale source revision"
    );
    assert_eq!(e.state().quotes.get("Q"), Some(&old_quote));
    e.state().validate()?;
    Ok(())
}

#[test]
fn rates_have_half_open_dates_and_identity_legs_do_not_need_records() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "DATES",
        vec![
            fx(
                "JUNE",
                Currency::USD,
                Currency::EUR,
                1,
                2,
                "2026-06-01",
                "2026-07-01",
            )?,
            fx(
                "JULY",
                Currency::USD,
                Currency::EUR,
                3,
                4,
                "2026-07-01",
                "2026-08-01",
            )?,
            uom("BOX", "VLV-100", "box", 12, 1)?,
        ],
    )?;
    for (day, amount, row) in [
        ("2026-06-01", 5_000, "JUNE"),
        ("2026-06-30", 5_000, "JUNE"),
        ("2026-07-01", 7_500, "JULY"),
        ("2026-07-31", 7_500, "JULY"),
    ] {
        let p = evaluate(
            e.state(),
            &request(day, "VLV-100", 1_000, Currency::EUR, "each")?,
        )?;
        assert_eq!(p.invoice_unit, Money(amount));
        let c = p.conversion.as_ref().ok_or("Missing conversion")?;
        assert!(c.uom.is_none());
        assert_eq!(c.fx.as_ref().ok_or("Missing FX")?.entry.id, row);
    }
    for day in ["2026-05-31", "2026-08-01"] {
        assert!(
            evaluate(
                e.state(),
                &request(day, "VLV-100", 1_000, Currency::EUR, "each")?
            )
            .is_err()
        );
    }
    let p = evaluate(
        e.state(),
        &request("2026-10-13", "VLV-100", 1_000, Currency::USD, "box")?,
    )?;
    assert!(p.conversion.as_ref().ok_or("Missing UOM")?.fx.is_none());
    assert!(
        evaluate(e.state(), &native("2026-10-13", "VLV-100", 1_000)?)?
            .conversion
            .is_none()
    );
    assert!(
        evaluate(
            e.state(),
            &request("2026-10-13", "VLV-100", 1_000, Currency::USD, "each")?
        )
        .is_err(),
        "All-identity requests must use the legacy absent selector"
    );
    Ok(())
}

#[test]
fn inverse_multihop_other_product_and_unapproved_rates_are_never_inferred() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "DIRECT",
        vec![
            fx(
                "INVERSE",
                Currency::GBP,
                Currency::USD,
                2,
                1,
                "2026-01-01",
                "2027-01-01",
            )?,
            fx(
                "LEG-ONE",
                Currency::USD,
                Currency::EUR,
                2,
                3,
                "2026-01-01",
                "2027-01-01",
            )?,
            fx(
                "LEG-TWO",
                Currency::EUR,
                Currency::GBP,
                3,
                2,
                "2026-01-01",
                "2027-01-01",
            )?,
            uom("OTHER-PRODUCT", "FLT-020", "box", 12, 1)?,
        ],
    )?;
    assert!(
        evaluate(
            e.state(),
            &request("2026-10-13", "VLV-100", 1_000, Currency::GBP, "each")?
        )
        .is_err()
    );
    assert!(
        evaluate(
            e.state(),
            &request("2026-10-13", "VLV-100", 1_000, Currency::USD, "box")?
        )
        .is_err()
    );
    let terms = ConversionTerms {
        id: "PENDING".into(),
        name: "Not approved".into(),
        entries: vec![fx(
            "ACTUAL-DIRECT",
            Currency::USD,
            Currency::GBP,
            7,
            5,
            "2026-01-01",
            "2027-01-01",
        )?],
    };
    e.run(
        actor("conversion-author", Role::Analyst),
        Command::ConversionCreate { terms },
    )?;
    e.run(
        actor("conversion-author", Role::Analyst),
        Command::ConversionSubmit {
            id: "PENDING".into(),
            version: 1,
        },
    )?;
    assert!(
        evaluate(
            e.state(),
            &request("2026-10-13", "VLV-100", 1_000, Currency::GBP, "each")?
        )
        .is_err()
    );
    let ordinary = evaluate(e.state(), &native("2026-10-13", "VLV-100", 1_000)?)?;
    assert_eq!(
        (ordinary.currency, ordinary.invoice_total),
        (Currency::USD, Money(10_000)),
        "Approval of rates never makes conversion implicit"
    );
    Ok(())
}

#[test]
fn exact_quantity_and_amount_failures_preserve_the_entire_quote_state() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "BOUNDARIES",
        vec![
            uom("THIRD", "VLV-100", "third", 1, 3)?,
            uom("MILLION", "VLV-100", "huge", 1_000_000, 1)?,
            uom("SMALL", "VLV-100", "half", 1, 2)?,
            fx(
                "EXPAND",
                Currency::USD,
                Currency::EUR,
                1_000_000_000,
                1,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    add_quote(&mut e, "Q", native("2026-10-13", "VLV-100", 1_000)?)?;
    for (quantity, currency, unit) in [
        (1_000, Currency::USD, "third"),
        (1_001, Currency::USD, "huge"),
        (1_000_000_002, Currency::USD, "half"),
        (1_000, Currency::EUR, "huge"),
    ] {
        let before = e.state().clone();
        assert!(
            e.run(
                actor("sales-author", Role::Sales),
                Command::QuoteAddLine {
                    id: "Q".into(),
                    version: 2,
                    request: request("2026-10-13", "VLV-100", quantity, currency, unit)?
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    let exact = evaluate(
        e.state(),
        &request("2026-10-13", "VLV-100", 3_000, Currency::USD, "third")?,
    )?;
    assert_eq!(
        exact
            .conversion
            .as_ref()
            .ok_or("Missing conversion")?
            .native_quantity,
        Quantity(1_000)
    );
    assert_eq!(
        exact.invoice_total,
        Money(9_999),
        "Unit cents are authoritative, not the native extended amount"
    );
    Ok(())
}

#[test]
fn independent_direct_rates_need_not_be_reciprocal_and_money_is_not_reversible() -> TestResult {
    let mut e = demo()?;
    product(&mut e, "USD-CENT", Currency::USD, "each", 1, 0)?;
    product(&mut e, "EUR-CENT", Currency::EUR, "each", 1, 0)?;
    book(
        &mut e,
        "TWO-DIRECTIONS",
        vec![
            fx(
                "FORWARD",
                Currency::USD,
                Currency::EUR,
                2,
                3,
                "2026-01-01",
                "2027-01-01",
            )?,
            fx(
                "RETURN",
                Currency::EUR,
                Currency::USD,
                3,
                2,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    assert_eq!(
        evaluate(
            e.state(),
            &request("2026-10-13", "USD-CENT", 1_000, Currency::EUR, "each")?
        )?
        .invoice_unit,
        Money(1)
    );
    assert_eq!(
        evaluate(
            e.state(),
            &request("2026-10-13", "EUR-CENT", 1_000, Currency::USD, "each")?
        )?
        .invoice_unit,
        Money(2)
    );
    book(
        &mut e,
        "NONRECIPROCAL",
        vec![
            fx(
                "OTHER-FORWARD",
                Currency::USD,
                Currency::GBP,
                2,
                1,
                "2026-01-01",
                "2027-01-01",
            )?,
            fx(
                "OTHER-RETURN",
                Currency::GBP,
                Currency::USD,
                3,
                1,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn mixed_currency_totals_and_scenarios_require_explicit_common_output() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "FX",
        vec![fx(
            "HALF",
            Currency::USD,
            Currency::EUR,
            1,
            2,
            "2026-01-01",
            "2027-01-01",
        )?],
    )?;
    let native_request = native("2026-10-13", "VLV-100", 1_000)?;
    let converted_request = request("2026-10-13", "VLV-100", 1_000, Currency::EUR, "each")?;
    let a = evaluate(e.state(), &native_request)?;
    let b = evaluate(e.state(), &converted_request)?;
    assert!(totals(&[a, b.clone()]).is_err());
    assert!(compare(e.state(), &native_request, &converted_request).is_err());
    let mut discounted = converted_request.clone();
    discounted.discount = Bps(1_000);
    assert_eq!(
        compare(e.state(), &converted_request, &discounted)?.pocket_delta,
        Money(-500)
    );
    let t = totals(&[b, evaluate(e.state(), &discounted)?])?;
    assert_eq!(
        (t.currency, t.invoice, t.cost, t.margin),
        (Currency::EUR, Money(9_500), Money(6_000), Money(3_500))
    );
    Ok(())
}

#[test]
fn reporting_reversal_uses_original_date_rate_and_replacement_keeps_independent_context()
-> TestResult {
    let mut e = demo()?;
    product(&mut e, "EURO", Currency::EUR, "each", 100, 50)?;
    book(
        &mut e,
        "DATED",
        vec![
            fx(
                "OCTOBER",
                Currency::USD,
                Currency::EUR,
                1,
                2,
                "2026-10-01",
                "2026-11-01",
            )?,
            fx(
                "NOVEMBER",
                Currency::USD,
                Currency::EUR,
                7,
                2,
                "2026-11-01",
                "2026-12-01",
            )?,
        ],
    )?;
    let original = invoice(
        "ORIGINAL",
        "2026-10-31",
        "VLV-100",
        "C-ATLAS",
        Currency::USD,
        4,
        1,
    )?;
    add_invoice(&mut e, original.clone())?;
    let report_request = reporting("2026-10-01", "2026-12-01", Currency::EUR)?;
    let old_revision = e.state().revision;
    let old = e.state().converted_actuals(&report_request, old_revision)?;
    let group = old.groups.first().ok_or("Missing group")?;
    assert_eq!(
        (group.invoice, group.cost, group.margin),
        (Money(2), Money(1), Money(1)),
        "Reporting converts invoice/cost separately, never a precomputed native margin"
    );
    let replacement = invoice(
        "REPLACEMENT",
        "2026-11-15",
        "EURO",
        "C-NORTH",
        Currency::EUR,
        9,
        5,
    )?;
    correction(&mut e, "ORIGINAL", Some(replacement.clone()))?;
    let current = e
        .state()
        .converted_actuals(&report_request, e.state().revision)?;
    assert_eq!(
        e.state().converted_actuals(&report_request, old_revision)?,
        old,
        "Historical visibility is based on approval revisions"
    );
    let original_row = current
        .rows
        .iter()
        .find(|r| r.native.id == "ORIGINAL")
        .ok_or("Missing original row")?;
    assert_eq!(original_row.native, original);
    assert_eq!(original_row.reversed_by.as_deref(), Some("CORRECTION"));
    assert_eq!(
        original_row.fx.as_ref().ok_or("Missing FX")?.entry.id,
        "OCTOBER"
    );
    assert_eq!(
        (original_row.invoice, original_row.cost),
        (Money(2), Money(1))
    );
    let old_customer = current
        .groups
        .iter()
        .find(|g| g.customer_id == "C-ATLAS")
        .ok_or("Missing old group")?;
    assert_eq!(
        (
            old_customer.gross_invoice,
            old_customer.reversed_invoice,
            old_customer.invoice,
            old_customer.cost
        ),
        (Money(2), Money(2), Money(0), Money(0))
    );
    assert_eq!(
        (
            old_customer.gross_count,
            old_customer.reversal_count,
            old_customer.margin_bps
        ),
        (1, 1, None)
    );
    let replacement_row = current
        .rows
        .iter()
        .find(|r| r.native.id == "REPLACEMENT")
        .ok_or("Missing replacement row")?;
    assert_eq!(replacement_row.native, replacement);
    assert!(replacement_row.fx.is_none());
    assert_eq!(
        (replacement_row.invoice, replacement_row.cost),
        (Money(9), Money(5))
    );
    let narrow = e.state().converted_actuals(
        &reporting("2026-10-31", "2026-11-01", Currency::EUR)?,
        e.state().revision,
    )?;
    assert_eq!(narrow.rows.len(), 1);
    assert_eq!(
        narrow.groups.first().ok_or("Missing narrow group")?.invoice,
        Money(0)
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn reporting_rounds_each_native_invoice_total_and_reverses_exact_rounded_pair() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "HALF",
        vec![fx(
            "HALF",
            Currency::USD,
            Currency::EUR,
            1,
            2,
            "2026-01-01",
            "2027-01-01",
        )?],
    )?;
    for id in ["A", "B"] {
        add_invoice(
            &mut e,
            invoice(id, "2026-10-13", "VLV-100", "C-ATLAS", Currency::USD, 1, 0)?,
        )?;
    }
    let req = reporting("2026-01-01", "2027-01-01", Currency::EUR)?;
    let before = e.state().converted_actuals(&req, e.state().revision)?;
    assert_eq!(
        before.groups.first().ok_or("Missing group")?.invoice,
        Money(2),
        "Two half-cent invoices each round to one cent; do not aggregate native totals first"
    );
    correction(&mut e, "A", None)?;
    let after = e.state().converted_actuals(&req, e.state().revision)?;
    let g = after.groups.first().ok_or("Missing group")?;
    assert_eq!(
        (g.gross_invoice, g.reversed_invoice, g.invoice),
        (Money(2), Money(1), Money(1))
    );
    Ok(())
}

#[test]
fn historical_reporting_requires_rates_approved_at_its_source_revision() -> TestResult {
    let mut e = demo()?;
    add_invoice(
        &mut e,
        invoice(
            "IN",
            "2026-06-01",
            "VLV-100",
            "C-ATLAS",
            Currency::USD,
            101,
            40,
        )?,
    )?;
    add_invoice(
        &mut e,
        invoice(
            "OUT",
            "2026-07-01",
            "VLV-100",
            "C-ATLAS",
            Currency::USD,
            99,
            10,
        )?,
    )?;
    let request = reporting("2026-06-01", "2026-07-01", Currency::EUR)?;
    let old_revision = e.state().revision;
    assert!(e.state().converted_actuals(&request, old_revision).is_err());
    book(
        &mut e,
        "LATER-APPROVAL",
        vec![fx(
            "JUNE",
            Currency::USD,
            Currency::EUR,
            1,
            2,
            "2026-06-01",
            "2026-07-01",
        )?],
    )?;
    assert!(
        e.state().converted_actuals(&request, old_revision).is_err(),
        "Later approval cannot manufacture historical rate availability"
    );
    let now = e.state().converted_actuals(&request, e.state().revision)?;
    assert_eq!(now.rows.len(), 1);
    assert_eq!(now.rows.first().ok_or("Missing row")?.native.id, "IN");
    assert_eq!(
        now.groups.first().ok_or("Missing group")?.invoice,
        Money(51)
    );
    assert!(
        e.state()
            .converted_actuals(
                &reporting("2026-06-01", "2026-07-02", Currency::EUR)?,
                e.state().revision
            )
            .is_err(),
        "One uncovered invoice rejects the whole report"
    );
    assert!(
        e.state()
            .converted_actuals(&request, e.state().revision + 1)
            .is_err()
    );
    assert!(
        e.state()
            .converted_actuals(
                &reporting("2026-07-01", "2026-07-01", Currency::EUR)?,
                e.state().revision
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn converted_waterfall_conserves_totals_across_quantities_and_discounts() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "INVARIANTS",
        vec![
            uom("BOX", "VLV-100", "box", 12, 1)?,
            fx(
                "FX",
                Currency::USD,
                Currency::EUR,
                7,
                9,
                "2026-01-01",
                "2027-01-01",
            )?,
        ],
    )?;
    for quantity in [1, 17, 1_666, 1_667, 3_001, 100_000] {
        let mut r = request("2026-10-13", "VLV-100", quantity, Currency::EUR, "box")?;
        for rebate in [0, 3_333, 10_000] {
            r.rebate = Bps(rebate);
            let mut prior: Option<(Money, Money)> = None;
            for discount in [0, 1, 125, 3_333, 9_999, 10_000] {
                r.discount = Bps(discount);
                let p = evaluate(e.state(), &r)?;
                p.validate_snapshot()?;
                assert_eq!(p.currency, Currency::EUR);
                assert_eq!(
                    p.conversion
                        .as_ref()
                        .ok_or("Missing conversion")?
                        .native_quantity,
                    Quantity(quantity * 12)
                );
                assert_eq!(p.cost_unit, Money(56_000));
                assert_eq!(p.pocket_total.add(p.rebate_total)?, p.invoice_total);
                assert_eq!(p.margin_total.add(p.cost_total)?, p.pocket_total);
                if let Some((unit, pocket)) = prior {
                    assert!(p.invoice_unit <= unit);
                    assert!(p.pocket_total <= pocket);
                }
                prior = Some((p.invoice_unit, p.pocket_total));
                if discount == 10_000 || rebate == 10_000 {
                    assert_eq!(p.pocket_total, Money(0));
                    assert_eq!(p.margin_bps, None);
                    assert!(p.hard_block);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn converted_values_cannot_become_native_grid_or_contract_origins() -> TestResult {
    let mut e = demo()?;
    book(
        &mut e,
        "FX",
        vec![fx(
            "HALF",
            Currency::USD,
            Currency::EUR,
            1,
            2,
            "2026-01-01",
            "2027-01-01",
        )?],
    )?;
    let mut converted = request("2026-10-13", "VLV-100", 1_000, Currency::EUR, "each")?;
    let before = e.state().clone();
    assert!(
        e.run(
            actor("grid-author", Role::Analyst),
            Command::GridCalculate {
                id: "CONVERTED".into(),
                version: 0,
                request: converted.clone()
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    e.run(
        actor("grid-author", Role::Analyst),
        Command::GridCalculate {
            id: "NATIVE".into(),
            version: 0,
            request: native("2026-10-13", "VLV-100", 1_000)?,
        },
    )?;
    e.run(
        actor("grid-author", Role::Analyst),
        Command::GridSubmit {
            id: "NATIVE".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("grid-reviewer", Role::Approver),
        Command::GridDecide {
            id: "NATIVE".into(),
            version: 2,
            approve: true,
            note: "Reviewed native origin".into(),
        },
    )?;
    converted.source = Some(PriceSource::Grid {
        id: "NATIVE".into(),
        approval_revision: e.state().revision,
    });
    let before = e.state().clone();
    assert!(
        e.run(
            actor("contract-author", Role::Analyst),
            Command::ContractCreate {
                terms: ContractTerms {
                    id: "CHAIN".into(),
                    edition: 1,
                    name: "Invalid converted origin".into(),
                    customer_id: "C-ATLAS".into(),
                    currency: Currency::USD,
                    start: date("2026-10-13")?,
                    end: date("2027-01-01")?,
                    priority: 10,
                    rows: vec![ContractRow {
                        id: "ROW".into(),
                        product_id: "VLV-100".into(),
                        unit: "each".into(),
                        min_quantity: Quantity(1_000),
                        max_quantity: None,
                        basis: ContractBasis::ApprovedSource { request: converted }
                    }],
                }
            }
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    Ok(())
}
