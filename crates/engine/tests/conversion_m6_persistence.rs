//! Independent M6 history, provenance, rollback and legacy-shape regression tests.
use copperline_engine::{Command, Engine, Request, evaluate, export_quote, export_report};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity, Rational};
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
fn date(value: &str) -> Result<Date> {
    Date::parse(value)
}
fn initialize(e: &mut Engine) -> Result<()> {
    e.run(actor("Admin", Role::Admin), Command::SeedDemo)?;
    Ok(())
}
fn fixture() -> Result<Engine> {
    let mut e = Engine::memory();
    initialize(&mut e)?;
    Ok(e)
}
fn terms(id: &str) -> Result<ConversionTerms> {
    Ok(ConversionTerms {
        id: id.into(),
        name: "Synthetic direct dated factors".into(),
        entries: vec![
            ConversionEntry {
                id: "FX".into(),
                conversion: ConversionKind::Fx {
                    from: Currency::USD,
                    to: Currency::EUR,
                },
                start: date("2026-01-01")?,
                end: date("2027-01-01")?,
                ratio: Rational {
                    numerator: 9,
                    denominator: 10,
                },
            },
            ConversionEntry {
                id: "UOM".into(),
                conversion: ConversionKind::Uom {
                    product_id: "VLV-100".into(),
                    from_unit: "box".into(),
                    to_unit: "each".into(),
                },
                start: date("2026-01-01")?,
                end: date("2027-01-01")?,
                ratio: Rational {
                    numerator: 12,
                    denominator: 1,
                },
            },
        ],
    })
}
fn create(e: &mut Engine, terms: ConversionTerms) -> Result<()> {
    e.run(
        actor("Book owner", Role::Analyst),
        Command::ConversionCreate { terms },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("Book owner", Role::Analyst),
        Command::ConversionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    Ok(())
}
fn decide(e: &mut Engine, id: &str, approve: bool) -> Result<()> {
    e.run(
        actor("Book reviewer", Role::Approver),
        Command::ConversionDecide {
            id: id.into(),
            version: 2,
            approve,
            note: "Independent synthetic schedule review".into(),
        },
    )?;
    Ok(())
}
fn approve(e: &mut Engine, terms: ConversionTerms) -> Result<()> {
    let id = terms.id.clone();
    create(e, terms)?;
    submit(e, &id)?;
    decide(e, &id, true)
}
fn price(converted: bool) -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(1000),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
        conversion: converted.then(|| ConversionRequest {
            currency: Currency::EUR,
            unit: "box".into(),
        }),
    })
}
fn quote(e: &mut Engine, request: PriceRequest, approve: bool) -> Result<Request> {
    e.run(
        actor("Seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic frozen conversion".into(),
            customer_id: request.customer_id.clone(),
            date: request.date.clone(),
        },
    )?;
    let replay = Request {
        request_id: "quote-line-once".into(),
        expected_revision: e.state().revision,
        actor: actor("Seller", Role::Sales),
        action: Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request,
        },
    };
    e.execute(replay.clone())?;
    if approve {
        e.run(
            actor("Seller", Role::Sales),
            Command::QuoteSubmit {
                id: "Q".into(),
                version: 2,
            },
        )?;
        e.run(
            actor("Quote reviewer", Role::Approver),
            Command::QuoteDecide {
                id: "Q".into(),
                version: 3,
                approve: true,
                note: "Independent synthetic quote review".into(),
            },
        )?;
    }
    Ok(replay)
}
fn invoice(id: &str, day: &str, revenue: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "synthetic-m6-review".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date(day)?,
        quantity: Quantity(1000),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(revenue),
        cost_total: Money(revenue / 2),
    })
}
fn add_invoice(e: &mut Engine, id: &str, day: &str, revenue: i64) -> Result<()> {
    e.run(
        actor("Invoice writer", Role::Analyst),
        Command::TransactionAdd(invoice(id, day, revenue)?),
    )?;
    Ok(())
}
fn report_request() -> Result<ReportingRequest> {
    Ok(ReportingRequest {
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
        currency: Currency::EUR,
    })
}
fn report(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("Report writer", Role::Analyst),
        Command::ReportConvert {
            id: id.into(),
            request: report_request()?,
        },
    )?;
    Ok(())
}
fn reverse(e: &mut Engine, id: &str, invoice_id: &str) -> Result<()> {
    e.run(
        actor("Correction writer", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: id.into(),
                original_transaction_id: invoice_id.into(),
                effective_date: date("2026-11-14")?,
                reason: "Synthetic correction dated after invoice".into(),
                replacement: None,
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
        actor("Finance", Role::Finance),
        Command::CorrectionDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Independent synthetic reversal review".into(),
        },
    )?;
    Ok(())
}

#[test]
fn book_terms_owner_review_and_lifecycle_remain_bound_to_exact_audits() -> TestResult {
    let mut e = fixture()?;
    approve(&mut e, terms("BOOK")?)?;
    for case in 0..19 {
        let mut invalid = e.state().clone();
        let b = invalid
            .conversion_books
            .get_mut("BOOK")
            .ok_or("Missing book")?;
        match case {
            0 => b.terms.name = "Changed rationale".into(),
            1 => b.owner = "Other owner".into(),
            2 => b.terms.entries[0].ratio.numerator += 1,
            3 => b.terms.entries[0].ratio.denominator += 1,
            4 => {
                b.terms.entries[0].conversion = ConversionKind::Fx {
                    from: Currency::EUR,
                    to: Currency::USD,
                }
            }
            5 => {
                b.terms.entries[1].conversion = ConversionKind::Uom {
                    product_id: "VLV-100".into(),
                    from_unit: "carton".into(),
                    to_unit: "each".into(),
                }
            }
            6 => b.terms.entries[0].start = date("2026-02-01")?,
            7 => b.terms.entries[0].end = date("2027-02-01")?,
            8 => b.terms.entries[0].id = "OTHER".into(),
            9 => b.terms.entries.reverse(),
            10 => b.review.as_mut().ok_or("Missing review")?.note = "Edited note".into(),
            11 => b.review.as_mut().ok_or("Missing review")?.actor.name = "Other reviewer".into(),
            12 => b.review.as_mut().ok_or("Missing review")?.actor.role = Role::Admin,
            13 => b.status = RecordStatus::Rejected,
            14 => b.submitted_revision = None,
            15 => b.version = u64::MAX,
            16 => b.created_revision += 1,
            17 => {
                let r = b.review.as_ref().ok_or("Missing review")?.revision;
                invalid
                    .audit
                    .iter_mut()
                    .find(|a| a.revision == r)
                    .ok_or("Missing audit")?
                    .target = Some(conversion_target("OTHER"));
            }
            _ => {
                let r = b.created_revision;
                invalid
                    .audit
                    .iter_mut()
                    .find(|a| a.revision == r)
                    .ok_or("Missing audit")?
                    .actor
                    .role = Role::Sales;
            }
        }
        assert!(invalid.validate().is_err(), "Accepted book mutation {case}");
    }
    Ok(())
}

#[test]
fn detached_or_deleted_conversion_and_report_records_fail_closed() -> TestResult {
    let mut e = fixture()?;
    approve(&mut e, terms("BOOK")?)?;
    add_invoice(&mut e, "T", "2026-10-13", 10_000)?;
    report(&mut e, "R")?;
    for prefix in ["conversion:", "report:"] {
        for case in 0..4 {
            let mut invalid = e.state().clone();
            if case < 2 {
                if prefix == "conversion:" {
                    invalid.conversion_books.clear();
                } else {
                    invalid.conversion_reports.clear();
                }
            }
            for a in invalid
                .audit
                .iter_mut()
                .filter(|a| a.target.as_deref().is_some_and(|t| t.starts_with(prefix)))
            {
                match case {
                    1 => a.command = "customer_put".into(),
                    2 => a.target = None,
                    3 => a.target = Some(format!("{prefix}UNRELATED")),
                    _ => {}
                }
            }
            assert!(
                invalid.validate().is_err(),
                "Accepted orphan {prefix} case {case}"
            );
        }
    }
    Ok(())
}

#[test]
fn overlapping_pending_books_recheck_at_approval_and_preserve_owner_and_role_boundaries()
-> TestResult {
    let mut e = fixture()?;
    create(&mut e, terms("A")?)?;
    let before = e.state().clone();
    for action in [
        Command::ConversionSubmit {
            id: "A".into(),
            version: 1,
        },
        Command::ConversionWithdraw {
            id: "A".into(),
            version: 1,
        },
    ] {
        assert!(e.run(actor("Other Admin", Role::Admin), action).is_err());
        assert_eq!(e.state(), &before);
    }
    create(&mut e, terms("B")?)?;
    submit(&mut e, "A")?;
    submit(&mut e, "B")?;
    let before = e.state().clone();
    for who in [
        actor("Book owner", Role::Approver),
        actor("Finance", Role::Finance),
        actor("Admin", Role::Admin),
    ] {
        assert!(
            e.run(
                who,
                Command::ConversionDecide {
                    id: "A".into(),
                    version: 2,
                    approve: true,
                    note: "Wrong reviewer".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    decide(&mut e, "A", true)?;
    let before = e.state().clone();
    assert!(decide(&mut e, "B", true).is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "B", false)?;
    let mut adjacent = terms("ADJACENT")?;
    for row in &mut adjacent.entries {
        row.start = date("2027-01-01")?;
        row.end = date("2028-01-01")?;
    }
    approve(&mut e, adjacent)?;
    create(&mut e, terms("WITHDRAW")?)?;
    submit(&mut e, "WITHDRAW")?;
    e.run(
        actor("Book owner", Role::Analyst),
        Command::ConversionWithdraw {
            id: "WITHDRAW".into(),
            version: 2,
        },
    )?;
    e.state().validate()?;
    Ok(())
}

#[test]
fn converted_quote_rejects_direction_factor_version_and_native_basis_tampering() -> TestResult {
    let mut e = fixture()?;
    approve(&mut e, terms("BOOK")?)?;
    quote(&mut e, price(true)?, false)?;
    for case in 0..16 {
        let mut invalid = e.state().clone();
        let line = invalid
            .quotes
            .get_mut("Q")
            .ok_or("Missing quote")?
            .lines
            .first_mut()
            .ok_or("Missing line")?;
        let c = line.conversion.as_mut().ok_or("Missing conversion")?;
        match case {
            0 => c.fx.as_mut().ok_or("Missing FX")?.book_id = "OTHER".into(),
            1 => c.fx.as_mut().ok_or("Missing FX")?.book_version += 1,
            2 => c.fx.as_mut().ok_or("Missing FX")?.entry.ratio.numerator += 1,
            3 => {
                c.fx.as_mut().ok_or("Missing FX")?.entry.conversion = ConversionKind::Fx {
                    from: Currency::EUR,
                    to: Currency::USD,
                }
            }
            4 => c.fx.as_mut().ok_or("Missing FX")?.terms_fingerprint = "0".repeat(64),
            5 => c.fx.as_mut().ok_or("Missing FX")?.approval_revision -= 1,
            6 => c.uom.as_mut().ok_or("Missing UOM")?.entry.ratio.numerator += 1,
            7 => c.native_quantity.0 += 1,
            8 => c.native_list_unit.0 += 1,
            9 => c.native_rule_unit.0 += 1,
            10 => c.native_cost_unit.0 += 1,
            11 => c.native_currency = Currency::GBP,
            12 => c.native_unit = "carton".into(),
            13 => c.captured_revision += 1,
            14 => c.fx = None,
            _ => line.request.conversion = None,
        }
        assert!(
            invalid.validate().is_err(),
            "Accepted converted line mutation {case}"
        );
    }
    let line = &e.state().quotes.get("Q").ok_or("Missing quote")?.lines[0];
    assert_eq!(line.invoice_unit, Money(108_000));
    assert_eq!(
        line.conversion.as_ref().map(|c| c.native_quantity),
        Some(Quantity(12_000))
    );
    Ok(())
}

#[test]
fn frozen_quotes_and_reports_survive_future_schedules_and_historical_reversals() -> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    initialize(&mut e)?;
    let mut first = terms("FIRST")?;
    for row in &mut first.entries {
        row.end = date("2026-11-01")?;
    }
    approve(&mut e, first)?;
    quote(&mut e, price(true)?, true)?;
    add_invoice(&mut e, "T1", "2026-10-13", 10_001)?;
    report(&mut e, "BEFORE")?;
    let old_quote = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let old_report = e
        .state()
        .conversion_reports
        .get("BEFORE")
        .cloned()
        .ok_or("Missing report")?;
    let quote_csv = export_quote(&old_quote)?;
    let report_csv = export_report(e.state(), "BEFORE")?;
    let mut future = terms("FUTURE")?;
    for row in &mut future.entries {
        row.start = date("2026-11-01")?;
        row.ratio.numerator *= 2;
    }
    approve(&mut e, future)?;
    add_invoice(&mut e, "T2", "2026-11-13", 20_000)?;
    reverse(&mut e, "VOID", "T1")?;
    report(&mut e, "AFTER")?;
    let after = &e
        .state()
        .conversion_reports
        .get("AFTER")
        .ok_or("Missing report")?
        .calculation;
    assert_eq!(after.rows.len(), 2);
    let first_row = after
        .rows
        .iter()
        .find(|r| r.native.id == "T1")
        .ok_or("Missing original")?;
    assert_eq!(first_row.invoice, Money(9001));
    assert_eq!(first_row.cost, Money(4500));
    assert_eq!(first_row.reversed_by.as_deref(), Some("VOID"));
    assert_eq!(
        first_row.fx.as_ref().map(|r| r.book_id.as_str()),
        Some("FIRST")
    );
    let group = after.groups.first().ok_or("Missing group")?;
    assert_eq!(group.gross_invoice, Money(45_001));
    assert_eq!(group.reversed_invoice, Money(9001));
    assert_eq!(group.invoice, Money(36_000));
    assert_eq!(group.cost, Money(18_000));
    assert_eq!(group.margin_bps, Some(5000));
    assert_eq!(e.state().quotes.get("Q"), Some(&old_quote));
    assert_eq!(
        e.state().conversion_reports.get("BEFORE"),
        Some(&old_report)
    );
    assert_eq!(
        export_quote(e.state().quotes.get("Q").ok_or("Missing quote")?)?,
        quote_csv
    );
    assert_eq!(export_report(e.state(), "BEFORE")?, report_csv);
    let expected = e.state().clone();
    drop(e);
    let e = Engine::open(&dir.0)?;
    assert_eq!(e.state(), &expected);
    assert_eq!(export_report(e.state(), "BEFORE")?, report_csv);
    Ok(())
}

#[test]
fn report_native_rows_rates_totals_owner_and_source_revision_are_immutable() -> TestResult {
    let mut e = fixture()?;
    approve(&mut e, terms("BOOK")?)?;
    add_invoice(&mut e, "T", "2026-10-13", 10_000)?;
    report(&mut e, "R")?;
    for case in 0..14 {
        let mut invalid = e.state().clone();
        let report = invalid
            .conversion_reports
            .get_mut("R")
            .ok_or("Missing report")?;
        match case {
            0 => report.owner = "Other writer".into(),
            1 => report.calculation.source_revision -= 1,
            2 => report.created_revision -= 1,
            3 => report.calculation.rows[0].native.invoice_total.0 += 1,
            4 => report.calculation.rows[0].native.date = date("2026-10-14")?,
            5 => report.calculation.rows[0].reversed_by = Some("FAKE".into()),
            6 => {
                report.calculation.rows[0]
                    .fx
                    .as_mut()
                    .ok_or("Missing FX")?
                    .entry
                    .ratio
                    .numerator += 1
            }
            7 => {
                report.calculation.rows[0]
                    .fx
                    .as_mut()
                    .ok_or("Missing FX")?
                    .captured_revision -= 1
            }
            8 => report.calculation.rows[0].fx = None,
            9 => report.calculation.rows[0].invoice.0 += 1,
            10 => report.calculation.groups[0].margin_bps = Some(0),
            11 => report.calculation.rows.clear(),
            12 => report.calculation.request.start = date("2026-01-02")?,
            _ => {
                let revision = report.created_revision;
                invalid
                    .audit
                    .iter_mut()
                    .find(|a| a.revision == revision)
                    .ok_or("Missing report audit")?
                    .actor
                    .role = Role::Sales;
            }
        }
        assert!(
            invalid.validate().is_err(),
            "Accepted report mutation {case}"
        );
    }
    Ok(())
}

#[test]
fn reporting_never_infers_inverse_or_uses_rates_approved_after_capture() -> TestResult {
    let mut e = fixture()?;
    add_invoice(&mut e, "T", "2026-10-13", 10_000)?;
    let before = e.state().clone();
    assert!(report(&mut e, "NO-RATE").is_err());
    assert_eq!(e.state(), &before);
    let mut inverse = terms("INVERSE")?;
    inverse.entries.truncate(1);
    inverse.entries[0].conversion = ConversionKind::Fx {
        from: Currency::EUR,
        to: Currency::USD,
    };
    approve(&mut e, inverse)?;
    let before = e.state().clone();
    assert!(report(&mut e, "NO-INVERSE").is_err());
    assert_eq!(e.state(), &before);
    let prior_revision = e.state().revision;
    approve(&mut e, terms("DIRECT")?)?;
    assert!(
        e.state()
            .converted_actuals(&report_request()?, prior_revision)
            .is_err()
    );
    report(&mut e, "DIRECT-REPORT")?;
    let key = ConversionKind::Fx {
        from: Currency::USD,
        to: Currency::EUR,
    };
    assert!(
        e.state()
            .conversion_rate_at(&key, &date("2027-01-01")?, e.state().revision)
            .is_err()
    );
    assert!(
        e.state()
            .conversion_rate_at(&key, &date("2025-12-31")?, e.state().revision)
            .is_err()
    );
    Ok(())
}

const HEADER: &str = "id,kind,product_id,from,to,start,end,numerator,denominator\n";
#[test]
fn conversion_csv_is_atomic_for_invalid_factors_duplicates_overlap_and_bounds() -> TestResult {
    let mut e = fixture()?;
    let valid = "R,fx,,USD,EUR,2026-01-01,2027-01-01,9,10\n";
    let invalid_rows = [
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,0,1\n",
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,1,0\n",
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,-1,1\n",
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,1000000001,1\n",
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,1.0,1\n",
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,9223372036854775808,1\n",
        "S,fx,,USD,USD,2027-01-01,2028-01-01,1,1\n",
        "S,fx,VLV-100,USD,EUR,2027-01-01,2028-01-01,1,1\n",
        "S,uom,VLV-100,box,missing-native,2027-01-01,2028-01-01,1,1\n",
        "S,fx,,USD,EUR,2027-01-01,2027-01-01,1,1\n",
        "R,fx,,USD,EUR,2027-01-01,2028-01-01,1,1\n",
        "S,fx,,USD,EUR,2026-12-31,2028-01-01,1,1\n",
        "S,fx,,USD,EUR,2027-01-01,2028-01-01,1,1,extra\n",
        "\"unterminated",
    ];
    let before = e.state().clone();
    for row in invalid_rows {
        let action = Command::ConversionImport {
            id: "CSV".into(),
            name: "Synthetic invalid batch".into(),
            csv: format!("{HEADER}{valid}{row}"),
        };
        assert!(
            e.run(actor("Book owner", Role::Analyst), action).is_err(),
            "Accepted {row}"
        );
        assert_eq!(e.state(), &before);
    }
    let too_many = (0..101)
        .map(|n| format!("R{n},uom,VLV-100,unit{n},each,2026-01-01,2027-01-01,1,1\n"))
        .collect::<String>();
    for csv in [format!("{HEADER}{too_many}"), "x".repeat(1_048_577)] {
        assert!(
            e.run(
                actor("Book owner", Role::Analyst),
                Command::ConversionImport {
                    id: "CSV".into(),
                    name: "Synthetic bound".into(),
                    csv
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    e.run(
        actor("Book owner", Role::Analyst),
        Command::ConversionImport {
            id: "CSV".into(),
            name: "Synthetic valid quoted BOM batch".into(),
            csv: format!(
                "\u{feff}{HEADER}\"R\",fx,,USD,EUR,2026-01-01,2027-01-01,\"9\",\"10\"\r\n"
            ),
        },
    )?;
    submit(&mut e, "CSV")?;
    decide(&mut e, "CSV", true)?;
    assert_eq!(
        e.state()
            .conversion_books
            .get("CSV")
            .ok_or("Missing book")?
            .terms
            .entries
            .len(),
        1
    );
    Ok(())
}

#[test]
fn quantity_precision_and_overflow_reject_without_partial_quote_mutation() -> TestResult {
    for (numerator, denominator, quantity) in [
        (1, 3, 1000),
        (1_000_000_000, 1, 1000),
        (12, 1, 1_000_000_000),
    ] {
        let mut e = fixture()?;
        let mut book = terms("BOOK")?;
        book.entries[1].ratio = Rational {
            numerator,
            denominator,
        };
        approve(&mut e, book)?;
        e.run(
            actor("Seller", Role::Sales),
            Command::QuoteCreate {
                id: "Q".into(),
                title: "Synthetic bounds".into(),
                customer_id: "C-ATLAS".into(),
                date: date("2026-10-13")?,
            },
        )?;
        let mut request = price(true)?;
        request.quantity = Quantity(quantity);
        let before = e.state().clone();
        assert!(
            e.run(
                actor("Seller", Role::Sales),
                Command::QuoteAddLine {
                    id: "Q".into(),
                    version: 1,
                    request
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-m6-persistence-{}-{}",
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
fn book_and_report_failed_commits_preserve_memory_evidence_and_exact_retry() -> TestResult {
    for is_report in [false, true] {
        let dir = Workspace::new()?;
        let mut e = Engine::open(&dir.0)?;
        initialize(&mut e)?;
        if is_report {
            approve(&mut e, terms("BOOK")?)?;
            add_invoice(&mut e, "T", "2026-10-13", 10_000)?;
        }
        let request = Request {
            request_id: "m6-once".into(),
            expected_revision: e.state().revision,
            actor: actor("Owner", Role::Analyst),
            action: if is_report {
                Command::ReportConvert {
                    id: "R".into(),
                    request: report_request()?,
                }
            } else {
                Command::ConversionCreate {
                    terms: terms("BOOK")?,
                }
            },
        };
        let before = e.state().clone();
        let pending = format!("{:020}.pending", before.revision + 1);
        let bytes = b"synthetic interrupted M6 evidence";
        fs::write(dir.0.join(&pending), bytes)?;
        assert!(e.execute(request.clone()).is_err());
        assert_eq!(e.state(), &before);
        assert_eq!(fs::read(dir.0.join(&pending))?, bytes);
        drop(e);
        let (store, actual) = copperline_storage::Store::open(&dir.0)?;
        assert_eq!(actual, before);
        let preserved =
            store.quarantine_pending(&pending, &copperline_storage::fingerprint(bytes))?;
        drop(store);
        let mut e = Engine::open(&dir.0)?;
        let result = e.execute(request.clone())?;
        assert!(!result.replayed);
        assert_eq!(result.revision, before.revision + 1);
        let committed = e.state().clone();
        drop(e);
        let mut e = Engine::open(&dir.0)?;
        assert_eq!(e.state(), &committed);
        assert!(e.execute(request.clone())?.replayed);
        let mut altered = request;
        altered.actor.name = "Changed owner".into();
        assert!(e.execute(altered).is_err());
        assert_eq!(e.state(), &committed);
        assert_eq!(fs::read(preserved.preserved_path)?, bytes);
    }
    Ok(())
}

fn approved_contract(e: &mut Engine) -> Result<()> {
    e.run(
        actor("Contract writer", Role::Analyst),
        Command::ContractCreate {
            terms: ContractTerms {
                id: "C".into(),
                edition: 1,
                name: "Synthetic M5 native agreement".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::USD,
                start: date("2026-01-01")?,
                end: date("2027-01-01")?,
                priority: 10,
                rows: vec![ContractRow {
                    id: "R".into(),
                    product_id: "VLV-100".into(),
                    unit: "each".into(),
                    min_quantity: Quantity(1000),
                    max_quantity: None,
                    basis: ContractBasis::Fixed {
                        invoice_unit: Money(9500),
                    },
                }],
            },
        },
    )?;
    e.run(
        actor("Contract writer", Role::Analyst),
        Command::ContractSubmit {
            id: "C".into(),
            edition: 1,
            version: 1,
        },
    )?;
    e.run(
        actor("Contract reviewer", Role::Approver),
        Command::ContractDecide {
            id: "C".into(),
            edition: 1,
            version: 2,
            approve: true,
            note: "Independent M5 approval".into(),
        },
    )?;
    Ok(())
}

#[test]
fn m5_shaped_contract_quote_bytes_and_retry_fingerprints_survive_m6_and_reopen() -> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    initialize(&mut e)?;
    approved_contract(&mut e)?;
    let mut request = price(false)?;
    request.source = Some(PriceSource::CustomerContract);
    let replay = quote(&mut e, request, true)?;
    let quote = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let csv = export_quote(&quote)?;
    let old_revision = e.state().revision;
    drop(e);
    let mut original = Vec::new();
    for n in 0..=old_revision {
        let path = dir.0.join(format!("{n:020}.json"));
        let bytes = fs::read(&path)?;
        let envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert!(envelope["state"].get("conversion_books").is_none());
        assert!(envelope["state"].get("conversion_reports").is_none());
        assert!(!std::str::from_utf8(&bytes)?.contains("\"conversion\":"));
        original.push((path, bytes));
    }
    assert!(!serde_json::to_string(&replay)?.contains("\"conversion\":"));
    let mut e = Engine::open(&dir.0)?;
    approve(&mut e, terms("BOOK")?)?;
    add_invoice(&mut e, "T", "2026-10-13", 10_000)?;
    report(&mut e, "R")?;
    assert!(e.execute(replay.clone())?.replayed);
    assert_eq!(e.state().quotes.get("Q"), Some(&quote));
    assert_eq!(
        export_quote(e.state().quotes.get("Q").ok_or("Missing quote")?)?,
        csv
    );
    for (path, bytes) in original {
        assert_eq!(fs::read(path)?, bytes);
    }
    let expected = e.state().clone();
    drop(e);
    let mut e = Engine::open(&dir.0)?;
    assert_eq!(e.state(), &expected);
    assert!(e.execute(replay)?.replayed);
    assert_eq!(e.state(), &expected);
    let mut converted = price(true)?;
    converted.source = Some(PriceSource::CustomerContract);
    assert_eq!(
        evaluate(e.state(), &converted)?.invoice_unit,
        Money(102_600)
    );
    Ok(())
}

#[test]
fn maximum_ids_fit_fingerprinted_audit_and_report_count_is_bounded() -> TestResult {
    let mut e = fixture()?;
    let id = "C".repeat(64);
    let mut book = terms(&id)?;
    book.name = "N".repeat(160);
    approve(&mut e, book)?;
    report(&mut e, &"R".repeat(64))?;
    for n in 1..100 {
        report(&mut e, &format!("R{n}"))?;
    }
    let before = e.state().clone();
    assert!(report(&mut e, "OVER-LIMIT").is_err());
    assert_eq!(e.state(), &before);
    e.state().validate()?;
    Ok(())
}
