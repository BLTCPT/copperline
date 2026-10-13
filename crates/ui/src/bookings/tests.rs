use super::*;
use copperline_money::Rational;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> std::io::Result<Self> {
        let p = std::env::temp_dir().join(format!(
            "copperline-native-booking-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p)?;
        Ok(Self(p))
    }
}
impl Drop for Directory {
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
fn quote(e: &Engine) -> Result<&Quote> {
    e.state()
        .quotes
        .get("Q-BOOK")
        .ok_or_else(|| error("Missing fixture quote"))
}
fn fixture(e: &mut Engine, converted: bool) -> Result<()> {
    e.run(actor("seed-author", Role::Admin), Command::SeedDemo)?;
    let p = Product {
        id: "PENNY".into(),
        name: "Synthetic rounding unit".into(),
        family: "Review".into(),
        currency: Currency::USD,
        unit: "each".into(),
        list_price: Money(1),
    };
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::ProductPut(p.clone()),
    )?;
    e.run(
        actor("catalog-author", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: p.id.clone(),
            version: 1,
            effective_from: Date::parse("2026-01-01")?,
            currency: Currency::USD,
            unit: "each".into(),
            amount: Money(i64::from(converted)),
        }),
    )?;
    if converted {
        let mut entries = vec![];
        for (id, conversion, numerator, denominator) in [
            (
                "USD-EUR",
                ConversionKind::Fx {
                    from: Currency::USD,
                    to: Currency::EUR,
                },
                2,
                1,
            ),
            (
                "HALF-EACH",
                ConversionKind::Uom {
                    product_id: p.id.clone(),
                    from_unit: "half".into(),
                    to_unit: "each".into(),
                },
                1,
                2,
            ),
        ] {
            entries.push(ConversionEntry {
                id: id.into(),
                conversion,
                start: Date::parse("2026-01-01")?,
                end: Date::parse("2027-01-01")?,
                ratio: Rational {
                    numerator,
                    denominator,
                },
            });
        }
        e.run(
            actor("rate-author", Role::Analyst),
            Command::ConversionCreate {
                terms: ConversionTerms {
                    id: "BOOK-RATES".into(),
                    name: "Synthetic exact rates".into(),
                    entries,
                },
            },
        )?;
        e.run(
            actor("rate-author", Role::Analyst),
            Command::ConversionSubmit {
                id: "BOOK-RATES".into(),
                version: 1,
            },
        )?;
        e.run(
            actor("rate-reviewer", Role::Approver),
            Command::ConversionDecide {
                id: "BOOK-RATES".into(),
                version: 2,
                approve: true,
                note: "Independent rate review".into(),
            },
        )?;
    }
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q-BOOK".into(),
            title: "Synthetic booking fixture".into(),
            customer_id: "C-ATLAS".into(),
            date: Date::parse("2026-10-13")?,
        },
    )?;
    for quantity in if converted {
        vec![1000, 3000]
    } else {
        vec![2000]
    } {
        let version = quote(e)?.version;
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteAddLine {
                id: "Q-BOOK".into(),
                version,
                request: PriceRequest {
                    product_id: p.id.clone(),
                    customer_id: "C-ATLAS".into(),
                    date: Date::parse("2026-10-13")?,
                    quantity: Quantity(quantity),
                    discount: Bps(0),
                    rebate: Bps(0),
                    source: None,
                    conversion: converted.then(|| ConversionRequest {
                        currency: Currency::EUR,
                        unit: "half".into(),
                    }),
                },
            },
        )?;
    }
    let version = quote(e)?.version;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q-BOOK".into(),
            version,
        },
    )?;
    for (name, role) in [
        ("commercial-reviewer", Role::Approver),
        ("finance-reviewer", Role::Finance),
    ] {
        if quote(e)?.status == Status::Pending {
            let version = quote(e)?.version;
            e.run(
                actor(name, role),
                Command::QuoteDecide {
                    id: "Q-BOOK".into(),
                    version,
                    approve: true,
                    note: "Independent frozen quote review".into(),
                },
            )?;
        }
    }
    ensure(
        quote(e)?.status == Status::Approved,
        "Fixture quote must be approved",
    )?;
    // Booking must never recalculate against this later catalog change.
    let mut later = p;
    later.list_price = Money(999);
    e.run(
        actor("later-catalog-author", Role::Analyst),
        Command::ProductPut(later),
    )?;
    Ok(())
}

#[test]
fn native_booking_role_date_export_and_durable_reopen() -> TestResult {
    let dir = Directory::new()?;
    let workspace = dir.0.join("workspace");
    let export = dir.0.join("booking.csv");
    let mut e = Engine::open(&workspace)?;
    fixture(&mut e, false)?;
    let mut app = Workbench::new(e);
    app.view = View::Quotes;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 1800.0))
        .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
    h.get_by_label("Book approved quote").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(h.state().view == View::Bookings);
    assert!(
        h.get_by_label("Book entire approved quote")
            .accesskit_node()
            .is_disabled()
    );
    let before = h.state().engine.state().clone();
    h.get_by_label("Book entire approved quote").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert_eq!(h.state().engine.state(), &before);
    h.state_mut().role = 1;
    h.state_mut().booking_form.invoice_date = "2026-10-12".into();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(
        h.get_by_label("Book entire approved quote")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label("Book entire approved quote").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert_eq!(h.state().engine.state(), &before);
    h.state_mut().booking_form.invoice_date = "2026-10-14".into();
    h.state_mut().booking_form.id = "BK-NATIVE".into();
    h.state_mut().booking_form.source = "synthetic-native-review".into();
    h.try_run().map_err(|e| e.to_string())?;
    h.get_by_label("Book entire approved quote").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    let b = h
        .state()
        .engine
        .state()
        .quote_bookings
        .get("BK-NATIVE")
        .ok_or_else(|| error("Native booking missing"))?;
    assert_eq!(b.lines.len(), 1);
    let line = b
        .lines
        .first()
        .ok_or_else(|| error("Native line missing"))?;
    assert_eq!(line.booked.invoice_total, Money(2));
    assert_eq!(line.booked.currency, Currency::USD);
    assert_eq!(line.booked.quantity, Quantity(2000));
    assert_eq!(line.booked.date, Date::parse("2026-10-14")?);
    assert_eq!(line.reconciliation.invoice, Money(0));
    assert_eq!(line.reconciliation.cost, Money(0));
    assert!(h.query_by_label("Book entire approved quote").is_none());
    let frozen = h.state().engine.state().clone();
    let csv = copperline_engine::export_booking(&frozen, "BK-NATIVE")?;
    h.state_mut().booking_form.export_path = export.to_string_lossy().into_owned();
    h.try_run().map_err(|e| e.to_string())?;
    h.get_by_label("Export booking CSV").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    assert_eq!(fs::read_to_string(&export)?, csv);
    h.get_by_label("Export booking CSV").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(h.state().failed);
    assert_eq!(fs::read_to_string(&export)?, csv);
    assert_eq!(h.state().engine.state(), &frozen);
    drop(h);
    let mut reopened = Engine::open(&workspace)?;
    assert_eq!(reopened.state(), &frozen);
    assert_eq!(
        copperline_engine::export_booking(reopened.state(), "BK-NATIVE")?,
        csv
    );
    // A pending preparation must not displace immutable evidence on a normal 960px view.
    let request = quote(&reopened)?
        .lines
        .first()
        .ok_or_else(|| error("Native quote line missing"))?
        .request
        .clone();
    reopened.run(
        actor("next-seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q-NEXT".into(),
            title: "Another eligible synthetic quote".into(),
            customer_id: request.customer_id.clone(),
            date: request.date.clone(),
        },
    )?;
    reopened.run(
        actor("next-seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q-NEXT".into(),
            version: 1,
            request,
        },
    )?;
    reopened.run(
        actor("next-seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q-NEXT".into(),
            version: 2,
        },
    )?;
    reopened.run(
        actor("next-reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q-NEXT".into(),
            version: 3,
            approve: true,
            note: "Independent review".into(),
        },
    )?;
    let before_navigation = reopened.state().clone();
    let mut app = Workbench::new(reopened);
    app.view = View::Bookings;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 960.0))
        .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
    assert!(h.query_by_label("Book entire approved quote").is_none());
    assert!(h.get_by_label("Recorded invoice / cost").rect().max.y < 900.0);
    assert!(h.get_by_label("BK-NATIVE-L001\nPENNY").rect().max.y < 900.0);
    h.state_mut().view = View::Quotes;
    h.state_mut().selected_quote = "Q-NEXT".into();
    h.try_run().map_err(|e| e.to_string())?;
    h.get_by_label("Book approved quote").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(h.query_by_label("Book entire approved quote").is_some());
    assert_eq!(h.state().booking_form.quote_id, "Q-NEXT");
    assert_eq!(h.state().engine.state(), &before_navigation);
    Ok(())
}

#[test]
fn native_converted_full_booking_rounding_reversal_and_report() -> TestResult {
    let mut e = Engine::memory();
    fixture(&mut e, true)?;
    let mut app = Workbench::new(e);
    app.view = View::Bookings;
    app.role = 1;
    app.booking_form.id = "BK-CONVERTED".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 1800.0))
        .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
    h.get_by_label("Book entire approved quote").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    let b = h
        .state()
        .engine
        .state()
        .quote_bookings
        .get("BK-CONVERTED")
        .ok_or_else(|| error("Converted booking missing"))?
        .clone();
    assert_eq!(b.lines.len(), 2);
    assert_eq!(h.state().engine.state().transactions.len(), 2);
    for (line, (quantity, total, native_quantity, native_total)) in
        b.lines.iter().zip([(1000, 1, 500, 1), (3000, 3, 1500, 2)])
    {
        assert_eq!(line.booked.currency, Currency::EUR);
        assert_eq!(line.booked.unit, "half");
        assert_eq!(line.booked.quantity, Quantity(quantity));
        assert_eq!(line.booked.invoice_total, Money(total));
        assert_eq!(line.booked.cost_total, Money(total));
        assert_eq!(line.native_basis.currency, Currency::USD);
        assert_eq!(line.native_basis.unit, "each");
        assert_eq!(line.native_basis.quantity, Quantity(native_quantity));
        assert_eq!(line.native_basis.invoice_total, Money(native_total));
        assert_eq!(line.native_basis.cost_total, Money(native_total));
        assert_eq!(line.reconciliation.invoice, Money(-1));
        assert_eq!(line.reconciliation.cost, Money(-1));
    }
    h.get_by_label("Full reversal · BK-CONVERTED-L001").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(h.state().view == View::Corrections);
    assert!(
        h.get_by_label("Replace with a corrected invoice atomically")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label("Replace with a corrected invoice atomically")
        .click();
    h.try_run().map_err(|e| e.to_string())?;
    h.get_by_label("Create correction proposal").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    let correction = h
        .state()
        .engine
        .state()
        .corrections
        .values()
        .next()
        .ok_or_else(|| error("Reversal proposal missing"))?
        .clone();
    assert_eq!(
        correction.proposal.original_transaction_id,
        "BK-CONVERTED-L001"
    );
    assert!(correction.proposal.replacement.is_none());
    h.get_by_label("Submit correction").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    h.state_mut().role = 4;
    h.try_run().map_err(|e| e.to_string())?;
    h.get_by_label("Approve correction").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    assert_eq!(
        h.state()
            .engine
            .state()
            .corrections
            .get(&correction.proposal.id)
            .map(|c| c.status),
        Some(RecordStatus::Approved)
    );
    assert_eq!(
        h.state().engine.state().quote_bookings.get("BK-CONVERTED"),
        Some(&b)
    );
    h.state_mut().view = View::Bookings;
    h.state_mut().role = 1;
    h.try_run().map_err(|e| e.to_string())?;
    assert!(h.query_by_label("Book entire approved quote").is_none());
    h.state_mut().view = View::Analytics;
    h.try_run().map_err(|e| e.to_string())?;
    h.get_by_label("Freeze converted actuals").click();
    h.try_run().map_err(|e| e.to_string())?;
    assert!(!h.state().failed, "{}", h.state().status);
    let report = h
        .state()
        .engine
        .state()
        .conversion_reports
        .values()
        .next()
        .ok_or_else(|| error("Converted report missing"))?;
    assert_eq!(report.calculation.rows.len(), 2);
    for row in &report.calculation.rows {
        assert_eq!(row.native.currency, Currency::EUR);
        assert_eq!(row.native.unit, "half");
        assert_eq!(
            row.booking.as_ref().map(|b| b.booking_id.as_str()),
            Some("BK-CONVERTED")
        );
    }
    let group = report
        .calculation
        .groups
        .first()
        .ok_or_else(|| error("Report group missing"))?;
    assert_eq!(group.invoice, Money(3));
    assert_eq!(group.cost, Money(3));
    assert_eq!(group.reversal_count, 1);
    h.state().engine.state().validate()?;
    Ok(())
}
