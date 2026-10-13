//! Independent checks of batch line replacement with explicit approved conversions.
use copperline_engine::{
    Command, Engine, QuoteLineReplacement, Request, evaluate, export_quote, totals,
};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity, Rational};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
type CommittedFiles = Vec<(String, Vec<u8>)>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-batch-review-{}-{}",
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
fn committed(path: &Path) -> std::io::Result<CommittedFiles> {
    let mut result = vec![];
    for item in fs::read_dir(path)? {
        let item = item?;
        if item
            .path()
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            result.push((
                item.file_name().to_string_lossy().into_owned(),
                fs::read(item.path())?,
            ));
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
fn actor(name: &str, role: Role) -> Actor {
    Actor {
        name: name.into(),
        role,
    }
}
fn date() -> Result<Date> {
    Date::parse("2026-10-13")
}
fn price() -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date()?,
        quantity: Quantity(1000),
        discount: Bps(0),
        rebate: Bps(0),
        source: None,
        conversion: None,
    })
}
fn demo() -> Result<Engine> {
    let mut e = Engine::memory();
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    Ok(e)
}
fn quote(e: &Engine) -> Result<&Quote> {
    e.state()
        .quotes
        .get("Q")
        .ok_or_else(|| error("Missing quote"))
}
fn create(e: &mut Engine) -> Result<()> {
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic batch review".into(),
            customer_id: "C-ATLAS".into(),
            date: date()?,
        },
    )?;
    Ok(())
}
fn add(e: &mut Engine, request: PriceRequest) -> Result<()> {
    let version = quote(e)?.version;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version,
            request,
        },
    )?;
    Ok(())
}
fn replacement(index: usize, request: PriceRequest) -> QuoteLineReplacement {
    QuoteLineReplacement { index, request }
}
fn batch(version: u64, replacements: Vec<QuoteLineReplacement>) -> Command {
    Command::QuoteReplaceLines {
        id: "Q".into(),
        version,
        replacements,
    }
}
fn reject_atomic(e: &mut Engine, who: Actor, command: Command) {
    let before = e.state().clone();
    assert!(e.run(who, command).is_err());
    assert_eq!(e.state(), &before);
}
fn converted(currency: Currency, unit: &str) -> Result<PriceRequest> {
    let mut p = price()?;
    p.conversion = Some(ConversionRequest {
        currency,
        unit: unit.into(),
    });
    Ok(p)
}
fn approve_rates(e: &mut Engine, entries: Vec<ConversionEntry>) -> Result<()> {
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: "RATES".into(),
                name: "Synthetic direct rates".into(),
                entries,
            },
        },
    )?;
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionSubmit {
            id: "RATES".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("rate-reviewer", Role::Approver),
        Command::ConversionDecide {
            id: "RATES".into(),
            version: 2,
            approve: true,
            note: "Independent rate review".into(),
        },
    )?;
    Ok(())
}
fn row(
    id: &str,
    kind: ConversionKind,
    numerator: i64,
    denominator: i64,
) -> Result<ConversionEntry> {
    Ok(ConversionEntry {
        id: id.into(),
        conversion: kind,
        start: Date::parse("2026-01-01")?,
        end: Date::parse("2027-01-01")?,
        ratio: Rational {
            numerator,
            denominator,
        },
    })
}
fn fx() -> Result<ConversionEntry> {
    row(
        "USD-EUR",
        ConversionKind::Fx {
            from: Currency::USD,
            to: Currency::EUR,
        },
        9,
        10,
    )
}
fn activate_grid(e: &mut Engine, id: &str, discount: Bps) -> Result<PriceSource> {
    let version = e.state().grid.get(id).map_or(0, |g| g.version);
    let mut request = price()?;
    request.discount = discount;
    e.run(
        actor("pricing", Role::Analyst),
        Command::GridCalculate {
            id: id.into(),
            version,
            request,
        },
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::GridSubmit {
            id: id.into(),
            version: version + 1,
        },
    )?;
    e.run(
        actor("reviewer", Role::Approver),
        Command::GridDecide {
            id: id.into(),
            version: version + 2,
            approve: true,
            note: "Approved source fixture".into(),
        },
    )?;
    Ok(PriceSource::Grid {
        id: id.into(),
        approval_revision: e.state().revision,
    })
}
fn selected(source: PriceSource) -> Result<PriceRequest> {
    let mut p = price()?;
    p.source = Some(source);
    Ok(p)
}

#[test]
fn multiple_stale_sources_are_repaired_together_in_one_revision_and_audit() -> TestResult {
    let mut e = demo()?;
    approve_rates(
        &mut e,
        vec![
            fx()?,
            row(
                "CASE",
                ConversionKind::Uom {
                    product_id: "VLV-100".into(),
                    from_unit: "case".into(),
                    to_unit: "each".into(),
                },
                12,
                1,
            )?,
        ],
    )?;
    let first = activate_grid(&mut e, "G1", Bps(1000))?;
    let second = activate_grid(&mut e, "G2", Bps(500))?;
    create(&mut e)?;
    add(&mut e, selected(first)?)?;
    add(&mut e, selected(second)?)?;
    let mut first = selected(activate_grid(&mut e, "G1", Bps(1500))?)?;
    let mut second = selected(activate_grid(&mut e, "G2", Bps(1000))?)?;
    first.conversion = Some(ConversionRequest {
        currency: Currency::EUR,
        unit: "case".into(),
    });
    second.conversion = first.conversion.clone();
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        Command::QuoteReplaceLine {
            id: "Q".into(),
            version: 3,
            index: 0,
            request: first.clone(),
        },
    );
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        batch(3, vec![replacement(1, second.clone())]),
    );
    let before = e.state().clone();
    let expected = vec![evaluate(&before, &first)?, evaluate(&before, &second)?];
    // Unsorted indices must still preserve document order.
    e.run(
        actor("seller", Role::Sales),
        batch(3, vec![replacement(1, second), replacement(0, first)]),
    )?;
    let q = quote(&e)?;
    assert_eq!(q.lines, expected);
    assert_eq!(q.lines[0].invoice_unit, Money(91800));
    assert_eq!(q.lines[1].invoice_unit, Money(97200));
    assert!(
        q.lines
            .iter()
            .all(|line| line.source.is_some() && line.conversion.is_some())
    );
    assert_eq!(q.version, 4);
    assert_eq!(q.source_revision, before.revision);
    assert_eq!(e.state().revision, before.revision + 1);
    assert_eq!(e.state().audit.len(), before.audit.len() + 1);
    assert_eq!(e.state().receipts.len(), before.receipts.len() + 1);
    let event = e.state().audit.last().ok_or("Missing audit")?;
    assert_eq!(event.command, "quote_replace_lines");
    assert_eq!(event.target.as_deref(), Some("quote:Q"));
    assert_eq!(event.detail, "Replaced 2 lines and recalculated Q");
    e.state().validate()?;
    Ok(())
}

#[test]
fn single_currency_change_fails_then_batch_converts_whole_quote_with_frozen_rates() -> TestResult {
    let mut e = demo()?;
    approve_rates(&mut e, vec![fx()?])?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    let mut second = price()?;
    second.quantity = Quantity(2000);
    add(&mut e, second)?;
    let first = converted(Currency::EUR, "each")?;
    let mut second = first.clone();
    second.quantity = Quantity(2000);
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        Command::QuoteReplaceLine {
            id: "Q".into(),
            version: 3,
            index: 0,
            request: first.clone(),
        },
    );
    let before = e.state().revision;
    e.run(
        actor("seller", Role::Sales),
        batch(3, vec![replacement(0, first), replacement(1, second)]),
    )?;
    let q = quote(&e)?;
    let t = totals(&q.lines)?;
    assert_eq!(
        (t.currency, t.invoice, t.cost),
        (Currency::EUR, Money(27000), Money(16200))
    );
    assert_eq!(q.source_revision, before);
    for line in &q.lines {
        let evidence = line.conversion.as_ref().ok_or("Missing conversion")?;
        assert_eq!(evidence.captured_revision, before);
        assert_eq!(evidence.native_currency, Currency::USD);
        assert_eq!(
            evidence.fx.as_ref().map(|r| r.book_id.as_str()),
            Some("RATES")
        );
        line.validate_snapshot()?;
    }
    let frozen = q.clone();
    e.run(
        actor("rate-author", Role::Analyst),
        Command::ConversionCreate {
            terms: ConversionTerms {
                id: "FUTURE".into(),
                name: "Future rates".into(),
                entries: vec![ConversionEntry {
                    id: "FUTURE-FX".into(),
                    conversion: ConversionKind::Fx {
                        from: Currency::USD,
                        to: Currency::EUR,
                    },
                    start: Date::parse("2027-01-01")?,
                    end: Date::parse("2028-01-01")?,
                    ratio: Rational {
                        numerator: 1,
                        denominator: 2,
                    },
                }],
            },
        },
    )?;
    assert_eq!(quote(&e)?, &frozen);
    e.state().validate()?;
    Ok(())
}

#[test]
fn batch_combines_fx_and_uom_once_and_qualifies_tiers_using_native_quantity() -> TestResult {
    let mut e = demo()?;
    approve_rates(
        &mut e,
        vec![
            fx()?,
            row(
                "CASE",
                ConversionKind::Uom {
                    product_id: "VLV-100".into(),
                    from_unit: "case".into(),
                    to_unit: "each".into(),
                },
                12,
                1,
            )?,
        ],
    )?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    add(&mut e, price()?)?;
    let first = converted(Currency::EUR, "case")?;
    let mut second = first.clone();
    second.quantity = Quantity(2000);
    e.run(
        actor("seller", Role::Sales),
        batch(3, vec![replacement(0, first), replacement(1, second)]),
    )?;
    let q = quote(&e)?;
    assert_eq!(q.lines[0].invoice_unit, Money(108000));
    assert_eq!(q.lines[1].invoice_unit, Money(99360));
    assert_eq!(
        q.lines[0].conversion.as_ref().map(|c| c.native_quantity),
        Some(Quantity(12000))
    );
    assert_eq!(
        q.lines[1].conversion.as_ref().map(|c| c.native_quantity),
        Some(Quantity(24000))
    );
    assert!(q.lines[0].rule.is_none());
    assert!(q.lines[1].rule.is_some());
    assert_eq!(totals(&q.lines)?.invoice, Money(306720));
    assert!(q.lines.iter().all(|l| l.cost_unit == Money(64800)));
    e.state().validate()?;
    Ok(())
}

#[test]
fn combined_conversion_rounds_once_and_nonintegral_native_quantity_rolls_back() -> TestResult {
    let mut e = demo()?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::ProductPut(Product {
            id: "CENT".into(),
            name: "Synthetic cent item".into(),
            family: "Test".into(),
            unit: "each".into(),
            currency: Currency::USD,
            list_price: Money(1),
        }),
    )?;
    e.run(
        actor("pricing", Role::Analyst),
        Command::CostAdd(Cost {
            product_id: "CENT".into(),
            version: 1,
            effective_from: date()?,
            amount: Money(0),
            currency: Currency::USD,
            unit: "each".into(),
        }),
    )?;
    approve_rates(
        &mut e,
        vec![
            row(
                "FX",
                ConversionKind::Fx {
                    from: Currency::USD,
                    to: Currency::EUR,
                },
                3,
                2,
            )?,
            row(
                "UOM",
                ConversionKind::Uom {
                    product_id: "CENT".into(),
                    from_unit: "bundle".into(),
                    to_unit: "each".into(),
                },
                2,
                3,
            )?,
        ],
    )?;
    create(&mut e)?;
    let mut native = price()?;
    native.product_id = "CENT".into();
    add(&mut e, native.clone())?;
    add(&mut e, native.clone())?;
    let mut converted = native;
    converted.conversion = Some(ConversionRequest {
        currency: Currency::EUR,
        unit: "bundle".into(),
    });
    converted.quantity = Quantity(3000);
    let mut bad = converted.clone();
    bad.quantity = Quantity(1);
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        batch(
            3,
            vec![replacement(0, converted.clone()), replacement(1, bad)],
        ),
    );
    e.run(
        actor("seller", Role::Sales),
        batch(
            3,
            vec![replacement(0, converted.clone()), replacement(1, converted)],
        ),
    )?;
    // 1 cent * 2/3 native units * 3/2 FX = 1 cent/bundle, then 3 bundles.
    for line in &quote(&e)?.lines {
        assert_eq!(line.invoice_unit, Money(1));
        assert_eq!(line.invoice_total, Money(3));
        assert_eq!(
            line.conversion.as_ref().map(|c| c.native_quantity),
            Some(Quantity(2000))
        );
    }
    assert_eq!(totals(&quote(&e)?.lines)?.invoice, Money(6));
    Ok(())
}

#[test]
fn malformed_batch_never_changes_memory_or_any_committed_snapshot() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    add(&mut e, price()?)?;
    let mut changed = price()?;
    changed.discount = Bps(500);
    let mut wrong_customer = price()?;
    wrong_customer.customer_id = "C-NORTH".into();
    let mut wrong_date = price()?;
    wrong_date.date = Date::parse("2026-10-14")?;
    let mut zero = price()?;
    zero.quantity = Quantity(0);
    let cases = vec![
        vec![],
        vec![replacement(0, changed.clone()), replacement(0, price()?)],
        vec![
            replacement(0, changed.clone()),
            replacement(usize::MAX, price()?),
        ],
        vec![
            replacement(0, price()?),
            replacement(1, price()?),
            replacement(2, price()?),
        ],
        vec![
            replacement(0, changed.clone()),
            replacement(1, wrong_customer),
        ],
        vec![replacement(0, changed.clone()), replacement(1, wrong_date)],
        vec![replacement(0, changed.clone()), replacement(1, zero)],
        vec![
            replacement(0, changed),
            replacement(1, converted(Currency::GBP, "each")?),
        ],
    ];
    let files = committed(&workspace.0)?;
    for changes in cases {
        reject_atomic(&mut e, actor("seller", Role::Sales), batch(3, changes));
        assert_eq!(committed(&workspace.0)?, files);
    }
    for who in [
        actor("outsider", Role::Sales),
        actor("outsider", Role::Admin),
        actor("seller", Role::Finance),
    ] {
        reject_atomic(&mut e, who, batch(3, vec![replacement(0, price()?)]));
        assert_eq!(committed(&workspace.0)?, files);
    }
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        batch(2, vec![replacement(0, price()?)]),
    );
    let old = e.state().clone();
    assert!(
        e.execute(Request {
            request_id: "stale-workspace".into(),
            expected_revision: old.revision - 1,
            actor: actor("seller", Role::Sales),
            action: batch(3, vec![replacement(0, price()?)])
        })
        .is_err()
    );
    assert_eq!(e.state(), &old);
    assert_eq!(committed(&workspace.0)?, files);
    drop(e);
    let e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &old);
    Ok(())
}

#[test]
fn failed_batch_commit_can_retry_once_and_replay_exactly_after_reopen() -> TestResult {
    let workspace = Workspace::new()?;
    let mut e = Engine::open(&workspace.0)?;
    e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
    approve_rates(&mut e, vec![fx()?])?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    add(&mut e, price()?)?;
    let before = e.state().clone();
    let files = committed(&workspace.0)?;
    let request = Request {
        request_id: "batch-retry".into(),
        expected_revision: before.revision,
        actor: actor("seller", Role::Sales),
        action: batch(
            3,
            vec![
                replacement(1, converted(Currency::EUR, "each")?),
                replacement(0, converted(Currency::EUR, "each")?),
            ],
        ),
    };
    let encoded = serde_json::to_string(&request)?;
    assert!(encoded.contains("quote_replace_lines"));
    let request: Request = serde_json::from_str(&encoded)?;
    let pending = workspace
        .0
        .join(format!("{:020}.pending", before.revision + 1));
    fs::write(&pending, b"Synthetic pending blocker")?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert_eq!(committed(&workspace.0)?, files);
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    assert_eq!(e.state(), &before);
    fs::remove_file(&pending)?;
    let accepted = e.execute(request.clone())?;
    assert!(!accepted.replayed);
    assert_eq!(quote(&e)?.version, 4);
    assert_eq!(committed(&workspace.0)?.len(), files.len() + 1);
    let state = e.state().clone();
    drop(e);
    let mut e = Engine::open(&workspace.0)?;
    let replay = e.execute(request.clone())?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, accepted.revision);
    assert_eq!(e.state(), &state);
    let mut reordered = request;
    if let Command::QuoteReplaceLines { replacements, .. } = &mut reordered.action {
        replacements.reverse();
    }
    assert!(
        e.execute(reordered).is_err(),
        "Idempotency replays require exact request content, even for equivalent index order"
    );
    assert_eq!(e.state(), &state);
    Ok(())
}

#[test]
fn subset_batch_reprices_untouched_lines_and_retains_approval_cycles_and_contributors() -> TestResult
{
    let mut e = demo()?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    add(&mut e, price()?)?;
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 3,
        },
    )?;
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        batch(4, vec![replacement(0, price()?)]),
    );
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 4,
            approve: true,
            note: "Reviewed original document".into(),
        },
    )?;
    let approved = quote(&e)?.clone();
    reject_atomic(
        &mut e,
        actor("seller", Role::Admin),
        batch(5, vec![replacement(0, price()?)]),
    );
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteRevise {
            id: "Q".into(),
            version: 5,
        },
    )?;
    let mut product = e
        .state()
        .products
        .get("VLV-100")
        .ok_or("Missing product")?
        .clone();
    product.list_price = Money(12000);
    e.run(
        actor("pricing", Role::Analyst),
        Command::ProductPut(product),
    )?;
    let policy = Policy {
        min_margin: Bps(4500),
        max_discount: Bps(900),
        hard_min_margin: Bps(0),
    };
    e.run(
        actor("admin", Role::Admin),
        Command::PolicySet(policy.clone()),
    )?;
    let mut changed = price()?;
    changed.discount = Bps(500);
    let before = e.state().clone();
    let expected = vec![
        evaluate(&before, &changed)?,
        evaluate(&before, &approved.lines[1].request)?,
    ];
    e.run(
        actor("seller", Role::Admin),
        batch(6, vec![replacement(0, changed)]),
    )?;
    let q = quote(&e)?;
    assert_eq!(q.lines, expected);
    assert_eq!(q.policy, policy);
    assert_eq!(q.source_revision, before.revision);
    assert_eq!(q.version, 7);
    assert_eq!(q.status, Status::Draft);
    assert_eq!(q.decisions, approved.decisions);
    assert_eq!(q.governance, approved.governance);
    assert!(export_quote(q).is_err());
    assert_eq!(
        e.state().quote_contributors(q),
        q.governance
            .as_ref()
            .ok_or("Missing governance")?
            .contributors
    );
    e.run(
        actor("seller", Role::Sales),
        Command::QuoteSubmit {
            id: "Q".into(),
            version: 7,
        },
    )?;
    reject_atomic(
        &mut e,
        actor("seller", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 8,
            approve: true,
            note: "Author cannot review batch".into(),
        },
    );
    e.run(
        actor("reviewer", Role::Approver),
        Command::QuoteDecide {
            id: "Q".into(),
            version: 8,
            approve: true,
            note: "Reviewed batch replacement".into(),
        },
    )?;
    assert_eq!(
        quote(&e)?.governance.as_ref().map(|g| g.cycles.len()),
        Some(2)
    );
    e.state().validate()?;
    Ok(())
}

#[test]
fn converted_aggregate_overflow_cannot_partially_change_a_batch() -> TestResult {
    let mut e = demo()?;
    approve_rates(
        &mut e,
        vec![row(
            "FX",
            ConversionKind::Fx {
                from: Currency::USD,
                to: Currency::EUR,
            },
            1_000_000_000,
            1,
        )?],
    )?;
    let mut product = e
        .state()
        .products
        .get("VLV-100")
        .ok_or("Missing product")?
        .clone();
    product.list_price = Money(1_000_000);
    e.run(
        actor("pricing", Role::Analyst),
        Command::ProductPut(product),
    )?;
    create(&mut e)?;
    add(&mut e, price()?)?;
    add(&mut e, price()?)?;
    let mut huge = converted(Currency::EUR, "each")?;
    huge.quantity = Quantity(10_000_000);
    assert!(
        evaluate(e.state(), &huge).is_ok(),
        "One converted line must fit so the test reaches aggregate overflow"
    );
    reject_atomic(
        &mut e,
        actor("seller", Role::Sales),
        batch(3, vec![replacement(0, huge.clone()), replacement(1, huge)]),
    );
    Ok(())
}
