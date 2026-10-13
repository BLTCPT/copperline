//! Validation evidence must preserve the checked commit and all-history read boundaries.
use copperline_model::{Actor, Audit, Cost, Customer, Product, Receipt, Role, State};
use copperline_storage::{MAX_BYTES, Store, fingerprint};
use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-validated-storage-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn snapshot(&self, revision: u64) -> PathBuf {
        self.0.join(format!("{revision:020}.json"))
    }
    fn pending(&self, revision: u64) -> PathBuf {
        self.0.join(format!("{revision:020}.pending"))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// The pre-M7 owned envelope and canonical checksum calculation are deliberately
// independent of the private borrowed envelope used by production commits.
#[derive(Serialize)]
struct OwnedEnvelope {
    previous_hash: String,
    hash: String,
    state: State,
}
fn owned_bytes(previous: &str, state: &State) -> serde_json::Result<(String, Vec<u8>)> {
    let mut input = previous.as_bytes().to_vec();
    input.extend(serde_json::to_vec(state)?);
    let hash = fingerprint(&input);
    let bytes = serde_json::to_vec(&OwnedEnvelope {
        previous_hash: previous.into(),
        hash: hash.clone(),
        state: state.clone(),
    })?;
    Ok((hash, bytes))
}
fn advance(state: &mut State, revision: u64) {
    state.revision = revision;
    state.audit.push(Audit {
        target: None,
        revision,
        actor: Actor {
            name: "Synthetic fixture author".into(),
            role: Role::Admin,
        },
        command: "test_fixture".into(),
        detail: "Synthetic storage fixture revision".into(),
    });
    state.receipts.push(Receipt {
        request_id: format!("fixture-{revision}"),
        revision,
        fingerprint: "1".repeat(64),
    });
    let id = format!("C-{revision}");
    state.customers.insert(
        id.clone(),
        Customer {
            id,
            name: format!("Synthetic customer {revision}"),
            segment: "OEM".into(),
        },
    );
}

#[test]
fn borrowed_and_checked_commits_match_original_envelope_bytes() -> TestResult {
    let checked = Workspace::new()?;
    let borrowed = Workspace::new()?;
    let (mut checked_store, mut state) = Store::open(&checked.0)?;
    let (mut borrowed_store, _) = Store::open(&borrowed.0)?;
    let mut previous = String::new();
    for revision in 0..4 {
        if revision > 0 {
            advance(&mut state, revision);
        }
        let token = state.validated()?;
        assert!(std::ptr::eq(token.state(), &state));
        checked_store.commit(&state)?;
        borrowed_store.commit_validated(token)?;
        let (hash, expected) = owned_bytes(&previous, &state)?;
        assert_eq!(fs::read(checked.snapshot(revision))?, expected);
        assert_eq!(fs::read(borrowed.snapshot(revision))?, expected);
        previous = hash;
    }
    drop(checked_store);
    drop(borrowed_store);
    let (_checked, checked_state) = Store::open(&checked.0)?;
    let (_borrowed, borrowed_state) = Store::open(&borrowed.0)?;
    assert_eq!(checked_state, state);
    assert_eq!(borrowed_state, state);
    Ok(())
}

#[test]
fn invalid_state_cannot_obtain_token_or_enter_public_checked_commit() -> TestResult {
    let workspace = Workspace::new()?;
    let (mut store, state) = Store::open(&workspace.0)?;
    let mut invalid = state.clone();
    invalid.schema_version = 99;
    assert!(invalid.validated().is_err());
    assert!(store.commit(&invalid).is_err());
    assert!(!workspace.snapshot(0).exists());
    assert!(!workspace.pending(0).exists());
    store.commit_validated(state.validated()?)?;
    Ok(())
}

#[test]
fn validated_commit_keeps_revision_and_pending_failure_atomicity() -> TestResult {
    let workspace = Workspace::new()?;
    let (mut store, mut state) = Store::open(&workspace.0)?;
    store.commit_validated(state.validated()?)?;
    let initial = fs::read(workspace.snapshot(0))?;
    assert!(store.commit_validated(state.validated()?).is_err());
    advance(&mut state, 1);
    let mut skipped = state.clone();
    advance(&mut skipped, 2);
    assert!(store.commit_validated(skipped.validated()?).is_err());
    assert!(!workspace.snapshot(2).exists());
    assert!(!workspace.pending(2).exists());

    fs::write(workspace.pending(1), b"preserve interrupted write")?;
    assert!(store.commit_validated(state.validated()?).is_err());
    assert_eq!(fs::read(workspace.snapshot(0))?, initial);
    assert_eq!(
        fs::read(workspace.pending(1))?,
        b"preserve interrupted write"
    );
    assert!(!workspace.snapshot(1).exists());
    fs::remove_file(workspace.pending(1))?;
    store.commit_validated(state.validated()?)?;
    drop(store);
    let (_store, reopened) = Store::open(&workspace.0)?;
    assert_eq!(reopened, state);
    Ok(())
}

#[cfg(unix)]
#[test]
fn validated_commit_preserves_pending_symlink_and_its_target() -> TestResult {
    let workspace = Workspace::new()?;
    let outside = Workspace::new()?;
    let target = outside.0.join("unrelated.txt");
    fs::write(&target, b"unrelated data")?;
    std::os::unix::fs::symlink(&target, workspace.pending(0))?;
    let (mut store, state) = Store::open(&workspace.0)?;
    assert!(store.commit_validated(state.validated()?).is_err());
    assert_eq!(fs::read(target)?, b"unrelated data");
    assert!(
        fs::symlink_metadata(workspace.pending(0))?
            .file_type()
            .is_symlink()
    );
    assert!(!workspace.snapshot(0).exists());
    Ok(())
}

#[test]
fn valid_tip_and_recomputed_chain_do_not_hide_invalid_middle_revision() -> TestResult {
    let workspace = Workspace::new()?;
    let mut state = State::default();
    let mut previous = String::new();
    let mut saved = vec![];
    for revision in 0..3 {
        if revision > 0 {
            advance(&mut state, revision);
        }
        state.validate()?;
        let mut on_disk = state.clone();
        if revision == 1 {
            on_disk.schema_version = 99;
        }
        let (hash, bytes) = owned_bytes(&previous, &on_disk)?;
        fs::write(workspace.snapshot(revision), &bytes)?;
        saved.push(bytes);
        previous = hash;
    }
    // The final State is independently valid, and every envelope hash and link
    // matches. Only validation of the middle historical State can reject this.
    state.validate()?;
    let result = Store::open(&workspace.0);
    assert!(result.is_err());
    if let Err(error) = result {
        assert!(error.to_string().contains("Unsupported store schema"));
    }
    for (revision, expected) in saved.iter().enumerate() {
        assert_eq!(fs::read(workspace.snapshot(revision as u64))?, *expected);
    }
    Ok(())
}

#[test]
fn validation_evidence_does_not_bypass_snapshot_size_limit() -> TestResult {
    let workspace = Workspace::new()?;
    let (mut store, empty) = Store::open(&workspace.0)?;
    let mut large = empty.clone();
    let product: Product = serde_json::from_value(serde_json::json!({
        "id": "P".repeat(64), "name": "N".repeat(160), "family": "F".repeat(160),
        "unit": "U".repeat(64), "currency": "USD", "list_price": 100
    }))?;
    for index in 0..10_000 {
        let mut p = product.clone();
        p.id = format!("P{index:063}");
        large.products.insert(p.id.clone(), p);
    }
    let cost: Cost = serde_json::from_value(serde_json::json!({
        "product_id": format!("P{:063}", 0), "version": 1,
        "effective_from": "2026-10-13", "amount": 50,
        "currency": "USD", "unit": "U".repeat(64)
    }))?;
    for version in 1..=50_000 {
        let mut c = cost.clone();
        c.version = version;
        large.costs.push(c);
    }
    let validated = large.validated()?;
    assert!(serde_json::to_vec(&large)?.len() as u64 > MAX_BYTES);
    let result = store.commit_validated(validated);
    assert!(result.is_err());
    if let Err(error) = result {
        assert!(error.to_string().contains("16 MiB snapshot limit"));
    }
    assert!(!workspace.snapshot(0).exists());
    assert!(!workspace.pending(0).exists());
    store.commit_validated(empty.validated()?)?;
    drop(store);
    let (_store, reopened) = Store::open(&workspace.0)?;
    assert_eq!(reopened, empty);
    Ok(())
}
