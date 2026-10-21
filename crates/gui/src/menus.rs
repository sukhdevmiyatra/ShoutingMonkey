//! The menu bar, built from the action list: File, Edit, View, Crawl,
//! Export, Tools and Help.

use egui::Button;

use crate::actions::{Action, EXPORT_GROUPS, EXPORTS};
use crate::app::{App, InsightTab};
use crate::dialogs::ConfigPage;
use crate::prefs::{Mode, ThemeChoice};

impl App {
    /// A menu entry for `action`: disabled with a reason when it cannot run.
    fn entry(&mut self, ui: &mut egui::Ui, action: Action, pending: &mut Option<Action>) {
        let blocked = action.blocked(self);
        let resp = ui.add_enabled(blocked.is_none(), Button::new(action.label()).shortcut_text(action.shortcut()));
        let resp = match blocked {
            Some(why) => resp.on_disabled_hover_text(format!("Not available: {why}")),
            None => resp,
        };
        if resp.clicked() {
            *pending = Some(action);
            ui.close();
        }
    }

    /// A check-mark entry for a panel or mode.
    fn check(&mut self, ui: &mut egui::Ui, action: Action, on: bool, pending: &mut Option<Action>) {
        let mut v = on;
        let enabled = action.blocked(self).is_none();
        if ui.add_enabled(enabled, egui::Checkbox::new(&mut v, action.label())).clicked() {
            *pending = Some(action);
            ui.close();
        }
    }

    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut pending: Option<Action> = None;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                self.entry(ui, Action::Open, &mut pending);
                ui.menu_button("Recent Crawls", |ui| {
                    if self.prefs.recent.is_empty() {
                        ui.add_enabled(false, Button::new("None yet"));
                    }
                    let mut open = None;
                    for path in self.prefs.recent.clone() {
                        let name = path
                            .parent()
                            .and_then(|d| d.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        if ui.add_enabled(!self.running(), Button::new(name)).on_hover_text(path.display().to_string()).clicked() {
                            open = Some(path);
                            ui.close();
                        }
                    }
                    if !self.prefs.recent.is_empty() {
                        ui.separator();
                        if ui.button("Clear this list").clicked() {
                            self.prefs.recent.clear();
                            self.prefs_dirty = true;
                            ui.close();
                        }
                    }
                    if let Some(p) = open {
                        self.open_crawl(p);
                    }
                });
                self.entry(ui, Action::SaveCopy, &mut pending);
                self.entry(ui, Action::Close, &mut pending);
                ui.separator();
                self.entry(ui, Action::Settings, &mut pending);
                ui.separator();
                self.entry(ui, Action::Exit, &mut pending);
            });

            ui.menu_button("Edit", |ui| {
                self.entry(ui, Action::Find, &mut pending);
                self.entry(ui, Action::CopyUrl, &mut pending);
                self.entry(ui, Action::OpenUrl, &mut pending);
                ui.separator();
                self.entry(ui, Action::Palette, &mut pending);
            });

            ui.menu_button("View", |ui| {
                let (n, i, d) = (self.prefs.show_navigator, self.prefs.show_insights, self.prefs.show_inspector);
                self.check(ui, Action::ToggleNavigator, n, &mut pending);
                self.check(ui, Action::ToggleInsights, i, &mut pending);
                self.check(ui, Action::ToggleInspector, d, &mut pending);
                ui.separator();
                ui.menu_button("Theme", |ui| {
                    for t in ThemeChoice::ALL {
                        if ui.radio(self.prefs.theme == t, t.label()).clicked() {
                            pending = Some(Action::Theme(t));
                            ui.close();
                        }
                    }
                });
                self.entry(ui, Action::ZoomIn, &mut pending);
                self.entry(ui, Action::ZoomOut, &mut pending);
                self.entry(ui, Action::ZoomReset, &mut pending);
                ui.separator();
                self.entry(ui, Action::ResetColumns, &mut pending);
                self.entry(ui, Action::ResetLayout, &mut pending);
            });

            ui.menu_button("Mode", |ui| {
                self.entry(ui, Action::Start, &mut pending);
                self.entry(ui, Action::PauseResume, &mut pending);
                self.entry(ui, Action::Stop, &mut pending);
                ui.separator();
                let mode = self.prefs.mode;
                self.check(ui, Action::SpiderMode, mode == Mode::Spider, &mut pending);
                self.check(ui, Action::ListMode, mode == Mode::List, &mut pending);
                self.entry(ui, Action::UploadList, &mut pending);
                ui.separator();
                self.entry(ui, Action::Compare, &mut pending);
            });

            ui.menu_button("Configuration", |ui| {
                for page in ConfigPage::ALL {
                    self.entry(ui, Action::Configure(page), &mut pending);
                }
                ui.separator();
                self.entry(ui, Action::SaveProfile, &mut pending);
                self.entry(ui, Action::LoadProfile, &mut pending);
                self.entry(ui, Action::DefaultProfile, &mut pending);
            });

            ui.menu_button("Bulk Export", |ui| {
                self.entry(ui, Action::ExportView, &mut pending);
                ui.separator();
                for group in EXPORT_GROUPS {
                    ui.menu_button(*group, |ui| {
                        for i in (0..EXPORTS.len()).filter(|&i| EXPORTS[i].group == *group) {
                            self.entry(ui, Action::Export(i), &mut pending);
                        }
                    });
                }
            });

            ui.menu_button("Tools", |ui| {
                self.entry(ui, Action::CrawlMap, &mut pending);
                ui.separator();
                for t in InsightTab::ALL {
                    self.entry(ui, Action::Insight(t), &mut pending);
                }
                ui.separator();
                self.entry(ui, Action::Palette, &mut pending);
            });

            ui.menu_button("Help", |ui| {
                self.entry(ui, Action::Guide, &mut pending);
                self.entry(ui, Action::Shortcuts, &mut pending);
                ui.separator();
                self.entry(ui, Action::OpenCrawlsFolder, &mut pending);
                self.entry(ui, Action::OpenLogFolder, &mut pending);
                ui.separator();
                self.entry(ui, Action::About, &mut pending);
            });
        });
        if let Some(a) = pending {
            self.run(a, &ctx);
        }
    }
}
