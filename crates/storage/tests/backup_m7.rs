//! Independent framing, full-history, opaque-evidence and no-overwrite regressions.
use copperline_model::{Actor, Audit, Customer, Receipt, Role, State};
use copperline_storage::{Store, backup, fingerprint};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-backup-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn advance(state: &mut State, revision: u64) {
    state.revision = revision;
    state.audit.push(Audit {
        target: None,
        revision,
        actor: Actor {
            name: "Synthetic author".into(),
            role: Role::Admin,
        },
        command: "fixture".into(),
        detail: "Synthetic storage revision".into(),
    });
    state.receipts.push(Receipt {
        request_id: format!("revision-{revision}"),
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
fn source(temp: &Temp) -> std::result::Result<(Store, State), Box<dyn std::error::Error>> {
    let (mut store, mut state) = Store::open(temp.0.join("source"))?;
    store.commit(&state)?;
    for revision in 1..=2 {
        advance(&mut state, revision);
        store.commit(&state)?;
    }
    Ok((store, state))
}
fn tree(path: &Path) -> std::io::Result<BTreeMap<String, Option<Vec<u8>>>> {
    fn add(
        root: &Path,
        relative: &str,
        out: &mut BTreeMap<String, Option<Vec<u8>>>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(root.join(relative))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if relative.is_empty() && name == "workspace.lock" {
                continue;
            }
            let key = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            if entry.file_type()?.is_dir() {
                out.insert(key.clone(), None);
                add(root, &key, out)?;
            } else {
                out.insert(key, Some(fs::read(entry.path())?));
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    add(path, "", &mut result)?;
    Ok(result)
}
fn archive_fixture(temp: &Temp) -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
    let (store, _) = source(temp)?;
    fs::create_dir_all(store.path().join("recovery/case/empty"))?;
    fs::write(
        store.path().join("recovery/case/evidence.bin"),
        [0, 1, 255, 10, 0],
    )?;
    fs::write(
        store.path().join("00000000000000000003.pending"),
        b"incomplete next write",
    )?;
    fs::write(
        store.path().join("unknown.pending"),
        b"unknown binary evidence\0\xff",
    )?;
    let archive = temp.0.join("backup.cpb");
    store.backup(&archive)?;
    Ok(archive)
}

// This deliberately independent encoder permits corruption with all outer hashes
// recomputed, so tests can reach the semantic and path-validation boundaries.
#[derive(Clone)]
struct Packed {
    manifest: Value,
    bodies: Vec<Vec<u8>>,
}
impl Packed {
    fn read(path: &Path) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let bytes = fs::read(path)?;
        let length = usize::try_from(u64::from_le_bytes(bytes[16..24].try_into()?))?;
        let manifest: Value = serde_json::from_slice(&bytes[56..56 + length])?;
        let mut cursor = 56 + length;
        let mut bodies = vec![];
        for entry in manifest["entries"].as_array().ok_or("Entries missing")? {
            let length = usize::try_from(entry["bytes"].as_u64().ok_or("Length missing")?)?;
            bodies.push(bytes[cursor..cursor + length].to_vec());
            cursor += length;
        }
        Ok(Self { manifest, bodies })
    }
    fn encode(&self) -> serde_json::Result<Vec<u8>> {
        let json = serde_json::to_vec(&self.manifest)?;
        let mut bytes = b"COPPERLINE-BKP1\n".to_vec();
        bytes.extend((json.len() as u64).to_le_bytes());
        bytes.extend(Sha256::digest(&json));
        bytes.extend(json);
        for body in &self.bodies {
            bytes.extend(body);
        }
        let body_hash = Sha256::digest(&bytes);
        bytes.extend(b"COPPERLINE-END1\n");
        bytes.extend(body_hash);
        Ok(bytes)
    }
    fn sort(&mut self) -> TestResult {
        let entries = self.manifest["entries"]
            .as_array()
            .ok_or("Entries missing")?
            .clone();
        let mut pairs: Vec<_> = entries.into_iter().zip(self.bodies.clone()).collect();
        pairs.sort_by(|a, b| a.0["path"].as_str().cmp(&b.0["path"].as_str()));
        self.manifest["entries"] = Value::Array(pairs.iter().map(|p| p.0.clone()).collect());
        self.bodies = pairs.into_iter().map(|p| p.1).collect();
        Ok(())
    }
}

#[test]
fn full_original_bytes_empty_recovery_directories_and_pending_evidence_roundtrip() -> TestResult {
    let temp = Temp::new()?;
    let (store, state) = source(&temp)?;
    // Raw whitespace is deliberately noncanonical but the semantic chain is valid.
    let snapshot = store.path().join("00000000000000000001.json");
    let value: Value = serde_json::from_slice(&fs::read(&snapshot)?)?;
    fs::write(&snapshot, serde_json::to_vec_pretty(&value)?)?;
    fs::create_dir_all(store.path().join("recovery/case/empty"))?;
    fs::write(
        store.path().join("recovery/case/raw.bin"),
        [0, 255, 1, 0, 10],
    )?;
    fs::write(
        store.path().join("00000000000000000003.pending"),
        b"partial write",
    )?;
    fs::write(store.path().join("unknown.pending"), b"unknown evidence")?;
    let before = tree(store.path())?;
    let path = temp.0.join("backup.cpb");
    let summary = store.backup(&path)?;
    assert_eq!(summary.snapshot_count, 3);
    assert_eq!(summary.committed_revision, Some(2));
    assert_eq!(summary.pending_count, 2);
    assert_eq!(summary.recovery_file_count, 1);
    assert_eq!(summary.recovery_directory_count, 3);
    assert_eq!(summary.total_bytes, fs::metadata(&path)?.len());
    assert_eq!(summary.sha256, fingerprint(&fs::read(&path)?));
    let second = temp.0.join("same.cpb");
    store.backup(&second)?;
    assert_eq!(fs::read(&path)?, fs::read(second)?);
    let inspected = backup::inspect(&path)?;
    assert_eq!(inspected.sha256, summary.sha256);
    let destination = temp.0.join("restored");
    let restored = backup::restore(&path, &destination)?;
    assert_eq!(restored.backup, inspected);
    assert_eq!(tree(&destination)?, before);
    assert_eq!(tree(store.path())?, before);
    assert!(Store::open(store.path()).is_err());
    let (mut restored_store, mut actual) = Store::open(&destination)?;
    assert_eq!(actual, state);
    advance(&mut actual, 3);
    assert!(
        restored_store.commit(&actual).is_err(),
        "Restore silently promoted or discarded pending data"
    );
    assert_eq!(tree(&destination)?, before);
    Ok(())
}

#[test]
fn uninitialized_source_existing_outputs_and_nested_output_are_rejected() -> TestResult {
    let empty = Temp::new()?;
    let (store, _) = Store::open(empty.0.join("empty"))?;
    assert!(store.backup(empty.0.join("absent.cpb")).is_err());
    assert!(!empty.0.join("absent.cpb").exists());
    let temp = Temp::new()?;
    let (store, _) = source(&temp)?;
    let before = tree(store.path())?;
    let output = temp.0.join("existing.cpb");
    fs::write(&output, b"user data")?;
    assert!(store.backup(&output).is_err());
    assert_eq!(fs::read(output)?, b"user data");
    assert!(store.backup(store.path().join("nested.cpb")).is_err());
    assert_eq!(tree(store.path())?, before);
    Ok(())
}

#[test]
fn framing_corruption_truncation_and_trailing_bytes_fail_before_restore_creation() -> TestResult {
    let temp = Temp::new()?;
    let source = archive_fixture(&temp)?;
    let bytes = fs::read(&source)?;
    let mut corruptions = vec![];
    let manifest_length = usize::try_from(u64::from_le_bytes(bytes[16..24].try_into()?))?;
    for index in [0, 24, 56, 56 + manifest_length, bytes.len() - 1] {
        let mut changed = bytes.clone();
        changed[index] ^= 1;
        corruptions.push(changed);
    }
    for length in [0, 15, 23, 55, bytes.len() / 2, bytes.len() - 1] {
        corruptions.push(bytes[..length].to_vec());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    corruptions.push(extra);
    for length in [0, u64::MAX, backup::MAX_MANIFEST_BYTES + 1] {
        let mut changed = bytes.clone();
        changed[16..24].copy_from_slice(&length.to_le_bytes());
        corruptions.push(changed);
    }
    for (n, changed) in corruptions.iter().enumerate() {
        let archive = temp.0.join(format!("corrupt-{n}.cpb"));
        fs::write(&archive, changed)?;
        let destination = temp.0.join(format!("reject-{n}"));
        assert!(
            backup::inspect(&archive).is_err(),
            "Accepted corruption {n}"
        );
        assert!(backup::restore(&archive, &destination).is_err());
        assert!(!destination.exists());
        assert_eq!(fs::read(&archive)?, *changed);
    }
    assert_eq!(fs::read(source)?, bytes);
    Ok(())
}

#[test]
fn strict_manifest_rejects_paths_duplicates_case_aliases_file_parents_and_caps() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive_fixture(&temp)?;
    let original = Packed::read(&archive)?;
    let entries = original.manifest["entries"]
        .as_array()
        .ok_or("Entries missing")?;
    let file = entries
        .iter()
        .position(|e| e["kind"] == "recovery_file")
        .ok_or("Recovery file missing")?;
    let mut invalid = vec![];
    for path in [
        "../escape",
        "recovery/../../escape",
        "C:/escape",
        "\\\\server\\share",
        "/absolute",
        "recovery//x",
        "recovery/a/../x",
        "recovery/NUL.bin",
        "recovery/trailing.",
        "recovery/x:y",
        "recovery/a\\b",
        "recovery/étalon",
    ] {
        let mut p = original.clone();
        p.manifest["entries"][file]["path"] = json!(path);
        invalid.push(p);
    }
    let mut p = original.clone();
    p.manifest["entries"][file]["path"] = json!(format!("recovery/{}", "a".repeat(241)));
    invalid.push(p);
    let mut p = original.clone();
    p.manifest["entries"][file]["path"] = json!("recovery/a/b/c/d/e/f/g/h/i");
    invalid.push(p);
    let mut p = original.clone();
    let duplicate = p.manifest["entries"][file].clone();
    p.manifest["entries"]
        .as_array_mut()
        .ok_or("Entries missing")?
        .push(duplicate);
    p.bodies.push(p.bodies[file].clone());
    p.sort()?;
    invalid.push(p);
    let mut p = original.clone();
    p.manifest["entries"][file]["path"] = json!("recovery/A");
    let mut other = p.manifest["entries"][file].clone();
    other["path"] = json!("recovery/a");
    p.manifest["entries"]
        .as_array_mut()
        .ok_or("Entries missing")?
        .push(other);
    p.bodies.push(p.bodies[file].clone());
    p.sort()?;
    invalid.push(p);
    for (field, value) in [
        ("format_version", json!(2)),
        ("schema_version", json!(2)),
        ("committed_revision", json!(99)),
        ("head_hash", json!("0".repeat(64))),
        ("unknown_future_flag", json!(true)),
    ] {
        let mut p = original.clone();
        p.manifest[field] = value;
        invalid.push(p);
    }
    let mut p = original.clone();
    p.manifest["entries"][file]["bytes"] = json!(16 * 1024 * 1024 + 1);
    invalid.push(p);
    let mut p = original.clone();
    p.manifest["entries"][file]["unexpected"] = json!(true);
    invalid.push(p);
    let mut p = original.clone();
    p.manifest["entries"] = json!(vec![entries[0].clone(); backup::MAX_ENTRIES + 1]);
    p.bodies.clear();
    invalid.push(p);
    let mut p = original.clone();
    let many:Vec<_>=(0..33).map(|n|json!({"path":format!("{n:020}.json"),"kind":"snapshot","bytes":16*1024*1024,"sha256":"0".repeat(64)})).collect();
    p.manifest["entries"] = json!(many);
    p.manifest["committed_revision"] = json!(32);
    p.bodies.clear();
    invalid.push(p);
    let directory = entries
        .iter()
        .position(|e| e["path"] == "recovery/case")
        .ok_or("Directory missing")?;
    let mut p = original.clone();
    p.manifest["entries"][directory]["kind"] = json!("recovery_file");
    invalid.push(p);
    for (n, packed) in invalid.iter().enumerate() {
        let archive = temp.0.join(format!("manifest-{n}.cpb"));
        fs::write(&archive, packed.encode()?)?;
        let destination = temp.0.join(format!("reject-{n}"));
        assert!(
            backup::inspect(&archive).is_err(),
            "Accepted invalid manifest {n}"
        );
        assert!(backup::restore(&archive, &destination).is_err());
        assert!(!destination.exists());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    previous_hash: String,
    hash: String,
    state: State,
}
fn encode_envelope(
    previous: &str,
    state: State,
) -> std::result::Result<(String, Vec<u8>), serde_json::Error> {
    let mut canonical = previous.as_bytes().to_vec();
    canonical.extend(serde_json::to_vec(&state)?);
    let hash = fingerprint(&canonical);
    let bytes = serde_json::to_vec(&Envelope {
        previous_hash: previous.into(),
        hash: hash.clone(),
        state,
    })?;
    Ok((hash, bytes))
}
#[test]
fn all_outer_hashes_and_valid_tip_cannot_hide_semantically_corrupt_middle_revision() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive_fixture(&temp)?;
    let mut packed = Packed::read(&archive)?;
    let mut previous = String::new();
    for (entry, body) in packed.manifest["entries"]
        .as_array_mut()
        .ok_or("Entries missing")?
        .iter_mut()
        .zip(&mut packed.bodies)
    {
        if entry["kind"] != "snapshot" {
            continue;
        }
        let mut envelope: Envelope = serde_json::from_slice(body)?;
        if envelope.state.revision == 1 {
            envelope.state.schema_version = 2;
        }
        let (head, bytes) = encode_envelope(&previous, envelope.state)?;
        entry["bytes"] = json!(bytes.len());
        entry["sha256"] = json!(fingerprint(&bytes));
        *body = bytes;
        previous = head;
    }
    packed.manifest["head_hash"] = json!(previous);
    let corrupt = temp.0.join("semantically-invalid.cpb");
    fs::write(&corrupt, packed.encode()?)?;
    let problem = backup::inspect(&corrupt)
        .err()
        .ok_or("Accepted corrupt middle revision")?;
    assert!(
        problem.to_string().contains("Unsupported store schema"),
        "Unexpected rejection: {problem}"
    );
    let target = temp.0.join("rejected");
    assert!(backup::restore(&corrupt, &target).is_err());
    assert!(!target.exists());
    Ok(())
}

#[test]
fn cached_store_does_not_backup_later_corrupted_committed_history() -> TestResult {
    let temp = Temp::new()?;
    let (store, _) = source(&temp)?;
    fs::write(
        store.path().join("00000000000000000001.json"),
        b"damaged middle revision",
    )?;
    let before = tree(store.path())?;
    let output = temp.0.join("no-backup.cpb");
    assert!(store.backup(&output).is_err());
    assert!(!output.exists());
    assert_eq!(tree(store.path())?, before);
    Ok(())
}

#[test]
fn existing_targets_and_unknown_source_entries_are_preserved_without_overwrite() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive_fixture(&temp)?;
    let empty = temp.0.join("empty");
    fs::create_dir(&empty)?;
    assert!(backup::restore(&archive, &empty).is_err());
    assert_eq!(fs::read_dir(&empty)?.count(), 0);
    let existing = temp.0.join("existing");
    fs::create_dir(&existing)?;
    fs::write(existing.join("sentinel"), b"untouched")?;
    assert!(backup::restore(&archive, &existing).is_err());
    assert_eq!(fs::read(existing.join("sentinel"))?, b"untouched");
    let file = temp.0.join("file");
    fs::write(&file, b"user file")?;
    assert!(backup::restore(&archive, &file).is_err());
    assert_eq!(fs::read(file)?, b"user file");
    let (store, _) = Store::open(temp.0.join("source"))?;
    fs::write(
        store.path().join("unknown.txt"),
        b"unsupported root evidence",
    )?;
    let before = tree(store.path())?;
    assert!(store.backup(temp.0.join("unknown.cpb")).is_err());
    assert_eq!(tree(store.path())?, before);
    assert!(!temp.0.join("unknown.cpb").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinks_dangling_targets_and_aliased_nested_backup_destinations_are_rejected() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive_fixture(&temp)?;
    let original = fs::read(&archive)?;
    let dangling = temp.0.join("dangling");
    std::os::unix::fs::symlink(temp.0.join("absent"), &dangling)?;
    assert!(backup::restore(&archive, &dangling).is_err());
    assert!(fs::symlink_metadata(&dangling)?.file_type().is_symlink());
    let archive_alias = temp.0.join("archive-alias");
    std::os::unix::fs::symlink(&archive, &archive_alias)?;
    assert!(backup::inspect(&archive_alias).is_err());
    let (store, _) = Store::open(temp.0.join("source"))?;
    let root_alias = temp.0.join("root-alias");
    std::os::unix::fs::symlink(store.path(), &root_alias)?;
    assert!(store.backup(root_alias.join("nested.cpb")).is_err());
    assert!(!store.path().join("nested.cpb").exists());
    let link = store.path().join("bad.pending");
    std::os::unix::fs::symlink(&archive, &link)?;
    assert!(store.backup(temp.0.join("symlink.cpb")).is_err());
    assert!(!temp.0.join("symlink.cpb").exists());
    fs::remove_file(&link)?;
    let recovery_link = store.path().join("recovery/external");
    std::os::unix::fs::symlink(&temp.0, &recovery_link)?;
    assert!(store.backup(temp.0.join("recursive.cpb")).is_err());
    assert_eq!(fs::read(&archive)?, original);
    Ok(())
}
