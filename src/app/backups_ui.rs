use super::App;
use crate::{common::state::State, pack::backup};
use eframe::egui;

impl App {
    /// Browse, restore, delete and configure retention of pack backups.
    pub(super) fn backups_ui(&mut self, ui: &mut egui::Ui) {
        if self.backups_dirty {
            self.backups = backup::list();
            self.backups_dirty = false;
            if !self
                .backups
                .iter()
                .any(|b| Some(&b.path) == self.selected_backup.as_ref().map(|(p, _)| p))
            {
                self.selected_backup = None;
            }
        }
        let idle = self.job.is_none();
        let mut keep = self.state.backups_to_keep.unwrap_or(backup::DEFAULT_KEEP);
        ui.horizontal(|ui| {
            ui.label("Backups to keep:");
            if ui
                .add(egui::DragValue::new(&mut keep).range(1..=50))
                .changed()
            {
                let mut s = State::load();
                s.backups_to_keep = Some(keep);
                self.status = match s.save().and_then(|_| backup::prune(keep)) {
                    Ok(()) => String::new(),
                    Err(e) => format!("Could not save setting: {e}"),
                };
                self.state.backups_to_keep = Some(keep);
                self.backups_dirty = true;
            }
            ui.label("Older backups are deleted automatically.");
        });
        if let Some(d) = backup::dir() {
            ui.label(format!("Stored in: {}", d.display()));
        }
        ui.separator();
        if self.backups.is_empty() {
            ui.label("No backups yet. One can be made before each pack update.");
            return;
        }
        let mut select = None;
        egui::ScrollArea::vertical()
            .id_salt("backup_list")
            .max_height(180.0)
            .show(ui, |ui| {
                for b in &self.backups {
                    let label = format!(
                        "{}  -  {}  ({:.1} MB)",
                        b.name(),
                        b.date(),
                        b.size as f64 / 1_048_576.0
                    );
                    let on = self
                        .selected_backup
                        .as_ref()
                        .is_some_and(|(p, _)| *p == b.path);
                    if ui.selectable_label(on, label).clicked() {
                        select = Some(b.clone());
                    }
                }
            });
        if let Some(b) = select {
            let files = backup::contents(&b).map_err(|e| e.to_string());
            self.selected_backup = Some((b.path.clone(), files));
        }
        let Some((path, files)) = &self.selected_backup else {
            return;
        };
        let Some(b) = self.backups.iter().find(|b| &b.path == path).cloned() else {
            return;
        };
        ui.separator();
        ui.label(format!("Contents of {} ({}):", b.name(), b.date()));
        match files {
            Ok(f) => {
                ui.label(format!("{} file(s)", f.len()));
                egui::ScrollArea::vertical()
                    .id_salt("backup_files")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for name in f {
                            ui.monospace(name);
                        }
                    });
            }
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
        ui.separator();
        let mut restore = false;
        ui.horizontal(|ui| {
            restore = ui
                .add_enabled(idle, egui::Button::new("Restore this backup"))
                .clicked();
            if ui.add_enabled(idle, egui::Button::new("Delete")).clicked() {
                self.status = match backup::delete(&b) {
                    Ok(()) => "Backup deleted.".into(),
                    Err(e) => format!("Could not delete backup: {e}"),
                };
                self.backups_dirty = true;
            }
        });
        ui.label("Restoring replaces your current pack folder.");
        self.progress_ui(ui);
        if restore {
            let Some(dir) = self.pack_dir() else {
                self.status = "Pack location unknown.".into();
                return;
            };
            self.start_job(move |worker| backup::restore(&b, &dir, worker));
            self.status.clear();
        }
    }
}
