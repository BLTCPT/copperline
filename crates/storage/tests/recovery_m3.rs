//! Interrupted-write recovery preserves evidence and never promotes uncommitted state.
use copperline_storage::{MAX_BYTES, Store, fingerprint};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
const PENDING: &str = "00000000000000000000.pending";
const COMMITTED: &str = "00000000000000000000.json";
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-recovery-m3-{}-{}",
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
fn interrupted_write_is_quarantined_then_retry_reopens_without_losing_evidence() -> TestResult {
    let workspace = Workspace::new()?;
    let bytes = b"{\"previous_hash\":\"\",\"hash\":\"interrupted";
    fs::write(workspace.0.join(PENDING), bytes)?;
    fs::write(workspace.0.join("unknown.pending"), b"unrelated evidence")?;
    let (mut store, state) = Store::open(&workspace.0)?;
    assert!(Store::open(&workspace.0).is_err());
    let inspection = store.inspect_pending()?;
    assert_eq!(inspection.committed_revision, None);
    assert_eq!(inspection.next_revision, 0);
    assert_eq!(inspection.pending.len(), 2);
    let pending = inspection.pending.first().ok_or("Missing pending report")?;
    assert!(pending.eligible_for_quarantine);
    assert_eq!(pending.sha256.as_deref(), Some(fingerprint(bytes).as_str()));
    assert!(
        pending
            .snapshot_status
            .as_deref()
            .is_some_and(|s| s.starts_with("Incomplete or invalid"))
    );
    assert!(store.commit(&state).is_err());
    assert!(store.quarantine_pending(PENDING, &"0".repeat(64)).is_err());
    assert_eq!(fs::read(workspace.0.join(PENDING))?, bytes);
    assert!(!workspace.0.join("recovery").exists());
    let recovered = store.quarantine_pending(PENDING, &fingerprint(bytes))?;
    assert_eq!(fs::read(&recovered.preserved_path)?, bytes);
    assert!(!workspace.0.join(PENDING).exists());
    assert!(!workspace.0.join(COMMITTED).exists());
    assert_eq!(
        fs::read(workspace.0.join("unknown.pending"))?,
        b"unrelated evidence"
    );
    store.commit(&state)?;
    drop(store);
    let (_store, reopened) = Store::open(&workspace.0)?;
    assert_eq!(reopened, state);
    assert_eq!(fs::read(recovered.preserved_path)?, bytes);
    Ok(())
}

#[test]
fn complete_candidate_is_preserved_without_promotion_and_recovery_never_overwrites() -> TestResult {
    let workspace = Workspace::new()?;
    let (mut store, state) = Store::open(&workspace.0)?;
    store.commit(&state)?;
    drop(store);
    fs::rename(workspace.0.join(COMMITTED), workspace.0.join(PENDING))?;
    let bytes = fs::read(workspace.0.join(PENDING))?;
    let (store, _) = Store::open(&workspace.0)?;
    let report = store.inspect_pending()?;
    assert!(
        report
            .pending
            .first()
            .and_then(|p| p.snapshot_status.as_deref())
            .is_some_and(|s| s.starts_with("Complete uncommitted"))
    );
    let first = store.quarantine_pending(PENDING, &fingerprint(&bytes))?;
    fs::write(workspace.0.join(PENDING), &bytes)?;
    let second = store.quarantine_pending(PENDING, &fingerprint(&bytes))?;
    assert_ne!(first.preserved_path, second.preserved_path);
    assert_eq!(fs::read(first.preserved_path)?, bytes);
    assert_eq!(fs::read(second.preserved_path)?, bytes);
    assert!(!workspace.0.join(COMMITTED).exists());
    Ok(())
}

#[test]
fn unknown_future_and_oversized_pending_files_remain_untouched() -> TestResult {
    let workspace = Workspace::new()?;
    let future = "00000000000000000001.pending";
    fs::write(workspace.0.join(future), b"future evidence")?;
    fs::write(workspace.0.join("unknown.pending"), b"unknown evidence")?;
    let oversized = fs::File::create(workspace.0.join(PENDING))?;
    oversized.set_len(MAX_BYTES + 1)?;
    let (store, _) = Store::open(&workspace.0)?;
    let report = store.inspect_pending()?;
    assert!(report.pending.iter().all(|p| !p.eligible_for_quarantine));
    assert_eq!(
        report.pending.first().map(|p| p.kind.as_str()),
        Some("oversized")
    );
    assert!(
        store
            .quarantine_pending(future, &fingerprint(b"future evidence"))
            .is_err()
    );
    assert!(
        store
            .quarantine_pending("../unknown.pending", &fingerprint(b"unknown evidence"))
            .is_err()
    );
    assert!(store.quarantine_pending(PENDING, &"0".repeat(64)).is_err());
    assert_eq!(
        fs::metadata(workspace.0.join(PENDING))?.len(),
        MAX_BYTES + 1
    );
    assert_eq!(fs::read(workspace.0.join(future))?, b"future evidence");
    assert!(!workspace.0.join("recovery").exists());
    Ok(())
}

#[test]
fn corrupt_committed_history_prevents_recovery_and_preserves_all_files() -> TestResult {
    let workspace = Workspace::new()?;
    fs::write(workspace.0.join(COMMITTED), b"corrupt committed revision")?;
    fs::write(workspace.0.join(PENDING), b"interrupted evidence")?;
    assert!(Store::open(&workspace.0).is_err());
    assert_eq!(
        fs::read(workspace.0.join(COMMITTED))?,
        b"corrupt committed revision"
    );
    assert_eq!(
        fs::read(workspace.0.join(PENDING))?,
        b"interrupted evidence"
    );
    assert!(!workspace.0.join("recovery").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_evidence_and_recovery_directory_are_rejected() -> TestResult {
    let workspace = Workspace::new()?;
    let outside = Workspace::new()?;
    let sentinel = outside.0.join("sentinel.txt");
    fs::write(&sentinel, b"outside evidence")?;
    std::os::unix::fs::symlink(&sentinel, workspace.0.join(PENDING))?;
    let (store, _) = Store::open(&workspace.0)?;
    let report = store.inspect_pending()?;
    assert_eq!(
        report.pending.first().map(|p| p.kind.as_str()),
        Some("symlink")
    );
    assert!(
        store
            .quarantine_pending(PENDING, &fingerprint(b"outside evidence"))
            .is_err()
    );
    fs::remove_file(workspace.0.join(PENDING))?;
    fs::write(workspace.0.join(PENDING), b"partial evidence")?;
    std::os::unix::fs::symlink(&outside.0, workspace.0.join("recovery"))?;
    assert!(
        store
            .quarantine_pending(PENDING, &fingerprint(b"partial evidence"))
            .is_err()
    );
    assert_eq!(fs::read(workspace.0.join(PENDING))?, b"partial evidence");
    assert_eq!(fs::read(sentinel)?, b"outside evidence");
    Ok(())
}
