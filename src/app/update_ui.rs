use super::{App, UpdateOptions};
use crate::{
    config,
    pack::{self, backup, layout, updater},
};
use eframe::egui;

impl App {
    /// Start installing/updating the pack on a worker thread, optionally backing up and preserving the layout first.
    pub(super) fn start_update(&mut self, opts: UpdateOptions) {
        if self.job.is_some() {
            return;
        }
        self.refresh_pack_version();
        let (Some(m), Some(dir)) = (&self.manifest, self.pack_dir()) else {
            self.status = "Update information unavailable".into();
            return;
        };
        let installed = self.state.pack_version;
        let Some(plan) = updater::plan(m, installed) else {
            self.status = "No releases found".into();
            return;
        };
        let installed_release =
            installed.and_then(|v| m.releases.iter().find(|r| r.version == v).cloned());
        if opts.save_layout && installed_release.is_none() {
            self.status =
                "Cannot save positions: installed pack version not found in the release list."
                    .into();
            return;
        }
        let cfg = if opts.apply_config {
            if !config::exists(Some(&dir)) {
                self.status =
                    "No saved settings to apply; configure them first or untick that option."
                        .into();
                return;
            }
            let c = config::load(Some(&dir));
            if let Some(msg) = c.validate() {
                self.status = format!("Saved settings are incomplete: {msg}");
                return;
            }
            Some(c)
        } else {
            None
        };
        let tag = installed.map(|v| v.to_string());
        let keep = self.state.backups_to_keep.unwrap_or(backup::DEFAULT_KEEP);
        self.start_job(move |worker| {
            // The reference download doesn't touch the pack, so it runs alongside the backup.
            let pristine = match (opts.save_layout, installed_release) {
                (true, Some(release)) => {
                    let w = worker.clone();
                    Some(std::thread::spawn(move || {
                        pack::fetch_pristine_asrs(&release, &w)
                    }))
                }
                _ => None,
            };
            if opts.backup {
                backup::create(&dir, tag.as_deref(), keep, worker)?;
            }
            if let Some(h) = pristine {
                let pristine = h
                    .join()
                    .map_err(|_| anyhow::anyhow!("layout download panicked"))??;
                layout::save_changes(&dir, &pristine)?;
            }
            updater::execute(plan, &dir, worker)?;
            match &cfg {
                // Also re-applies any saved screen layout.
                Some(cfg) => config::apply(cfg, &dir, worker)?,
                None => {}
            }
            Ok(())
        });
        self.status.clear();
    }

    pub(super) fn pack_ui(&mut self, ui: &mut egui::Ui) {
        self.refresh_pack_version();
        match self.state.pack_version {
            Some(v) => ui.label(format!("Installed pack version: {v}")),
            None => ui.label("Controller pack not installed."),
        };
        if let Some(dir) = self.pack_dir() {
            ui.label(format!("Location: {}", dir.display()));
        }
        self.updates_ui(ui);
        self.navdata_ui(ui);
        self.progress_ui(ui, false);
    }

    fn navdata_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);
        ui.separator();
        ui.heading("Navigation data");
        ui.label(
            "AeroNav navdata is not included with the controller pack. Compare its AIRAC cycle after importing; AeroNav may not have published a new cycle yet.",
        );

        let Some(dir) = self.pack_dir() else {
            ui.weak("Install the controller pack before importing navdata.");
            return;
        };
        let missing = pack::navdata::missing_files(&dir);
        if missing.is_empty() {
            ui.label("All six navdata files are present.");
        } else {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Navdata files missing: {}", missing.join(", ")),
            );
        }
        match pack::navdata::airac_cycles(&dir) {
            Ok((file_count, cycles)) if cycles.is_empty() => {
                ui.weak(format!(
                    "No AIRAC cycle found in the {file_count} AeroNav files in Datafiles."
                ));
            }
            Ok((file_count, cycles)) => {
                let mut by_cycle = std::collections::BTreeMap::new();
                for (name, cycle) in &cycles {
                    by_cycle
                        .entry(*cycle)
                        .or_insert_with(Vec::new)
                        .push(name.as_str());
                }
                for (cycle, files) in by_cycle {
                    ui.label(format!("AIRAC {cycle}: {}", files.join(", ")));
                }
                if let Some(version) = self.state.pack_version {
                    if cycles
                        .iter()
                        .all(|(_, cycle)| pack::navdata::matches_pack_cycle(*cycle, version))
                    {
                        ui.label(format!(
                            "All detected cycles match controller pack {version}."
                        ));
                    } else {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "One or more navdata files differ from controller pack {version}. You may need to update the navdata from AeroNav."
                            ),
                        );
                    }
                }
                let unlabelled = file_count.saturating_sub(cycles.len());
                if unlabelled > 0 {
                    ui.weak(format!(
                        "No AIRAC cycle found in {unlabelled} of {file_count} AeroNav files."
                    ));
                }
            }
            Err(error) => {
                ui.weak(format!("Could not check navdata AIRAC cycles: {error}"));
            }
        }

        ui.horizontal(|ui| {
            ui.hyperlink_to("Open AeroNav downloads", pack::navdata::AERONAV_URL);
            if ui
                .add_enabled(
                    self.job.is_none() && dir.is_dir(),
                    egui::Button::new("Select downloaded archive..."),
                )
                .clicked()
            {
                let dialog = rfd::FileDialog::new()
                    .add_filter("Navdata archives", &["zip", "7z"])
                    .add_filter("ZIP archives", &["zip"])
                    .add_filter("7z archives", &["7z"]);
                let dialog = match dirs::download_dir().filter(|path| path.is_dir()) {
                    Some(downloads) => dialog.set_directory(downloads),
                    None => dialog,
                };
                if let Some(zip_path) = dialog.pick_file() {
                    self.navdata_archive_after_import = Some(zip_path.clone());
                    self.start_job(move |worker| {
                        crate::common::download::set_message(worker, "Importing AeroNav navdata");
                        pack::navdata::import_gng_archive(&zip_path, &dir)
                    });
                    self.status.clear();
                }
            }
        });
    }

    pub(super) fn navdata_archive_delete_prompt(&mut self, ui: &mut egui::Ui) {
        let Some(archive_path) = self.navdata_archive_delete_prompt.clone() else {
            return;
        };
        let mut delete = false;
        let mut keep = false;
        let modal =
            egui::Modal::new(egui::Id::new("navdata_archive_delete_prompt")).show(ui.ctx(), |ui| {
                ui.set_width(400.0);
                ui.heading("Navdata import complete");
                ui.label("Would you like to delete the downloaded archive?");
                ui.weak(archive_path.display().to_string());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    delete = ui.button("Delete archive").clicked();
                    keep = ui.button("Keep archive").clicked();
                });
            });

        if delete {
            match std::fs::remove_file(&archive_path) {
                Ok(()) => self.status = "Navdata imported; downloaded archive deleted.".into(),
                Err(error) => {
                    self.status = format!("Navdata imported, but could not delete archive: {error}")
                }
            }
            self.navdata_archive_delete_prompt = None;
        } else if keep || modal.should_close() {
            self.status = "Navdata imported; downloaded archive kept.".into();
            self.navdata_archive_delete_prompt = None;
        }
    }

    pub(super) fn updates_ui(&mut self, ui: &mut egui::Ui) {
        self.refresh_pack_version();
        let Some(m) = &self.manifest else {
            ui.label("Update information unavailable.");
            return;
        };
        let plan = updater::plan(m, self.state.pack_version);
        let mut start = false;
        let idle = self.job.is_none();
        let existing = self.state.pack_version.is_some();
        match &plan {
            None => {
                ui.label("No releases found.");
            }
            Some(updater::Plan::UpToDate) => {
                ui.label("You are up to date.");
            }
            Some(updater::Plan::Patches(p)) => {
                ui.label(format!("{} update(s) to apply, in order:", p.len()));
                for r in p {
                    ui.label(format!("  {}", r.version));
                }
                start = ui.add_enabled(idle, egui::Button::new("Update")).clicked();
            }
            Some(updater::Plan::Reinstall(r)) => {
                ui.label(format!("Full install of {}.", r.version));
                start = ui.add_enabled(idle, egui::Button::new("Install")).clicked();
            }
        }
        if start {
            if existing {
                self.confirm_update = true;
            } else {
                self.start_update(UpdateOptions::default());
            }
        }
        if self.confirm_update {
            self.update_prompt(ui);
        }
    }

    /// Modal asking the two questions that are easy to miss as tick boxes.
    pub(super) fn update_prompt(&mut self, ui: &mut egui::Ui) {
        let dir = self.pack_dir();
        let settings_ok =
            config::exists(dir.as_deref()) && config::load(dir.as_deref()).validate().is_none();
        let saved_layout = layout::load_layout().len();
        let mut opts = self.update_opts;
        let (mut go, mut cancel) = (false, false);
        let yes_no = |ui: &mut egui::Ui, question: &str, value: &mut bool, enabled: bool| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(question).strong().size(18.0));
            ui.horizontal(|ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.selectable_value(value, true, "Yes");
                    ui.selectable_value(value, false, "No");
                });
            });
        };
        let modal = egui::Modal::new(egui::Id::new("update_prompt")).show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.heading("Before we update");
            yes_no(
                ui,
                "Back up your current pack first?",
                &mut opts.backup,
                true,
            );
            if !settings_ok {
                opts.apply_config = false;
            }
            yes_no(
                ui,
                "Apply your saved settings and screen layout once the update is done?",
                &mut opts.apply_config,
                settings_ok,
            );
            if !settings_ok {
                ui.weak("No complete saved settings were found, so there is nothing to apply.");
            }
            yes_no(
                ui,
                "Take a new snapshot of your current screen layout first?",
                &mut opts.save_layout,
                true,
            );
            ui.weak(if opts.save_layout {
                "Your current EuroScope window positions will replace the saved layout."
            } else if saved_layout > 0 {
                "No new snapshot: your previously saved layout will be re-applied as it is."
            } else {
                "No new snapshot, and no layout has been saved yet."
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                go = ui
                    .add_enabled(
                        self.job.is_none(),
                        egui::Button::new(egui::RichText::new("Start update").strong())
                            .fill(ui.visuals().selection.bg_fill)
                            .min_size(egui::vec2(160.0, 38.0)),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        self.update_opts = opts;
        if go {
            self.confirm_update = false;
            self.start_update(opts);
        } else if cancel || modal.should_close() {
            self.confirm_update = false;
        }
    }
}
