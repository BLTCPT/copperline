//! Independent mixed-evidence export and read-surface review.
use copperline_engine::{
    Command, Engine, analytics, export_credit, export_period_close, export_report,
};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity, Rational};
use serde_json::Value;
use std::collections::BTreeMap;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
fn run(e: &mut Engine, name: &str, role: Role, command: Command) -> Result<()> {
    e.run(
        Actor {
            name: name.into(),
            role,
        },
        command,
    )?;
    Ok(())
}
fn date(s: &str) -> Result<Date> {
    Date::parse(s)
}
fn reporting() -> Result<ReportingRequest> {
    Ok(ReportingRequest {
        start: date("2026-10-01")?,
        end: date("2026-11-01")?,
        currency: Currency::EUR,
    })
}
// Test-only decoder for the exporter: data cells are quoted, printable fields and
// compact JSON cannot contain physical newlines. Decode independently of csv.rs.
fn decoded(input: &str) -> Result<Vec<BTreeMap<String, String>>> {
    let mut lines = input.lines();
    let header: Vec<_> = lines
        .next()
        .ok_or_else(|| error("CSV header missing"))?
        .split(',')
        .map(str::to_owned)
        .collect();
    let mut records = vec![];
    for line in lines {
        let mut chars = line.chars().peekable();
        let mut fields = vec![];
        while let Some(c) = chars.next() {
            ensure(c == '"', "Expected quoted exported cell")?;
            let mut cell = String::new();
            loop {
                match chars.next() {
                    Some('"') if chars.peek() == Some(&'"') => {
                        chars.next();
                        cell.push('"');
                    }
                    Some('"') => break,
                    Some(c) => cell.push(c),
                    None => return Err(error("Unclosed exported cell")),
                }
            }
            fields.push(cell);
            if let Some(c) = chars.next() {
                ensure(c == ',', "Expected exported separator")?;
            }
        }
        ensure(
            fields.len() == header.len(),
            "Exported row/header column mismatch",
        )?;
        records.push(header.iter().cloned().zip(fields).collect());
    }
    Ok(records)
}
fn field<'a>(row: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    row.get(name)
        .map(String::as_str)
        .ok_or_else(|| error(format!("Missing export field {name}")))
}
fn credit(e: &mut Engine, id: &str, original: &str, quantity: i64) -> Result<()> {
    run(
        e,
        "=credit-owner \"quoted\",name",
        Role::Analyst,
        Command::CreditCreate {
            proposal: CreditProposal {
                id: id.into(),
                original_transaction_id: original.into(),
                effective_date: date("2027-02-01")?,
                reason: "=synthetic reason, with \"quotes\"".into(),
                quantity: Quantity(quantity),
            },
        },
    )?;
    let before = e.state().clone();
    assert!(export_credit(e.state(), id).is_err());
    assert_eq!(e.state(), &before);
    run(
        e,
        "=credit-owner \"quoted\",name",
        Role::Analyst,
        Command::CreditSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    run(
        e,
        "@credit-finance",
        Role::Finance,
        Command::CreditDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Independent Finance review".into(),
        },
    )
}

#[test]
fn mixed_booking_credit_csv_preserves_whole_fx_zero_fields_and_safe_text() -> TestResult {
    let mut e = Engine::memory();
    run(&mut e, "seed", Role::Admin, Command::SeedDemo)?;
    run(
        &mut e,
        "rate-owner",
        Role::Analyst,
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: "RATES".into(),
                name: "Synthetic independent conversion".into(),
                entries: vec![ConversionEntry {
                    id: "USD-EUR".into(),
                    conversion: ConversionKind::Fx {
                        from: Currency::USD,
                        to: Currency::EUR,
                    },
                    start: date("2026-01-01")?,
                    end: date("2027-01-01")?,
                    ratio: Rational {
                        numerator: 2,
                        denominator: 3,
                    },
                }],
            },
        },
    )?;
    run(
        &mut e,
        "rate-owner",
        Role::Analyst,
        Command::ConversionSubmit {
            id: "RATES".into(),
            version: 1,
        },
    )?;
    run(
        &mut e,
        "rate-reviewer",
        Role::Approver,
        Command::ConversionDecide {
            id: "RATES".into(),
            version: 2,
            approve: true,
            note: "Reviewed direct rate".into(),
        },
    )?;
    for (id, invoice, cost) in [("A-RAW", 2, 1), ("Z-UNTOUCHED", 50, 20)] {
        run(
            &mut e,
            "importer",
            Role::Analyst,
            Command::TransactionAdd(Transaction {
                id: id.into(),
                source: "Synthetic review".into(),
                product_id: "VLV-100".into(),
                customer_id: "C-ATLAS".into(),
                date: date("2026-10-13")?,
                quantity: Quantity(3000),
                unit: "each".into(),
                currency: Currency::USD,
                invoice_total: Money(invoice),
                cost_total: Money(cost),
            }),
        )?;
    }
    run(
        &mut e,
        "seller",
        Role::Sales,
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic native quote".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    run(
        &mut e,
        "seller",
        Role::Sales,
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: PriceRequest {
                product_id: "VLV-100".into(),
                customer_id: "C-ATLAS".into(),
                date: date("2026-10-13")?,
                quantity: Quantity(1000),
                discount: Bps(0),
                rebate: Bps(0),
                source: None,
                conversion: None,
            },
        },
    )?;
    run(
        &mut e,
        "seller",
        Role::Sales,
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 2,
        },
    )?;
    run(
        &mut e,
        "quote-reviewer",
        Role::Approver,
        Command::QuoteDecide {
            id: "Q".into(),
            version: 3,
            approve: true,
            note: "Independent approved terms".into(),
        },
    )?;
    run(
        &mut e,
        "booker",
        Role::Analyst,
        Command::QuoteBook {
            request: BookingRequest {
                id: "B".into(),
                quote_id: "Q".into(),
                quote_version: 4,
                invoice_date: date("2026-10-15")?,
                source: "Synthetic booking".into(),
            },
        },
    )?;
    run(
        &mut e,
        "reporter",
        Role::Analyst,
        Command::ReportConvert {
            id: "OLD".into(),
            request: reporting()?,
        },
    )?;
    let old = export_report(e.state(), "OLD")?;
    assert!(old.lines().next().is_some_and(
        |h| h.ends_with("booking_evidence_json") && !h.contains("credit_evidence_json")
    ));
    let old_json = serde_json::to_value(
        e.state()
            .conversion_reports
            .get("OLD")
            .ok_or("Old report missing")?,
    )?;
    for row in old_json["calculation"]["rows"]
        .as_array()
        .ok_or("Rows missing")?
    {
        assert!(row.get("credits").is_none());
    }
    for g in old_json["calculation"]["groups"]
        .as_array()
        .ok_or("Groups missing")?
    {
        for key in [
            "credit_count",
            "fully_credited_count",
            "credited_invoice",
            "credited_cost",
        ] {
            assert!(g.get(key).is_none());
        }
    }
    let old_analytics = serde_json::to_value(analytics(e.state())?)?;
    for key in [
        "credit_count",
        "fully_credited_count",
        "credited_invoice",
        "credited_cost",
    ] {
        assert!(old_analytics[0].get(key).is_none());
    }
    credit(&mut e, "C-RAW", "A-RAW", 1000)?;
    credit(&mut e, "C-BOOK", "B-L001", 500)?;
    run(
        &mut e,
        "reporter",
        Role::Analyst,
        Command::ReportConvert {
            id: "NEW".into(),
            request: reporting()?,
        },
    )?;
    let frozen = e.state().clone();
    let csv = export_report(e.state(), "NEW")?;
    assert!(csv.lines().next().is_some_and(|h| {
        h.ends_with("booking_evidence_json,credit_evidence_json") && h.contains("recorded_currency")
    }));
    let rows = decoded(&csv)?;
    assert_eq!(rows.len(), 3);
    for row in &rows {
        let credits: Value = serde_json::from_str(field(row, "credit_evidence_json")?)?;
        match field(row, "invoice_id")? {
            "A-RAW" => {
                assert_eq!(field(row, "booking_evidence_json")?, "null");
                assert_eq!(credits[0]["invoice"], 0);
                assert_eq!(credits[0]["cost"], 0);
                assert_eq!(field(row, "net_invoice")?, "0.01");
            }
            "B-L001" => {
                let booking: Value = serde_json::from_str(field(row, "booking_evidence_json")?)?;
                assert_eq!(booking["booking_id"], "B");
                assert_eq!(credits[0]["invoice"], 3334); // 1/2 of whole converted 6667, not FX of 5000.
                assert_eq!(credits[0]["cost"], 2000);
                assert_eq!(field(row, "net_invoice")?, "33.33");
                assert_eq!(field(row, "net_cost")?, "20.00");
            }
            "Z-UNTOUCHED" => {
                assert_eq!(field(row, "booking_evidence_json")?, "null");
                assert_eq!(credits, serde_json::json!([]));
                assert_eq!(field(row, "net_invoice")?, "0.33");
            }
            other => return Err(error(format!("Unexpected row {other}")).into()),
        }
    }
    let g = analytics(e.state())?.remove(0);
    assert_eq!(
        (g.transaction_count, g.credit_count, g.fully_credited_count),
        (3, 2, 0)
    );
    let credit_csv = decoded(&export_credit(e.state(), "C-BOOK")?)?;
    let row = credit_csv.first().ok_or("Credit export row missing")?;
    assert_eq!(field(row, "owner")?, "'=credit-owner \"quoted\",name");
    assert_eq!(field(row, "finance_reviewer")?, "'@credit-finance");
    let evidence: Credit = serde_json::from_str(field(row, "evidence_json")?)?;
    assert_eq!(Some(&evidence), e.state().credits.get("C-BOOK"));
    assert_eq!(export_report(e.state(), "OLD")?, old);
    assert_eq!(
        serde_json::to_value(
            e.state()
                .conversion_reports
                .get("OLD")
                .ok_or("Old report missing")?
        )?,
        old_json
    );
    assert!(export_credit(e.state(), "MISSING").is_err());
    assert!(export_period_close(e.state(), "MISSING").is_err());
    assert_eq!(e.state(), &frozen);

    run(
        &mut e,
        "+close-owner,quoted",
        Role::Analyst,
        Command::PeriodCloseCreate {
            terms: PeriodCloseTerms {
                id: "CLOSE".into(),
                start: date("2026-10-01")?,
                end: date("2026-11-01")?,
                currency: Currency::USD,
                reason: "=nominal close, no posting".into(),
            },
        },
    )?;
    assert!(export_period_close(e.state(), "CLOSE").is_err());
    run(
        &mut e,
        "+close-owner,quoted",
        Role::Analyst,
        Command::PeriodCloseSubmit {
            id: "CLOSE".into(),
            version: 1,
        },
    )?;
    run(
        &mut e,
        "=close-finance",
        Role::Finance,
        Command::PeriodCloseDecide {
            id: "CLOSE".into(),
            version: 2,
            approve: true,
            note: "Independent review".into(),
        },
    )?;
    let before = e.state().clone();
    let close_csv = decoded(&export_period_close(e.state(), "CLOSE")?)?;
    let row = close_csv.first().ok_or("Close row missing")?;
    assert_eq!(field(row, "owner")?, "'+close-owner,quoted");
    assert_eq!(field(row, "finance_reviewer")?, "'=close-finance");
    let evidence: PeriodClose = serde_json::from_str(field(row, "evidence_json")?)?;
    assert_eq!(Some(&evidence), e.state().period_closes.get("CLOSE"));
    assert_eq!(e.state(), &before);
    e.state().validate()?;
    Ok(())
}
