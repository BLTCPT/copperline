//! Independent command, arithmetic and persistence review of whole-quote booking.
use copperline_engine::{Command, Engine, Request, analytics, export_booking};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity, Rational};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-booking-review-{}-{}",
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
fn files(path: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut values = vec![];
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|v| v == "json") {
            values.push((
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path())?,
            ));
        }
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(values)
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
fn seed(e: &mut Engine) -> Result<()> {
    e.run(actor("seed", Role::Admin), Command::SeedDemo)?;
    e.run(
        actor("catalog", Role::Analyst),
        Command::ProductPut(Product {
            id: "P-ROUND".into(),
            name: "Synthetic rounding product".into(),
            family: "Review".into(),
            unit: "each".into(),
            currency: Currency::USD,
            list_price: Money(101),
        }),
    )?;
    e.run(
        actor("catalog", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "P-ROUND".into(),
            version: 1,
            effective_from: date("2026-01-01")?,
            amount: Money(67),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    Ok(())
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    seed(&mut e)?;
    Ok(e)
}
fn rate(id: &str, conversion: ConversionKind, n: i64, d: i64) -> Result<ConversionEntry> {
    Ok(ConversionEntry {
        id: id.into(),
        conversion,
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
        ratio: Rational {
            numerator: n,
            denominator: d,
        },
    })
}
fn fx(id: &str, from: Currency, to: Currency, n: i64, d: i64) -> Result<ConversionEntry> {
    rate(id, ConversionKind::Fx { from, to }, n, d)
}
fn uom(unit: &str, n: i64, d: i64) -> Result<ConversionEntry> {
    rate(
        unit,
        ConversionKind::Uom {
            product_id: "P-ROUND".into(),
            from_unit: unit.into(),
            to_unit: "each".into(),
        },
        n,
        d,
    )
}
fn rates(e: &mut Engine, id: &str, entries: Vec<ConversionEntry>) -> Result<()> {
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: id.into(),
                name: "Synthetic rates".into(),
                entries,
            },
        },
    )?;
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("rate-reviewer", Role::Approver),
        Command::ConversionDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Independent review".into(),
        },
    )?;
    Ok(())
}
fn price() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "P-ROUND".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(1000),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
        conversion: None,
    })
}
fn converted_price() -> Result<PriceRequest> {
    let mut p = price()?;
    p.discount = Bps(1234);
    p.rebate = Bps(1000);
    p.conversion = Some(ConversionRequest {
        currency: Currency::EUR,
        unit: "box".into(),
    });
    Ok(p)
}
fn q<'a>(e: &'a Engine, id: &str) -> Result<&'a Quote> {
    e.state()
        .quotes
        .get(id)
        .ok_or_else(|| error("Missing test quote"))
}
fn draft(e: &mut Engine, id: &str, requests: Vec<PriceRequest>) -> Result<()> {
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: id.into(),
            title: "Synthetic booking review".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    for request in requests {
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteAddLine {
                id: id.into(),
                version: q(e, id)?.version,
                request,
            },
        )?;
    }
    Ok(())
}
fn submit(e: &mut Engine, id: &str) -> Result<()> {
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: id.into(),
            version: q(e, id)?.version,
        },
    )?;
    Ok(())
}
fn review(e: &mut Engine, id: &str, name: &str, role: Role) -> Result<()> {
    e.run(
        actor(name, role),
        Command::QuoteDecide {
            id: id.into(),
            version: q(e, id)?.version,
            approve: true,
            note: "Independent quote review".into(),
        },
    )?;
    Ok(())
}
fn approve(e: &mut Engine, id: &str) -> Result<()> {
    submit(e, id)?;
    review(e, id, "commercial-reviewer", Role::Approver)?;
    if q(e, id)?.status == Status::Pending {
        review(e, id, "finance-reviewer", Role::Finance)?;
    }
    ensure(q(e, id)?.status == Status::Approved, "Quote not approved")
}
fn booking(e: &Engine, id: &str, quote_id: &str) -> Result<Command> {
    Ok(Command::QuoteBook {
        request: BookingRequest {
            id: id.into(),
            quote_id: quote_id.into(),
            quote_version: q(e, quote_id)?.version,
            invoice_date: date("2026-11-01")?,
            source: "synthetic-booking-test".into(),
        },
    })
}
fn book(e: &mut Engine, id: &str, quote_id: &str) -> Result<()> {
    e.run(actor("booker", Role::Analyst), booking(e, id, quote_id)?)?;
    Ok(())
}
fn record<'a>(e: &'a Engine, id: &str) -> Result<&'a QuoteBooking> {
    e.state()
        .quote_bookings
        .get(id)
        .ok_or_else(|| error("Missing booking"))
}
fn reject(e: &mut Engine, who: Actor, command: Command) {
    let before = e.state().clone();
    assert!(e.run(who, command).is_err());
    assert_eq!(e.state(), &before);
}
fn invoice(id: &str) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "synthetic-native-test".into(),
        product_id: "P-ROUND".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-11-01")?,
        quantity: Quantity(1000),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(101),
        cost_total: Money(67),
    })
}
fn correction(
    e: &mut Engine,
    id: &str,
    original: &str,
    replacement: Option<Transaction>,
) -> Result<()> {
    e.run(
        actor("correction-author", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: id.into(),
                original_transaction_id: original.into(),
                effective_date: date("2026-11-02")?,
                reason: "Synthetic full reversal review".into(),
                replacement,
            },
        },
    )?;
    Ok(())
}
fn reverse(e: &mut Engine, id: &str, original: &str) -> Result<()> {
    correction(e, id, original, None)?;
    e.run(
        actor("correction-author", Role::Analyst),
        Command::CorrectionSubmit {
            id: id.into(),
            version: 1,
        },
    )?;
    e.run(
        actor("correction-reviewer", Role::Finance),
        Command::CorrectionDecide {
            id: id.into(),
            version: 2,
            approve: true,
            note: "Independent full reversal".into(),
        },
    )?;
    Ok(())
}
fn reporting(currency: Currency) -> Result<ReportingRequest> {
    Ok(ReportingRequest {
        start: date("2026-01-01")?,
        end: date("2027-01-01")?,
        currency,
    })
}
fn converted_quote(e: &mut Engine, id: &str) -> Result<()> {
    rates(
        e,
        "RATES",
        vec![
            fx("USD-EUR", Currency::USD, Currency::EUR, 2, 3)?,
            uom("box", 3, 1)?,
        ],
    )?;
    draft(e, id, vec![converted_price()?])?;
    approve(e, id)
}

#[test]
fn booking_uses_frozen_booked_cents_and_keeps_native_basis_nonposted() -> TestResult {
    let mut e = demo()?;
    converted_quote(&mut e, "Q")?;
    let frozen = q(&e, "Q")?.clone();
    let mut changed = e.state().products["P-ROUND"].clone();
    changed.list_price = Money(900);
    e.run(
        actor("catalog", Role::Analyst),
        Command::ProductPut(changed),
    )?;
    e.run(
        actor("catalog", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "P-ROUND".into(),
            version: 2,
            effective_from: date("2026-10-01")?,
            amount: Money(800),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    book(&mut e, "B", "Q")?;
    assert_eq!(q(&e, "Q")?, &frozen);
    let b = record(&e, "B")?;
    assert_eq!(b.lines.len(), 1);
    let l = &b.lines[0];
    assert_eq!(l.index, 0);
    assert_eq!(l.booked.id, "B-L001");
    assert_eq!(l.booked.invoice_total, Money(177));
    assert_eq!(l.booked.cost_total, Money(134));
    assert_eq!(l.booked.currency, Currency::EUR);
    assert_eq!(l.booked.unit, "box");
    assert_eq!(l.booked.quantity, Quantity(1000));
    assert_eq!(l.native_basis.currency, Currency::USD);
    assert_eq!(l.native_basis.unit, "each");
    assert_eq!(l.native_basis.quantity, Quantity(3000));
    assert_eq!(l.native_basis.invoice_unit, Money(89));
    assert_eq!(l.native_basis.invoice_total, Money(267));
    assert_eq!(l.native_basis.cost_total, Money(201));
    assert_eq!(l.reconciliation.invoice, Money(-1));
    assert_eq!(l.reconciliation.cost, Money(0));
    assert_eq!(e.state().transactions.len(), 1);
    assert_eq!(
        e.state().transaction_revisions["B-L001"],
        b.created_revision
    );
    assert_eq!(analytics(e.state())?[0].invoice, Money(177));
    assert_ne!(frozen.lines[0].pocket_total, l.booked.invoice_total);
    let report = e
        .state()
        .converted_actuals(&reporting(Currency::EUR)?, e.state().revision)?;
    assert_eq!(report.groups[0].invoice, Money(177));
    assert!(report.rows[0].fx.is_none());
    assert!(!export_booking(e.state(), "B")?.is_empty());
    Ok(())
}

#[test]
fn same_currency_uom_rounding_uses_booked_actuals_instead_of_native_basis() -> TestResult {
    let mut e = demo()?;
    rates(&mut e, "THIRDS", vec![uom("third", 1, 3)?])?;
    let mut p = price()?;
    p.quantity = Quantity(3000);
    p.conversion = Some(ConversionRequest {
        currency: Currency::USD,
        unit: "third".into(),
    });
    draft(&mut e, "Q", vec![p])?;
    approve(&mut e, "Q")?;
    book(&mut e, "B", "Q")?;
    let b = record(&e, "B")?;
    assert_eq!(b.lines[0].booked.invoice_total, Money(102));
    assert_eq!(b.lines[0].booked.cost_total, Money(66));
    assert_eq!(b.lines[0].native_basis.invoice_total, Money(101));
    assert_eq!(b.lines[0].native_basis.cost_total, Money(67));
    assert_eq!(b.lines[0].reconciliation.invoice, Money(1));
    assert_eq!(b.lines[0].reconciliation.cost, Money(-1));
    let report = e
        .state()
        .converted_actuals(&reporting(Currency::USD)?, e.state().revision)?;
    assert_eq!(report.groups[0].invoice, Money(102));
    assert_eq!(report.groups[0].cost, Money(66));
    assert!(report.rows[0].fx.is_none());
    Ok(())
}

#[test]
fn booking_requires_current_complete_approval_version_date_and_actuals_role() -> TestResult {
    let mut e = demo()?;
    e.run(
        actor("admin", Role::Admin),
        Command::QuoteApprovalPolicySet {
            expected_version: 0,
            thresholds: ApprovalThresholds {
                max_discount: Bps(0),
                min_margin: Bps(0),
                usd_value: Money(1),
                eur_value: Money(1),
                gbp_value: Money(1),
            },
        },
    )?;
    draft(&mut e, "Q", vec![price()?])?;
    let command = booking(&e, "DRAFT", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    submit(&mut e, "Q")?;
    review(&mut e, "Q", "commercial-reviewer", Role::Approver)?;
    assert_eq!(q(&e, "Q")?.status, Status::Pending);
    let command = booking(&e, "PENDING", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    review(&mut e, "Q", "finance-reviewer", Role::Finance)?;
    for role in [Role::Sales, Role::Approver, Role::Finance] {
        let command = booking(&e, "WRONG-ROLE", "Q")?;
        reject(&mut e, actor("seller", role), command);
    }
    let Command::QuoteBook { request } = booking(&e, "B", "Q")? else {
        return Err("Unexpected command".into());
    };
    let mut stale = request.clone();
    stale.quote_version -= 1;
    reject(
        &mut e,
        actor("booker", Role::Analyst),
        Command::QuoteBook { request: stale },
    );
    let mut early = request;
    early.invoice_date = date("2026-10-12")?;
    reject(
        &mut e,
        actor("booker", Role::Analyst),
        Command::QuoteBook { request: early },
    );
    // A previous approved cycle must not be enough when a newer draft exists.
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: q(&e, "Q")?.version,
        },
    )?;
    let command = booking(&e, "OLD-CYCLE", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    submit(&mut e, "Q")?;
    let command = booking(&e, "OLD-CYCLE-PENDING", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    review(&mut e, "Q", "commercial-reviewer", Role::Approver)?;
    review(&mut e, "Q", "finance-reviewer", Role::Finance)?;
    book(&mut e, "B", "Q")?;
    assert_eq!(e.state().quote_bookings.len(), 1);
    assert!(record(&e, "B")?.quote_contributors.contains("seller"));
    Ok(())
}

#[test]
fn later_reporting_rate_cannot_backfill_a_booking_or_loosen_raw_native_ingestion() -> TestResult {
    let mut e = demo()?;
    converted_quote(&mut e, "Q")?;
    book(&mut e, "B", "Q")?;
    let booked_revision = record(&e, "B")?.created_revision;
    assert!(
        e.state()
            .converted_actuals(&reporting(Currency::USD)?, e.state().revision)
            .is_err()
    );
    rates(
        &mut e,
        "LATE-RETURN",
        vec![fx("EUR-USD", Currency::EUR, Currency::USD, 3, 2)?],
    )?;
    assert!(
        e.state()
            .converted_actuals(&reporting(Currency::USD)?, e.state().revision)
            .is_err()
    );
    assert!(
        e.state()
            .converted_actuals(&reporting(Currency::USD)?, booked_revision - 1)?
            .rows
            .is_empty()
    );
    let mut spoof = record(&e, "B")?.lines[0].booked.clone();
    spoof.id = "RAW-CONVERTED".into();
    reject(
        &mut e,
        actor("booker", Role::Analyst),
        Command::TransactionAdd(spoof),
    );
    reject(&mut e, actor("booker", Role::Analyst), Command::TransactionsImport {
        csv: "id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total\nRAW,synthetic,P-ROUND,C-ATLAS,2026-11-01,1,box,EUR,1.77,1.34\n".into(),
    });
    assert_eq!(e.state().transactions.len(), 1);
    Ok(())
}

#[test]
fn approved_reporting_cents_reverse_exactly_and_booking_capacity_never_reopens() -> TestResult {
    let mut e = demo()?;
    converted_quote(&mut e, "Q")?;
    rates(
        &mut e,
        "RETURN",
        vec![fx("EUR-USD", Currency::EUR, Currency::USD, 3, 2)?],
    )?;
    book(&mut e, "B", "Q")?;
    let revision = record(&e, "B")?.created_revision;
    let request = reporting(Currency::USD)?;
    let actuals = e.state().converted_actuals(&request, e.state().revision)?;
    assert_eq!(actuals.groups[0].invoice, Money(266));
    assert_eq!(actuals.groups[0].cost, Money(201));
    assert_eq!(
        actuals.rows[0]
            .fx
            .as_ref()
            .ok_or("Missing FX")?
            .captured_revision,
        revision - 1
    );
    assert_ne!(
        actuals.groups[0].invoice,
        record(&e, "B")?.lines[0].native_basis.invoice_total
    );
    e.run(
        actor("report-author", Role::Analyst),
        Command::ReportConvert {
            id: "BEFORE".into(),
            request: request.clone(),
        },
    )?;
    let frozen = e.state().conversion_reports["BEFORE"].clone();
    let before = e.state().clone();
    assert!(
        correction(
            &mut e,
            "REPLACE",
            "B-L001",
            Some(invoice("RAW-REPLACEMENT")?)
        )
        .is_err()
    );
    assert_eq!(e.state(), &before);
    reverse(&mut e, "REVERSE", "B-L001")?;
    let after = e.state().converted_actuals(&request, e.state().revision)?;
    assert_eq!(after.groups[0].gross_invoice, Money(266));
    assert_eq!(after.groups[0].reversed_invoice, Money(266));
    assert_eq!(after.groups[0].invoice, Money(0));
    assert_eq!(after.groups[0].cost, Money(0));
    assert_eq!(e.state().conversion_reports["BEFORE"], frozen);
    let command = booking(&e, "AGAIN", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: q(&e, "Q")?.version,
        },
    )?;
    approve(&mut e, "Q")?;
    let command = booking(&e, "NEW-EDITION", "Q")?;
    reject(&mut e, actor("booker", Role::Admin), command);
    assert_eq!(e.state().transactions.len(), 1);
    assert_eq!(e.state().quote_bookings.len(), 1);
    e.state().validate()?;
    Ok(())
}

#[test]
fn generated_ids_respect_raw_and_correction_reservations_without_partial_rows() -> TestResult {
    let mut e = demo()?;
    draft(&mut e, "Q", vec![price()?, price()?])?;
    approve(&mut e, "Q")?;
    e.run(
        actor("ingester", Role::Analyst),
        Command::TransactionAdd(invoice("ORIGINAL")?),
    )?;
    correction(&mut e, "RESERVED", "ORIGINAL", Some(invoice("B-L002")?))?;
    let command = booking(&e, "B", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    assert!(!e.state().transactions.contains_key("B-L001"));
    e.run(
        actor("correction-author", Role::Analyst),
        Command::CorrectionWithdraw {
            id: "RESERVED".into(),
            version: 1,
        },
    )?;
    book(&mut e, "B", "Q")?;
    assert_eq!(record(&e, "B")?.lines.len(), 2);
    assert_eq!(e.state().transactions.len(), 3);
    draft(&mut e, "Q2", vec![price()?, price()?])?;
    approve(&mut e, "Q2")?;
    e.run(
        actor("ingester", Role::Analyst),
        Command::TransactionAdd(invoice("X-L002")?),
    )?;
    let command = booking(&e, "X", "Q2")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    assert!(!e.state().transactions.contains_key("X-L001"));
    Ok(())
}

#[test]
fn failed_booking_commit_retries_once_and_frozen_export_survives_quote_revision() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    seed(&mut e)?;
    converted_quote(&mut e, "Q")?;
    let before = e.state().clone();
    let previous_files = files(&workspace.0)?;
    let request = Request {
        request_id: "exact-booking-retry".into(),
        expected_revision: e.state().revision,
        actor: actor("booker", Role::Analyst),
        action: booking(&e, "B", "Q")?,
    };
    let pending = workspace
        .0
        .join(format!("{:020}.pending", before.revision + 1));
    fs::write(&pending, b"preserve interrupted write")?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert_eq!(files(&workspace.0)?, previous_files);
    assert_eq!(fs::read(&pending)?, b"preserve interrupted write");
    fs::remove_file(pending)?;
    let outcome = e.execute(request.clone())?;
    assert!(!outcome.replayed);
    assert_eq!(outcome.revision, before.revision + 1);
    let csv = export_booking(e.state(), "B")?;
    let booking = record(&e, "B")?.clone();
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: q(&e, "Q")?.version,
        },
    )?;
    let state = e.state().clone();
    let committed = files(&workspace.0)?;
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &state);
    assert_eq!(record(&e, "B")?, &booking);
    assert_eq!(export_booking(e.state(), "B")?, csv);
    let replay = e.execute(request.clone())?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, outcome.revision);
    let mut conflict = request;
    if let Command::QuoteBook { request } = &mut conflict.action {
        request.source = "changed source".into();
    }
    assert!(e.execute(conflict).is_err());
    assert_eq!(e.state(), &state);
    assert_eq!(files(&workspace.0)?, committed);
    assert_eq!(export_booking(e.state(), "B")?, csv);
    Ok(())
}

fn rejected_snapshot(state: &State, change: impl FnOnce(&mut Value)) -> TestResult {
    let mut encoded = serde_json::to_value(state)?;
    change(&mut encoded);
    let changed: State = serde_json::from_value(encoded)?;
    assert!(changed.validate().is_err());
    Ok(())
}

#[test]
fn snapshot_cannot_erase_retarget_or_rewrite_booking_evidence() -> TestResult {
    let mut e = demo()?;
    converted_quote(&mut e, "Q")?;
    book(&mut e, "B", "Q")?;
    let state = e.state();
    for (field, replacement) in [
        ("invoice_total", json!(178)),
        ("cost_total", json!(135)),
        ("unit", json!("each")),
        ("currency", json!("USD")),
        ("quantity", json!(3000)),
        ("source", json!("forged source")),
    ] {
        rejected_snapshot(state, |s| {
            s["quote_bookings"]["B"]["lines"][0]["booked"][field] = replacement.clone();
        })?;
        rejected_snapshot(state, |s| {
            s["transactions"]["B-L001"][field] = replacement;
        })?;
    }
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["lines"][0]["native_basis"]["invoice_total"] = json!(266);
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["lines"][0]["reconciliation"]["invoice"] = json!(0);
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["lines"][0]["index"] = json!(1);
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["owner"] = json!("seller");
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["request"]["invoice_date"] = json!("2026-12-01");
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["request"]["quote_id"] = json!("OTHER");
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["quote_contributors"] = json!([]);
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["approval_cycle"]["reviews"][0]["approved"] = json!(false);
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"] = json!({});
    })?;
    rejected_snapshot(state, |s| {
        s["transactions"] = json!({});
        s["transaction_revisions"] = json!({});
    })?;
    rejected_snapshot(state, |s| {
        s["quote_bookings"]["B"]["lines"] = json!([]);
    })?;
    rejected_snapshot(state, |s| {
        s["transaction_revisions"]["B-L001"] = json!(1);
    })?;
    Ok(())
}

#[test]
fn forged_booking_of_a_historical_cycle_after_revision_is_rejected_even_with_new_fingerprint()
-> TestResult {
    let mut e = demo()?;
    draft(&mut e, "Q", vec![price()?])?;
    approve(&mut e, "Q")?;
    book(&mut e, "B", "Q")?;
    let old_revision = record(&e, "B")?.created_revision;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: q(&e, "Q")?.version,
        },
    )?;
    e.run(
        actor("booker", Role::Analyst),
        Command::CustomerPut(Customer {
            id: "C-NEW".into(),
            name: "Synthetic later event".into(),
            segment: "OEM".into(),
        }),
    )?;
    let mut state = e.state().clone();
    let revision = state.revision;
    let b = state.quote_bookings.get_mut("B").ok_or("Missing booking")?;
    b.created_revision = revision;
    let detail = booking_created_detail(b)?;
    state
        .transaction_revisions
        .insert("B-L001".into(), revision);
    let old = state
        .audit
        .iter_mut()
        .find(|a| a.revision == old_revision)
        .ok_or("Missing old event")?;
    old.command = "fixture".into();
    old.target = None;
    old.detail = "Synthetic unrelated earlier event".into();
    let latest = state.audit.last_mut().ok_or("Missing latest event")?;
    latest.command = "quote_book".into();
    latest.target = Some(booking_target("B"));
    latest.detail = detail;
    let failure = state
        .validate()
        .err()
        .ok_or("Accepted historical approval after quote revision")?;
    assert!(
        failure.to_string().contains("current approved version"),
        "{failure}"
    );
    for target in [None, Some(quote_target("OTHER"))] {
        let mut altered = state.clone();
        let intervening_revision = altered
            .audit
            .iter_mut()
            .find(|a| a.command == "quote_revise")
            .ok_or("Missing intervening quote revision")?;
        intervening_revision.target = target.clone();
        let failure = altered
            .validate()
            .err()
            .ok_or("Accepted historical approval after erasing or redirecting revision target")?;
        assert!(
            failure.to_string().contains("current approved version"),
            "Unexpected validation error for target {target:?}: {failure}"
        );
    }
    Ok(())
}

#[test]
fn maximum_booking_and_quote_ids_fit_audits_and_oversized_ids_roll_back() -> TestResult {
    let mut e = demo()?;
    let quote_id = "Q".repeat(64);
    draft(&mut e, &quote_id, vec![price()?])?;
    approve(&mut e, &quote_id)?;
    let too_long = booking(&e, &"B".repeat(60), &quote_id)?;
    reject(&mut e, actor("booker", Role::Analyst), too_long);
    let booking_id = "B".repeat(59);
    book(&mut e, &booking_id, &quote_id)?;
    assert_eq!(record(&e, &booking_id)?.lines[0].booked.id.len(), 64);
    assert!(
        e.state()
            .audit
            .last()
            .is_some_and(|a| a.detail.len() <= 160)
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn booking_invoice_aggregate_overflow_preserves_every_row_and_receipt() -> TestResult {
    let mut e = demo()?;
    draft(&mut e, "Q", vec![price()?, price()?])?;
    approve(&mut e, "Q")?;
    let mut huge = invoice("HUGE")?;
    huge.invoice_total = Money(i64::MAX);
    huge.cost_total = Money(0);
    e.run(
        actor("ingester", Role::Analyst),
        Command::TransactionAdd(huge),
    )?;
    let command = booking(&e, "B", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    assert!(!e.state().transactions.contains_key("B-L001"));
    assert!(!e.state().transactions.contains_key("B-L002"));
    Ok(())
}

#[test]
fn booked_rebate_basis_uses_invoice_currency_and_excludes_quote_authors_from_settlement()
-> TestResult {
    let mut e = demo()?;
    converted_quote(&mut e, "Q")?;
    book(&mut e, "B", "Q")?;
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementCreate {
            terms: AgreementTerms {
                id: "A".into(),
                name: "Synthetic EUR invoice rebate".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::EUR,
                start: date("2026-01-01")?,
                end: date("2027-01-01")?,
                tiers: vec![RebateTier {
                    threshold: Money(0),
                    rate: Bps(1000),
                }],
            },
        },
    )?;
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementSubmit {
            id: "A".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("agreement-reviewer", Role::Approver),
        Command::AgreementDecide {
            id: "A".into(),
            version: 2,
            approve: true,
            note: "Independent agreement review".into(),
        },
    )?;
    let basis = e.state().accrual_basis("A", e.state().revision)?;
    assert_eq!(basis.transaction_ids, vec!["B-L001"]);
    assert_eq!(basis.eligible_invoice, Money(177));
    e.run(
        actor("accrual-author", Role::Analyst),
        Command::RebateAccrue {
            id: "ACCRUAL".into(),
            agreement_id: "A".into(),
            version: 3,
        },
    )?;
    assert_eq!(e.state().accruals["ACCRUAL"].rebate, Money(18));
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementCreate {
            id: "S".into(),
            accrual_id: "ACCRUAL".into(),
            reason: "Synthetic recognition".into(),
        },
    )?;
    let settlement = &e.state().settlements["S"];
    assert!(settlement.contributors.contains("seller"));
    assert!(settlement.contributors.contains("booker"));
    assert_eq!(settlement.adjustment, Money(18));
    e.run(
        actor("settlement-author", Role::Analyst),
        Command::SettlementSubmit {
            id: "S".into(),
            version: 1,
        },
    )?;
    for name in ["seller", "booker"] {
        reject(
            &mut e,
            actor(name, Role::Finance),
            Command::SettlementDecide {
                id: "S".into(),
                version: 2,
                approve: true,
                note: "Cannot approve my source".into(),
            },
        );
    }
    e.run(
        actor("settlement-reviewer", Role::Finance),
        Command::SettlementDecide {
            id: "S".into(),
            version: 2,
            approve: true,
            note: "Independent obligation review".into(),
        },
    )?;
    let frozen = e.state().accruals["ACCRUAL"].clone();
    reverse(&mut e, "REVERSE", "B-L001")?;
    let current = e.state().rebate_position("A", e.state().revision)?;
    assert_eq!(current.basis.eligible_invoice, Money(0));
    assert_eq!(current.remaining_adjustment, Money(-18));
    assert_eq!(e.state().accruals["ACCRUAL"], frozen);
    Ok(())
}

#[test]
fn malformed_booking_revision_returns_reporting_error_without_panicking() -> TestResult {
    let mut e = demo()?;
    converted_quote(&mut e, "Q")?;
    book(&mut e, "B", "Q")?;
    let mut state = e.state().clone();
    state
        .quote_bookings
        .get_mut("B")
        .ok_or("Missing booking")?
        .created_revision = 0;
    let request = reporting(Currency::EUR)?;
    let result = std::panic::catch_unwind(|| state.converted_actuals(&request, state.revision));
    assert!(
        result.is_ok(),
        "Malformed booking revision escaped the Result boundary"
    );
    if let Ok(report) = result {
        assert!(report.is_err());
    }
    Ok(())
}

#[test]
fn small_converted_totals_do_not_hide_native_basis_aggregate_overflow() -> TestResult {
    let mut e = demo()?;
    let mut product = e.state().products["P-ROUND"].clone();
    product.list_price = Money(1_000_000_000_000);
    e.run(
        actor("catalog", Role::Analyst),
        Command::ProductPut(product),
    )?;
    e.run(
        actor("catalog", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "P-ROUND".into(),
            version: 2,
            effective_from: date("2026-01-01")?,
            amount: Money(0),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    rates(
        &mut e,
        "SMALL-FX",
        vec![fx("USD-EUR", Currency::USD, Currency::EUR, 1, 1_000_000)?],
    )?;
    let mut p = price()?;
    p.quantity = Quantity(1_000_000_000);
    p.conversion = Some(ConversionRequest {
        currency: Currency::EUR,
        unit: "each".into(),
    });
    // The seeded volume rule reduces each native total to 920 quadrillion cents;
    // eleven such totals exceed i64 even though the converted quote fits easily.
    draft(&mut e, "Q", vec![p; 11])?;
    approve(&mut e, "Q")?;
    assert_eq!(q(&e, "Q")?.lines[0].invoice_total, Money(920_000_000_000));
    let command = booking(&e, "B", "Q")?;
    reject(&mut e, actor("booker", Role::Analyst), command);
    assert!(e.state().transactions.is_empty());
    assert!(e.state().quote_bookings.is_empty());
    Ok(())
}
