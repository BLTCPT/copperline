//! Independent regression coverage for hostile workspace files and valid-checksum corruption.
use copperline_model::{Customer, Quote, State};
use copperline_storage::{Store, fingerprint};
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
            "copperline-storage-review-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
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
fn pending_file_is_preserved_and_explicit_removal_allows_retry() -> TestResult {
    let workspace = Workspace::new()?;
    let pending = workspace.0.join("00000000000000000000.pending");
    let committed = workspace.0.join("00000000000000000000.json");
    fs::write(&pending, b"recoverable interrupted write")?;
    let (mut store, state) = Store::open(&workspace.0)?;
    assert!(store.commit(&state).is_err());
    assert_eq!(fs::read(&pending)?, b"recoverable interrupted write");
    assert!(!committed.exists());
    fs::remove_file(&pending)?;
    store.commit(&state)?;
    assert!(committed.is_file());
    drop(store);
    let (_store, reopened) = Store::open(&workspace.0)?;
    assert_eq!(reopened, state);
    Ok(())
}

#[test]
fn directory_with_committed_filename_is_rejected() -> TestResult {
    let workspace = Workspace::new()?;
    let path = workspace.0.join("00000000000000000000.json");
    fs::create_dir(&path)?;
    assert!(Store::open(&workspace.0).is_err());
    assert!(path.is_dir());
    Ok(())
}

#[cfg(unix)]
#[test]
fn pending_symlink_does_not_overwrite_target() -> TestResult {
    let workspace = Workspace::new()?;
    let outside = Workspace::new()?;
    let sentinel = outside.0.join("valuable.txt");
    fs::write(&sentinel, b"preserve unrelated user data")?;
    let pending = workspace.0.join("00000000000000000000.pending");
    std::os::unix::fs::symlink(&sentinel, &pending)?;
    let (mut store, state) = Store::open(&workspace.0)?;
    assert!(store.commit(&state).is_err());
    assert_eq!(fs::read(&sentinel)?, b"preserve unrelated user data");
    assert!(fs::symlink_metadata(pending)?.file_type().is_symlink());
    assert!(!workspace.0.join("00000000000000000000.json").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_lock_and_committed_snapshot_are_rejected() -> TestResult {
    let source = Workspace::new()?;
    let (mut source_store, state) = Store::open(&source.0)?;
    source_store.commit(&state)?;
    drop(source_store);
    let lock_workspace = Workspace::new()?;
    std::os::unix::fs::symlink(
        source.0.join("workspace.lock"),
        lock_workspace.0.join("workspace.lock"),
    )?;
    assert!(Store::open(&lock_workspace.0).is_err());
    let snapshot_workspace = Workspace::new()?;
    std::os::unix::fs::symlink(
        source.0.join("00000000000000000000.json"),
        snapshot_workspace.0.join("00000000000000000000.json"),
    )?;
    assert!(Store::open(&snapshot_workspace.0).is_err());
    let (_store, reopened) = Store::open(&source.0)?;
    assert_eq!(reopened, state);
    Ok(())
}

#[test]
fn invalid_approved_quote_is_rejected_on_commit_and_valid_checksum_reopen() -> TestResult {
    let mut invalid = State::default();
    invalid.customers.insert(
        "C".into(),
        Customer {
            id: "C".into(),
            name: "Synthetic customer".into(),
            segment: "OEM".into(),
        },
    );
    let quote: Quote = serde_json::from_value(serde_json::json!({
        "id": "Q", "title": "Impossible approval", "owner": "seller", "customer_id": "C",
        "date": "2026-10-13", "version": 1, "source_revision": 0,
        "policy": {"min_margin": 2500, "max_discount": 1500, "hard_min_margin": 0},
        "status": "approved", "lines": [], "decisions": [], "submitted_by": "seller"
    }))?;
    invalid.quotes.insert("Q".into(), quote);
    let workspace = Workspace::new()?;
    let (mut store, valid) = Store::open(&workspace.0)?;
    assert!(store.commit(&invalid).is_err());
    assert!(!workspace.0.join("00000000000000000000.json").exists());
    store.commit(&valid)?;
    drop(store);

    // A valid semantic checksum must not substitute for business validation.
    let encoded_state = serde_json::to_vec(&invalid)?;
    let envelope = serde_json::json!({"previous_hash": "", "hash": fingerprint(&encoded_state), "state": invalid});
    let snapshot = workspace.0.join("00000000000000000000.json");
    let bytes = serde_json::to_vec(&envelope)?;
    fs::write(&snapshot, &bytes)?;
    assert!(Store::open(&workspace.0).is_err());
    assert_eq!(fs::read(snapshot)?, bytes);
    Ok(())
}
