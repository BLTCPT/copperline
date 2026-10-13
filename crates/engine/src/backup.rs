//! Local operational file actions, separate from audited business commands.
use super::{BackupSummary, Engine, RestoreSummary};
use copperline_model::{Result, ensure, error};
use copperline_storage::{Store, backup};
use std::path::Path;

impl Engine {
    /// Copy the complete durable history under this engine's existing workspace lock.
    pub fn backup(&self, archive: impl AsRef<Path>) -> Result<BackupSummary> {
        self.store
            .as_ref()
            .ok_or_else(|| error("An in-memory workspace has no durable history to back up"))?
            .backup(archive)
    }

    /// Back up an existing workspace without creating or bootstrapping a missing source.
    pub fn backup_workspace(
        workspace: impl AsRef<Path>,
        archive: impl AsRef<Path>,
    ) -> Result<BackupSummary> {
        let metadata = std::fs::symlink_metadata(workspace.as_ref())
            .map_err(|e| error(format!("Backup source must be an existing workspace: {e}")))?;
        ensure(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Backup source must be an existing regular directory",
        )?;
        let (store, _) = Store::open(workspace)?;
        store.backup(archive)
    }

    /// Fully validate an archive without changing any workspace.
    pub fn inspect_backup(archive: impl AsRef<Path>) -> Result<BackupSummary> {
        backup::inspect(archive)
    }

    /// Restore only to a new directory; existing workspaces are never replaced.
    pub fn restore_backup(
        archive: impl AsRef<Path>,
        new_workspace: impl AsRef<Path>,
    ) -> Result<RestoreSummary> {
        backup::restore(archive, new_workspace)
    }
}
