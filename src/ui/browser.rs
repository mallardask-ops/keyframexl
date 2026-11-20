//! Asset browser.
use super::{
    App,
    files::{self, FileDrag},
};
use crate::{plugins::Scanned, theme as t};
use eframe::egui::{self, RichText, Ui};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct PluginDrag(pub Scanned);

/// What a click on a favorite or tagged asset asks for.
enum AssetAct {
    AddPlugin(Scanned),
    Preset(PathBuf),
    Preview(PathBuf),
    Load(PathBuf),
    Star(String),
}

pub(crate) fn matches(query: &str, name: &str) -> bool {
    name.to_lowercase().contains(&query.trim().to_lowercase())
}

fn width_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("keyframexl").join("browser-width"))
}

pub(super) fn width(ctx: &egui::Context) -> f32 {
    let id = egui::Id::new("browser-width-value");
    ctx.data_mut(|d| {
        *d.get_temp_mut_or_insert_with(id, || {
            width_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(super::BROWSER_W)
                .clamp(160.0, 360.0)
        })
    })
}

fn favorites_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("keyframexl")
            .join("browser-favorites"),
    )
}

/// Read favorite paths (one per line); a missing or unreadable file is empty.
pub(crate) fn load_favorites_from(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Write favorite paths, creating the config folder. The path is a parameter
/// so tests can use a temp dir instead of the real config.
pub(crate) fn save_favorites_to(path: &Path, favs: &[String]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, favs.join("\n"))
}

/// Favorites cached in egui memory (loaded from disk once per session).
fn favorites(ctx: &egui::Context) -> Vec<String> {
    let id = egui::Id::new("browser-favorites");
    ctx.data_mut(|d| {
        if d.get_temp::<Vec<String>>(id).is_none() {
            let favs = favorites_path()
                .map(|p| load_favorites_from(&p))
                .unwrap_or_default();
            d.insert_temp(id, favs);
        }
        d.get_temp(id).unwrap_or_default()
    })
}

fn set_favorites(ctx: &egui::Context, favs: Vec<String>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("browser-favorites"), favs.clone()));
    if let Some(p) = favorites_path() {
        let _ = save_favorites_to(&p, &favs);
    }
}

pub(super) fn is_favorite(ctx: &egui::Context, key: &str) -> bool {
    favorites(ctx).iter().any(|f| f == key)
}

pub(super) fn toggle_favorite(ctx: &egui::Context, key: String) {
    let mut favs = favorites(ctx);
    if favs.iter().any(|f| f == &key) {
        favs.retain(|f| f != &key);
    } else {
        favs.push(key);
    }
    set_favorites(ctx, favs);
}

fn tags_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("keyframexl").join("browser-tags"))
}

/// Tags by asset key (a file or plugin path): one line each, `tag,tag<TAB>path`.
/// Tags come first because paths may hold commas; tags never hold tabs.
pub(crate) type TagMap = std::collections::BTreeMap<String, Vec<String>>;

pub(crate) fn parse_tag_file(text: &str) -> TagMap {
    let mut map = TagMap::new();
    for line in text.lines() {
        if let Some((tags, key)) = line.split_once('\t') {
            let tags = parse_tag_input(tags);
            if !tags.is_empty() && !key.trim().is_empty() {
                map.insert(key.trim().to_owned(), tags);
            }
        }
    }
    map
}

pub(crate) fn tag_file_text(map: &TagMap) -> String {
    map.iter()
        .filter(|(_, t)| !t.is_empty())
        .map(|(k, t)| format!("{}\t{k}", t.join(",")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// "Kick, 808,,kick " → ["Kick", "808"]: trimmed, no empties, no repeats
/// (case-insensitive, the first spelling wins), at most 32 characters each.
pub(crate) fn parse_tag_input(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in text
        .split(',')
        .map(|t| t.trim().replace('\t', " "))
        .filter(|t| !t.is_empty())
    {
        let t: String = t.chars().take(32).collect();
        if !out.iter().any(|o| o.to_lowercase() == t.to_lowercase()) {
            out.push(t);
        }
    }
    out
}

/// Every tag in use with how many assets carry it, sorted by name.
pub(crate) fn tag_counts(map: &TagMap) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for t in map.values().flatten() {
        match out
            .iter_mut()
            .find(|(o, _)| o.to_lowercase() == t.to_lowercase())
        {
            Some((_, n)) => *n += 1,
            None => out.push((t.clone(), 1)),
        }
    }
    out.sort_by_key(|(t, _)| t.to_lowercase());
    out
}

/// Keys carrying the `picked` tags: any of them, or all with `all`.
pub(crate) fn tagged(map: &TagMap, picked: &[String], all: bool) -> Vec<String> {
    let has =
        |tags: &Vec<String>, p: &String| tags.iter().any(|t| t.to_lowercase() == p.to_lowercase());
    map.iter()
        .filter(|(_, tags)| {
            if all {
                picked.iter().all(|p| has(tags, p))
            } else {
                picked.iter().any(|p| has(tags, p))
            }
        })
        .map(|(k, _)| k.clone())
        .collect()
}

/// Tags cached in egui memory (loaded from disk once per session).
fn tags(ctx: &egui::Context) -> TagMap {
    let id = egui::Id::new("browser-tags");
    ctx.data_mut(|d| {
        if d.get_temp::<TagMap>(id).is_none() {
            let map = tags_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|s| parse_tag_file(&s))
                .unwrap_or_default();
            d.insert_temp(id, map);
        }
        d.get_temp(id).unwrap_or_default()
    })
}

fn set_tags(ctx: &egui::Context, map: TagMap) {
    if let Some(p) = tags_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, tag_file_text(&map));
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("browser-tags"), map));
}

/// The "Tags" submenu of an asset's context menu (FL: Tags / Add a tag):
/// tick the tags in use, or type new ones, comma-separated.
pub(super) fn tag_menu(ui: &mut Ui, key: &str) {
    let mine = tags(ui.ctx()).get(key).cloned().unwrap_or_default();
    let title = if mine.is_empty() {
        "Tags".to_string()
    } else {
        format!("Tags · {}", mine.join(", "))
    };
    ui.menu_button(title, |ui| {
        super::kit::menu_style(ui);
        let mut map = tags(ui.ctx());
        let mut changed = false;
        for (tag, _) in tag_counts(&map) {
            let entry = map.entry(key.to_owned()).or_default();
            let mut on = entry.iter().any(|t| t.to_lowercase() == tag.to_lowercase());
            if ui.checkbox(&mut on, &tag).changed() {
                if on {
                    entry.push(tag.clone());
                } else {
                    entry.retain(|t| t.to_lowercase() != tag.to_lowercase());
                }
                changed = true;
            }
        }
        let id = egui::Id::new(("tag-input", key));
        let mut text: String = ui.data(|d| d.get_temp(id).unwrap_or_default());
        let r = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text("Add tags: kick, 808")
                .desired_width(180.0),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let entry = map.entry(key.to_owned()).or_default();
            let joined = format!("{},{text}", entry.join(","));
            *entry = parse_tag_input(&joined);
            text.clear();
            changed = true;
        }
        ui.data_mut(|d| d.insert_temp(id, text));
        if !mine.is_empty() && ui.button("Remove all tags").clicked() {
            map.remove(key);
            changed = true;
            ui.close_menu();
        }
        if changed {
            map.retain(|_, t| !t.is_empty());
            set_tags(ui.ctx(), map);
        }
    });
}

pub(super) fn save_width(ctx: &egui::Context, width: f32, persist: bool) {
    let id = egui::Id::new("browser-width-value");
    let old = self::width(ctx);
    if (old - width).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(id, width));
        if persist && let Some(p) = width_path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, width.to_string());
        }
    }
}

impl App {
    pub(super) fn browser(&mut self, ui: &mut Ui) {
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: 8,
                right: 6,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| self.browser_body(ui));
    }

    fn browser_body(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        let id = egui::Id::new("kfxl-browser-search");
        let mut q: String = ui.ctx().data(|d| d.get_temp(id).unwrap_or_default());
        let response = ui.add(
            egui::TextEdit::singleline(&mut q)
                .id(id)
                .hint_text("Search assets")
                .font(t::sans(13.0))
                .desired_width(f32::INFINITY),
        );
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            q.clear();
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, q.clone()));
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("browser-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.favorites_section(ui);
                self.tags_section(ui, &q);
                // Instruments and effects stay in their own groups (FL browser):
                // one long mixed list hides where a click will put the plugin.
                let insts: Vec<_> = self
                    .plugin_choices(true)
                    .into_iter()
                    .filter(|p| matches(&q, &p.name))
                    .collect();
                let mut fxs: Vec<_> = self
                    .plugin_choices(false)
                    .into_iter()
                    .filter(|p| matches(&q, &p.name))
                    .collect();
                fxs.sort_by_cached_key(|p| p.name.to_lowercase());
                let problems = self.plugins.list.iter().filter(|p| !p.usable()).count();
                let scanning = self.plugins.scan_rx.is_some();
                egui::CollapsingHeader::new(
                    RichText::new(format!("Instruments ({})", self.plugin_choices(true).len()))
                        .font(t::sans(13.0)),
                )
                .default_open(true)
                .show(ui, |ui| {
                    if insts.is_empty() {
                        let note = if scanning {
                            "Checking plugins…"
                        } else {
                            "No instruments yet."
                        };
                        ui.label(RichText::new(note).font(t::sans(12.0)).color(t::FAINT));
                    }
                    for plugin in &insts {
                        let r = ui.add(
                            egui::Button::new(RichText::new(&plugin.name).font(t::sans(13.0)))
                                .frame(false)
                                .sense(egui::Sense::click_and_drag())
                                .min_size(egui::vec2(ui.available_width(), 22.0)),
                        );
                        if r.drag_started() {
                            egui::DragAndDrop::set_payload(ui.ctx(), PluginDrag(plugin.clone()));
                        }
                        let r = r.on_hover_text(format!(
                            "{}  ·  {}\nClick adds it to the channel rack",
                            plugin.vendor,
                            plugin.path.display()
                        ));
                        if r.clicked() {
                            self.add_plugin_channel(plugin);
                        }
                        let fav_key = plugin.path.display().to_string();
                        let starred = is_favorite(ui.ctx(), &fav_key);
                        r.context_menu(|ui| {
                            super::kit::menu_style(ui);
                            if ui.button("Add to channel rack").clicked() {
                                self.add_plugin_channel(plugin);
                                ui.close_menu();
                            }
                            if ui
                                .button(if starred {
                                    "Remove from favorites"
                                } else {
                                    "Add to favorites"
                                })
                                .clicked()
                            {
                                toggle_favorite(ui.ctx(), fav_key.clone());
                                ui.close_menu();
                            }
                            tag_menu(ui, &fav_key);
                        });
                    }
                });
                let builtins = &*super::mixer::BUILTIN_GROUPS;
                let effect_count =
                    keyframexl::fx::BUILTINS.len() + self.plugin_choices(false).len();
                let searching = !q.trim().is_empty();
                egui::CollapsingHeader::new(
                    RichText::new(format!("Effects ({effect_count})")).font(t::sans(13.0)),
                )
                .id_salt("browser-effects")
                .default_open(true)
                .open(searching.then_some(true))
                .show(ui, |ui| {
                    let track = self.mix_sel.min(self.song.mixer.len() - 1);
                    let next = self.song.mixer[track].fx.len();
                    let room = next < keyframexl::sequence::FX_SLOTS;
                    let mut found = false;
                    for (&category, effects) in builtins {
                        let visible = |name: &str| matches(&q, category) || matches(&q, name);
                        if !effects.iter().any(|e| visible(e.name)) {
                            continue;
                        }
                        found = true;
                        egui::CollapsingHeader::new(category)
                            .open(searching.then_some(true))
                            .show(ui, |ui| {
                                for effect in effects.iter().filter(|e| visible(e.name)) {
                                    let button =
                                        ui.add_enabled(
                                            room,
                                            egui::Button::new(effect.name).frame(false),
                                        )
                                        .on_disabled_hover_text("All ten effect slots are occupied")
                                        .on_hover_text(
                                            format!("Add to {}", self.mixer_name(track)),
                                        );
                                    if button.clicked() {
                                        self.set_slot(track, next, Some(effect.kind));
                                        self.open_fx_window(track, next);
                                    }
                                }
                            });
                    }
                    if !found && fxs.is_empty() {
                        ui.label(
                            RichText::new("No matching effects")
                                .font(t::sans(12.0))
                                .color(t::FAINT),
                        );
                    }
                    if !fxs.is_empty() {
                        ui.label(
                            RichText::new("External plugins")
                                .font(t::sans(12.0))
                                .color(t::MUTED),
                        );
                    }
                    for plugin in &fxs {
                        let r = ui.add(
                            egui::Button::new(RichText::new(&plugin.name).font(t::sans(13.0)))
                                .frame(false)
                                .min_size(egui::vec2(ui.available_width(), 22.0)),
                        );
                        let tip = format!("Click puts it on {}", self.mixer_name(self.mix_sel));
                        let r = r.on_hover_text(format!(
                            "{}  ·  {}\n{tip}",
                            plugin.vendor,
                            plugin.path.display()
                        ));
                        if r.clicked() {
                            let t = self.mix_sel;
                            let k = self.song.mixer.get(t).map(|m| m.fx.len()).unwrap_or(0);
                            self.set_slot_plugin(t, k, plugin);
                        }
                        let fav_key = plugin.path.display().to_string();
                        let starred = is_favorite(ui.ctx(), &fav_key);
                        r.context_menu(|ui| {
                            super::kit::menu_style(ui);
                            if ui
                                .button(format!("Add to {}", self.mixer_name(self.mix_sel)))
                                .clicked()
                            {
                                let t = self.mix_sel;
                                let k = self.song.mixer.get(t).map(|m| m.fx.len()).unwrap_or(0);
                                self.set_slot_plugin(t, k, plugin);
                                ui.close_menu();
                            }
                            if ui
                                .button(if starred {
                                    "Remove from favorites"
                                } else {
                                    "Add to favorites"
                                })
                                .clicked()
                            {
                                toggle_favorite(ui.ctx(), fav_key.clone());
                                ui.close_menu();
                            }
                            tag_menu(ui, &fav_key);
                        });
                    }
                });
                self.synth_presets_section(ui, &q);
                if problems > 0 && !scanning {
                    ui.label(
                        RichText::new(format!(
                            "{problems} plugin(s) with problems — Settings → Plugins → Problems"
                        ))
                        .font(t::sans(12.0))
                        .color(t::WAIT),
                    );
                }
                egui::CollapsingHeader::new(RichText::new("Presets").font(t::sans(13.0))).show(
                    ui,
                    |ui| {
                        for kind in keyframexl::preset::Kind::ALL {
                            egui::CollapsingHeader::new(kind.folder()).show(ui, |ui| {
                                if let Some(base) = keyframexl::preset::dir(kind) {
                                    self.browser_directory(ui, &base, &q);
                                }
                            });
                        }
                    },
                );
                for (title, path) in [
                    ("Packs", "Packs"),
                    ("Libraries (.dwp)", "Libraries"),
                    ("Recorded", "Recorded"),
                    ("Rendered", "Rendered"),
                ] {
                    egui::CollapsingHeader::new(RichText::new(title).font(t::sans(13.0))).show(
                        ui,
                        |ui| {
                            let base = dirs::audio_dir()
                                .or_else(dirs::home_dir)
                                .unwrap_or_default()
                                .join("KeyFrameXL")
                                .join(path);
                            self.browser_directory(ui, &base, &q);
                        },
                    );
                }
                egui::CollapsingHeader::new(RichText::new("Folders").font(t::sans(13.0)))
                    .default_open(true)
                    .show(ui, |ui| self.files_tab(ui, &q));
            });
    }

    fn browser_directory(&mut self, ui: &mut Ui, path: &Path, q: &str) {
        let shown = |p: &Path| {
            files::SHOWN.contains(&files::ext(p).as_str())
                || keyframexl::preset::Kind::of_path(p).is_some()
        };
        // Walk subfolders too: libraries like Bass/Brass/... keep .dwp files one
        // level down, and users expect them listed, not an empty folder.
        let mut files: Vec<PathBuf> = Vec::new();
        let mut stack: Vec<(PathBuf, usize)> = vec![(path.to_path_buf(), 0)];
        let mut seen_dirs = 0usize;
        while let Some((dir, depth)) = stack.pop() {
            if depth > 6 || files.len() >= 5000 || seen_dirs >= 500 {
                continue;
            }
            seen_dirs += 1;
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut subdirs: Vec<PathBuf> = Vec::new();
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if p.is_dir() {
                    subdirs.push(p);
                } else if shown(&p) {
                    files.push(p);
                }
            }
            subdirs.sort();
            // Depth-first in order.
            for s in subdirs.into_iter().rev() {
                stack.push((s, depth + 1));
            }
        }
        files.sort();
        if files.is_empty() {
            ui.label(
                RichText::new(format!("Files: {}", path.display()))
                    .font(t::sans(12.0))
                    .color(t::FAINT),
            );
            return;
        }
        let rel_name = |p: &Path| match p
            .parent()
            .and_then(|par| par.strip_prefix(path).ok())
            .map(|r| r.to_string_lossy().to_string())
            .filter(|r| !r.is_empty())
        {
            Some(rel) => format!(
                "{rel} / {}",
                p.file_name().unwrap_or_default().to_string_lossy()
            ),
            None => p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        };
        // Search: flat list across all subfolders (name or folder matches).
        if !q.is_empty() {
            for p in files.iter().filter(|p| matches(q, &rel_name(p))) {
                self.browser_file_row(ui, p, &rel_name(p));
            }
            return;
        }
        // Top-level files first, then one collapsible header per subfolder.
        let (top, mut rest): (Vec<_>, Vec<_>) =
            files.into_iter().partition(|p| p.parent() == Some(path));
        for p in &top {
            let preset = keyframexl::preset::Kind::of_path(p).is_some();
            let name = if preset {
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };
            self.browser_file_row(ui, p, &name);
        }
        rest.sort_by(|a, b| {
            let ra = a
                .parent()
                .and_then(|p| p.strip_prefix(path).ok())
                .map(|p| p.to_path_buf());
            let rb = b
                .parent()
                .and_then(|p| p.strip_prefix(path).ok())
                .map(|p| p.to_path_buf());
            ra.cmp(&rb).then_with(|| a.cmp(b))
        });
        let mut i = 0;
        while i < rest.len() {
            let dir_rel = rest[i]
                .parent()
                .and_then(|p| p.strip_prefix(path).ok())
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            let label = dir_rel.to_string_lossy().to_string();
            let mut j = i;
            while j < rest.len()
                && rest[j]
                    .parent()
                    .and_then(|p| p.strip_prefix(path).ok())
                    .map(|p| p.to_path_buf())
                    .as_ref()
                    == Some(&dir_rel)
            {
                j += 1;
            }
            egui::CollapsingHeader::new(RichText::new(label).font(t::sans(13.0)))
                .default_open(false)
                .show(ui, |ui| {
                    for p in &rest[i..j] {
                        let preset = keyframexl::preset::Kind::of_path(p).is_some();
                        let name = if preset {
                            p.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        };
                        self.browser_file_row(ui, p, &name);
                    }
                });
            i = j;
        }
    }

    /// One file row inside `browser_directory` (shared by top-level and subfolders).
    fn browser_file_row(&mut self, ui: &mut Ui, path: &Path, name: &str) {
        let path = path.to_path_buf();
        let label = name.to_owned();
        let r = ui.add(
            egui::Label::new(RichText::new(label).font(t::sans(13.0)))
                .sense(egui::Sense::click_and_drag()),
        );
        // Presets (FL: click a channel preset to add it, double-click a mixer state to load it).
        if keyframexl::preset::Kind::of_path(&path).is_some() {
            let r = r.on_hover_text(format!(
                "{}\nClick: load · right-click for more",
                path.display()
            ));
            if r.clicked() {
                self.open_preset_file(&path);
            }
            let fav_key = path.display().to_string();
            let starred = is_favorite(ui.ctx(), &fav_key);
            r.context_menu(|ui| {
                super::kit::menu_style(ui);
                if ui.button("Load").clicked() {
                    self.open_preset_file(&path);
                    ui.close_menu();
                }
                if ui
                    .button(if starred {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    })
                    .clicked()
                {
                    toggle_favorite(ui.ctx(), fav_key.clone());
                    ui.close_menu();
                }
                tag_menu(ui, &fav_key);
                if ui.button("Open containing folder").clicked() {
                    if let Some(parent) = path.parent() {
                        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                    }
                    ui.close_menu();
                }
            });
            return;
        }
        let r = r.on_hover_text(format!("{}\nDrag onto the Channel Rack for a new sampler channel, or onto a row to replace it · double-click loads into the selected channel", path.display()));
        if r.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), FileDrag(path.clone()));
        }
        if r.double_clicked() {
            self.load_file(path.clone(), false);
        } else if r.clicked() && files::is_audio(&path) {
            self.toggle_preview(path.clone());
        }
        r.context_menu(|ui| {
            super::kit::menu_style(ui);
            if ui.button("Add to rack").clicked() {
                self.load_file(path.clone(), true);
                ui.close_menu();
            }
            let fav_key = path.display().to_string();
            if ui
                .button(if is_favorite(ui.ctx(), &fav_key) {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                })
                .clicked()
            {
                toggle_favorite(ui.ctx(), fav_key.clone());
                ui.close_menu();
            }
            tag_menu(ui, &fav_key);
            if ui.button("Open containing folder").clicked() {
                if let Some(parent) = path.parent() {
                    let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                }
                ui.close_menu();
            }
        });
    }

    /// KeyFrame Tide's presets: factory, then saved (FL: a plugin's presets
    /// in the browser). Click loads one into the selected synth channel, or
    /// adds a synth channel when the selected channel is something else.
    fn synth_presets_section(&mut self, ui: &mut Ui, q: &str) {
        use keyframexl::synth::SynthPatch;
        let user = super::presets_ui::cached_synth_presets(ui.ctx());
        let factory: Vec<(usize, &str)> = SynthPatch::PRESETS
            .iter()
            .copied()
            .enumerate()
            // A category name finds its presets too ("bass", "pad").
            .filter(|(i, n)| matches(q, n) || matches(q, SynthPatch::CATEGORIES[*i]))
            .collect();
        let saved: Vec<&(PathBuf, keyframexl::preset::SynthPreset)> =
            user.iter().filter(|(_, u)| matches(q, &u.name)).collect();
        if !q.trim().is_empty() && factory.is_empty() && saved.is_empty() {
            return;
        }
        let target = self.selected_synth();
        let into = target
            .and_then(|id| self.song.instruments.iter().find(|i| i.id == id))
            .map(|i| i.name.clone());
        let tip = match &into {
            Some(name) => format!("Click: load into {name} · right-click: a new channel"),
            None => "Click: a new KeyFrame Tide channel with this patch".to_string(),
        };
        let mut pick: Option<(Option<u64>, String, SynthPatch)> = None;
        egui::CollapsingHeader::new(
            RichText::new(format!(
                "KeyFrame Tide ({})",
                SynthPatch::PRESETS.len() + user.len()
            ))
            .font(t::sans(13.0)),
        )
        .default_open(!q.trim().is_empty())
        .show(ui, |ui| {
            let mut row =
                |ui: &mut Ui, name: &str, patch: &dyn Fn() -> SynthPatch, key: Option<String>| {
                    let r = ui.add(
                        egui::Button::new(RichText::new(name).font(t::sans(13.0)))
                            .frame(false)
                            .min_size(egui::vec2(ui.available_width(), 22.0)),
                    );
                    let r = r.on_hover_text(&tip);
                    if r.clicked() {
                        pick = Some((target, name.to_owned(), patch()));
                    }
                    r.context_menu(|ui| {
                        super::kit::menu_style(ui);
                        if let Some(n) = &into
                            && ui.button(format!("Load into {n}")).clicked()
                        {
                            pick = Some((target, name.to_owned(), patch()));
                            ui.close_menu();
                        }
                        if ui.button("Add as a new channel").clicked() {
                            pick = Some((None, name.to_owned(), patch()));
                            ui.close_menu();
                        }
                        if let Some(key) = &key {
                            if ui
                                .button(if is_favorite(ui.ctx(), key) {
                                    "Remove from favorites"
                                } else {
                                    "Add to favorites"
                                })
                                .clicked()
                            {
                                toggle_favorite(ui.ctx(), key.clone());
                                ui.close_menu();
                            }
                            tag_menu(ui, key);
                        }
                    });
                };
            for cat in SynthPatch::CATEGORY_ORDER {
                let mut first = true;
                for (i, name) in factory
                    .iter()
                    .filter(|(i, _)| SynthPatch::CATEGORIES[*i] == cat)
                {
                    if first && cat != "Init" {
                        ui.label(RichText::new(cat).font(t::sans(11.5)).color(t::FAINT));
                    }
                    first = false;
                    let key = format!("factory:{}", name.to_lowercase().replace(' ', "-"));
                    row(ui, name, &|| SynthPatch::factory(*i), Some(key));
                }
            }
            if !saved.is_empty() {
                ui.label(RichText::new("Saved").font(t::sans(11.5)).color(t::FAINT));
            }
            for (path, u) in &saved {
                row(
                    ui,
                    &u.name,
                    &|| u.patch.clone(),
                    Some(path.display().to_string()),
                );
            }
        });
        if let Some((at, name, patch)) = pick {
            let msg = self.apply_synth_patch(at, &name, patch);
            self.publish();
            self.say(super::Tone::Done, msg);
        }
    }

    /// Starred plugins and files, under the search. Hidden when empty.
    /// A favorite stores its plain path: a scanned plugin bundle or a file.
    fn favorites_section(&mut self, ui: &mut Ui) {
        let favs = favorites(ui.ctx());
        if favs.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(RichText::new("Favorites").font(t::sans(13.0)))
            .default_open(true)
            .show(ui, |ui| {
                let known: Vec<Scanned> = self
                    .plugin_choices(true)
                    .into_iter()
                    .chain(self.plugin_choices(false))
                    .collect();
                let mut act = None;
                for fav in &favs {
                    act = self.asset_row(ui, fav, "★ ", &known).or(act);
                }
                self.asset_act(ui, act);
            });
    }

    /// Tagged plugins and files (FL: the browser's TAGS filter): the tags in
    /// use as chips, Any/All, and what carries the picked ones. Typing in
    /// the search lists what has a matching tag too. Hidden with no tags.
    fn tags_section(&mut self, ui: &mut Ui, q: &str) {
        let map = tags(ui.ctx());
        let counts = tag_counts(&map);
        if counts.is_empty() {
            return;
        }
        let pick_id = egui::Id::new("browser-tag-pick");
        let all_id = egui::Id::new("browser-tag-all");
        let mut picked: Vec<String> = ui.data(|d| d.get_temp(pick_id).unwrap_or_default());
        picked.retain(|p| counts.iter().any(|(t, _)| t == p));
        let mut all: bool = ui.data(|d| d.get_temp(all_id).unwrap_or(false));
        egui::CollapsingHeader::new(
            RichText::new(format!("Tags ({})", counts.len())).font(t::sans(13.0)),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (tag, n) in &counts {
                    let on = picked.contains(tag);
                    let r = ui.add(
                        egui::Button::new(RichText::new(format!("{tag} {n}")).font(t::sans(12.0)))
                            .selected(on)
                            .min_size(egui::vec2(0.0, 22.0)),
                    );
                    if r.clicked() {
                        if on {
                            picked.retain(|p| p != tag);
                        } else {
                            picked.push(tag.clone());
                        }
                    }
                }
            });
            if picked.len() > 1 {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Match").font(t::sans(12.0)).color(t::FAINT));
                    if ui.selectable_label(!all, "Any").clicked() {
                        all = false;
                    }
                    if ui.selectable_label(all, "All").clicked() {
                        all = true;
                    }
                });
            }
            let keys: Vec<String> = if !picked.is_empty() {
                tagged(&map, &picked, all)
            } else if !q.trim().is_empty() {
                map.iter()
                    .filter(|(_, tags)| tags.iter().any(|t| matches(q, t)))
                    .map(|(k, _)| k.clone())
                    .collect()
            } else {
                ui.label(
                    RichText::new("Pick tags to list what has them.")
                        .font(t::sans(12.0))
                        .color(t::FAINT),
                );
                Vec::new()
            };
            if (!picked.is_empty() || !q.trim().is_empty()) && keys.is_empty() {
                ui.label(
                    RichText::new("Nothing has these tags.")
                        .font(t::sans(12.0))
                        .color(t::FAINT),
                );
            }
            let known: Vec<Scanned> = self
                .plugin_choices(true)
                .into_iter()
                .chain(self.plugin_choices(false))
                .collect();
            let mut act = None;
            for key in &keys {
                act = self.asset_row(ui, key, "", &known).or(act);
            }
            self.asset_act(ui, act);
        });
        ui.data_mut(|d| {
            d.insert_temp(pick_id, picked);
            d.insert_temp(all_id, all);
        });
    }

    /// One favorite or tagged asset: a scanned plugin, a preset or a file.
    fn asset_row(
        &mut self,
        ui: &mut Ui,
        key: &str,
        mark: &str,
        known: &[Scanned],
    ) -> Option<AssetAct> {
        let mut act = None;
        let starred = is_favorite(ui.ctx(), key);
        let menu_tail = |ui: &mut Ui, act: &mut Option<AssetAct>| {
            super::kit::menu_style(ui);
            if ui
                .button(if starred {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                })
                .clicked()
            {
                *act = Some(AssetAct::Star(key.to_owned()));
                ui.close_menu();
            }
            tag_menu(ui, key);
        };
        if let Some(p) = known.iter().find(|p| p.path.display().to_string() == key) {
            let r = ui.add(
                egui::Button::new(RichText::new(format!("{mark}{}", p.name)).font(t::sans(13.0)))
                    .frame(false)
                    .min_size(egui::vec2(ui.available_width(), 24.0)),
            );
            let tip = if p.instrument {
                "Click adds it to the channel rack".to_string()
            } else {
                format!("Click puts it on {}", self.mixer_name(self.mix_sel))
            };
            let r = r.on_hover_text(format!("{}  ·  {}\n{tip}", p.vendor, p.path.display()));
            if r.clicked() {
                act = Some(AssetAct::AddPlugin(p.clone()));
            }
            r.context_menu(|ui| menu_tail(ui, &mut act));
            return act;
        }
        let path = PathBuf::from(key);
        if !path.exists() {
            ui.horizontal(|ui| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| key.to_owned());
                let r = ui
                    .label(
                        RichText::new(format!("{mark}{name}"))
                            .font(t::sans(13.0))
                            .color(t::FAINT),
                    )
                    .on_hover_text(format!("{key} is missing"));
                r.context_menu(|ui| menu_tail(ui, &mut act));
            });
            return act;
        }
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| key.to_owned());
        let preset = keyframexl::preset::Kind::of_path(&path).is_some();
        let r = ui.add(
            egui::Label::new(RichText::new(format!("{mark}{name}")).font(t::sans(13.0)))
                .sense(egui::Sense::click_and_drag()),
        );
        if r.clicked() {
            if preset {
                act = Some(AssetAct::Preset(path.clone()));
            } else if files::is_audio(&path) {
                act = Some(AssetAct::Preview(path.clone()));
            }
        }
        if r.double_clicked() && !preset {
            act = Some(AssetAct::Load(path.clone()));
        }
        if r.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), FileDrag(path.clone()));
        }
        r.context_menu(|ui| {
            super::kit::menu_style(ui);
            if !preset && ui.button("Load into the selected channel").clicked() {
                act = Some(AssetAct::Load(path.clone()));
                ui.close_menu();
            }
            menu_tail(ui, &mut act);
        });
        act
    }

    fn asset_act(&mut self, ui: &mut Ui, act: Option<AssetAct>) {
        match act {
            Some(AssetAct::AddPlugin(p)) => {
                if p.instrument {
                    self.add_plugin_channel(&p);
                } else {
                    let t = self.mix_sel;
                    let k = self.song.mixer.get(t).map(|m| m.fx.len()).unwrap_or(0);
                    self.set_slot_plugin(t, k, &p);
                }
            }
            Some(AssetAct::Preset(p)) => self.open_preset_file(&p),
            Some(AssetAct::Preview(p)) => self.toggle_preview(p),
            Some(AssetAct::Load(p)) => self.load_file(p, false),
            Some(AssetAct::Star(key)) => toggle_favorite(ui.ctx(), key),
            None => {}
        }
    }

    /// FL's plugin picker (F8): a centered window with a focused search field
    /// and Instruments / Effects groups. Effects land on the selected mixer
    /// track, exactly like the browser. The open flag, the query and the
    /// focus request live in egui memory, so this needs no App fields.
    /// Called every frame from `update`.
    pub(super) fn plugin_picker(&mut self, ctx: &egui::Context) {
        let open_id = egui::Id::new("kfxl-picker-open");
        if !ctx.data(|d| d.get_temp(open_id).unwrap_or(false)) {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.data_mut(|d| d.insert_temp(open_id, false));
            return;
        }
        let qid = egui::Id::new("kfxl-picker-query");
        let mut query: String = ctx.data(|d| d.get_temp(qid).unwrap_or_default());
        enum Pick {
            Inst(Scanned),
            Fx(Scanned),
        }
        let mut pick: Option<Pick> = None;
        let mut open = true;
        egui::Window::new("Add plugin")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .max_size(egui::vec2(720.0, 480.0))
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(qid)
                        .hint_text("Search plugins")
                        .font(t::sans(13.0))
                        .desired_width(f32::INFINITY),
                );
                let asked: bool = ctx.data(|d| {
                    d.get_temp(egui::Id::new("kfxl-picker-focus"))
                        .unwrap_or(false)
                });
                if !asked {
                    resp.request_focus();
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-picker-focus"), true));
                }
                ctx.data_mut(|d| d.insert_temp(qid, query.clone()));
                let inst: Vec<Scanned> = self
                    .plugin_choices(true)
                    .into_iter()
                    .filter(|p| matches(&query, &p.name) || matches(&query, &p.vendor))
                    .collect();
                let fx: Vec<Scanned> = self
                    .plugin_choices(false)
                    .into_iter()
                    .filter(|p| matches(&query, &p.name) || matches(&query, &p.vendor))
                    .collect();
                if inst.is_empty() && fx.is_empty() {
                    let note = if self.plugins.scan_rx.is_some() {
                        "Checking plugins…"
                    } else {
                        "No plugins found. Options → Plugins adds folders."
                    };
                    ui.label(RichText::new(note).font(t::sans(13.0)).color(t::FAINT));
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .auto_shrink([false, true])
                    .id_salt("plugin-picker-list")
                    .show(ui, |ui| {
                        for (title, items, is_inst) in
                            [("Instruments", &inst, true), ("Effects", &fx, false)]
                        {
                            if items.is_empty() {
                                continue;
                            }
                            ui.label(RichText::new(title).font(t::sans(13.0)).color(t::MUTED));
                            for p in items {
                                let label = if p.vendor.is_empty() {
                                    p.name.clone()
                                } else {
                                    format!("{}  ·  {}", p.name, p.vendor)
                                };
                                let r = ui.add(
                                    egui::Button::new(RichText::new(label).font(t::sans(13.0)))
                                        .frame(false)
                                        .sense(egui::Sense::click_and_drag())
                                        .min_size(egui::vec2(ui.available_width(), 24.0)),
                                );
                                if is_inst && r.drag_started() {
                                    egui::DragAndDrop::set_payload(ctx, PluginDrag(p.clone()));
                                }
                                let tip = if is_inst {
                                    "Click adds it to the channel rack".to_string()
                                } else {
                                    format!("Click puts it on {}", self.mixer_name(self.mix_sel))
                                };
                                if r.on_hover_text(format!("{}\n{tip}", p.path.display()))
                                    .clicked()
                                {
                                    pick = Some(if is_inst {
                                        Pick::Inst(p.clone())
                                    } else {
                                        Pick::Fx(p.clone())
                                    });
                                }
                            }
                        }
                    });
                // Enter adds the first match (the search keeps focus).
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(p) = inst.first() {
                        pick = Some(Pick::Inst(p.clone()));
                    } else if let Some(p) = fx.first() {
                        pick = Some(Pick::Fx(p.clone()));
                    }
                }
            });
        if !open {
            ctx.data_mut(|d| d.insert_temp(open_id, false));
        }
        match pick {
            Some(Pick::Inst(p)) => self.add_plugin_channel(&p),
            Some(Pick::Fx(p)) => {
                let t = self.mix_sel;
                let k = self.song.mixer.get(t).map(|m| m.fx.len()).unwrap_or(0);
                self.set_slot_plugin(t, k, &p);
            }
            None => {}
        }
    }

    /// F8 toggles the picker. The focus request and the query reset each time
    /// it opens, so the search field is ready to type in.
    pub(super) fn toggle_picker(ctx: &egui::Context) {
        let id = egui::Id::new("kfxl-picker-open");
        ctx.data_mut(|d| {
            let open: bool = d.get_temp(id).unwrap_or(false);
            d.insert_temp(id, !open);
        });
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-picker-focus"), false));
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("kfxl-picker-query"), String::new()));
    }
}

#[cfg(test)]
mod browser_tests {
    use super::{load_favorites_from, save_favorites_to};

    #[test]
    fn favorites_round_trip() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("keyframexl").join("browser-favorites");
        let favs = vec![
            "/home/u/.vst3/Piano.vst3".to_string(),
            "/home/u/Music/break.wav".to_string(),
        ];
        save_favorites_to(&path, &favs).expect("save");
        assert_eq!(load_favorites_from(&path), favs);
        // Blank lines are ignored; a missing file loads as empty.
        std::fs::write(&path, "\n  \n/home/u/Music/break.wav\n\n").unwrap();
        assert_eq!(
            load_favorites_from(&path),
            vec!["/home/u/Music/break.wav".to_string()]
        );
        assert!(load_favorites_from(&dir.path().join("missing")).is_empty());
        // Clearing persists too.
        save_favorites_to(&path, &[]).expect("save empty");
        assert!(load_favorites_from(&path).is_empty());
    }
}

#[cfg(test)]
mod tag_tests {
    use super::*;

    #[test]
    fn typed_tags_are_trimmed_and_kept_once() {
        assert_eq!(
            parse_tag_input(" Kick, 808,,kick ,C#\t1"),
            ["Kick", "808", "C# 1"]
        );
        assert_eq!(parse_tag_input(&"x".repeat(40))[0].len(), 32);
    }

    #[test]
    fn the_tag_file_round_trips_paths_with_commas() {
        let mut map = TagMap::new();
        map.insert(
            "/s/Kick, hard.wav".into(),
            vec!["Kick".into(), "808".into()],
        );
        map.insert("/s/pad.wav".into(), vec!["Pad".into()]);
        map.insert("/s/none.wav".into(), vec![]);
        let text = tag_file_text(&map);
        assert!(
            !text.contains("none.wav"),
            "an untagged file is not written"
        );
        let back = parse_tag_file(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back["/s/Kick, hard.wav"], ["Kick", "808"]);
        assert!(parse_tag_file("garbage line\n\t/no/tags").is_empty());
    }

    #[test]
    fn the_filter_matches_any_or_all_picked_tags_ignoring_case() {
        let mut map = TagMap::new();
        map.insert("a".into(), vec!["Kick".into(), "808".into()]);
        map.insert("b".into(), vec!["kick".into()]);
        map.insert("c".into(), vec!["Snare".into()]);
        assert_eq!(
            tag_counts(&map),
            [
                ("808".to_string(), 1),
                ("Kick".to_string(), 2),
                ("Snare".to_string(), 1)
            ]
        );
        let pick = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            tagged(&map, &pick(&["KICK", "snare"]), false),
            ["a", "b", "c"]
        );
        assert_eq!(tagged(&map, &pick(&["kick", "808"]), true), ["a"]);
    }
}
