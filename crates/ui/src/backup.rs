use super::{NAVY, SLATE, TEAL, Workbench, primary};
use copperline_engine::{BackupSummary, Engine};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

pub(super) struct BackupForm {
    pub archive: String,
    pub destination: String,
    pub inspected: Option<(String, BackupSummary)>,
    pub restored: Option<PathBuf>,
}
impl Default for BackupForm {
    fn default() -> Self {
        Self {
            archive: "copperline-backup.clbak".into(),
            destination: "copperline-restored".into(),
            inspected: None,
            restored: None,
        }
    }
}

impl Workbench {
    pub fn show_backup_inspection(&mut self, path: &str, summary: BackupSummary) {
        self.backup_form.archive = path.into();
        self.backup_form.inspected = Some((path.into(), summary));
    }
    pub(super) fn backup_view(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Keep the complete pricing record")
                .size(20.0)
                .color(NAVY),
        );
        ui.label("A portable archive preserves every committed revision, approval and retry receipt, together with interrupted-write and recovery evidence.");
        ui.add_space(12.0);
        ui.label(RichText::new("Current workspace").strong());
        ui.label(
            self.engine
                .path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "In-memory workspace — no durable history".into()),
        );
        ui.small(format!(
            "Current revision {} · Backup and restore do not add business revisions",
            self.engine.state().revision
        ));
        ui.add_space(16.0);
        ui.label(
            RichText::new("1  Create or verify an archive")
                .strong()
                .color(NAVY),
        );
        ui.label(
            "Use a new archive path outside the workspace. An existing archive is never replaced.",
        );
        ui.horizontal(|ui| {
            ui.label("Archive file");
            if ui
                .add(egui::TextEdit::singleline(&mut self.backup_form.archive).desired_width(600.0))
                .changed()
            {
                self.backup_form.inspected = None;
                self.backup_form.restored = None;
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.engine.path().is_some(),
                    egui::Button::new("Create complete backup"),
                )
                .clicked()
            {
                self.backup_form.inspected = None;
                let archive = self.backup_form.archive.clone();
                match self.engine.backup(&archive) {
                    Ok(summary) => {
                        self.backup_form.inspected = Some((archive, summary));
                        self.status =
                            "Complete backup created and verified. Current workspace is unchanged."
                                .into();
                        self.failed = false;
                    }
                    Err(e) => self.result::<()>(Err(e), ""),
                }
            }
            if ui.button("Verify archive").clicked() {
                let archive = self.backup_form.archive.clone();
                match Engine::inspect_backup(&archive) {
                    Ok(summary) => {
                        self.backup_form.inspected = Some((archive, summary));
                        self.status = "Archive bytes and full revision history verified.".into();
                        self.failed = false;
                    }
                    Err(e) => {
                        self.backup_form.inspected = None;
                        self.result::<()>(Err(e), "");
                    }
                }
            }
        });
        if let Some((path, summary)) = &self.backup_form.inspected {
            ui.add_space(10.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new("Verified archive").strong().color(TEAL));
                ui.label(path);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Revision {}", summary.committed_revision.map(|r| r.to_string()).unwrap_or_else(|| "none".into())));
                    ui.separator();
                    ui.label(format!("{} snapshots", summary.snapshot_count));
                    ui.separator();
                    ui.label(format!("{} bytes", summary.total_bytes));
                });
                ui.small(format!("{} pending files · {} recovery files · {} recovery directories", summary.pending_count, summary.recovery_file_count, summary.recovery_directory_count));
                ui.small(format!("Archive format {} · State schema {}", summary.format_version, summary.schema_version));
                ui.label(RichText::new(format!("SHA-256 {}", summary.sha256)).monospace().size(11.0));
                for warning in &summary.warnings {
                    ui.colored_label(SLATE, warning);
                }
                ui.small("Verification describes the bytes read at that time. Restore verifies them again.");
            });
        }
        ui.add_space(16.0);
        ui.label(
            RichText::new("2  Restore into a new workspace")
                .strong()
                .color(NAVY),
        );
        ui.label("Choose a new folder in an existing parent directory. The complete archive is verified before copying. Existing folders, including empty ones, are refused.");
        ui.horizontal(|ui| {
            ui.label("New folder");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.backup_form.destination)
                        .desired_width(600.0),
                )
                .changed()
            {
                self.backup_form.restored = None;
            }
        });
        if primary(ui, "Restore into new folder") {
            self.backup_form.restored = None;
            match Engine::restore_backup(&self.backup_form.archive, &self.backup_form.destination) {
                Ok(result) => {
                    self.backup_form.restored = Some(result.workspace.clone());
                    self.backup_form.inspected =
                        Some((self.backup_form.archive.clone(), result.backup));
                    self.status = format!(
                        "Restored {}. {}",
                        result.workspace.display(),
                        result.warnings.join(" ")
                    );
                    self.failed = false;
                }
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        if let Some(path) = self.backup_form.restored.clone() {
            ui.label(format!("Restored workspace: {}", path.display()));
            if ui.button("Open restored workspace").clicked() {
                match Engine::open(&path) {
                    Ok(engine) => {
                        *self = Workbench::new(engine);
                        self.status = format!("Opened restored workspace {}", path.display());
                    }
                    Err(e) => self.result::<()>(Err(e), ""),
                }
            }
        }
        ui.add_space(12.0);
        ui.small("An interrupted restore stays blocked and preserves its partial folder for inspection. Pending writes remain uncommitted; restoring never approves or replays them.");
        ui.small("Local filesystem access only. Archives are not encrypted or digitally signed. Large archives can take time to validate.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::View;
    use copperline_engine::Command;
    use copperline_model::{Actor, Role};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> std::io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "copperline-native-backup-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn native_backup_restore_and_open_preserve_state_and_existing_target()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = Directory::new()?;
        let source = directory.0.join("source");
        let archive = directory.0.join("complete.clbak");
        let destination = directory.0.join("restored");
        let mut engine = Engine::open(&source)?;
        engine.run(
            Actor {
                name: "Synthetic test".into(),
                role: Role::Admin,
            },
            Command::SeedDemo,
        )?;
        let state = engine.state().clone();
        let mut app = Workbench::new(engine);
        app.view = View::Backup;
        app.backup_form.archive = archive.display().to_string();
        app.backup_form.destination = destination.display().to_string();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 1200.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
        h.get_by_label("Create complete backup").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        assert_eq!(
            h.state()
                .backup_form
                .inspected
                .as_ref()
                .map(|(_, s)| s.snapshot_count),
            Some(2)
        );
        assert_eq!(h.state().engine.state(), &state);
        let archive_bytes = fs::read(&archive)?;
        h.get_by_label("Create complete backup").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(h.state().failed);
        assert_eq!(fs::read(&archive)?, archive_bytes);
        h.get_by_label("Verify archive").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        h.get_by_label("Restore into new folder").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        let snapshot = fs::read(destination.join("00000000000000000001.json"))?;
        h.get_by_label("Restore into new folder").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(h.state().failed);
        assert_eq!(
            fs::read(destination.join("00000000000000000001.json"))?,
            snapshot
        );
        assert!(h.state().backup_form.restored.is_none());
        // Only a confirmed successful restore offers an open action.
        h.state_mut().backup_form.destination =
            directory.0.join("restored-next").display().to_string();
        h.get_by_label("Restore into new folder").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        h.get_by_label("Open restored workspace").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(h.state().engine.state(), &state);
        assert_eq!(
            h.state().engine.path(),
            Some(fs::canonicalize(directory.0.join("restored-next"))?.as_path())
        );
        drop(h);
        let reopened = Engine::open(source)?;
        assert_eq!(reopened.state(), &state);
        Ok(())
    }
    #[test]
    fn empty_workspace_can_use_backup_without_seeding_demo()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = Directory::new()?;
        let source = directory.0.join("empty");
        let engine = Engine::open(source)?;
        let mut app = Workbench::new(engine);
        app.view = View::Backup;
        app.backup_form.archive = directory.0.join("empty.clbak").display().to_string();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 1200.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
        h.get_by_label("Create complete backup").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        assert_eq!(h.state().engine.state().revision, 0);
        assert!(h.state().engine.state().products.is_empty());
        Ok(())
    }
}
