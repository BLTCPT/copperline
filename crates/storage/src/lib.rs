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
        let expected = if self.has_state {
            self.revision
                .checked_add(1)
                .ok_or_else(|| error("Revision overflow"))?
        } else {
            0
        };
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
