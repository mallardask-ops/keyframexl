use super::{App, Cmd, Tab, Tone, kit};
use crate::theme as t;
use eframe::egui::{self, Ui};
use keyframexl::{
    riff::{DIRECTIONS, PROGRESSIONS, RATES, Recipe, STAGES},
    scale::{Kind, ROOTS},
    sequence::{BAR, MAX_BARS, Note, STEP, Tick},
};
use std::{collections::HashSet, sync::atomic::Ordering::Relaxed};

pub(super) struct Session {
    recipe: Recipe,
    instrument: u64,
    pattern: u64,
    edits: u64,
    origin: Tick,
    source: Vec<Note>,
    selected_only: bool,
    existing: bool,
    message: String,
    tab: usize,
    partial: bool,
    preview: Option<Transport>,
    cached: Option<(Recipe, bool, usize)>,
    notes: Vec<Note>,
}
struct Transport {
    playing: bool,
    frame: u64,
    record: bool,
}

impl Session {
    fn span(&self) -> Tick {
        self.recipe.length().min(MAX_BARS * BAR - self.origin)
    }
    fn render(&self, through: usize) -> Vec<Note> {
        let span = self.span();
        let existing: Vec<_> = self
            .source
            .iter()
            .filter(|n| n.start < span)
            .map(|n| Note {
                length: n.length.min(span - n.start),
                ..*n
            })
            .collect();
        self.recipe.generate(
            self.instrument,
            self.existing.then_some(existing.as_slice()),
            span,
            through,
        )
    }
    fn update_notes(&mut self) -> bool {
        let through = if self.partial { self.tab } else { 7 };
        let key = (self.recipe.clone(), self.existing, through);
        if self.cached.as_ref() == Some(&key) {
            return false;
        }
        self.notes = self.render(through);
        self.cached = Some(key);
        true
    }
}

fn choice(ui: &mut Ui, label: &str, value: &mut usize, items: &[&str]) {
    egui::ComboBox::from_label(label)
        .selected_text(items[*value])
        .show_ui(ui, |ui| {
            kit::menu_style(ui);
            for (i, name) in items.iter().enumerate() {
                ui.selectable_value(value, i, *name);
            }
        });
}
fn slider(ui: &mut Ui, label: &str, v: &mut u8, range: std::ops::RangeInclusive<u8>) {
    ui.add(egui::Slider::new(v, range).text(label));
}
fn controls(ui: &mut Ui, r: &mut Recipe, tab: usize) {
    match tab {
        0 => {
            choice(ui, "Progression", &mut r.progression, &PROGRESSIONS);
            slider(ui, "Changes per bar", &mut r.changes, 1..=4);
        }
        1 => {
            slider(ui, "Chord voices", &mut r.voices, 2..=5);
            r.inversion = r.inversion.min(r.voices - 1);
            slider(ui, "Inversion", &mut r.inversion, 0..=r.voices - 1);
        }
        2 => {
            choice(ui, "Rhythm", &mut r.rhythm, &keyframexl::riff::RHYTHMS);
            choice(ui, "Direction", &mut r.direction, &DIRECTIONS);
            choice(ui, "Division", &mut r.rate, &RATES.map(|r| r.0));
            slider(ui, "Octaves", &mut r.octaves, 1..=3);
            ui.checkbox(&mut r.continuous, "Continue across chords");
        }
        3 => {
            ui.checkbox(&mut r.reverse, "Reverse time");
            ui.checkbox(&mut r.invert, "Invert pitches");
        }
        4 => {
            for (name, val, max) in [
                ("Velocity", &mut r.velocity, 64),
                ("Pan spread", &mut r.pan, 100),
                ("Release", &mut r.release, 100),
                ("Fine pitch · cents", &mut r.fine, 100),
                ("Mod X", &mut r.mod_x, 100),
                ("Mod Y", &mut r.mod_y, 100),
            ] {
                slider(ui, name, val, 0..=max);
            }
        }
        5 => {
            slider(ui, "Length · %", &mut r.gate, 10..=180);
            slider(ui, "Variation · %", &mut r.variation, 0..=60);
        }
        6 => {
            choice(ui, "Grid", &mut r.grid, &RATES.map(|it| it.0));
            choice(
                ui,
                "Timing",
                &mut r.groove_end,
                &["Keep length", "Keep end", "Snap end"],
            );
            slider(ui, "Grid strength · %", &mut r.snap, 0..=100);
            slider(ui, "Swing · %", &mut r.swing, 0..=75);
            slider(ui, "Timing variation · ticks", &mut r.jitter, 0..=50);
        }
        _ => {
            let mut root = r.scale.root as usize;
            choice(ui, "Key", &mut root, &ROOTS);
            r.scale.root = root as u8;
            egui::ComboBox::from_label("Scale")
                .selected_text(r.scale.kind.label())
                .show_ui(ui, |ui| {
                    kit::menu_style(ui);
                    for k in Kind::ALL {
                        ui.selectable_value(&mut r.scale.kind, k, k.label());
                    }
                });
            slider(ui, "Lowest MIDI note", &mut r.bottom, 0..=r.top - 11);
            slider(ui, "Highest MIDI note", &mut r.top, r.bottom + 11..=127);
        }
    }
}

fn recipes(ui: &mut Ui, s: &mut Session) {
    ui.menu_button("Recipes", |ui| {
        kit::menu_style(ui);
        if ui.button("Reset all stages").clicked() {
            s.recipe = Recipe::default();
            ui.close_menu();
        }
        if ui.button("Save recipe…").clicked() {
            ui.close_menu();
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("riff.json")
                .add_filter("Riff recipe", &["json"])
                .save_file()
            {
                s.message =
                    match std::fs::write(path, serde_json::to_vec_pretty(&s.recipe).unwrap()) {
                        Ok(()) => "Recipe saved".into(),
                        Err(e) => format!("Could not save recipe: {e}"),
                    };
            }
        }
        if ui.button("Load recipe…").clicked() {
            ui.close_menu();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Riff recipe", &["json"])
                .pick_file()
            {
                let read = std::fs::File::open(path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        serde_json::from_reader::<_, Recipe>(f).map_err(|e| e.to_string())
                    });
                s.message = match read {
                    Ok(r) if r.valid() => {
                        s.recipe = r;
                        "Recipe loaded".into()
                    }
                    Ok(_) => "Recipe parameters are out of range".into(),
                    Err(e) => format!("Could not load recipe: {e}"),
                };
            }
        }
    });
}

const MINT: egui::Color32 = egui::Color32::from_rgb(146, 225, 176);
const PAPER: egui::Color32 = egui::Color32::from_rgb(22, 31, 28);

fn score(ui: &mut Ui, notes: &[Note], span: Tick, height: f32, cursor: Option<Tick>) {
    use egui::{Align2, Color32, Rect, Stroke, pos2, vec2};
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    let p = ui.painter_at(r);
    p.rect_filled(r, 6., PAPER);
    let grid = Rect::from_min_max(r.min + vec2(38., 24.), r.max - vec2(12., 28.));
    let x = |tick: Tick| grid.left() + grid.width() * tick as f32 / span.max(1) as f32;
    let lo = notes.iter().map(|n| n.pitch as i32).min().unwrap_or(48);
    let hi = notes.iter().map(|n| n.pitch as i32).max().unwrap_or(60);
    let low = (lo - 2).max(0);
    let high = (hi + 2).max(low + 12).min(127);
    let row = grid.height() / (high - low + 1) as f32;
    let y = |pitch: i32| grid.bottom() - (pitch - low) as f32 * row;
    for key in low..=high {
        if [1, 3, 6, 8, 10].contains(&(key % 12)) {
            p.rect_filled(
                Rect::from_min_max(pos2(grid.left(), y(key) - row), pos2(grid.right(), y(key))),
                0.,
                Color32::from_rgb(18, 26, 23),
            );
        }
        if key % 12 == 0 {
            p.hline(
                grid.x_range(),
                y(key),
                Stroke::new(1.0_f32, Color32::from_gray(64)),
            );
            p.text(
                pos2(grid.left() - 6., y(key) - row * 0.5),
                Align2::RIGHT_CENTER,
                super::roll::note_name(key as u8),
                t::mono(10.),
                t::muted(),
            );
        }
    }
    for time in (0..span).step_by(keyframexl::sequence::PPQ as usize) {
        let bar = time % BAR == 0;
        p.vline(
            x(time),
            grid.top()..=r.bottom() - 5.,
            Stroke::new(
                1.0_f32,
                if bar {
                    Color32::from_gray(71)
                } else {
                    Color32::from_gray(42)
                },
            ),
        );
        if bar {
            p.text(
                pos2(x(time) + 3., r.top() + 6.),
                Align2::LEFT_TOP,
                format!("{:02}", time / BAR + 1),
                t::mono(11.),
                MINT,
            );
        }
    }
    for n in notes {
        let bottom = y(n.pitch.into());
        let left = x(n.start);
        let right = (x(n.end()) - 1.).max(left + 1.).min(grid.right());
        let color = MINT.gamma_multiply(0.52 + f32::from(n.velocity) / 260.);
        p.rect_filled(
            Rect::from_min_max(pos2(left, bottom - row + 0.5), pos2(right, bottom - 0.3)),
            1.,
            color,
        );
        p.rect_filled(
            Rect::from_min_max(
                pos2(left, r.bottom() - 5. - 15. * f32::from(n.velocity) / 127.),
                pos2((left + 2.).min(grid.right()), r.bottom() - 5.),
            ),
            0.,
            color,
        );
    }
    p.text(
        pos2(grid.left() - 6., r.bottom() - 12.),
        Align2::RIGHT_CENTER,
        "VEL",
        t::mono(9.),
        t::muted(),
    );
    if let Some(tick) = cursor {
        p.vline(
            x(tick % span),
            grid.top()..=r.bottom() - 4.,
            Stroke::new(1.5_f32, t::enc_orange()),
        );
    }
}

fn stage_list(ui: &mut Ui, s: &mut Session) {
    for (i, label) in STAGES.iter().enumerate() {
        let selected = s.tab == i;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.), egui::Sense::click());
        let p = ui.painter();
        if selected || response.hovered() || response.has_focus() {
            p.rect_filled(rect, 4., if selected { PAPER } else { t::selection() });
        }
        if selected {
            p.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(3., rect.height())),
                1.,
                MINT,
            );
        }
        p.text(
            rect.left_center() + egui::vec2(12., 0.),
            egui::Align2::LEFT_CENTER,
            label,
            t::medium(14.),
            if selected { MINT } else { t::text() },
        );
        if s.recipe.enabled[i] {
            p.circle_filled(rect.right_center() - egui::vec2(12., 0.), 3., MINT);
        } else {
            p.circle_stroke(
                rect.right_center() - egui::vec2(12., 0.),
                3.,
                egui::Stroke::new(1.0_f32, t::muted()),
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, *label)
        });
        if response.clicked() {
            s.tab = i;
        }
    }
}
impl App {
    pub(super) fn open_riff(&mut self, inst: u64) {
        if self.riff.is_some() {
            self.win(Cmd::Show(Tab::Riff));
            return;
        }
        if self.pattern().automation.is_some() || self.song.instrument(inst).is_none() {
            self.say(
                Tone::Wait,
                "Select a note pattern and channel for Riff Machine.",
            );
            return;
        }
        let ids: HashSet<_> = self.roll_sel.iter().copied().collect();
        let selected = self
            .pattern()
            .notes
            .iter()
            .any(|n| n.instrument == inst && ids.contains(&n.id));
        let mut source: Vec<_> = self
            .pattern()
            .notes
            .iter()
            .filter(|n| n.instrument == inst && (!selected || ids.contains(&n.id)))
            .copied()
            .collect();
        let origin = if selected {
            source.iter().map(|n| n.start).min().unwrap_or(0)
        } else {
            0
        };
        for n in &mut source {
            n.start -= origin;
        }
        let mut recipe = Recipe::default();
        if let Some(scale) = self.roll_scale {
            recipe.scale = scale;
        }
        if selected {
            recipe.bars = ((source.iter().map(Note::end).max().unwrap_or(BAR) + BAR - 1) / BAR)
                .clamp(1, 16) as u8;
        }
        self.riff = Some(Session {
            recipe,
            instrument: inst,
            pattern: self.selected,
            edits: self.edits,
            origin,
            source,
            existing: selected,
            selected_only: selected,
            message: String::new(),
            tab: 0,
            partial: false,
            preview: None,
            cached: None,
            notes: Vec::new(),
        });
        self.win(Cmd::Show(Tab::Riff));
    }
    fn audition_riff(&mut self, s: &mut Session) {
        let Some(a) = &self.audio else { return };
        if s.preview.is_none() {
            s.preview = Some(Transport {
                playing: a.playing.load(Relaxed),
                frame: a.position.load(Relaxed),
                record: self.rec.on,
            });
            self.rec.on = false;
        }
        let mut song = self.song.clone();
        let first_id = song.next_note_id();
        let pat = song.pattern_mut(s.pattern).unwrap();
        pat.notes = s.notes.clone();
        pat.length = ((s.span() + STEP - 1) / STEP) * STEP;
        pat.loop_range = Some((0, s.span()));
        pat.slides.retain(|g| {
            g.instrument == s.instrument && g.start >= s.origin && g.start < s.origin + s.span()
        });
        for g in &mut pat.slides {
            g.start -= s.origin;
            g.length = g.length.min(s.span() - g.start);
        }
        let lyrics = std::mem::take(&mut pat.lyrics);
        for (i, n) in pat.notes.iter_mut().enumerate() {
            let id = first_id + i as u64;
            if let Some(word) = lyrics.get(&n.id) {
                pat.lyrics.insert(id, word.clone());
            }
            n.id = id;
        }
        a.send_song(song);
        a.pattern.store(s.pattern, Relaxed);
        a.song_mode.store(false, Relaxed);
        a.position.store(0, Relaxed);
        a.playing.store(true, Relaxed);
    }
    pub(super) fn end_riff_preview(&mut self, s: &mut Session) {
        if let Some(saved) = s.preview.take() {
            self.rec.on = saved.record;
            if let Some(a) = &self.audio {
                a.playing.store(false, Relaxed);
                self.publish();
                a.song_mode.store(self.song_mode, Relaxed);
                a.position.store(saved.frame, Relaxed);
                a.playing.store(saved.playing, Relaxed);
            }
        }
    }
    fn apply_riff(&mut self, s: &Session) {
        let notes = s.render(7);
        if notes.is_empty() {
            return;
        }
        let span = s.span();
        let removed: HashSet<_> = s
            .source
            .iter()
            .filter(|n| n.start < span)
            .map(|n| n.id)
            .collect();
        let mut next = self.song.next_note_id();
        self.checkpoint();
        self.selected = s.pattern;
        let pat = self.pattern_mut();
        let lyrics = pat.lyrics.clone();
        // Keep the tail of a source note that crosses the edited phrase boundary.
        let end = s.origin + span;
        pat.notes.retain_mut(|n| {
            if !removed.contains(&n.id) {
                return true;
            }
            if n.end() <= end {
                return false;
            }
            n.length = n.end() - end;
            n.start = end;
            true
        });
        pat.lyrics
            .retain(|id, _| !removed.contains(id) || pat.notes.iter().any(|n| n.id == *id));
        let mut ids = Vec::with_capacity(notes.len());
        for mut n in notes {
            if let Some(word) = lyrics.get(&n.id) {
                pat.lyrics.insert(next, word.clone());
            }
            n.id = next;
            next += 1;
            ids.push(n.id);
            n.start += s.origin;
            pat.notes.push(n);
        }
        pat.length = pat.length.max(((s.origin + span + STEP - 1) / STEP) * STEP);
        self.open_roll(s.instrument);
        self.roll_inst = s.instrument;
        self.roll_sel = ids;
        self.publish();
        self.say(
            Tone::Done,
            format!(
                "Riff Machine added {} notes. Ctrl+Z undoes.",
                self.roll_sel.len()
            ),
        );
    }
    pub(super) fn close_riff(&mut self) {
        if let Some(mut session) = self.riff.take() {
            self.end_riff_preview(&mut session);
        }
    }

    pub(super) fn riff_main_input(&mut self, ctx: &egui::Context) {
        if self.riff.as_ref().is_some_and(|s| s.preview.is_some())
            && ctx.input(|i| {
                i.focused
                    && i.events.iter().any(|e| {
                        matches!(
                            e,
                            egui::Event::PointerButton { pressed: true, .. }
                                | egui::Event::Key { pressed: true, .. }
                                | egui::Event::WindowFocused(true)
                        )
                    })
            })
        {
            let mut s = self.riff.take().unwrap();
            self.end_riff_preview(&mut s);
            self.riff = Some(s);
        }
    }
    pub(super) fn riff_tab(&mut self, ui: &mut Ui) {
        let Some(mut s) = self.riff.take() else {
            return;
        };
        let Some(pattern) = self.song.pattern(s.pattern) else {
            self.end_riff_preview(&mut s);
            self.win(Cmd::Close(Tab::Riff));
            return;
        };
        if self.song.instrument(s.instrument).is_none() {
            self.end_riff_preview(&mut s);
            self.win(Cmd::Close(Tab::Riff));
            return;
        }
        if s.edits != self.edits {
            let ids: HashSet<_> = s.source.iter().map(|n| n.id).collect();
            let mut current: Vec<_> = pattern
                .notes
                .iter()
                .filter(|n| {
                    n.instrument == s.instrument && (!s.selected_only || ids.contains(&n.id))
                })
                .copied()
                .collect();
            let origin = if s.selected_only {
                current.iter().map(|n| n.start).min().unwrap_or(s.origin)
            } else {
                0
            };
            for n in &mut current {
                n.start -= origin;
            }
            if pattern.automation.is_some() {
                self.end_riff_preview(&mut s);
                self.win(Cmd::Close(Tab::Riff));
                return;
            }
            if current != s.source || origin != s.origin {
                s.source = current;
                s.origin = origin;
                s.cached = None;
                s.message = "Source notes updated from Piano Roll.".into();
            }
            s.edits = self.edits;
        }
        let pattern_name = pattern.name.clone();
        let inst_name = self.instrument_name(s.instrument);
        let (mut apply, mut play, mut changed) = (false, false, false);
        let mut cancel = !ui.memory(|m| m.any_popup_open())
            && !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Escape));
        egui::TopBottomPanel::top("riff-header")
            .frame(egui::Frame::new().fill(t::side()).inner_margin(14))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&inst_name)
                            .font(t::medium(22.))
                            .color(t::text()),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(&pattern_name)
                            .font(t::sans(14.))
                            .color(t::muted()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::button(ui, "Throw dice", kit::Kind::Normal, true).clicked() {
                            s.recipe.roll_dice();
                        }
                        recipes(ui, &mut s);
                    });
                });
            });
        egui::TopBottomPanel::bottom("riff-transport")
            .frame(egui::Frame::new().fill(t::side()).inner_margin(12))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let available = self.audio.is_some()
                        && !(self.rec.on && self.is_playing())
                        && !self.waiting_input;
                    play = kit::button(
                        ui,
                        if s.preview.is_some() {
                            "Stop preview"
                        } else {
                            "Play preview"
                        },
                        kit::Kind::Normal,
                        available,
                    )
                    .on_disabled_hover_text("Preview needs audio output and an idle recorder")
                    .clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        apply = kit::button(
                            ui,
                            "Apply full riff",
                            kit::Kind::Primary,
                            !s.notes.is_empty(),
                        )
                        .clicked();
                        cancel |= kit::button(ui, "Cancel", kit::Kind::Ghost, true).clicked();
                    });
                });
            });
        egui::SidePanel::left("riff-stages")
            .resizable(false)
            .exact_width(174.)
            .frame(egui::Frame::new().fill(t::panel_bottom()).inner_margin(10))
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(egui::RichText::new("Stages").font(t::medium(16.)));
                    ui.add_space(5.);
                    stage_list(ui, &mut s);
                    ui.add_space(10.);
                    ui.separator();
                    ui.add_space(6.);
                    ui.horizontal(|ui| {
                        ui.label("Length");
                        ui.add(
                            egui::DragValue::new(&mut s.recipe.bars)
                                .range(1..=16)
                                .suffix(" bars"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Seed");
                        ui.add(egui::DragValue::new(&mut s.recipe.seed));
                    });
                    ui.add_enabled(
                        !s.source.is_empty(),
                        egui::Checkbox::new(&mut s.existing, "Use existing score"),
                    );
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t::bg()).inner_margin(16))
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // The bar takes a column of the viewport; size the body
                        // to what is left, or full-width rows push the content
                        // wider than the view and the bar covers their edge.
                        ui.set_max_width((ui.available_width() - 14.0).max(320.0));
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Phrase").font(t::medium(20.)));
                            ui.label(
                                egui::RichText::new(s.recipe.scale.name())
                                    .font(t::mono(13.))
                                    .color(MINT),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut s.partial, "Through this stage");
                                },
                            );
                        });
                        ui.add_space(6.);
                        changed = s.update_notes();
                        let cursor = if s.preview.is_some() {
                            self.audio.as_ref().map(|a| {
                                (a.position.load(Relaxed) as f64
                                    * self.song.tempo
                                    * keyframexl::sequence::PPQ as f64
                                    / (60. * a.running.rate as f64))
                                    as Tick
                            })
                        } else {
                            None
                        };
                        score(
                            ui,
                            &s.notes,
                            s.span(),
                            (ui.ctx().screen_rect().height() * 0.32).clamp(135., 360.),
                            cursor,
                        );
                        ui.add_space(4.);
                        let replaced = s.source.iter().filter(|n| n.start < s.span()).count();
                        ui.label(
                            egui::RichText::new(format!(
                                "{} preview notes   ·   {} source notes to replace",
                                s.notes.len(),
                                replaced
                            ))
                            .font(t::sans(12.))
                            .color(t::muted()),
                        );
                        ui.add_space(12.);
                        egui::Frame::new()
                            .fill(t::surface())
                            .inner_margin(12)
                            .corner_radius(6)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(STAGES[s.tab]).font(t::medium(18.)),
                                    );
                                    ui.checkbox(&mut s.recipe.enabled[s.tab], "Enabled");
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Reset").clicked() {
                                                s.recipe.reset_stage(s.tab);
                                            }
                                            if ui.small_button("Vary").clicked() {
                                                s.recipe.vary_stage(s.tab);
                                            }
                                        },
                                    );
                                });
                                ui.separator();
                                ui.add_enabled_ui(s.recipe.enabled[s.tab] || s.tab == 7, |ui| {
                                    controls(ui, &mut s.recipe, s.tab);
                                });
                            });
                        if !s.message.is_empty() {
                            ui.label(&s.message);
                        }
                    });
            });
        if s.preview.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
        if apply || cancel {
            if !s.message.is_empty() {
                self.say(Tone::Wait, &s.message);
            }
            self.end_riff_preview(&mut s);
            self.win(Cmd::Close(Tab::Riff));
            if apply {
                self.apply_riff(&s);
            }
        } else {
            if play && s.preview.is_some() {
                self.end_riff_preview(&mut s);
            } else if play || changed && s.preview.is_some() {
                self.audition_riff(&mut s);
            }
            self.riff = Some(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyframexl::sequence::Song;

    #[test]
    fn preview_then_apply_preserves_other_channels_lyrics_and_undo() {
        let mut song = Song::demo();
        let inst = 4;
        let id = song.patterns[0]
            .notes
            .iter()
            .find(|n| n.instrument == inst)
            .unwrap()
            .id;
        std::sync::Arc::make_mut(&mut song.patterns[0])
            .lyrics
            .insert(id, "la".into());
        let original = serde_json::to_value(&song).unwrap();
        let other: Vec<_> = song.patterns[0]
            .notes
            .iter()
            .filter(|n| n.instrument != inst)
            .copied()
            .collect();
        let mut app = App::with_song(song, None);
        app.roll_sel = vec![id];
        app.open_riff(inst);
        let mut session = app.riff.take().unwrap();
        assert!(session.existing);
        session.tab = 0;
        session.partial = true;
        session.update_notes();
        assert_eq!(session.notes.len(), 1);
        assert_eq!(serde_json::to_value(&app.song).unwrap(), original);
        app.apply_riff(&session);
        assert!(app.roll_sel.len() > 1);
        assert_eq!(
            app.pattern()
                .notes
                .iter()
                .filter(|n| n.instrument != inst)
                .copied()
                .collect::<Vec<_>>(),
            other
        );
        for id in &app.roll_sel {
            assert_eq!(app.pattern().lyrics.get(id).unwrap(), "la");
        }
        app.song.validate().unwrap();
        assert!(app.undo());
        assert_eq!(serde_json::to_value(&app.song).unwrap(), original);
    }
}
