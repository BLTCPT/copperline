//! Independent storage checks for complete, immutable booking history.
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
            "copperline-booking-storage-{}-{}",
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
fn fixture() -> std::result::Result<Vec<State>, Box<dyn std::error::Error>> {
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

#[test]
fn revised_quote_booking_reopens_and_archives_original_full_history() -> TestResult {
    let workspace = Workspace::new()?;
    let history = fixture()?;
    let original = write_history(&workspace.0, &history)?;
    for state in history.iter().take(5) {
        let value = serde_json::to_value(state)?;
        assert!(value.get("quote_bookings").is_none());
    }
    let (store, reopened) = Store::open(&workspace.0)?;
    assert_eq!(Some(&reopened), history.last());
    assert_eq!(reopened.quotes["Q"].status, Status::Draft);
    assert_eq!(reopened.quote_bookings["B"].lines.len(), 2);
    assert_eq!(reopened.conversion_reports["R"].calculation.rows.len(), 2);
    let output = Workspace::new()?;
    let archive = output.0.join("history.clbak");
    let summary = store.backup(&archive)?;
    assert_eq!(summary.snapshot_count, 8);
    let destination = output.0.join("restored");
    backup::restore(&archive, &destination)?;
    let (_restored_store, restored) = Store::open(&destination)?;
    assert_eq!(restored, reopened);
    unchanged(&destination, &original)?;
    unchanged(&workspace.0, &original)?;
    Ok(())
}

#[test]
fn rehashed_corrupt_booking_middle_rejects_reopen_and_backup_without_erasing_evidence() -> TestResult
{
    let cases: Vec<(&str, Vec<&str>, Value)> = vec![
        (
            "owner",
            vec!["quote_bookings", "B", "owner"],
            json!("Other"),
        ),
        (
            "basis",
            vec![
                "quote_bookings",
                "B",
                "lines",
                "0",
                "native_basis",
                "invoice_total",
            ],
            json!(99),
        ),
        ("erased booking", vec!["quote_bookings"], json!({})),
        (
            "partial booking",
            vec!["quote_bookings", "B", "lines"],
            json!([]),
        ),
        (
            "detached revision",
            vec!["transaction_revisions", "B-L001"],
            json!(4),
        ),
        (
            "altered approved review",
            vec![
                "quote_bookings",
                "B",
                "approval_cycle",
                "reviews",
                "0",
                "note",
            ],
            json!("Altered"),
        ),
        (
            "audit target",
            vec!["audit", "4", "target"],
            json!("booking:OTHER"),
        ),
        (
            "audit command",
            vec!["audit", "4", "command"],
            json!("transaction_add"),
        ),
        (
            "erased report booking evidence",
            vec![
                "conversion_reports",
                "R",
                "calculation",
                "rows",
                "0",
                "booking",
            ],
            Value::Null,
        ),
        (
            "changed report rate knowledge",
            vec![
                "conversion_reports",
                "R",
                "calculation",
                "rows",
                "0",
                "booking",
                "rate_knowledge_revision",
            ],
            json!(5),
        ),
        (
            "changed report native basis",
            vec![
                "conversion_reports",
                "R",
                "calculation",
                "rows",
                "0",
                "booking",
                "native_basis",
                "invoice_total",
            ],
            json!(99),
        ),
    ];
    for (label, path, replacement) in cases {
        let workspace = Workspace::new()?;
        let mut history = fixture()?;
        write_history(&workspace.0, &history)?;
        let (store, _) = Store::open(&workspace.0)?;
        let revision = if path.first() == Some(&"conversion_reports") {
            6
        } else {
            5
        };
        let mut value = serde_json::to_value(&history[revision])?;
        let mut cursor = &mut value;
        for part in path {
            cursor = if cursor.is_array() {
                cursor
                    .get_mut(part.parse::<usize>()?)
                    .ok_or("Missing fixture array entry")?
            } else {
                cursor.get_mut(part).ok_or("Missing fixture field")?
            };
        }
        *cursor = replacement;
        history[revision] = serde_json::from_value(value)?;
        assert!(
            history[revision].validate().is_err(),
            "Invalid mutation fixture: {label}"
        );
        history.last().ok_or("Missing fixture tip")?.validate()?;
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
fn checked_commit_rejects_changed_booking_and_preserves_original_snapshots() -> TestResult {
    let workspace = Workspace::new()?;
    let history = fixture()?;
    let original = write_history(&workspace.0, &history)?;
    let (mut store, mut state) = Store::open(&workspace.0)?;
    state
        .quote_bookings
        .get_mut("B")
        .ok_or("Missing fixture booking")?
        .request
        .source = "Changed".into();
    event(
        &mut state,
        "Fixture",
        Role::Admin,
        "fixture",
        None,
        "Synthetic next revision".into(),
    )?;
    assert!(store.commit(&state).is_err());
    assert!(!workspace.0.join("00000000000000000008.json").exists());
    assert!(!workspace.0.join("00000000000000000008.pending").exists());
    unchanged(&workspace.0, &original)?;
    drop(store);
    let (_store, reopened) = Store::open(&workspace.0)?;
    assert_eq!(Some(&reopened), history.last());
    Ok(())
}
