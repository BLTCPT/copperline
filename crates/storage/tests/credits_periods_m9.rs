//! Independent rehashed-history checks for quantity credits and nominal period closes.
use copperline_model::*;
use copperline_storage::{Store, backup, fingerprint};
use serde::Serialize;
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
            "copperline-m9-history-{}-{}",
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
fn event(
    state: &mut State,
    name: &str,
    role: Role,
    command: &str,
    target: Option<String>,
    detail: String,
) -> Result<()> {
    let revision = state
        .revision
        .checked_add(1)
        .ok_or_else(|| error("Fixture revision overflow"))?;
    state.revision = revision;
    state.audit.push(Audit {
        target,
        revision,
        actor: Actor {
            name: name.into(),
            role,
        },
        command: command.into(),
        detail,
    });
    state.receipts.push(Receipt {
        request_id: format!("storage-fixture-{revision}"),
        revision,
        fingerprint: "1".repeat(64),
    });
    Ok(())
}
fn booking_history() -> std::result::Result<Vec<State>, Box<dyn std::error::Error>> {
    let mut state = State::default();
    let product: Product = serde_json::from_value(json!({
        "id":"P", "name":"Synthetic valve", "family":"Industrial", "unit":"each",
        "currency":"USD", "list_price":100
    }))?;
    state.products.insert(product.id.clone(), product);
    state.customers.insert(
        "C".into(),
        Customer {
            id: "C".into(),
            name: "Synthetic customer".into(),
            segment: "OEM".into(),
        },
    );
    state.costs.push(serde_json::from_value(json!({
        "product_id":"P", "version":1, "effective_from":"2026-01-01",
        "amount":60, "currency":"USD", "unit":"each"
    }))?);
    state.validate()?;
    let mut history = vec![state.clone()];
    state.quotes.insert(
        "Q".into(),
        Quote {
            governance: Some(QuoteGovernance {
                contributors: ["Seller".into()].into(),
                cycles: vec![],
            }),
            id: "Q".into(),
            title: "Synthetic booking".into(),
            owner: "Seller".into(),
            customer_id: "C".into(),
            date: Date::parse("2026-10-13")?,
            version: 1,
            source_revision: 0,
            policy: Policy::default(),
            status: Status::Draft,
            lines: vec![],
            decisions: vec![],
            submitted_by: None,
        },
    );
    event(
        &mut state,
        "Seller",
        Role::Sales,
        "quote_create",
        Some(quote_target("Q")),
        "Created quote Q".into(),
    )?;
    state.validate()?;
    history.push(state.clone());
    let line: Evaluation = serde_json::from_value(json!({
        "request":{"product_id":"P", "customer_id":"C", "date":"2026-10-13",
            "quantity":1000, "discount":0, "rebate":0},
        "currency":"USD", "unit":"each", "product_name":"Synthetic valve",
        "list_unit":100, "rule_unit":100, "invoice_unit":100, "rebate_unit":0,
        "pocket_unit":100, "cost_unit":60, "invoice_total":100, "rebate_total":0,
        "pocket_total":100, "cost_total":60, "margin_total":40, "margin_bps":4000,
        "rule":null, "cost_version":1, "steps":[], "warnings":[], "hard_block":false
    }))?;
    let quote = state.quotes.get_mut("Q").ok_or("Missing fixture quote")?;
    quote.lines = vec![line.clone(), line.clone()];
    quote.source_revision = 1;
    quote.version = 2;
    event(
        &mut state,
        "Seller",
        Role::Sales,
        "quote_add_line",
        Some(quote_target("Q")),
        "Added line and recalculated Q".into(),
    )?;
    state.validate()?;
    history.push(state.clone());
    let quote = state.quotes.get_mut("Q").ok_or("Missing fixture quote")?;
    let cycle = QuoteApprovalCycle {
        submitted_revision: 3,
        submitted_version: 3,
        source_revision: 1,
        submitted_by: "Seller".into(),
        policy: None,
        pricing_policy: quote.policy.clone(),
        lines: quote.lines.clone(),
        stages: quote_approval_stages(&quote.lines, None)?,
        reviews: vec![],
    };
    quote
        .governance
        .as_mut()
        .ok_or("Missing fixture governance")?
        .cycles
        .push(cycle);
    quote.status = Status::Pending;
    quote.version = 3;
    quote.submitted_by = Some("Seller".into());
    event(
        &mut state,
        "Seller",
        Role::Sales,
        "quote_submit",
        Some(quote_target("Q")),
        "Submitted Q for 1 sequential approval stage(s)".into(),
    )?;
    state.validate()?;
    history.push(state.clone());
    let quote = state.quotes.get_mut("Q").ok_or("Missing fixture quote")?;
    let review = QuoteReview {
        actor: Actor {
            name: "Reviewer".into(),
            role: Role::Approver,
        },
        note: "Independent synthetic approval".into(),
        revision: 4,
        approved: true,
    };
    quote
        .governance
        .as_mut()
        .ok_or("Missing fixture governance")?
        .cycles
        .last_mut()
        .ok_or("Missing fixture cycle")?
        .reviews
        .push(review.clone());
    quote.status = Status::Approved;
    quote.version = 4;
    quote.decisions.push(Decision {
        actor: review.actor.name.clone(),
        note: review.note,
        revision: 4,
        approved: true,
    });
    event(
        &mut state,
        "Reviewer",
        Role::Approver,
        "quote_decide",
        Some(quote_target("Q")),
        "Quote Q: Approved; document v4".into(),
    )?;
    state.validate()?;
    history.push(state.clone());

    let request = BookingRequest {
        id: "B".into(),
        quote_id: "Q".into(),
        quote_version: 4,
        invoice_date: Date::parse("2026-10-14")?,
        source: "Synthetic storage review".into(),
    };
    let lines = state.preview_quote_booking(&request)?;
    let quote = state.quotes.get("Q").ok_or("Missing fixture quote")?;
    let booking = QuoteBooking {
        request,
        owner: "Booker".into(),
        created_revision: 5,
        rounding: BookingRounding::ExactApprovedTotalsNativeUnitDiscountThenExtendV1,
        approval_cycle: quote
            .governance
            .as_ref()
            .ok_or("Missing fixture governance")?
            .cycles
            .last()
            .ok_or("Missing fixture cycle")?
            .clone(),
        quote_contributors: state.quote_contributors(quote),
        lines,
    };
    let detail = booking_created_detail(&booking)?;
    for line in &booking.lines {
        state
            .transactions
            .insert(line.booked.id.clone(), line.booked.clone());
        state
            .transaction_revisions
            .insert(line.booked.id.clone(), booking.created_revision);
    }
    state.quote_bookings.insert("B".into(), booking);
    event(
        &mut state,
        "Booker",
        Role::Analyst,
        "quote_book",
        Some(booking_target("B")),
        detail,
    )?;
    state.validate()?;
    history.push(state.clone());
    let report = ConversionReport {
        id: "R".into(),
        owner: "Reporter".into(),
        created_revision: 6,
        calculation: state.converted_actuals(
            &ReportingRequest {
                start: Date::parse("2026-10-01")?,
                end: Date::parse("2026-11-01")?,
                currency: Currency::USD,
            },
            5,
        )?,
    };
    let detail = report_created_detail(&report)?;
    state.conversion_reports.insert("R".into(), report);
    event(
        &mut state,
        "Reporter",
        Role::Analyst,
        "report_convert",
        Some(report_target("R")),
        detail,
    )?;
    state.validate()?;
    history.push(state.clone());
    let quote = state.quotes.get_mut("Q").ok_or("Missing fixture quote")?;
    quote.status = Status::Draft;
    quote.version = 5;
    quote.submitted_by = None;
    event(
        &mut state,
        "Seller",
        Role::Sales,
        "quote_revise",
        Some(quote_target("Q")),
        "Reopened Q; previous approval no longer applies".into(),
    )?;
    state.validate()?;
    history.push(state);
    Ok(history)
}

#[derive(Serialize)]
struct Envelope<'a> {
    previous_hash: &'a str,
    hash: &'a str,
    state: &'a State,
}
fn write_history(
    root: &Path,
    history: &[State],
) -> std::result::Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut previous = String::new();
    let mut written = vec![];
    for state in history {
        let mut input = previous.as_bytes().to_vec();
        input.extend(serde_json::to_vec(state)?);
        let hash = fingerprint(&input);
        let bytes = serde_json::to_vec(&Envelope {
            previous_hash: &previous,
            hash: &hash,
            state,
        })?;
        fs::write(root.join(format!("{:020}.json", state.revision)), &bytes)?;
        written.push(bytes);
        previous = hash;
    }
    Ok(written)
}
fn unchanged(root: &Path, expected: &[Vec<u8>]) -> TestResult {
    for (revision, bytes) in expected.iter().enumerate() {
        assert_eq!(&fs::read(root.join(format!("{revision:020}.json")))?, bytes);
    }
    Ok(())
}

// Genuine M8 source 0ab4395fdbf3d295173ba5314d33334e9e698912, macOS CLI
// SHA256 ab2ec603087374b07d82c69e29ac739ad61c42eee25565a3461ca607e27e2641.
// The old binary reopened these original bytes before they were frozen here.
// This portable regression covers an old seed history; the release compatibility
// run separately covers legacy bookings, reports, settlements and CSV exports.
const LEGACY_M8: [&str; 2] = [
    r#"{"previous_hash":"","hash":"20597816b453a303526ee556442cb375e3f37710b520fe97a03d1dd5e229950c","state":{"schema_version":1,"revision":0,"products":{},"customers":{},"costs":[],"rules":[],"quotes":{},"price_lists":{},"policy":{"min_margin":2500,"max_discount":1500,"hard_min_margin":0},"audit":[],"receipts":[]}}"#,
    r#"{"previous_hash":"20597816b453a303526ee556442cb375e3f37710b520fe97a03d1dd5e229950c","hash":"19c472daae38531e6cff3fb625ce675f0a429d6e0115b6b8ab19f77e39027859","state":{"schema_version":1,"revision":1,"products":{"FLT-020":{"id":"FLT-020","name":"Inline process filter","family":"Consumables","unit":"each","currency":"USD","list_price":8500},"KIT-450":{"id":"KIT-450","name":"Chamber service kit","family":"Service","unit":"each","currency":"USD","list_price":128000},"MFC-240":{"id":"MFC-240","name":"Mass flow controller","family":"Flow control","unit":"each","currency":"USD","list_price":148000},"PMP-080":{"id":"PMP-080","name":"Dry vacuum pump","family":"Vacuum","unit":"each","currency":"USD","list_price":465000},"SNS-120":{"id":"SNS-120","name":"Pressure transducer","family":"Instrumentation","unit":"each","currency":"USD","list_price":64000},"VLV-100":{"id":"VLV-100","name":"Precision flow valve","family":"Flow control","unit":"each","currency":"USD","list_price":10000}},"customers":{"C-ATLAS":{"id":"C-ATLAS","name":"Atlas Process Systems (synthetic)","segment":"OEM"},"C-DELTA":{"id":"C-DELTA","name":"Delta Research Labs (synthetic)","segment":"Research"},"C-NORTH":{"id":"C-NORTH","name":"Northline Service (synthetic)","segment":"Service"}},"costs":[{"product_id":"VLV-100","version":1,"effective_from":"2026-01-01","amount":6000,"currency":"USD","unit":"each"},{"product_id":"MFC-240","version":1,"effective_from":"2026-01-01","amount":89000,"currency":"USD","unit":"each"},{"product_id":"PMP-080","version":1,"effective_from":"2026-01-01","amount":294000,"currency":"USD","unit":"each"},{"product_id":"FLT-020","version":1,"effective_from":"2026-01-01","amount":3900,"currency":"USD","unit":"each"},{"product_id":"KIT-450","version":1,"effective_from":"2026-01-01","amount":72500,"currency":"USD","unit":"each"},{"product_id":"SNS-120","version":1,"effective_from":"2026-01-01","amount":36500,"currency":"USD","unit":"each"}],"rules":[{"id":"OEM-VOLUME","version":1,"name":"OEM volume tier","priority":10,"product_id":null,"customer_id":null,"segment":"OEM","effective_from":"2026-01-01","effective_to":null,"min_quantity":20000,"action":{"kind":"discount","value":800},"currency":"USD"}],"quotes":{},"price_lists":{},"policy":{"min_margin":2500,"max_discount":1500,"hard_min_margin":0},"audit":[{"revision":1,"actor":{"name":"Demo administrator","role":"admin"},"command":"seed_demo","detail":"Loaded synthetic industrial catalog, costs, customers and rules"}],"receipts":[{"request_id":"demo-1","revision":1,"fingerprint":"97d0f78710812a3eec8c1f5f39abe4b822d9e77526790972c2afe46e5b703f41"}]}}"#,
];

fn capture(history: &mut Vec<State>, state: &State) -> Result<()> {
    state.validate()?;
    history.push(state.clone());
    Ok(())
}

fn credit(history: &mut Vec<State>, state: &mut State, key: &str, quantity: i64) -> TestResult {
    let proposal: CreditProposal = serde_json::from_value(json!({
        "id":key, "original_transaction_id":"B-L001", "quantity":quantity,
        "effective_date":"2027-01-01", "reason":"Synthetic quantity return"
    }))?;
    let record = state.credit_preview(&proposal, "Credit author")?;
    let detail = credit_created_detail(&record)?;
    state.credits.insert(key.into(), record);
    event(
        state,
        "Credit author",
        Role::Analyst,
        "credit_create",
        Some(credit_target(key)),
        detail,
    )?;
    capture(history, state)?;
    let revision = state.revision + 1;
    let record = state.credits.get_mut(key).ok_or("Missing credit")?;
    record.version = 2;
    record.status = RecordStatus::Pending;
    record.submitted_revision = Some(revision);
    event(
        state,
        "Credit author",
        Role::Analyst,
        "credit_submit",
        Some(credit_target(key)),
        format!("Submitted credit {key}"),
    )?;
    capture(history, state)?;
    let review = RecordReview {
        actor: Actor {
            name: "Credit finance".into(),
            role: Role::Finance,
        },
        revision: state.revision + 1,
        approved: true,
        note: "Synthetic independent credit review".into(),
    };
    let record = state.credits.get_mut(key).ok_or("Missing credit")?;
    let detail = credit_decision_detail(record, &review)?;
    record.version = 3;
    record.status = RecordStatus::Approved;
    record.review = Some(review);
    event(
        state,
        "Credit finance",
        Role::Finance,
        "credit_decide",
        Some(credit_target(key)),
        detail,
    )?;
    capture(history, state)?;
    Ok(())
}

fn fixture() -> std::result::Result<Vec<State>, Box<dyn std::error::Error>> {
    let mut history = booking_history()?;
    let mut state = history.last().ok_or("Missing booking history")?.clone();
    for (id, quantity) in [("A", 333), ("B", 333), ("C", 334)] {
        credit(&mut history, &mut state, id, quantity)?;
    }
    let report = ConversionReport {
        id: "RC".into(),
        owner: "Reporter".into(),
        created_revision: state.revision + 1,
        calculation: state.converted_actuals(
            &ReportingRequest {
                start: Date::parse("2026-10-01")?,
                end: Date::parse("2026-11-01")?,
                currency: Currency::USD,
            },
            state.revision,
        )?,
    };
    let detail = report_created_detail(&report)?;
    state.conversion_reports.insert("RC".into(), report);
    event(
        &mut state,
        "Reporter",
        Role::Analyst,
        "report_convert",
        Some(report_target("RC")),
        detail,
    )?;
    capture(&mut history, &state)?;
    let terms = PeriodCloseTerms {
        id: "OCT".into(),
        start: Date::parse("2026-10-01")?,
        end: Date::parse("2026-11-01")?,
        currency: Currency::USD,
        reason: "Synthetic nominal close".into(),
    };
    let close = state.period_close_preview(&terms, "Close author")?;
    let detail = period_close_created_detail(&close)?;
    state.period_closes.insert("OCT".into(), close);
    event(
        &mut state,
        "Close author",
        Role::Analyst,
        "period_close_create",
        Some(period_close_target("OCT")),
        detail,
    )?;
    capture(&mut history, &state)?;
    let revision = state.revision + 1;
    let close = state.period_closes.get_mut("OCT").ok_or("Missing close")?;
    close.version = 2;
    close.status = RecordStatus::Pending;
    close.submitted_revision = Some(revision);
    event(
        &mut state,
        "Close author",
        Role::Analyst,
        "period_close_submit",
        Some(period_close_target("OCT")),
        "Submitted period close OCT".into(),
    )?;
    capture(&mut history, &state)?;
    let revision = state.revision + 1;
    let close = state.period_closes.get_mut("OCT").ok_or("Missing close")?;
    close.version = 3;
    close.status = RecordStatus::Approved;
    close.review = Some(RecordReview {
        actor: Actor {
            name: "Close finance".into(),
            role: Role::Finance,
        },
        revision,
        approved: true,
        note: "Synthetic independent close review".into(),
    });
    let detail = period_close_decided_detail(close)?;
    event(
        &mut state,
        "Close finance",
        Role::Finance,
        "period_close_decide",
        Some(period_close_target("OCT")),
        detail,
    )?;
    capture(&mut history, &state)?;
    event(
        &mut state,
        "Fixture",
        Role::Admin,
        "fixture",
        None,
        "Synthetic later revision".into(),
    )?;
    capture(&mut history, &state)?;
    Ok(history)
}

#[test]
fn booked_credits_close_and_frozen_reports_reopen_and_restore_exact_history() -> TestResult {
    let workspace = Workspace::new()?;
    let history = fixture()?;
    let original = write_history(&workspace.0, &history)?;
    let (store, state) = Store::open(&workspace.0)?;
    assert_eq!(Some(&state), history.last());
    assert_eq!(state.revision, 21);
    assert_eq!(state.credits["C"].prior_credit_ids, vec!["A", "B"]);
    assert_eq!(state.credits["C"].prior_quantity.0, 666);
    assert_eq!(state.credits["C"].invoice.0, 33);
    assert_eq!(
        state.credits["C"]
            .booking
            .as_ref()
            .ok_or("Missing native evidence")?
            .native_quantity
            .0,
        334
    );
    assert_eq!(
        state.period_closes["OCT"].basis.credit_ids,
        vec!["A", "B", "C"]
    );
    assert_eq!(state.period_closes["OCT"].basis.credited_invoice.0, 100);
    assert_eq!(state.period_closes["OCT"].basis.invoice.0, 100);
    assert_eq!(
        state.conversion_reports["R"],
        history[6].conversion_reports["R"]
    );
    let row = state.conversion_reports["RC"]
        .calculation
        .rows
        .iter()
        .find(|r| r.native.id == "B-L001")
        .ok_or("Missing credited report row")?;
    assert_eq!(row.credits.len(), 3);
    assert_eq!(row.credits.iter().map(|c| c.invoice.0).sum::<i64>(), 100);
    assert_eq!(row.credits.iter().map(|c| c.cost.0).sum::<i64>(), 60);
    let output = Workspace::new()?;
    let archive = output.0.join("history.clbak");
    assert_eq!(store.backup(&archive)?.snapshot_count, 22);
    let destination = output.0.join("restored");
    backup::restore(&archive, &destination)?;
    let (_restored_store, restored) = Store::open(&destination)?;
    assert_eq!(restored, state);
    unchanged(&destination, &original)?;
    unchanged(&workspace.0, &original)?;
    Ok(())
}

#[test]
fn rehashed_middle_credit_close_corruption_is_rejected_before_backup_or_reopen() -> TestResult {
    let cases: Vec<(&str, usize, &str, Value)> = vec![
        ("erased credit history", 10, "/credits", json!({})),
        ("credit owner", 10, "/credits/A/owner", json!("Other")),
        (
            "credit contributors",
            10,
            "/credits/A/contributors",
            json!([]),
        ),
        (
            "credit original",
            10,
            "/credits/A/original/invoice_total",
            json!(99),
        ),
        (
            "credit ingestion",
            10,
            "/credits/A/original_revision",
            json!(4),
        ),
        ("credit allocation", 10, "/credits/A/invoice", json!(34)),
        ("credit cost allocation", 10, "/credits/A/cost", json!(21)),
        (
            "credit quantity",
            10,
            "/credits/A/proposal/quantity",
            json!(334),
        ),
        (
            "credit native evidence erased",
            10,
            "/credits/A/booking",
            Value::Null,
        ),
        (
            "credit native allocation",
            10,
            "/credits/A/booking/native_invoice",
            json!(34),
        ),
        (
            "credit native quantity",
            10,
            "/credits/A/booking/native_quantity",
            json!(332),
        ),
        (
            "credit native original basis",
            10,
            "/credits/A/booking/native_basis/invoice_total",
            json!(99),
        ),
        (
            "credit booking fingerprint",
            10,
            "/credits/A/booking/booking_fingerprint",
            json!("0".repeat(64)),
        ),
        (
            "credit booking source",
            10,
            "/credits/A/booking/booking_revision",
            json!(4),
        ),
        ("credit line", 10, "/credits/A/booking/line_index", json!(2)),
        (
            "credit review role",
            10,
            "/credits/A/review/actor/role",
            json!("admin"),
        ),
        (
            "credit review author",
            10,
            "/credits/A/review/actor/name",
            json!("Booker"),
        ),
        (
            "credit review note",
            10,
            "/credits/A/review/note",
            json!("Changed"),
        ),
        (
            "credit review time",
            10,
            "/credits/A/review/revision",
            json!(9),
        ),
        (
            "credit lifecycle",
            10,
            "/credits/A/status",
            json!("Rejected"),
        ),
        (
            "credit audit retarget",
            10,
            "/audit/7/target",
            json!("credit:OTHER"),
        ),
        (
            "credit audit command",
            10,
            "/audit/9/command",
            json!("fixture"),
        ),
        (
            "credit prior ID erased",
            13,
            "/credits/B/prior_credit_ids",
            json!([]),
        ),
        (
            "credit baseline quantity",
            13,
            "/credits/B/prior_quantity",
            json!(0),
        ),
        (
            "credit prior order",
            16,
            "/credits/C/prior_credit_ids",
            json!(["B", "A"]),
        ),
        (
            "credit quantity exhaustion",
            16,
            "/credits/C/proposal/quantity",
            json!(335),
        ),
        (
            "credit original zero quantity",
            16,
            "/credits/C/original/quantity",
            json!(0),
        ),
        (
            "report credits erased",
            17,
            "/conversion_reports/RC/calculation/rows/0/credits",
            json!([]),
        ),
        (
            "report allocated cents",
            17,
            "/conversion_reports/RC/calculation/rows/0/credits/0/invoice",
            json!(32),
        ),
        (
            "report original knowledge cutoff",
            17,
            "/conversion_reports/RC/calculation/rows/0/booking/rate_knowledge_revision",
            json!(16),
        ),
        ("erased close history", 20, "/period_closes", json!({})),
        (
            "close original IDs",
            20,
            "/period_closes/OCT/basis/transaction_ids",
            json!(["B-L002"]),
        ),
        (
            "close credit IDs",
            20,
            "/period_closes/OCT/basis/credit_ids",
            json!(["A", "B"]),
        ),
        (
            "close net basis",
            20,
            "/period_closes/OCT/basis/invoice",
            json!(101),
        ),
        (
            "close reviewed range",
            20,
            "/period_closes/OCT/terms/end",
            json!("2026-10-14"),
        ),
        (
            "close source",
            20,
            "/period_closes/OCT/source_revision",
            json!(16),
        ),
        (
            "close contributor binding",
            20,
            "/period_closes/OCT/contributors",
            json!(["Close author"]),
        ),
        (
            "close review role",
            20,
            "/period_closes/OCT/review/actor/role",
            json!("approver"),
        ),
        (
            "close review author",
            20,
            "/period_closes/OCT/review/actor/name",
            json!("Credit author"),
        ),
        (
            "close review note",
            20,
            "/period_closes/OCT/review/note",
            json!("Changed"),
        ),
        (
            "close review time",
            20,
            "/period_closes/OCT/review/revision",
            json!(19),
        ),
        (
            "close audit retarget",
            20,
            "/audit/17/target",
            json!("period_close:OTHER"),
        ),
        (
            "close audit command",
            20,
            "/audit/19/command",
            json!("fixture"),
        ),
    ];
    let clean = fixture()?;
    for (label, revision, pointer, replacement) in cases {
        let workspace = Workspace::new()?;
        let mut history = clean.clone();
        write_history(&workspace.0, &history)?;
        let (store, _) = Store::open(&workspace.0)?;
        let mut value = serde_json::to_value(&history[revision])?;
        *value
            .pointer_mut(pointer)
            .ok_or_else(|| format!("Missing mutation path: {label}"))? = replacement;
        history[revision] = serde_json::from_value(value)?;
        assert!(
            history[revision].validate().is_err(),
            "Mutation accepted: {label}"
        );
        history.last().ok_or("Missing tip")?.validate()?;
        // Every outer checksum, including the later semantically valid tip, is recomputed.
        let preserved = write_history(&workspace.0, &history)?;
        let output = Workspace::new()?;
        let archive = output.0.join("must-not-exist.clbak");
        assert!(store.backup(&archive).is_err(), "Backup accepted {label}");
        assert!(!archive.exists());
        drop(store);
        assert!(
            Store::open(&workspace.0).is_err(),
            "Reopen accepted {label}"
        );
        unchanged(&workspace.0, &preserved)?;
    }
    Ok(())
}

#[test]
fn genuine_m8_seed_bytes_keep_hashes_and_skip_empty_m9_fields() -> TestResult {
    let workspace = Workspace::new()?;
    let expected = LEGACY_M8
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect::<Vec<_>>();
    for (revision, bytes) in expected.iter().enumerate() {
        fs::write(workspace.0.join(format!("{revision:020}.json")), bytes)?;
        let value: Value = serde_json::from_slice(bytes)?;
        let state: State = serde_json::from_value(value["state"].clone())?;
        state.validate()?;
        assert!(state.credits.is_empty() && state.period_closes.is_empty());
        let serialized = serde_json::to_value(&state)?;
        assert!(serialized.get("credits").is_none());
        assert!(serialized.get("period_closes").is_none());
        assert_eq!(serialized, value["state"]);
        let envelope = serde_json::to_vec(&Envelope {
            previous_hash: value["previous_hash"]
                .as_str()
                .ok_or("Missing previous hash")?,
            hash: value["hash"].as_str().ok_or("Missing hash")?,
            state: &state,
        })?;
        assert_eq!(&envelope, bytes);
    }
    let (store, state) = Store::open(&workspace.0)?;
    assert_eq!(state.revision, 1);
    let output = Workspace::new()?;
    let archive = output.0.join("m8-seed.clbak");
    assert_eq!(store.backup(&archive)?.snapshot_count, 2);
    let destination = output.0.join("restored");
    backup::restore(&archive, &destination)?;
    unchanged(&workspace.0, &expected)?;
    unchanged(&destination, &expected)?;
    Ok(())
}
