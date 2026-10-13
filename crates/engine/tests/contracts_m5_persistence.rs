//! Independent contract provenance, persistence, frozen selection and failure tests.
use copperline_engine::{Command, Engine, Request, evaluate, export_quote};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
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
fn price(source: Option<PriceSource>) -> Result<PriceRequest> {
    Ok(PriceRequest {
        product_id: "VLV-100".into(),
        customer_id: "C-ATLAS".into(),
        date: date("2026-10-13")?,
        quantity: Quantity(1000),
        discount: Bps(0),
        rebate: Bps(0),
        source,
    })
}
fn fixed_terms(id: &str, edition: u64, amount: i64) -> Result<ContractTerms> {
    Ok(ContractTerms {
        id: id.into(),
        edition,
        name: "Synthetic immutable terms".into(),
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
                invoice_unit: Money(amount),
            },
        }],
    })
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
fn create(e: &mut Engine, terms: ContractTerms) -> Result<()> {
    e.run(
        actor("Contract owner", Role::Analyst),
        Command::ContractCreate { terms },
    )?;
    Ok(())
}
fn submit(e: &mut Engine, id: &str, edition: u64) -> Result<()> {
    e.run(
        actor("Contract owner", Role::Analyst),
        Command::ContractSubmit {
            id: id.into(),
            edition,
            version: 1,
        },
    )?;
    Ok(())
}
fn decide(e: &mut Engine, id: &str, edition: u64, approve: bool, reviewer: &str) -> Result<()> {
    e.run(
        actor(reviewer, Role::Approver),
        Command::ContractDecide {
            id: id.into(),
            edition,
            version: 2,
            approve,
            note: "Synthetic independent commercial review".into(),
        },
    )?;
    Ok(())
}
fn approve(e: &mut Engine, terms: ContractTerms) -> Result<()> {
    let id = terms.id.clone();
    let edition = terms.edition;
    create(e, terms)?;
    submit(e, &id, edition)?;
    decide(e, &id, edition, true, "Contract reviewer")
}
fn grid(e: &mut Engine, discount: Bps) -> Result<PriceSource> {
    let version = e.state().grid.get("G").map(|g| g.version).unwrap_or(0);
    let mut request = price(None)?;
    request.discount = discount;
    e.run(
        actor("Origin writer", Role::Analyst),
        Command::GridCalculate {
            id: "G".into(),
            version,
            request,
        },
    )?;
    e.run(
        actor("Origin writer", Role::Analyst),
        Command::GridSubmit {
            id: "G".into(),
            version: version + 1,
        },
    )?;
    e.run(
        actor("Origin reviewer", Role::Approver),
        Command::GridDecide {
            id: "G".into(),
            version: version + 2,
            approve: true,
            note: "Synthetic origin review".into(),
        },
    )?;
    Ok(PriceSource::Grid {
        id: "G".into(),
        approval_revision: e.state().revision,
    })
}
fn origin_terms(source: PriceSource) -> Result<ContractTerms> {
    let mut terms = fixed_terms("C", 1, 9000)?;
    terms.rows[0].basis = ContractBasis::ApprovedSource {
        request: price(Some(source))?,
    };
    Ok(terms)
}
fn approved_quote(e: &mut Engine, source: PriceSource) -> Result<Request> {
    e.run(
        actor("Seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic frozen source quote".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    let request = Request {
        request_id: "quote-line-retry".into(),
        expected_revision: e.state().revision,
        actor: actor("Seller", Role::Sales),
        action: Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: price(Some(source))?,
        },
    };
    e.execute(request.clone())?;
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
            note: "Independent quote review".into(),
        },
    )?;
    Ok(request)
}

#[test]
fn fixed_and_origin_terms_remain_bound_to_exact_creation_and_review_audits() -> TestResult {
    let mut e = fixture()?;
    let source = grid(&mut e, Bps(500))?;
    approve(&mut e, origin_terms(source)?)?;
    for case in 0..15 {
        let mut invalid = e.state().clone();
        let c = invalid
            .price_contracts
            .get_mut(&contract_key("C", 1))
            .ok_or("Missing contract")?;
        match case {
            0 => c.terms.name = "Changed justification".into(),
            1 => c.terms.priority += 1,
            2 => c.terms.end = date("2028-01-01")?,
            3 => c.terms.rows[0].max_quantity = Some(Quantity(999_000)),
            4 => c.rows[0].invoice_unit = Money(1),
            5 => c.rows[0].origin = None,
            6 => {
                c.rows[0]
                    .origin
                    .as_mut()
                    .ok_or("Missing origin")?
                    .captured_revision -= 1
            }
            7 => c.contributors.clear(),
            8 => c.owner = "Different owner".into(),
            9 => c.review.as_mut().ok_or("Missing review")?.note = "Changed review note".into(),
            10 => c.review.as_mut().ok_or("Missing review")?.actor.role = Role::Admin,
            11 => c.status = RecordStatus::Rejected,
            12 => c.submitted_revision = None,
            13 => c.version = u64::MAX,
            _ => {
                let revision = c.review.as_ref().ok_or("Missing review")?.revision;
                invalid
                    .audit
                    .iter_mut()
                    .find(|a| a.revision == revision)
                    .ok_or("Missing review audit")?
                    .target = Some(contract_target("OTHER", 1));
            }
        }
        assert!(
            invalid.validate().is_err(),
            "Accepted immutable contract mutation {case}"
        );
    }
    Ok(())
}

#[test]
fn owner_and_origin_author_cannot_review_and_wrong_roles_cannot_substitute() -> TestResult {
    let mut e = fixture()?;
    let source = grid(&mut e, Bps(500))?;
    create(&mut e, origin_terms(source)?)?;
    let before = e.state().clone();
    for command in [
        Command::ContractSubmit {
            id: "C".into(),
            edition: 1,
            version: 1,
        },
        Command::ContractWithdraw {
            id: "C".into(),
            edition: 1,
            version: 1,
        },
    ] {
        assert!(e.run(actor("Other Admin", Role::Admin), command).is_err());
        assert_eq!(e.state(), &before);
    }
    submit(&mut e, "C", 1)?;
    let before = e.state().clone();
    for name in ["Contract owner", "Origin writer"] {
        assert!(decide(&mut e, "C", 1, true, name).is_err());
        assert_eq!(e.state(), &before);
    }
    for role in [Role::Finance, Role::Admin, Role::Analyst, Role::Sales] {
        assert!(
            e.run(
                actor("Independent wrong role", role),
                Command::ContractDecide {
                    id: "C".into(),
                    edition: 1,
                    version: 2,
                    approve: true,
                    note: "Wrong role".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
    }
    decide(&mut e, "C", 1, true, "Origin reviewer")?;
    assert_eq!(e.state().contract("C", 1)?.status, RecordStatus::Approved);
    Ok(())
}

#[test]
fn replacing_grid_origin_preserves_contract_price_authorship_and_future_quotes() -> TestResult {
    let mut e = fixture()?;
    let source = grid(&mut e, Bps(500))?;
    approve(&mut e, origin_terms(source)?)?;
    let contract = e.state().contract("C", 1)?.clone();
    grid(&mut e, Bps(1000))?;
    assert_eq!(e.state().contract("C", 1)?, &contract);
    let evaluated = evaluate(e.state(), &price(Some(PriceSource::CustomerContract))?)?;
    assert_eq!(evaluated.invoice_unit, Money(9500));
    assert_eq!(
        evaluated.source.as_ref().map(|s| s.source.clone()),
        Some(PriceSource::Contract {
            id: "C".into(),
            edition: 1
        })
    );
    e.state().validate()?;
    let mut invalid = e.state().clone();
    invalid
        .grid
        .get_mut("G")
        .ok_or("Missing grid")?
        .approved_history
        .clear();
    assert!(
        invalid.validate().is_err(),
        "Origin history used by immutable contract was erased"
    );
    Ok(())
}

#[test]
fn automatic_quote_and_export_remain_frozen_after_successors_and_higher_priorities() -> TestResult {
    let mut e = fixture()?;
    approve(&mut e, fixed_terms("C", 1, 9500)?)?;
    let request = approved_quote(&mut e, PriceSource::CustomerContract)?;
    let quote = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let csv = export_quote(&quote)?;
    let mut successor = fixed_terms("C", 2, 9000)?;
    successor.start = date("2026-10-01")?;
    approve(&mut e, successor)?;
    let mut preferred = fixed_terms("PREFERRED", 1, 8500)?;
    preferred.priority = 20;
    approve(&mut e, preferred)?;
    assert_eq!(
        evaluate(e.state(), &price(Some(PriceSource::CustomerContract))?)?.invoice_unit,
        Money(8500)
    );
    assert_eq!(e.state().quotes.get("Q"), Some(&quote));
    assert_eq!(
        export_quote(e.state().quotes.get("Q").ok_or("Missing quote")?)?,
        csv
    );
    let replay = e.execute(request)?;
    assert!(replay.replayed);
    assert_eq!(e.state().quotes.get("Q"), Some(&quote));
    let mut invalid = e.state().clone();
    let current = invalid.revision;
    let q = invalid.quotes.get_mut("Q").ok_or("Missing quote")?;
    q.lines[0]
        .source
        .as_mut()
        .ok_or("Missing evidence")?
        .captured_revision = current;
    assert!(
        invalid.validate().is_err(),
        "Old auto-selection cannot claim capture after newer higher-priority approval"
    );
    Ok(())
}

#[test]
fn editions_and_records_cannot_be_erased_retargeted_or_reviewed_out_of_order() -> TestResult {
    let mut e = fixture()?;
    create(&mut e, fixed_terms("C", 1, 9500)?)?;
    create(&mut e, fixed_terms("C", 2, 9000)?)?;
    submit(&mut e, "C", 1)?;
    submit(&mut e, "C", 2)?;
    decide(&mut e, "C", 2, true, "Contract reviewer")?;
    let before = e.state().clone();
    assert!(decide(&mut e, "C", 1, true, "Contract reviewer").is_err());
    assert_eq!(e.state(), &before);
    decide(&mut e, "C", 1, false, "Contract reviewer")?;
    for case in 0..4 {
        let mut invalid = e.state().clone();
        match case {
            0 => {
                invalid.price_contracts.remove(&contract_key("C", 1));
            }
            1 => {
                invalid.price_contracts.clear();
                for a in &mut invalid.audit {
                    if a.target
                        .as_deref()
                        .is_some_and(|t| t.starts_with("contract:"))
                    {
                        a.command = "customer_put".into();
                    }
                }
            }
            2 => {
                let c = invalid
                    .price_contracts
                    .get_mut(&contract_key("C", 2))
                    .ok_or("Missing contract")?;
                c.review = e.state().contract("C", 1)?.review.clone();
            }
            _ => {
                invalid
                    .price_contracts
                    .get_mut(&contract_key("C", 2))
                    .ok_or("Missing contract")?
                    .created_revision = e.state().contract("C", 1)?.created_revision;
            }
        }
        assert!(
            invalid.validate().is_err(),
            "Accepted erased or retargeted contract history {case}"
        );
    }
    Ok(())
}

#[test]
fn automatic_source_cannot_be_substituted_with_a_valid_lower_priority_contract() -> TestResult {
    let mut e = fixture()?;
    approve(&mut e, fixed_terms("LOWER", 1, 9500)?)?;
    let mut higher = fixed_terms("HIGHER", 1, 9000)?;
    higher.priority = 20;
    approve(&mut e, higher)?;
    e.run(
        actor("Seller", Role::Sales),
        Command::QuoteCreate {
            id: "Q".into(),
            title: "Synthetic draft for source substitution".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
        },
    )?;
    e.run(
        actor("Seller", Role::Sales),
        Command::QuoteAddLine {
            id: "Q".into(),
            version: 1,
            request: price(Some(PriceSource::CustomerContract))?,
        },
    )?;
    let mut lower = evaluate(
        e.state(),
        &price(Some(PriceSource::Contract {
            id: "LOWER".into(),
            edition: 1,
        }))?,
    )?;
    lower.request.source = Some(PriceSource::CustomerContract);
    // This is structurally valid evidence for a real approved source. Only the
    // historical auto-selection comparison can detect that it was not the winner.
    lower.validate_snapshot()?;
    let mut invalid = e.state().clone();
    let revision = invalid.revision;
    let quote = invalid.quotes.get_mut("Q").ok_or("Missing quote")?;
    quote.source_revision = revision;
    quote.lines = vec![lower];
    let problem = invalid
        .validate()
        .err()
        .ok_or("Accepted lower-priority substitution")?;
    assert!(
        problem
            .to_string()
            .contains("Frozen source evidence differs"),
        "Unexpected rejection: {problem}"
    );
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-contract-persistence-{}-{}",
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
fn failed_commit_leaves_contract_receipts_and_memory_unchanged_then_exact_retry_commits_once()
-> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    initialize(&mut e)?;
    let request = Request {
        request_id: "contract-once".into(),
        expected_revision: e.state().revision,
        actor: actor("Contract owner", Role::Analyst),
        action: Command::ContractCreate {
            terms: fixed_terms("C", 1, 9500)?,
        },
    };
    let before = e.state().clone();
    let pending = format!("{:020}.pending", before.revision + 1);
    let evidence = b"synthetic interrupted bytes";
    fs::write(dir.0.join(&pending), evidence)?;
    assert!(e.execute(request.clone()).is_err());
    assert_eq!(e.state(), &before);
    assert_eq!(fs::read(dir.0.join(&pending))?, evidence);
    drop(e);
    let (store, state) = copperline_storage::Store::open(&dir.0)?;
    assert_eq!(state, before);
    let preserved =
        store.quarantine_pending(&pending, &copperline_storage::fingerprint(evidence))?;
    drop(store);
    let mut e = Engine::open(&dir.0)?;
    let result = e.execute(request.clone())?;
    assert!(!result.replayed);
    assert_eq!(result.revision, before.revision + 1);
    let committed = e.state().clone();
    drop(e);
    let mut e = Engine::open(&dir.0)?;
    let retry = e.execute(request.clone())?;
    assert!(retry.replayed);
    assert_eq!(retry.revision, result.revision);
    assert_eq!(e.state(), &committed);
    let mut changed = request;
    changed.actor.name = "Different owner".into();
    assert!(e.execute(changed).is_err());
    assert_eq!(e.state(), &committed);
    assert_eq!(fs::read(preserved.preserved_path)?, evidence);
    Ok(())
}

#[test]
fn old_m4_source_snapshot_bytes_and_retry_fingerprints_survive_first_contract_and_reopen()
-> TestResult {
    let dir = Workspace::new()?;
    let mut e = Engine::open(&dir.0)?;
    initialize(&mut e)?;
    let source = grid(&mut e, Bps(500))?;
    let replay_request = approved_quote(&mut e, source.clone())?;
    e.run(
        actor("Invoice writer", Role::Analyst),
        Command::TransactionAdd(Transaction {
            id: "T".into(),
            source: "synthetic-m4".into(),
            product_id: "VLV-100".into(),
            customer_id: "C-ATLAS".into(),
            date: date("2026-10-13")?,
            quantity: Quantity(1000),
            unit: "each".into(),
            currency: Currency::USD,
            invoice_total: Money(9500),
            cost_total: Money(6000),
        }),
    )?;
    e.run(
        actor("Correction writer", Role::Analyst),
        Command::CorrectionCreate {
            proposal: CorrectionProposal {
                id: "VOID".into(),
                original_transaction_id: "T".into(),
                effective_date: date("2026-10-14")?,
                reason: "Synthetic M4 correction".into(),
                replacement: None,
            },
        },
    )?;
    e.run(
        actor("Correction writer", Role::Analyst),
        Command::CorrectionSubmit {
            id: "VOID".into(),
            version: 1,
        },
    )?;
    e.run(
        actor("Finance", Role::Finance),
        Command::CorrectionDecide {
            id: "VOID".into(),
            version: 2,
            approve: true,
            note: "Independent correction review".into(),
        },
    )?;
    let quote = e.state().quotes.get("Q").cloned().ok_or("Missing quote")?;
    let csv = export_quote(&quote)?;
    let revision = e.state().revision;
    drop(e);
    let mut old = Vec::new();
    for n in 0..=revision {
        let path = dir.0.join(format!("{n:020}.json"));
        let bytes = fs::read(&path)?;
        let envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert!(envelope["state"].get("price_contracts").is_none());
        let encoded = std::str::from_utf8(&bytes)?;
        assert!(!encoded.contains("\"contract\":"));
        old.push((path, bytes));
    }
    let mut e = Engine::open(&dir.0)?;
    approve(&mut e, origin_terms(source)?)?;
    assert!(e.execute(replay_request.clone())?.replayed);
    assert_eq!(e.state().quotes.get("Q"), Some(&quote));
    assert_eq!(
        export_quote(e.state().quotes.get("Q").ok_or("Missing quote")?)?,
        csv
    );
    for (path, bytes) in old {
        assert_eq!(fs::read(path)?, bytes);
    }
    let after = e.state().clone();
    drop(e);
    let mut e = Engine::open(&dir.0)?;
    assert_eq!(e.state(), &after);
    assert!(e.execute(replay_request)?.replayed);
    assert_eq!(e.state(), &after);
    Ok(())
}

#[test]
fn maximum_contract_id_and_names_fit_fingerprinted_audit_limits() -> TestResult {
    let mut e = fixture()?;
    let id = "C".repeat(64);
    let mut terms = fixed_terms(&id, 1, 9500)?;
    terms.name = "N".repeat(160);
    approve(&mut e, terms)?;
    assert_eq!(e.state().contract(&id, 1)?.status, RecordStatus::Approved);
    e.state().validate()?;
    Ok(())
}
