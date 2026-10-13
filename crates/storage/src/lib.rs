//! Single-writer, append-only committed snapshots. A pending write is never a committed revision.
use copperline_model::{Result, State, ensure, error};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Envelope {
    previous_hash: String,
    hash: String,
    state: State,
}
pub fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash(previous: &str, state: &State) -> Result<String> {
    let mut b = previous.as_bytes().to_vec();
    b.extend(serde_json::to_vec(state).map_err(|e| error(e.to_string()))?);
    Ok(fingerprint(&b))
}
fn io(e: std::io::Error) -> copperline_model::Error {
    error(format!("Storage: {e}"))
}
pub struct Store {
    root: PathBuf,
    _lock: File,
    revision: u64,
    hash: String,
    has_state: bool,
}
#[derive(Debug, Serialize)]
pub struct PendingInspection {
    pub name: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub eligible_for_quarantine: bool,
    pub snapshot_status: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct StorageInspection {
    pub committed_revision: Option<u64>,
    pub next_revision: u64,
    pub pending: Vec<PendingInspection>,
}
#[derive(Debug, Serialize)]
pub struct QuarantineOutcome {
    pub preserved_path: PathBuf,
    pub sha256: String,
    pub next_revision: u64,
    pub warnings: Vec<String>,
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, State)> {
        let root = path.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(io)?;
        let lock_path = root.join("workspace.lock");
        if let Ok(meta) = fs::symlink_metadata(&lock_path) {
            ensure(
                meta.is_file() && !meta.file_type().is_symlink(),
                "Workspace lock must be a regular file",
            )?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("workspace.lock"))
            .map_err(io)?;
        lock.try_lock()
            .map_err(|_| error("Workspace is already open in another process; close it first"))?;
        let mut paths = vec![];
        for entry in fs::read_dir(&root).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let meta = fs::symlink_metadata(&path).map_err(io)?;
                ensure(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "Committed snapshot must be a regular file",
                )?;
                paths.push(path);
            }
            ensure(paths.len() <= 100_000, "Too many persisted revisions")?;
        }
        paths.sort();
        let mut state = State::default();
        let mut previous = String::new();
        let mut has_state = false;
        for (i, path) in paths.iter().enumerate() {
            let f = File::open(path).map_err(io)?;
            ensure(
                f.metadata().map_err(io)?.len() <= MAX_BYTES,
                "Snapshot exceeds 16 MiB limit",
            )?;
            let mut bytes = vec![];
            f.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(io)?;
            ensure(bytes.len() as u64 <= MAX_BYTES, "Snapshot too large")?;
            let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|e| {
                error(format!(
                    "Corrupt committed snapshot {}: {e}",
                    path.display()
                ))
            })?;
            ensure(
                envelope.state.revision == i as u64,
                "Revision chain has a gap",
            )?;
            ensure(
                path.file_name().and_then(|v| v.to_str())
                    == Some(format!("{:020}.json", envelope.state.revision).as_str()),
                "Unexpected snapshot filename",
            )?;
            ensure(
                envelope.previous_hash == previous
                    && envelope.hash == hash(&previous, &envelope.state)?,
                "Snapshot checksum chain mismatch; preserve the workspace for recovery",
            )?;
            envelope.state.validate()?;
            state = envelope.state;
            previous = envelope.hash;
            has_state = true;
        }
        let store = Self {
            root,
            _lock: lock,
            revision: state.revision,
            hash: previous,
            has_state,
        };
        Ok((store, state))
    }
    pub fn commit(&mut self, state: &State) -> Result<()> {
        state.validate()?;
        let expected = self.next_revision()?;
        ensure(state.revision == expected, "Storage revision conflict")?;
        let new_hash = hash(&self.hash, state)?;
        let bytes = serde_json::to_vec(&Envelope {
            previous_hash: self.hash.clone(),
            hash: new_hash.clone(),
            state: state.clone(),
        })
        .map_err(|e| error(e.to_string()))?;
        ensure(
            bytes.len() as u64 <= MAX_BYTES,
            "Workspace exceeds 16 MiB snapshot limit",
        )?;
        let target = self.root.join(format!("{:020}.json", state.revision));
        ensure(!target.exists(), "Revision already exists")?;
        let pending = self.root.join(format!("{:020}.pending", state.revision));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(io)?;
        file.write_all(&bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        fs::rename(&pending, &target).map_err(io)?;
        // Renamed file is the commit point. A subsequent directory-sync failure cannot report rollback.
        #[cfg(unix)]
        if let Ok(dir) = File::open(&self.root) {
            let _ = dir.sync_all();
        }
        self.revision = state.revision;
        self.hash = new_hash;
        self.has_state = true;
        Ok(())
    }
    pub fn path(&self) -> &Path {
        &self.root
    }
    fn next_revision(&self) -> Result<u64> {
        if self.has_state {
            self.revision
                .checked_add(1)
                .ok_or_else(|| error("Revision overflow"))
        } else {
            Ok(0)
        }
    }
    /// Reports interrupted writes under the exclusive workspace lock. No pending file is promoted.
    pub fn inspect_pending(&self) -> Result<StorageInspection> {
        let next = self.next_revision()?;
        let expected_name = format!("{next:020}.pending");
        let mut pending = vec![];
        for entry in fs::read_dir(&self.root).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "pending") {
                continue;
            }
            ensure(
                pending.len() < 1000,
                "Too many pending files; preserve workspace for manual inspection",
            )?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(&path).map_err(io)?;
            let kind = if meta.file_type().is_symlink() {
                "symlink"
            } else if meta.is_dir() {
                "directory"
            } else if !meta.is_file() {
                "nonregular"
            } else if meta.len() > MAX_BYTES {
                "oversized"
            } else {
                "regular"
            };
            let mut sha256 = None;
            let mut snapshot_status = None;
            if kind == "regular" {
                let bytes = read_pending_bytes(&path)?;
                sha256 = Some(fingerprint(&bytes));
                snapshot_status = Some(match self.validate_pending_candidate(&bytes, next) {
                    Ok(()) => "Complete uncommitted candidate; must still be retried through the command API".into(),
                    Err(e) => format!("Incomplete or invalid uncommitted candidate: {e}"),
                });
            }
            pending.push(PendingInspection {
                eligible_for_quarantine: kind == "regular" && name == expected_name,
                name,
                kind: kind.into(),
                bytes: meta.len(),
                sha256,
                snapshot_status,
            });
        }
        pending.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(StorageInspection {
            committed_revision: self.has_state.then_some(self.revision),
            next_revision: next,
            pending,
        })
    }
    fn validate_pending_candidate(&self, bytes: &[u8], next: u64) -> Result<()> {
        let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| error(e.to_string()))?;
        ensure(
            envelope.state.revision == next,
            "Unexpected pending revision",
        )?;
        ensure(
            envelope.previous_hash == self.hash
                && envelope.hash == hash(&self.hash, &envelope.state)?,
            "Pending checksum mismatch",
        )?;
        envelope.state.validate()
    }
    /// Preserve exactly the next interrupted write in a new recovery directory.
    /// The caller must supply the byte checksum from inspection. Never replay or delete evidence.
    pub fn quarantine_pending(&self, name: &str, expected_hash: &str) -> Result<QuarantineOutcome> {
        let next = self.next_revision()?;
        ensure(
            name == format!("{next:020}.pending"),
            "Only the exactly-next pending filename can be quarantined",
        )?;
        ensure(
            expected_hash.len() == 64 && expected_hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "Supply the SHA-256 returned by storage-inspect",
        )?;
        let source = self.root.join(name);
        let (file, bytes) = read_pending_file(&source, true)?;
        let actual_hash = fingerprint(&bytes);
        ensure(
            actual_hash == expected_hash,
            "Pending bytes changed; inspect again before quarantine",
        )?;
        // A pending write may have crashed before the original file sync. Flush
        // exactly the inspected handle before moving it; failure leaves the path intact.
        file.sync_all().map_err(io)?;
        drop(file);
        let recovery = self.root.join("recovery");
        match fs::create_dir(&recovery) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(&recovery).map_err(io)?;
                ensure(
                    meta.is_dir() && !meta.file_type().is_symlink(),
                    "Recovery location must be a regular directory",
                )?;
            }
            Err(e) => return Err(io(e)),
        }
        let mut case = None;
        for suffix in 0..1000 {
            let candidate = recovery.join(format!("pending-{next:020}-{actual_hash}-{suffix:03}"));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    case = Some(candidate);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(io(e)),
            }
        }
        let case =
            case.ok_or_else(|| error("Recovery directory limit reached; nothing was moved"))?;
        let preserved_path = case.join(name);
        // The destination directory was exclusively created while the Store lock is held.
        // Its unique path avoids overwriting any earlier recovery evidence on every platform.
        fs::rename(&source, &preserved_path).map_err(io)?;
        #[cfg(unix)]
        let mut warnings = Vec::new();
        #[cfg(unix)]
        for dir in [&self.root, &recovery, &case] {
            if let Err(e) = File::open(dir).and_then(|f| f.sync_all()) {
                warnings.push(format!(
                    "Evidence was moved to {}; directory sync failed for {}: {e}",
                    preserved_path.display(),
                    dir.display()
                ));
            }
        }
        #[cfg(not(unix))]
        let warnings =
            vec!["Evidence was moved; directory sync is not available on this platform".into()];
        Ok(QuarantineOutcome {
            preserved_path,
            sha256: actual_hash,
            next_revision: next,
            warnings,
        })
    }
}
fn read_pending_bytes(path: &Path) -> Result<Vec<u8>> {
    read_pending_file(path, false).map(|(_, bytes)| bytes)
}
fn read_pending_file(path: &Path, writable: bool) -> Result<(File, Vec<u8>)> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    ensure(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Pending evidence must be a regular file, not a symlink",
    )?;
    ensure(
        meta.len() <= MAX_BYTES,
        "Pending file exceeds 16 MiB; preserve for manual inspection",
    )?;
    let mut bytes = Vec::new();
    let mut file = OpenOptions::new()
        .read(true)
        .write(writable)
        .open(path)
        .map_err(io)?;
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    ensure(
        bytes.len() as u64 <= MAX_BYTES,
        "Pending file grew during inspection",
    )?;
    Ok((file, bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reopen_lock_corruption() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("copperline-store-{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(io)?;
        }
        let (mut store, mut state) = Store::open(&dir)?;
        store.commit(&state)?;
        assert!(Store::open(&dir).is_err());
        state.revision = 1;
        state.audit.push(copperline_model::Audit {
            revision: 1,
            actor: copperline_model::Actor {
                name: "Test".into(),
                role: copperline_model::Role::Admin,
            },
            command: "test".into(),
            detail: "Storage fixture".into(),
            target: None,
        });
        state.receipts.push(copperline_model::Receipt {
            request_id: "test".into(),
            revision: 1,
            fingerprint: "a".repeat(64),
        });
        store.commit(&state)?;
        drop(store);
        let (store, reopened) = Store::open(&dir)?;
        assert_eq!(reopened, state);
        drop(store);
        fs::write(dir.join("00000000000000000001.json"), b"broken").map_err(io)?;
        assert!(Store::open(&dir).is_err());
        fs::remove_dir_all(dir).map_err(io)?;
        Ok(())
    }
}
