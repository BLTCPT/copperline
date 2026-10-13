use super::*;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-backup-fault-{}-{}",
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
fn archive(temp: &Temp) -> Result<PathBuf> {
    let source = temp.0.join("source");
    let (mut store, state) = Store::open(source)?;
    store.commit(&state)?;
    let path = temp.0.join("backup.cpb");
    store.backup(&path)?;
    Ok(path)
}

#[test]
fn all_precommit_restore_faults_preserve_source_and_block_owned_partial_targets() -> TestResult {
    for stage in [
        RestoreStage::GuardReady,
        RestoreStage::TargetCreated,
        RestoreStage::EntryCopied,
        RestoreStage::Validated,
    ] {
        let temp = Temp::new()?;
        let archive = archive(&temp)?;
        let original = fs::read(&archive)?;
        let target = temp.0.join("Restored");
        let parent = fs::canonicalize(&temp.0)?;
        let guard = guard_path(&parent, "Restored");
        let result = restore_impl(&archive, &target, |at| {
            if at == stage {
                Err(error("Synthetic restore fault"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&archive)?, original);
        if stage == RestoreStage::GuardReady {
            assert!(!target.exists());
            assert!(!guard.exists());
        } else {
            assert!(target.is_dir());
            assert!(guard.is_file());
            let message = result.err().ok_or("Missing failure")?.to_string();
            // Windows TEMP may use an 8.3 alias; restore reports the resolved path.
            assert!(message.contains(&fs::canonicalize(&target)?.display().to_string()));
            assert!(message.contains(&guard.display().to_string()));
            assert!(Store::open(&target).is_err());
            assert!(Store::open(target.join("nested").join("workspace")).is_err());
            assert!(!target.join("nested").exists());
            if stage == RestoreStage::EntryCopied || stage == RestoreStage::Validated {
                assert_eq!(
                    fs::read(target.join("00000000000000000000.json"))?,
                    fs::read(temp.0.join("source/00000000000000000000.json"))?
                );
            }
        }
    }
    Ok(())
}

#[test]
fn destination_created_after_preflight_is_never_owned_or_overwritten() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive(&temp)?;
    let target = temp.0.join("raced");
    let guard = guard_path(&fs::canonicalize(&temp.0)?, "raced");
    let result = restore_impl(&archive, &target, |stage| {
        if stage == RestoreStage::GuardReady {
            fs::create_dir(&target).map_err(io)?;
            fs::write(target.join("sentinel"), b"existing user data").map_err(io)?;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert!(!guard.exists());
    assert_eq!(fs::read(target.join("sentinel"))?, b"existing user data");
    assert!(!target.join("workspace.lock").exists());
    Ok(())
}

#[test]
fn active_guard_blocks_normal_open_before_and_after_target_creation() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive(&temp)?;
    let target = temp.0.join("restored");
    restore_impl(&archive, &target, |stage| {
        if stage == RestoreStage::GuardReady || stage == RestoreStage::TargetCreated {
            ensure(Store::open(&target).is_err(), "Open bypassed active guard")?;
            ensure(
                Store::open(target.join("child")).is_err(),
                "Nested open bypassed active guard",
            )?;
        }
        Ok(())
    })?;
    let (_store, state) = Store::open(&target)?;
    assert_eq!(state.revision, 0);
    Ok(())
}

#[test]
fn changed_archive_or_copied_bytes_cannot_remove_the_guard() -> TestResult {
    for change_archive in [false, true] {
        let temp = Temp::new()?;
        let archive = archive(&temp)?;
        let target = temp.0.join("restored");
        let result = restore_impl(&archive, &target, |stage| {
            if change_archive && stage == RestoreStage::TargetCreated {
                fs::write(&archive, b"changed after successful preflight").map_err(io)?;
            } else if !change_archive && stage == RestoreStage::EntryCopied {
                fs::write(
                    target.join("00000000000000000000.json"),
                    b"damaged copied file",
                )
                .map_err(io)?;
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(guard_path(&fs::canonicalize(&temp.0)?, "restored").is_file());
        assert!(Store::open(&target).is_err());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn canonical_parent_and_workspace_aliases_cannot_bypass_a_partial_restore_guard() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive(&temp)?;
    let actual_parent = temp.0.join("actual");
    fs::create_dir(&actual_parent)?;
    let alias_parent = temp.0.join("parent-alias");
    std::os::unix::fs::symlink(&actual_parent, &alias_parent)?;
    let target = alias_parent.join("Restored");
    assert!(
        restore_impl(&archive, &target, |stage| {
            if stage == RestoreStage::EntryCopied {
                Err(error("Synthetic interrupted copy"))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    let actual_target = actual_parent.join("Restored");
    let alias_target = temp.0.join("workspace-alias");
    std::os::unix::fs::symlink(&actual_target, &alias_target)?;
    for path in [&target, &actual_target, &alias_target] {
        assert!(Store::open(path).is_err());
        assert!(Store::open(path.join("nested")).is_err());
    }
    let lower_guard = guard_path(&fs::canonicalize(&actual_parent)?, "restored");
    assert!(lower_guard.exists());
    Ok(())
}

#[test]
fn changed_guard_is_never_silently_deleted() -> TestResult {
    let temp = Temp::new()?;
    let archive = archive(&temp)?;
    let target = temp.0.join("restored");
    let guard = guard_path(&fs::canonicalize(&temp.0)?, "restored");
    let result = restore_impl(&archive, &target, |stage| {
        if stage == RestoreStage::Validated {
            fs::write(&guard, b"unknown replacement evidence").map_err(io)?;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read(&guard)?, b"unknown replacement evidence");
    assert!(Store::open(target).is_err());
    Ok(())
}

#[test]
fn interrupted_backup_preserves_partial_archive_and_never_changes_source() -> TestResult {
    for stage in [BackupStage::OutputCreated, BackupStage::EntryWritten] {
        let temp = Temp::new()?;
        let source = temp.0.join("source");
        let (mut store, state) = Store::open(&source)?;
        store.commit(&state)?;
        fs::write(
            source.join("00000000000000000001.pending"),
            b"unknown pending bytes",
        )?;
        let committed = fs::read(source.join("00000000000000000000.json"))?;
        let output = temp.0.join("partial.cpb");
        let result = store.backup_impl(&output, |at| {
            if at == stage {
                Err(error("Synthetic archive write failure"))
            } else {
                Ok(())
            }
        });
        let problem = result.err().ok_or("Expected injected archive failure")?;
        assert!(
            problem
                .to_string()
                .contains(&fs::canonicalize(&output)?.display().to_string())
        );
        assert!(output.is_file());
        assert!(inspect(&output).is_err());
        let preserved = fs::read(&output)?;
        assert!(store.backup(&output).is_err());
        assert_eq!(fs::read(&output)?, preserved);
        assert_eq!(
            fs::read(source.join("00000000000000000000.json"))?,
            committed
        );
        assert_eq!(
            fs::read(source.join("00000000000000000001.pending"))?,
            b"unknown pending bytes"
        );
    }
    Ok(())
}
