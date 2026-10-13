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
            "copperline-native-m9-{}-{}",
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
fn date(s: &str) -> Result<Date> {
    Date::parse(s)
}
fn invoice(id: &str, qty: i64, invoice: i64, cost: i64) -> Result<Transaction> {
    Ok(Transaction {
        id: id.into(),
        source: "synthetic-native-test".into(),
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(qty),
        unit: "each".into(),
        currency: Currency::USD,
        invoice_total: Money(invoice),
        cost_total: Money(cost),
    })
}
fn seed(e: &mut Engine) -> Result<()> {
    e.run(actor("seed", Role::Admin), Command::SeedDemo)?;
    e.run(
        actor("original-author", Role::Analyst),
        Command::TransactionAdd(invoice("T-THIRDS", 3000, 2, 1)?),
    )?;
    e.run(
        actor("original-author", Role::Analyst),
        Command::TransactionAdd(invoice("T-OPEN", 2000, 100, 60)?),
    )?;
    Ok(())
}
fn render(h: &mut Harness<'_, Workbench>) -> TestResult {
    h.try_run().map_err(|e| e.to_string())?;
    Ok(())
}
fn click(h: &mut Harness<'_, Workbench>, label: &str) -> TestResult {
    h.get_by_label(label).click();
    render(h)?;
    assert!(!h.state().failed, "{}", h.state().status);
    Ok(())
}

#[test]
fn native_credits_exhaust_thirds_then_close_export_and_reopen() -> TestResult {
    let dir = Directory::new()?;
    let workspace = dir.0.join("workspace");
    let mut e = Engine::open(&workspace)?;
    seed(&mut e)?;
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: "REPORT".into(),
                name: "Synthetic reporting".into(),
                entries: vec![ConversionEntry {
                    id: "USD-EUR".into(),
                    conversion: ConversionKind::Fx {
                        from: Currency::USD,
                        to: Currency::EUR,
                    },
                    start: date("2026-10-01")?,
                    end: date("2026-11-01")?,
                    ratio: Rational {
                        numerator: 2,
                        denominator: 1,
                    },
                }],
            },
        },
    )?;
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionSubmit {
            id: "REPORT".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("rate-reviewer", Role::Approver),
        Command::ConversionDecide {
            id: "REPORT".into(),
            version: 2,
            approve: true,
            note: "Independent review".into(),
        },
    )?;
    e.run(
        actor("agreement-author", Role::Analyst),
        Command::AgreementCreate {
            terms: AgreementTerms {
                id: "A-OCT".into(),
                name: "Synthetic original-period rebate".into(),
                customer_id: "C-ATLAS".into(),
                currency: Currency::USD,
                start: date("2026-10-01")?,
                end: date("2026-11-01")?,
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
            id: "A-OCT".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("agreement-reviewer", Role::Approver),
        Command::AgreementDecide {
            id: "A-OCT".into(),
            version: 2,
            approve: true,
            note: "Independent review".into(),
        },
    )?;
    let mut app = Workbench::new(e);
    app.view = View::Credits;
    app.role = 1;
    app.credit_form.invoice = "T-THIRDS".into();
    app.credit_form.id = "CN-1".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 1800.0))
        .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
    for (i, expected_invoice, expected_cost) in [(1, 1, 0), (2, 0, 1), (3, 1, 0)] {
        if i > 1 {
            click(&mut h, "Prepare another quantity credit")?;
        }
        h.state_mut().role = 1;
        h.state_mut().credit_form.id = format!("CN-{i}");
        h.state_mut().credit_form.date = "2026-11-10".into();
        render(&mut h)?;
        let before = h.state().engine.state().clone();
        render(&mut h)?;
        assert_eq!(h.state().engine.state(), &before);
        click(&mut h, "Create credit proposal")?;
        let c = h
            .state()
            .engine
            .state()
            .credits
            .get(&format!("CN-{i}"))
            .ok_or_else(|| error("Credit missing"))?;
        assert_eq!(
            (c.invoice, c.cost),
            (Money(expected_invoice), Money(expected_cost))
        );
        click(&mut h, "Submit credit")?;
        h.state_mut().role = 3;
        render(&mut h)?;
        assert!(
            h.get_by_label("Approve credit")
                .accesskit_node()
                .is_disabled()
        );
        h.state_mut().role = 4;
        render(&mut h)?;
        click(&mut h, "Approve credit")?;
    }
    let groups = copperline_engine::analytics(h.state().engine.state())?;
    let g = groups.first().ok_or_else(|| error("Analytics missing"))?;
    assert_eq!(
        (g.credit_count, g.fully_credited_count, g.transaction_count),
        (3, 1, 1)
    );
    assert_eq!(
        (g.credited_invoice, g.credited_cost, g.invoice, g.cost),
        (Money(2), Money(1), Money(100), Money(60))
    );
    let credit_csv = dir.0.join("credit.csv");
    h.state_mut().credit_form.export_path = credit_csv.to_string_lossy().into_owned();
    render(&mut h)?;
    click(&mut h, "Export credit evidence")?;
    assert_eq!(
        fs::read_to_string(&credit_csv)?,
        copperline_engine::export_credit(h.state().engine.state(), "CN-3")?
    );
    h.state_mut().view = View::Analytics;
    h.state_mut().role = 1;
    render(&mut h)?;
    click(&mut h, "Freeze converted actuals")?;
    let report = h
        .state()
        .engine
        .state()
        .conversion_reports
        .values()
        .next()
        .ok_or_else(|| error("Report missing"))?;
    let row = report
        .calculation
        .rows
        .iter()
        .find(|r| r.native.id == "T-THIRDS")
        .ok_or_else(|| error("Reporting row missing"))?;
    assert_eq!(
        row.credits
            .iter()
            .map(|c| (c.invoice, c.cost))
            .collect::<Vec<_>>(),
        vec![
            (Money(1), Money(1)),
            (Money(2), Money(0)),
            (Money(1), Money(1))
        ]
    );
    h.state_mut().view = View::Settlements;
    render(&mut h)?;
    click(&mut h, "Calculate corrected accrual")?;
    let accrual = h
        .state()
        .engine
        .state()
        .accruals
        .values()
        .next()
        .ok_or_else(|| error("Accrual missing"))?;
    assert_eq!(accrual.credit_ids, vec!["CN-1", "CN-2", "CN-3"]);
    assert_eq!(accrual.eligible_invoice, Money(100));
    h.state_mut().view = View::PeriodCloses;
    h.state_mut().period_close_form.id = "PC-OCT".into();
    render(&mut h)?;
    click(&mut h, "Create period close proposal")?;
    click(&mut h, "Submit period close")?;
    h.state_mut().role = 3;
    render(&mut h)?;
    assert!(
        h.get_by_label("Approve period close")
            .accesskit_node()
            .is_disabled()
    );
    h.state_mut().role = 4;
    render(&mut h)?;
    click(&mut h, "Approve period close")?;
    let closed = h
        .state()
        .engine
        .state()
        .period_closes
        .get("PC-OCT")
        .ok_or_else(|| error("Close missing"))?;
    assert_eq!(closed.status, RecordStatus::Approved);
    assert_eq!(closed.basis.credit_ids.len(), 3);
    assert_eq!(
        (closed.basis.invoice, closed.basis.cost),
        (Money(100), Money(60))
    );
    let close_csv = dir.0.join("close.csv");
    h.state_mut().period_close_form.export_path = close_csv.to_string_lossy().into_owned();
    render(&mut h)?;
    click(&mut h, "Export period close evidence")?;
    assert_eq!(
        fs::read_to_string(&close_csv)?,
        copperline_engine::export_period_close(h.state().engine.state(), "PC-OCT")?
    );
    h.state_mut().open_quantity_credit("T-OPEN");
    h.state_mut().role = 1;
    render(&mut h)?;
    h.state_mut().credit_form.date = "2026-12-31".into();
    render(&mut h)?;
    assert!(
        h.get_by_label("Create credit proposal")
            .accesskit_node()
            .is_disabled()
    );
    let frozen = h.state().engine.state().clone();
    h.get_by_label("Create credit proposal").click();
    render(&mut h)?;
    assert_eq!(h.state().engine.state(), &frozen);
    drop(h);
    let reopened = Engine::open(&workspace)?;
    assert_eq!(reopened.state(), &frozen);
    reopened.state().validate()?;
    Ok(())
}

#[test]
fn native_close_staleness_credit_withdrawal_rejection_and_empty_period() -> TestResult {
    let mut e = Engine::memory();
    seed(&mut e)?;
    let mut app = Workbench::new(e);
    app.view = View::PeriodCloses;
    app.role = 1;
    app.period_close_form.id = "PC-STALE".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 1800.0))
        .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
    click(&mut h, "Create period close proposal")?;
    h.state_mut().open_quantity_credit("T-THIRDS");
    render(&mut h)?;
    h.state_mut().credit_form.id = "CN-RESERVE".into();
    render(&mut h)?;
    click(&mut h, "Create credit proposal")?;
    h.state_mut().view = View::PeriodCloses;
    render(&mut h)?;
    assert!(
        h.get_by_label("Submit period close")
            .accesskit_node()
            .is_disabled()
    );
    h.state_mut().view = View::Credits;
    render(&mut h)?;
    h.state_mut().role = 3;
    render(&mut h)?;
    assert!(
        h.get_by_label("Withdraw credit")
            .accesskit_node()
            .is_disabled()
    );
    h.state_mut().role = 1;
    render(&mut h)?;
    click(&mut h, "Withdraw credit")?;
    h.state_mut().view = View::PeriodCloses;
    render(&mut h)?;
    click(&mut h, "Submit period close")?;
    h.state_mut().engine.run(
        actor("late-author", Role::Analyst),
        Command::TransactionAdd(invoice("T-LATE-ZERO", 1000, 0, 0)?),
    )?;
    h.state_mut().role = 4;
    render(&mut h)?;
    assert!(
        h.get_by_label("Approve period close")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        !h.get_by_label("Reject period close")
            .accesskit_node()
            .is_disabled()
    );
    click(&mut h, "Reject period close")?;
    assert_eq!(
        h.state()
            .engine
            .state()
            .period_closes
            .get("PC-STALE")
            .map(|c| c.status),
        Some(RecordStatus::Rejected)
    );
    h.state_mut().open_quantity_credit("T-THIRDS");
    h.state_mut().role = 1;
    render(&mut h)?;
    h.state_mut().credit_form.id = "CN-REJECT".into();
    render(&mut h)?;
    click(&mut h, "Create credit proposal")?;
    click(&mut h, "Submit credit")?;
    h.state_mut().role = 4;
    render(&mut h)?;
    click(&mut h, "Reject credit")?;
    assert_eq!(
        h.state()
            .engine
            .state()
            .credits
            .get("CN-REJECT")
            .map(|c| c.status),
        Some(RecordStatus::Rejected)
    );
    h.state().engine.state().validate()?;

    h.state_mut().view = View::PeriodCloses;
    h.state_mut().role = 1;
    render(&mut h)?;
    click(&mut h, "Prepare another period close")?;
    h.state_mut().period_close_form.id = "PC-WITHDRAW".into();
    render(&mut h)?;
    click(&mut h, "Create period close proposal")?;
    h.state_mut().role = 3;
    render(&mut h)?;
    assert!(
        h.get_by_label("Withdraw period close")
            .accesskit_node()
            .is_disabled()
    );
    h.state_mut().role = 1;
    render(&mut h)?;
    click(&mut h, "Withdraw period close")?;
    assert_eq!(
        h.state()
            .engine
            .state()
            .period_closes
            .get("PC-WITHDRAW")
            .map(|c| c.status),
        Some(RecordStatus::Withdrawn)
    );

    let mut app = Workbench::new(Engine::memory());
    app.view = View::PeriodCloses;
    app.role = 1;
    app.period_close_form.id = "EMPTY".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 1200.0))
        .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
    assert!(h.query_by_label("Load synthetic demo").is_none());
    click(&mut h, "Create period close proposal")?;
    click(&mut h, "Submit period close")?;
    h.state_mut().role = 4;
    render(&mut h)?;
    click(&mut h, "Approve period close")?;
    assert!(h.state().engine.state().products.is_empty());
    let c = h
        .state()
        .engine
        .state()
        .period_closes
        .get("EMPTY")
        .ok_or_else(|| error("Empty close missing"))?;
    assert_eq!(c.status, RecordStatus::Approved);
    assert_eq!(c.basis.invoice, Money(0));
    h.state().engine.state().validate()?;
    Ok(())
}
