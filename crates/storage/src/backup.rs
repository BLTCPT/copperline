//! Bounded portable history archives and guarded, never-overwriting restores.
//!
//! Format: 16-byte magic, little-endian u64 manifest length, 32-byte manifest
//! SHA-256, strict JSON manifest, raw file bodies in manifest order, 16-byte end
//! marker and 32-byte SHA-256 of everything preceding the end marker. No trailing
//! bytes, links, compression, paths outside the declared tree, or replay of pending
//! writes are supported. Checksums establish consistency, not authentication.
use super::{Envelope, MAX_BYTES, Store, fingerprint, hash, io};
use copperline_model::{Result, ensure, error};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 10_000;
const MAGIC: &[u8; 16] = b"COPPERLINE-BKP1\n";
const END: &[u8; 16] = b"COPPERLINE-END1\n";
const FIXED_BYTES: u64 = 16 + 8 + 32 + 16 + 32;
const FORMAT_VERSION: u32 = 1;
const SCHEMA_VERSION: u32 = 1;
const MAX_DEPTH: usize = 8;
static NEXT_RESTORE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupSummary {
    pub format_version: u32,
    pub schema_version: u32,
    pub committed_revision: Option<u64>,
    pub head_hash: Option<String>,
    pub snapshot_count: u64,
    pub pending_count: u64,
    pub recovery_file_count: u64,
    pub recovery_directory_count: u64,
    /// Complete archive length, including framing, manifest and footer.
    pub total_bytes: u64,
    /// SHA-256 of the complete archive, including its footer.
    pub sha256: String,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreSummary {
    pub workspace: PathBuf,
    pub backup: BackupSummary,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Snapshot,
    Pending,
    RecoveryFile,
    RecoveryDirectory,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    kind: Kind,
    bytes: u64,
    sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    schema_version: u32,
    committed_revision: u64,
    head_hash: String,
    entries: Vec<Entry>,
}

fn portable_component(name: &str) -> Result<()> {
    ensure(
        !name.is_empty() && name.len() <= 128 && name != "." && name != "..",
        "Portable path component must contain 1–128 characters and cannot be dot or dot-dot",
    )?;
    ensure(
        name.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            && !name.ends_with('.'),
        "Portable paths require ASCII letters, digits, dots, underscores or hyphens; no trailing dot",
    )?;
    let base = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let device = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base
                .as_bytes()
                .last()
                .is_some_and(|b| (b'1'..=b'9').contains(b)));
    ensure(
        !device,
        "Windows device names are not portable archive paths",
    )
}
fn portable_path(path: &str) -> Result<()> {
    ensure(path.len() <= 240, "Portable archive path exceeds 240 bytes")?;
    let parts: Vec<_> = path.split('/').collect();
    ensure(
        parts.len() <= MAX_DEPTH,
        "Portable archive path is too deep",
    )?;
    for part in parts {
        portable_component(part)?;
    }
    Ok(())
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn snapshot_number(name: &str) -> Option<u64> {
    let digits = name.strip_suffix(".json")?;
    (digits.len() == 20 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}
impl Manifest {
    fn validate(&self) -> Result<u64> {
        ensure(
            self.format_version == FORMAT_VERSION && self.schema_version == SCHEMA_VERSION,
            "Unsupported backup format or state schema",
        )?;
        ensure(
            !self.entries.is_empty() && self.entries.len() <= MAX_ENTRIES,
            "Backup entry count must be between 1 and 10,000",
        )?;
        ensure(valid_hash(&self.head_hash), "Backup chain head is invalid")?;
        let mut names = BTreeMap::new();
        let mut folded = BTreeSet::new();
        let mut last: Option<&str> = None;
        let mut snapshots = 0_u64;
        let mut bytes = 0_u64;
        for e in &self.entries {
            portable_path(&e.path)?;
            ensure(
                last.is_none_or(|p| p < e.path.as_str()),
                "Backup paths must be unique and sorted",
            )?;
            last = Some(&e.path);
            ensure(
                folded.insert(e.path.to_ascii_lowercase()),
                "Case-colliding backup paths are not portable",
            )?;
            ensure(
                e.bytes <= MAX_BYTES && valid_hash(&e.sha256),
                "Backup entry length or checksum invalid",
            )?;
            if let Some((parent, _)) = e.path.rsplit_once('/') {
                ensure(
                    names.get(parent) == Some(&Kind::RecoveryDirectory),
                    "Backup parent directory missing or is a file",
                )?;
            }
            match e.kind {
                Kind::Snapshot => {
                    ensure(
                        snapshot_number(&e.path) == Some(snapshots),
                        "Committed backup history has a gap or invalid name",
                    )?;
                    snapshots = snapshots
                        .checked_add(1)
                        .ok_or_else(|| error("Snapshot count overflow"))?;
                }
                Kind::Pending => ensure(
                    !e.path.contains('/') && e.path.ends_with(".pending"),
                    "Pending evidence must retain its root filename",
                )?,
                Kind::RecoveryFile => ensure(
                    e.path.starts_with("recovery/"),
                    "Recovery files must remain under recovery/",
                )?,
                Kind::RecoveryDirectory => ensure(
                    (e.path == "recovery" || e.path.starts_with("recovery/"))
                        && e.bytes == 0
                        && e.sha256 == fingerprint(&[]),
                    "Recovery directory metadata invalid",
                )?,
            }
            bytes = bytes
                .checked_add(e.bytes)
                .ok_or_else(|| error("Backup byte count overflow"))?;
            ensure(
                bytes <= MAX_ARCHIVE_BYTES,
                "Backup exceeds 512 MiB total limit",
            )?;
            names.insert(e.path.as_str(), e.kind);
        }
        ensure(
            snapshots > 0 && self.committed_revision.checked_add(1) == Some(snapshots),
            "Backup must include every committed snapshot from zero through its declared revision",
        )?;
        Ok(bytes)
    }
    fn summary(&self, total_bytes: u64, sha256: String) -> BackupSummary {
        let count = |kind| self.entries.iter().filter(|e| e.kind == kind).count() as u64;
        BackupSummary {
            format_version: FORMAT_VERSION,
            schema_version: SCHEMA_VERSION,
            committed_revision: Some(self.committed_revision),
            head_hash: Some(self.head_hash.clone()),
            snapshot_count: count(Kind::Snapshot),
            pending_count: count(Kind::Pending),
            recovery_file_count: count(Kind::RecoveryFile),
            recovery_directory_count: count(Kind::RecoveryDirectory),
            total_bytes,
            sha256,
            warnings: vec![],
        }
    }
}

fn regular_file(path: &Path, maximum: u64) -> Result<File> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    ensure(
        meta.is_file() && !meta.file_type().is_symlink(),
        &format!("Only regular files are supported: {}", path.display()),
    )?;
    ensure(
        meta.len() <= maximum,
        &format!("File exceeds bounded size: {}", path.display()),
    )?;
    let file = File::open(path).map_err(io)?;
    ensure(
        file.metadata().map_err(io)?.len() <= maximum,
        "File grew beyond its size limit",
    )?;
    Ok(file)
}
fn file_bytes(path: &Path) -> Result<Vec<u8>> {
    let file = regular_file(path, MAX_BYTES)?;
    let mut bytes = vec![];
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    ensure(
        bytes.len() as u64 <= MAX_BYTES,
        "Evidence file grew beyond 16 MiB",
    )?;
    Ok(bytes)
}
fn inventory(root: &Path) -> Result<Vec<Entry>> {
    fn walk(root: &Path, relative: &str, result: &mut Vec<Entry>) -> Result<()> {
        for entry in fs::read_dir(root.join(relative)).map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| error("Non-UTF-8 evidence names cannot be portably backed up"))?;
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            portable_path(&path)?;
            let meta = fs::symlink_metadata(entry.path()).map_err(io)?;
            ensure(
                !meta.file_type().is_symlink(),
                &format!("Symlink evidence cannot be backed up: {path}"),
            )?;
            if relative.is_empty() && name == "workspace.lock" {
                ensure(meta.is_file(), "Workspace lock must be a regular file")?;
                continue;
            }
            ensure(
                result.len() < MAX_ENTRIES,
                "Backup exceeds 10,000 entries; nothing may be omitted",
            )?;
            let kind = if relative.is_empty() {
                if snapshot_number(&name).is_some() && meta.is_file() {
                    Kind::Snapshot
                } else if name.ends_with(".pending") && meta.is_file() {
                    Kind::Pending
                } else if name == "recovery" && meta.is_dir() {
                    Kind::RecoveryDirectory
                } else {
                    return Err(error(format!(
                        "Unsupported root evidence {path}; preserve it separately before backup"
                    )));
                }
            } else if meta.is_dir() {
                Kind::RecoveryDirectory
            } else if meta.is_file() {
                Kind::RecoveryFile
            } else {
                return Err(error(format!(
                    "Nonregular recovery evidence is unsupported: {path}"
                )));
            };
            let is_dir = kind == Kind::RecoveryDirectory;
            ensure(
                is_dir || meta.len() <= MAX_BYTES,
                &format!("Evidence exceeds 16 MiB: {path}"),
            )?;
            result.push(Entry {
                path: path.clone(),
                kind,
                bytes: if is_dir { 0 } else { meta.len() },
                sha256: String::new(),
            });
            if is_dir {
                walk(root, &path, result)?;
            }
        }
        Ok(())
    }
    let mut result = vec![];
    walk(root, "", &mut result)?;
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
#[derive(Default)]
struct Chain {
    next: u64,
    head: String,
}
impl Chain {
    fn add(&mut self, entry: &Entry, bytes: &[u8]) -> Result<()> {
        if entry.kind != Kind::Snapshot {
            return Ok(());
        }
        let envelope: Envelope = serde_json::from_slice(bytes)
            .map_err(|e| error(format!("Invalid committed snapshot {}: {e}", entry.path)))?;
        ensure(
            snapshot_number(&entry.path) == Some(self.next) && envelope.state.revision == self.next,
            "Backup committed revision chain has a gap",
        )?;
        ensure(
            envelope.previous_hash == self.head
                && envelope.hash == hash(&self.head, &envelope.state)?,
            "Backup semantic snapshot checksum chain mismatch",
        )?;
        envelope.state.validate()?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| error("Backup revision overflow"))?;
        self.head = envelope.hash;
        Ok(())
    }
    fn finish(&self, manifest: &Manifest) -> Result<()> {
        ensure(
            manifest.committed_revision.checked_add(1) == Some(self.next)
                && manifest.head_hash == self.head,
            "Backup declared revision/head differs from validated committed history",
        )
    }
}
fn source_manifest(root: &Path) -> Result<Manifest> {
    let mut entries = inventory(root)?;
    let mut chain = Chain::default();
    let mut total = 0_u64;
    for entry in &mut entries {
        let bytes = if entry.kind == Kind::RecoveryDirectory {
            vec![]
        } else {
            file_bytes(&root.join(&entry.path))?
        };
        ensure(
            entry.bytes == bytes.len() as u64,
            "Evidence changed during inventory",
        )?;
        total = total
            .checked_add(entry.bytes)
            .ok_or_else(|| error("Backup byte count overflow"))?;
        ensure(
            total <= MAX_ARCHIVE_BYTES,
            "Backup exceeds 512 MiB total limit; history cannot be truncated",
        )?;
        entry.sha256 = fingerprint(&bytes);
        chain.add(entry, &bytes)?;
    }
    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        schema_version: SCHEMA_VERSION,
        committed_revision: chain
            .next
            .checked_sub(1)
            .ok_or_else(|| error("Backup requires initialized committed history"))?,
        head_hash: chain.head.clone(),
        entries,
    };
    manifest.validate()?;
    chain.finish(&manifest)?;
    Ok(manifest)
}

struct ArchiveReader {
    file: File,
    hasher: Sha256,
    count: u64,
    length: u64,
}
impl ArchiveReader {
    fn open(path: &Path) -> Result<Self> {
        let file = regular_file(path, MAX_ARCHIVE_BYTES)?;
        let length = file.metadata().map_err(io)?.len();
        Ok(Self {
            file,
            hasher: Sha256::new(),
            count: 0,
            length,
        })
    }
    fn read(&mut self, length: u64) -> Result<Vec<u8>> {
        ensure(
            length <= MAX_BYTES,
            "Archive frame allocation exceeds limit",
        )?;
        self.count = self
            .count
            .checked_add(length)
            .ok_or_else(|| error("Archive length overflow"))?;
        ensure(
            self.count <= self.length && self.count <= MAX_ARCHIVE_BYTES,
            "Truncated or oversized archive",
        )?;
        let mut bytes =
            vec![0; usize::try_from(length).map_err(|_| error("Unsupported archive length"))?];
        self.file.read_exact(&mut bytes).map_err(io)?;
        self.hasher.update(&bytes);
        Ok(bytes)
    }
}
fn scan_archive(
    path: &Path,
    mut visit: impl FnMut(&Entry, &[u8]) -> Result<()>,
) -> Result<(Manifest, BackupSummary)> {
    let mut reader = ArchiveReader::open(path)?;
    ensure(
        reader.read(16)? == MAGIC,
        "Not a supported Copperline backup",
    )?;
    let length: [u8; 8] = reader
        .read(8)?
        .try_into()
        .map_err(|_| error("Invalid manifest frame"))?;
    let length = u64::from_le_bytes(length);
    ensure(
        length > 0 && length <= MAX_MANIFEST_BYTES,
        "Manifest exceeds 2 MiB or is empty",
    )?;
    let expected_hash = reader.read(32)?;
    let bytes = reader.read(length)?;
    ensure(
        Sha256::digest(&bytes).as_slice() == expected_hash,
        "Backup manifest checksum mismatch",
    )?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| error(format!("Invalid backup manifest: {e}")))?;
    let body_bytes = manifest.validate()?;
    let expected_length = FIXED_BYTES
        .checked_add(length)
        .and_then(|n| n.checked_add(body_bytes))
        .ok_or_else(|| error("Archive length overflow"))?;
    ensure(
        expected_length == reader.length && expected_length <= MAX_ARCHIVE_BYTES,
        "Archive length differs from manifest; truncated or trailing bytes",
    )?;
    let mut chain = Chain::default();
    for entry in &manifest.entries {
        let bytes = reader.read(entry.bytes)?;
        ensure(
            fingerprint(&bytes) == entry.sha256,
            &format!("Backup payload checksum mismatch: {}", entry.path),
        )?;
        chain.add(entry, &bytes)?;
        visit(entry, &bytes)?;
    }
    chain.finish(&manifest)?;
    let body_hash = reader.hasher.clone().finalize();
    ensure(reader.read(16)? == END, "Backup completion marker missing")?;
    ensure(
        reader.read(32)?.as_slice() == body_hash.as_slice(),
        "Backup body checksum mismatch",
    )?;
    let mut extra = [0_u8; 1];
    ensure(
        reader.file.read(&mut extra).map_err(io)? == 0,
        "Backup grew or has trailing bytes",
    )?;
    let summary = manifest.summary(reader.count, format!("{:x}", reader.hasher.finalize()));
    Ok((manifest, summary))
}
/// Read-only validation of every byte checksum and every committed state revision.
pub fn inspect(path: impl AsRef<Path>) -> Result<BackupSummary> {
    scan_archive(path.as_ref(), |_, _| Ok(())).map(|(_, summary)| summary)
}
fn write_hashed(file: &mut File, hasher: &mut Sha256, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes).map_err(io)?;
    hasher.update(bytes);
    Ok(())
}
fn new_file_location(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| error("Output path requires a filename"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(fs::canonicalize(parent).map_err(io)?.join(name))
}
fn directory_sync(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path).and_then(|f| f.sync_all()).map_err(io)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
fn platform_warnings() -> Vec<String> {
    if cfg!(unix) {
        vec![]
    } else {
        vec!["File contents were synced; directory-entry sync is unavailable on this platform, so power-loss durability is not confirmed".into()]
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupStage {
    OutputCreated,
    EntryWritten,
}
impl Store {
    /// Preserve the full raw committed history and opaque pending/recovery evidence.
    /// The session's exclusive Store lock remains held for the entire operation.
    pub fn backup(&self, path: impl AsRef<Path>) -> Result<BackupSummary> {
        self.backup_impl(path.as_ref(), |_| Ok(()))
    }
    fn backup_impl(
        &self,
        path: &Path,
        mut checkpoint: impl FnMut(BackupStage) -> Result<()>,
    ) -> Result<BackupSummary> {
        ensure(
            self.has_state,
            "Backup requires initialized committed history",
        )?;
        check_restore_guards(&self.root)?;
        let root = fs::canonicalize(&self.root).map_err(io)?;
        let destination = new_file_location(path)?;
        ensure(
            !destination.starts_with(&root),
            "Backup output must be outside the source workspace",
        )?;
        let manifest = source_manifest(&root)?;
        ensure(
            manifest.committed_revision == self.revision && manifest.head_hash == self.hash,
            "Source history changed since this Store opened; preserve and inspect it",
        )?;
        let json = serde_json::to_vec(&manifest).map_err(|e| error(e.to_string()))?;
        ensure(
            json.len() as u64 <= MAX_MANIFEST_BYTES,
            "Backup manifest exceeds 2 MiB",
        )?;
        let payload_bytes = manifest.validate()?;
        let total = FIXED_BYTES
            .checked_add(json.len() as u64)
            .and_then(|n| n.checked_add(payload_bytes))
            .ok_or_else(|| error("Backup length overflow"))?;
        ensure(
            total <= MAX_ARCHIVE_BYTES,
            "Backup exceeds 512 MiB including metadata",
        )?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(io)?;
        let result = (|| -> Result<BackupSummary> {
            checkpoint(BackupStage::OutputCreated)?;
            let mut hasher = Sha256::new();
            write_hashed(&mut file, &mut hasher, MAGIC)?;
            write_hashed(&mut file, &mut hasher, &(json.len() as u64).to_le_bytes())?;
            write_hashed(&mut file, &mut hasher, &Sha256::digest(&json))?;
            write_hashed(&mut file, &mut hasher, &json)?;
            for entry in &manifest.entries {
                let bytes = if entry.kind == Kind::RecoveryDirectory {
                    vec![]
                } else {
                    file_bytes(&root.join(&entry.path))?
                };
                ensure(
                    bytes.len() as u64 == entry.bytes && fingerprint(&bytes) == entry.sha256,
                    &format!("Source evidence changed during backup: {}", entry.path),
                )?;
                write_hashed(&mut file, &mut hasher, &bytes)?;
                checkpoint(BackupStage::EntryWritten)?;
            }
            let body_hash = hasher.clone().finalize();
            write_hashed(&mut file, &mut hasher, END)?;
            write_hashed(&mut file, &mut hasher, &body_hash)?;
            file.sync_all().map_err(io)?;
            let expected = manifest.summary(total, format!("{:x}", hasher.finalize()));
            let mut verified = inspect(&destination)?;
            ensure(verified == expected, "Written archive failed verification")?;
            // The complete synced archive exists. Later directory-sync errors are warnings.
            verified.warnings = platform_warnings();
            if let Some(parent) = destination.parent()
                && let Err(e) = directory_sync(parent)
            {
                verified.warnings.push(format!(
                    "Archive is complete at {}; directory sync failed: {e}",
                    destination.display()
                ));
            }
            Ok(verified)
        })();
        result.map_err(|e|error(format!("{e}; newly created archive remains at {} and may be incomplete; backup did not modify the source workspace",destination.display())))
    }
}

fn guard_path(parent: &Path, basename: &str) -> PathBuf {
    parent.join(format!(
        ".copperline-restore-{}.guard",
        fingerprint(basename.to_ascii_lowercase().as_bytes())
    ))
}
/// Resolve existing symlink ancestors before appending as-yet nonexistent names.
/// Do not guess lexical `..` semantics through a missing or symlinked component.
fn resolved_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(io)?.join(path)
    };
    let mut existing = absolute.clone();
    loop {
        match fs::canonicalize(&existing) {
            Ok(base) => {
                let remainder = absolute
                    .strip_prefix(&existing)
                    .map_err(|_| error("Cannot resolve workspace path"))?;
                ensure(
                    remainder
                        .components()
                        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
                    "Resolve dot-dot through existing directories before opening a workspace",
                )?;
                return Ok(base.join(remainder));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ensure(existing.pop(), "Cannot resolve workspace ancestor")?;
            }
            Err(e) => return Err(io(e)),
        }
    }
}
pub(crate) fn check_restore_guards(path: &Path) -> Result<()> {
    let path = resolved_path(path)?;
    for ancestor in path.ancestors() {
        let Some(parent) = ancestor.parent() else {
            continue;
        };
        let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Restores require ASCII names; aliases resolving to them converge here.
        if !name.is_ascii() {
            continue;
        }
        let guard = guard_path(parent, name);
        match fs::symlink_metadata(&guard) {
            Ok(_) => {
                return Err(error(format!(
                    "Incomplete or active restore guard at {}; workspace opening is blocked. Preserve the guard, target and source archive",
                    guard.display()
                )));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
    }
    Ok(())
}
#[derive(Serialize)]
struct GuardRecord<'a> {
    format_version: u32,
    owner: String,
    target: &'a Path,
    archive_sha256: &'a str,
}
fn remove_owned_guard(path: &Path, expected: &[u8], allow_partial: bool) -> Result<()> {
    let actual = file_bytes(path)?;
    ensure(
        actual == expected || (allow_partial && expected.starts_with(&actual)),
        "Restore guard changed; it was preserved",
    )?;
    fs::remove_file(path).map_err(io)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreStage {
    GuardReady,
    TargetCreated,
    EntryCopied,
    Validated,
}
/// Restore only to a new directory. Partial restores remain blocked by a sibling
/// guard; no pending bytes are promoted and no existing target is overwritten.
pub fn restore(
    archive: impl AsRef<Path>,
    new_workspace: impl AsRef<Path>,
) -> Result<RestoreSummary> {
    restore_impl(archive.as_ref(), new_workspace.as_ref(), |_| Ok(()))
}
fn restore_impl(
    archive: &Path,
    target: &Path,
    mut checkpoint: impl FnMut(RestoreStage) -> Result<()>,
) -> Result<RestoreSummary> {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| error("Restore target requires a portable directory name"))?;
    portable_component(name)?;
    let target = new_file_location(target)?;
    let parent = target
        .parent()
        .ok_or_else(|| error("Restore target requires a parent directory"))?;
    check_restore_guards(&target)?;
    match fs::symlink_metadata(&target) {
        Ok(_) => {
            return Err(error(
                "Restore refuses every existing destination, including empty directories and symlinks",
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(e)),
    }
    let (manifest, backup) = scan_archive(archive, |_, _| Ok(()))?;
    let guard = guard_path(parent, name);
    let record = GuardRecord {
        format_version: 1,
        owner: format!(
            "{}-{}",
            std::process::id(),
            NEXT_RESTORE.fetch_add(1, Ordering::Relaxed)
        ),
        target: &target,
        archive_sha256: &backup.sha256,
    };
    let guard_bytes = serde_json::to_vec(&record).map_err(|e| error(e.to_string()))?;
    let guard_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&guard)
        .map_err(io)?;
    let mut guard_file = Some(guard_file);
    let mut owns_target = false;
    let result = (|| -> Result<Vec<String>> {
        let mut file = guard_file
            .take()
            .ok_or_else(|| error("Restore guard handle missing"))?;
        file.write_all(&guard_bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        directory_sync(parent)?;
        checkpoint(RestoreStage::GuardReady)?;
        fs::create_dir(&target).map_err(io)?;
        owns_target = true;
        checkpoint(RestoreStage::TargetCreated)?;
        let lock_path = target.join("workspace.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(lock_path)
            .map_err(io)?;
        lock.try_lock()
            .map_err(|_| error("Restored workspace lock could not be acquired"))?;
        let (copied_manifest, copied_summary) = scan_archive(archive, |entry, bytes| {
            let destination = target.join(&entry.path);
            if entry.kind == Kind::RecoveryDirectory {
                fs::create_dir(&destination).map_err(io)?;
            } else {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)
                    .map_err(io)?;
                file.write_all(bytes).map_err(io)?;
                file.sync_all().map_err(io)?;
            }
            checkpoint(RestoreStage::EntryCopied)
        })?;
        ensure(
            copied_manifest == manifest && copied_summary == backup,
            "Archive changed after preflight; partial restore remains guarded",
        )?;
        ensure(
            source_manifest(&target)? == manifest,
            "Copied workspace differs from validated archive",
        )?;
        for entry in manifest
            .entries
            .iter()
            .rev()
            .filter(|e| e.kind == Kind::RecoveryDirectory)
        {
            directory_sync(&target.join(&entry.path))?;
        }
        directory_sync(&target)?;
        checkpoint(RestoreStage::Validated)?;
        // Keep the workspace lock across guard removal, the application commit point.
        remove_owned_guard(&guard, &guard_bytes, false)?;
        let mut warnings = platform_warnings();
        if let Err(e) = directory_sync(parent) {
            warnings.push(format!(
                "Restore is complete at {}; guard was removed but directory sync failed: {e}",
                target.display()
            ));
        }
        drop(lock);
        Ok(warnings)
    })();
    drop(guard_file);
    match result {
        Ok(warnings) => Ok(RestoreSummary {
            workspace: target,
            backup,
            warnings,
        }),
        Err(e) if owns_target => Err(error(format!(
            "{e}; incomplete restore is preserved at {} with guard {}. Restore did not modify the source archive; use another new destination, do not remove the guard",
            target.display(),
            guard.display()
        ))),
        Err(e) => {
            let cleanup = remove_owned_guard(&guard, &guard_bytes, true);
            let detail = match cleanup {
                Ok(()) => {
                    let _ = directory_sync(parent);
                    "This attempt's guard was removed; destination was not owned or changed"
                        .to_string()
                }
                Err(cleanup) => format!(
                    "This attempt's guard remains at {}: {cleanup}; destination was not owned or changed",
                    guard.display()
                ),
            };
            Err(error(format!("{e}; {detail}")))
        }
    }
}

#[cfg(test)]
#[path = "backup_fault_tests.rs"]
mod fault_tests;
