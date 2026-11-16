//! "New font folder…": make a folder on the disk (where the person chooses) that is a font folder
//! of FontCraft right away – also while it is still empty (it shows 0).

use super::*;
use egui::{vec2, Align2, RichText};
use std::path::Path;

/// Characters a folder name can't have on Windows, macOS or Linux.
const BAD: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

fn documents() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
    let docs = home.join("Documents");
    if docs.is_dir() {
        docs
    } else {
        home
    }
}

impl FontCraftApp {
    /// Where a new font folder goes unless the person chooses otherwise: next to the font folders
    /// already on this computer, else in Documents.
    fn new_folder_place(&self) -> PathBuf {
        let online: Vec<PathBuf> = Provider::ALL.iter().map(|p| p.fonts_dir()).collect();
        let data = config::data_dir();
        self.cfg
            .folders
            .iter()
            .filter(|f| !self.is_net_folder(f) && !online.contains(*f) && !f.starts_with(&data))
            .find_map(|f| f.parent().filter(|p| p.is_dir()).map(Path::to_path_buf))
            .unwrap_or_else(documents)
    }

    /// Open the window; `inside`: make a sub-folder of this folder.
    pub(super) fn new_folder_open(&mut self, inside: Option<PathBuf>) {
        let place = inside.unwrap_or_else(|| self.new_folder_place());
        self.new_folder = Some(("New font folder".into(), place.display().to_string()));
    }

    pub(super) fn new_folder_window(&mut self, ctx: &egui::Context) {
        let Some((mut name, mut place)) = self.new_folder.take() else { return };
        let p = self.p();
        let mut open = true;
        let mut go: Option<bool> = None;
        let mut existing: Option<PathBuf> = None;
        let clean = name.trim().to_string();
        let bad_name = clean.is_empty() || clean.contains(BAD) || clean == "." || clean == "..";
        let target = PathBuf::from(place.trim()).join(&clean);
        let inside = self.cfg.folders.iter().find(|f| target.starts_with(f)).cloned();
        egui::Window::new(RichText::new("New font folder").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label("Name");
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !bad_name {
                    go = Some(true);
                }
                ui.add_space(6.0);
                ui.label("Where");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut place).desired_width(410.0));
                    if ui.button("Choose…").clicked() {
                        let mut d = rfd::FileDialog::new().set_title("Where should the new font folder go?");
                        if Path::new(place.trim()).is_dir() {
                            d = d.set_directory(place.trim());
                        }
                        if let Some(dir) = d.pick_folder() {
                            place = dir.display().to_string();
                        }
                    }
                });
                ui.add_space(8.0);
                if bad_name {
                    ui.label(RichText::new(format!("Give the folder a name (without {})", BAD.iter().collect::<String>())).color(p.heart));
                } else {
                    ui.label(RichText::new(target.display().to_string()).monospace().size(11.5).color(p.text));
                    let note = if target.is_dir() {
                        "This folder exists already – FontCraft adds it as a font folder.".to_string()
                    } else if let Some(root) = &inside {
                        let n = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        format!("It is inside your font folder “{n}” and shows there in the folder tree.")
                    } else {
                        "FontCraft creates it and adds it to your Folders – empty folders show 0 until fonts are in them.".to_string()
                    };
                    ui.label(RichText::new(note).size(12.0).color(p.weak));
                    ui.label(
                        RichText::new("Only font files (.ttf, .otf, .ttc, .otc) in it count; any other files are ignored.")
                            .size(12.0)
                            .color(p.weak),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if inside.is_none() && ui.button("Add an existing folder instead…").clicked() {
                        go = Some(false);
                        if let Some(dir) = rfd::FileDialog::new().set_title("Add a font folder").pick_folder() {
                            existing = Some(dir);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b = egui::Button::new(RichText::new("Create").color(p.on_accent).strong()).fill(p.accent);
                        if ui.add_enabled(!bad_name && !place.trim().is_empty(), b).clicked() {
                            go = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            go = Some(false);
                        }
                    });
                });
            });
        match go {
            Some(true) => match std::fs::create_dir_all(&target) {
                Ok(()) => {
                    if inside.is_some() {
                        // a sub-folder of a font folder: it shows in the tree after the next look
                        self.view = View::Folder(target.clone());
                        self.set_status(format!("Created {}", target.display()));
                        self.rescan();
                    } else {
                        self.add_folder(target.clone());
                        self.set_status(format!("Created the font folder {}", target.display()));
                    }
                }
                Err(e) => {
                    self.set_status(format!("The folder could not be created: {e}"));
                    self.new_folder = Some((name, place));
                }
            },
            Some(false) => {
                if let Some(dir) = existing {
                    self.add_folder(dir);
                }
            }
            None if open => self.new_folder = Some((name, place)),
            None => {}
        }
    }
}
