use crate::{audio::Audio, theme as t};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use keyframexl::sequence::{Clip, Glide, Note, RATE, Song, render};
use std::{
    sync::{Arc, atomic::Ordering, mpsc},
    time::Duration,
};

pub struct App {
    song: Song,
    history: Vec<Song>,
    future: Vec<Song>,
    moving: Option<Moving>,
    snap: Snap,
    instrument: u64,
    taps: Vec<std::time::Instant>,
    audio: Option<Audio>,
    selected: u64,
    clip: Option<usize>,
    screen: Screen,
    slide: bool,
    status: String,
    drag: Option<(usize, u8)>,
    song_mode: bool,
    velocity: u8,
    capture: Option<(std::path::PathBuf, u32)>,
    importing: Option<mpsc::Receiver<Result<keyframexl::sequence::SamplePreset, String>>>,
    #[cfg(test)]
    probe: Option<Rect>,
    #[cfg(test)]
    probe2: Option<Rect>,
    #[cfg(test)]
    probe3: Option<Rect>,
}
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Rack,
    Roll,
    Playlist,
}
#[derive(Clone, Copy)]
enum MoveMode {
    Move,
    ResizeRight,
}
#[derive(Clone, Copy, PartialEq, Default)]
enum Snap {
    Bar,
    Beat,
    #[default]
    Step,
}
impl Snap {
    fn grid(self) -> u8 {
        match self {
            Snap::Bar => 16,
            Snap::Beat => 4,
            Snap::Step => 1,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Snap::Bar => "1 bar",
            Snap::Beat => "1/4",
            Snap::Step => "1/16",
        }
    }
}
/// Move/resize target shared by the ghost preview and the drag_stopped commit,
/// so both always agree (start, length, pitch).
fn move_target(
    mode: MoveMode,
    note_start: u8,
    note_len: u8,
    note_pitch: u8,
    grab: usize,
    end_step: usize,
    end_pitch: u8,
    grid: u8,
    alt: bool,
) -> (u8, u8, u8) {
    match mode {
        MoveMode::Move => {
            let raw = (end_step.saturating_sub(grab) as u8).min(16u8.saturating_sub(note_len));
            (snap_step(raw, grid, alt), note_len, end_pitch)
        }
        // Resize never transposes: pitch stays, only length follows the snapped edge.
        MoveMode::ResizeRight => {
            let es = snap_step(end_step as u8, grid, alt);
            (
                note_start,
                (es.saturating_sub(note_start) + 1).clamp(1, 16 - note_start),
                note_pitch,
            )
        }
    }
}
/// Snap a step index to the grid unless Alt bypasses it.
fn snap_step(v: u8, grid: u8, alt: bool) -> u8 {
    if alt || grid <= 1 {
        return v.min(15);
    }
    ((v as u16 + grid as u16 / 2) / grid as u16 * grid as u16).min(15) as u8
}
#[derive(Clone, Copy)]
struct Moving {
    idx: usize,
    mode: MoveMode,
    grab: usize,
    pitch: u8,
}
fn note_name(pitch: u8) -> String {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", names[(pitch % 12) as usize], pitch as i16 / 12 - 1)
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        t::setup(&cc.egui_ctx);
        let args: Vec<_> = std::env::args().collect();
        let capture_kind = args.get(1).map(String::as_str);
        let capture = matches!(capture_kind, Some("--capture-rack" | "--capture-roll" | "--capture-playlist"));
        if capture {
            cc.egui_ctx.set_pixels_per_point(1.0);
        }
        let song = if capture || args.iter().any(|a| a == "--demo") {
            Song::demo()
        } else {
            Song::default()
        };
        let (audio, status) = match Audio::new(song.clone()) {
            Ok(a) => (Some(a), "Audio ready · 48 kHz".into()),
            Err(e) => (None, format!("Audio unavailable: {e}")),
        };
        if capture && let Some(a) = &audio {
            a.playing.store(true, Ordering::Relaxed);
        }
        Self {
            song,
            history: Vec::new(),
            future: Vec::new(),
            moving: None,
            snap: Snap::default(),
            instrument: 4,
            taps: Vec::new(),
            audio,
            selected: 1,
            clip: None,
            screen: if capture_kind == Some("--capture-roll") {
                Screen::Roll
            } else if capture_kind == Some("--capture-playlist") {
                Screen::Playlist
            } else {
                Screen::Rack
            },
            slide: capture_kind == Some("--capture-roll"),
            status,
            drag: None,
            song_mode: false,
            velocity: 100,
            capture: if capture {
                Some((
                    args.get(2)
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| "capture.png".into()),
                    0,
                ))
            } else {
                None
            },
            importing: None,
            #[cfg(test)]
            probe: None,
            #[cfg(test)]
            probe2: None,
            #[cfg(test)]
            probe3: None,
        }
    }
    fn checkpoint(&mut self) {
        self.history.push(self.song.clone());
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        self.future.clear();
    }
    fn undo(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.future.push(self.song.clone());
            self.song = prev;
            self.publish();
            self.status = "Undone — Ctrl+Shift+Z to redo".into();
            true
        } else {
            self.status = "Nothing to undo".into();
            false
        }
    }
    fn redo(&mut self) -> bool {
        if let Some(next) = self.future.pop() {
            self.history.push(self.song.clone());
            self.song = next;
            self.publish();
            self.status = "Redone".into();
            true
        } else {
            self.status = "Nothing to redo".into();
            false
        }
    }
    fn publish(&self) {
        if let Some(a) = &self.audio {
            a.song.store(Arc::new(self.song.clone()));
            a.pattern.store(self.selected, Ordering::Relaxed);
        }
    }
    fn pattern_mut(&mut self) -> &mut keyframexl::sequence::Pattern {
        self.song
            .patterns
            .iter_mut()
            .find(|p| p.id == self.selected)
            .expect("selected pattern exists")
    }
    fn save(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("KeyFrameXL", &["kfxl"])
            .set_file_name("project.kfxl")
            .save_file()
        {
            let result = keyframexl::project_io::save(&self.song, &path);
            self.status = match result {
                Ok(()) => format!("Saved {}", path.display()),
                Err(e) => format!("Save failed: {e}"),
            };
        }
    }
    fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("KeyFrameXL", &["kfxl"])
            .pick_file()
        {
            let res = keyframexl::project_io::open(&path);
            match res {
                Ok(s) => {
                    self.selected = s.patterns[0].id;
                    self.song = s;
                    self.clip = None;
                    self.publish();
                    self.status = format!("Opened {}", path.display());
                }
                Err(e) => self.status = format!("Open failed: {e}"),
            }
        }
    }
    fn export(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV", &["wav"])
            .set_file_name("song.wav")
            .save_file()
        {
            let mode = !self.song.clips.is_empty();
            let data = render(&self.song, self.selected, mode);
            self.status = match keyframexl::write_wav(&path, &data) {
                Ok(()) => format!("Exported {}", path.display()),
                Err(e) => format!("Export failed: {e}"),
            }
        }
    }
    fn import_dwp(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("DirectWave", &["dwp"])
            .pick_file()
        {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::import::dwp(&path));
            });
            self.importing = Some(rx);
            self.status = "Importing DWP...".into();
        }
    }
    fn playing_step(&self) -> Option<usize> {
        let audio = self.audio.as_ref()?;
        if !audio.playing.load(Ordering::Relaxed) {
            return None;
        }
        Some(
            ((audio.position.load(Ordering::Relaxed) as f64
                / RATE as f64
                / keyframexl::sequence::seconds_per_step(self.song.tempo)) as usize)
                % 16,
        )
    }
    fn sampler_row(&mut self, ui: &mut egui::Ui, name_width: f32, grid_width: f32, inst_id: u64) {
        let inst_name = self
            .song
            .instruments
            .iter()
            .find(|i| i.id == inst_id)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "Sampler".into());
        let notes: Vec<keyframexl::sequence::Note> = self
            .pattern_mut()
            .notes
            .iter()
            .filter(|n| n.instrument == inst_id)
            .cloned()
            .collect();
        ui.horizontal(|ui| {
            let (label, response) =
                ui.allocate_exact_size(Vec2::new(name_width, 78.0), Sense::click());
            let p = ui.painter_at(label);
            p.text(
                label.left_center() + Vec2::new(14.0, -10.0),
                egui::Align2::LEFT_CENTER,
                inst_name.to_uppercase(),
                egui::FontId::proportional(15.0),
                t::TEXT,
            );
            p.text(
                label.left_center() + Vec2::new(14.0, 17.0),
                egui::Align2::LEFT_CENTER,
                "INSTRUMENT - MASTER",
                egui::FontId::monospace(11.0),
                t::MUTED,
            );
            if response.has_focus() {
                p.rect_stroke(
                    label.shrink(2.0),
                    3.0,
                    Stroke::new(2.0_f32, t::BRAND),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                self.instrument = inst_id;
                self.screen = Screen::Roll;
            }
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(grid_width, 78.0), Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 5.0, t::SURFACE);
            p.rect_stroke(
                rect,
                5.0,
                Stroke::new(1.0_f32, t::STRONG),
                egui::StrokeKind::Inside,
            );
            let preview = Rect::from_min_max(
                rect.min + Vec2::new(12.0, 12.0),
                Pos2::new(rect.right() - 190.0, rect.bottom() - 12.0),
            );
            for n in &notes {
                let x = preview.left() + n.start as f32 / 16.0 * preview.width();
                let y = preview.center().y - (n.pitch as f32 - 54.0) * 1.5;
                p.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, y),
                        Vec2::new(n.length as f32 / 16.0 * preview.width(), 5.0),
                    ),
                    2.0,
                    t::BRAND,
                );
            }
            if notes.is_empty() {
                p.text(
                    preview.center(),
                    egui::Align2::CENTER_CENTER,
                    "- EMPTY -",
                    egui::FontId::monospace(11.0),
                    t::MUTED,
                );
            }
            let button = Rect::from_min_size(
                Pos2::new(rect.right() - 175.0, rect.top() + 22.0),
                Vec2::new(160.0, 34.0),
            );
            p.rect_filled(
                button,
                4.0,
                if response.hovered() {
                    t::CTL
                } else {
                    t::RAISED
                },
            );
            p.rect_stroke(
                button,
                4.0,
                Stroke::new(1.0_f32, t::STRONG),
                egui::StrokeKind::Inside,
            );
            p.text(
                button.center(),
                egui::Align2::CENTER_CENTER,
                "Open Piano Roll",
                egui::FontId::proportional(15.0),
                t::TEXT,
            );
            if response.has_focus() {
                p.rect_stroke(
                    rect.shrink(2.0),
                    5.0,
                    Stroke::new(2.0_f32, t::BRAND),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                self.instrument = inst_id;
                self.screen = Screen::Roll;
            }
        });
    }

    fn rack(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("RACK").font(egui::FontId::new(20.0, egui::FontFamily::Name("plex-semibold".into()))));
            ui.add_space(14.0);
            let tempo_hdr = self.song.tempo;
            let pname = self.pattern_mut().name.to_uppercase();
            ui.label(
                egui::RichText::new(format!("{pname}  /  1 BAR  -  {tempo_hdr:.2} BPM"))
                .monospace()
                .size(12.0)
                .color(t::MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.importing.is_none(),
                        egui::Button::new("Import DWP...").min_size(Vec2::new(0.0, 34.0)),
                    )
                    .on_hover_text("Import a DirectWave .dwp multi-zone preset")
                    .clicked()
                {
                    self.import_dwp();
                }
            });
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("CLICK step (right-click erases, SHIFT accents)  \u{00b7}  M mute / S solo  \u{00b7}  SPACE play  \u{00b7}  CTRL+Z undo")
                .monospace()
                .size(11.0)
                .color(t::MUTED),
        );
        ui.add_space(8.0);
        let name_width = 174.0;
        let group_gap = 14.0;
        let gap = 5.0;
        let grid_width = (ui.available_width() - name_width - 18.0).max(480.0);
        let group_width = (grid_width - 3.0 * group_gap) / 4.0;
        let step_width = (group_width - 3.0 * gap) / 4.0;
        let playing = self.playing_step();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_space(Vec2::new(name_width, 28.0));
            for beat in 0..4 {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(group_width, 28.0), Sense::hover());
                let p = ui.painter();
                let accent = [t::BEAT1, t::BEAT2, t::BEAT3, t::BEAT4][beat];
                p.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(rect.left(), rect.top() + 2.0),
                        Vec2::new(group_width, 2.0),
                    ),
                    1.0,
                    accent,
                );
                p.text(
                    rect.left_center() + Vec2::new(0.0, 4.0),
                    egui::Align2::LEFT_CENTER,
                    format!("BEAT {}", beat + 1),
                    egui::FontId::monospace(11.0),
                    t::MUTED,
                );
                if playing.is_some_and(|step| step / 4 == beat) {
                    let step = playing.unwrap() % 4;
                    let x = rect.left() + step as f32 * (step_width + gap) + step_width / 2.0;
                    let y = rect.center().y;
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            Pos2::new(x - 5.0, y - 4.0),
                            Pos2::new(x + 5.0, y - 4.0),
                            Pos2::new(x, y + 4.0),
                        ],
                        t::TEXT,
                        Stroke::NONE,
                    ));
                }
                if beat < 3 {
                    ui.add_space(group_gap - gap);
                }
            }
        });
        let mut changed = false;
        let solo_any = self.song.patterns.iter().find(|p| p.id == self.selected).is_some_and(|p| p.solo.iter().any(|&x| x));
        for (row, name) in ["KICK", "SNARE", "HAT"].iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let (label, _) =
                    ui.allocate_exact_size(Vec2::new(name_width, 66.0), Sense::hover());
                let muted = self.song.patterns.iter().find(|p| p.id == self.selected).is_some_and(|p| p.muted[row]);
                let soloed = self.song.patterns.iter().find(|p| p.id == self.selected).is_some_and(|p| p.solo[row]);
                let audible = !muted && (!solo_any || soloed);
                for (k, (tag, active)) in [("M", muted), ("S", soloed)].iter().enumerate() {
                    let led = Rect::from_min_size(
                        Pos2::new(label.left() + k as f32 * 28.0, label.center().y - 22.0),
                        Vec2::new(24.0, 24.0),
                    );
                    let lr = ui
                        .interact(led, ui.id().with(("ms", row, k)), Sense::click())
                        .on_hover_text(if k == 0 {
                            "Mute row (click). Ctrl+1/2/3 works too."
                        } else {
                            "Solo row (click)"
                        });
                    let p = ui.painter();
                    let active_fill = if k == 0 { t::BRAND_FILL } else { t::WAIT };
                    p.rect_filled(led, 4.0, if *active { active_fill } else { t::RAISED });
                    p.rect_stroke(led, 4.0, Stroke::new(1.0_f32, t::STRONG), egui::StrokeKind::Inside);
                    p.text(
                        led.center(),
                        egui::Align2::CENTER_CENTER,
                        *tag,
                        egui::FontId::monospace(12.0),
                        if *active { t::BG } else { t::MUTED },
                    );
                    if lr.has_focus() {
                        p.rect_stroke(led.expand(2.0), 4.0, Stroke::new(2.0_f32, t::BRAND), egui::StrokeKind::Outside);
                    }
                    let toggle_mute = k == 0 && lr.clicked();
                    let toggle_solo = k == 1 && lr.clicked();
                    if toggle_mute {
                        self.checkpoint();
                        let m = &mut self.pattern_mut().muted[row];
                        *m = !*m;
                        self.status = format!("{} {} - Ctrl+Z to undo", name, if *m { "muted" } else { "unmuted" });
                        self.publish();
                    } else if toggle_solo {
                        self.checkpoint();
                        let sl = &mut self.pattern_mut().solo[row];
                        *sl = !*sl;
                        self.status = format!("{} {} - Ctrl+Z to undo", name, if *sl { "soloed" } else { "unsoloed" });
                        self.publish();
                    }
                }
                ui.painter().text(
                    label.left_center() + Vec2::new(60.0, -8.0),
                    egui::Align2::LEFT_CENTER,
                    *name,
                    egui::FontId::proportional(15.0),
                    if audible { t::TEXT } else { t::MUTED },
                );
                ui.painter().text(
                    label.left_center() + Vec2::new(60.0, 16.0),
                    egui::Align2::LEFT_CENTER,
                    "DRUM - MASTER",
                    egui::FontId::monospace(11.0),
                    t::MUTED,
                );
                for beat in 0..4 {
                    let (group, _) =
                        ui.allocate_exact_size(Vec2::new(group_width, 66.0), Sense::hover());
                    ui.painter().rect_filled(group, 5.0, t::SURFACE);
                    for offset in 0..4 {
                        let i = beat * 4 + offset;
                        let rect = Rect::from_min_size(
                            Pos2::new(
                                group.left() + offset as f32 * (step_width + gap),
                                group.top() + 7.0,
                            ),
                            Vec2::new(step_width, 52.0),
                        );
                        let response = ui.interact(rect, ui.id().with((row, i)), Sense::click());
                        #[cfg(test)]
                        if row == 0 && i == 0 {
                            self.probe = Some(rect);
                        }
                        let vel = self.pattern_mut().drums[row][i];
                        let on = vel > 0;
                        let accent = vel > 100;
                        let current = playing == Some(i);
                        let row_muted = self.song.patterns.iter().find(|p| p.id == self.selected).is_some_and(|p| p.muted[row] || (solo_any && !p.solo[row]));
                        let fill = if on {
                            if row_muted { t::BRAND_SOFT } else { t::FILL }
                        } else if response.hovered() {
                            t::CTL
                        } else if row_muted {
                            t::SUNKEN
                        } else {
                            t::RAISED
                        };
                        let p = ui.painter();
                        p.rect_filled(rect, 4.0, fill);
                        p.rect_stroke(
                            rect,
                            4.0,
                            Stroke::new(
                                if current { 2.0_f32 } else { 1.0_f32 },
                                if current {
                                    t::TEXT
                                } else if on {
                                    t::BRAND
                                } else {
                                    t::STRONG
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        p.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{:02}", i + 1),
                            egui::FontId::monospace(13.0),
                            if on || current { t::TEXT } else { t::MUTED },
                        );
                        if on {
                            let y = rect.bottom() - 7.0;
                            p.line_segment(
                                [
                                    Pos2::new(rect.left() + 9.0, y),
                                    Pos2::new(rect.right() - 9.0, y),
                                ],
                                Stroke::new(if accent { 5.0_f32 } else { 3.0_f32 }, t::TEXT),
                            );
                        }
                        if response.has_focus() {
                            p.rect_stroke(
                                rect.expand(2.0),
                                4.0,
                                Stroke::new(2.0_f32, t::BRAND),
                                egui::StrokeKind::Outside,
                            );
                        }
                        let shift = ui.input(|i| i.modifiers.shift);
                        if response.clicked() {
                            self.checkpoint();
                            if shift {
                                // accent cycle: off->127, 100->127, 127->70, 70->100
                                let nv = match vel {
                                    0 => 127,
                                    127 => 70,
                                    70 => 100,
                                    _ => 127,
                                };
                                self.pattern_mut().drums[row][i] = nv;
                                self.status = format!("Step {:02} velocity {nv} - Ctrl+Z to undo", i + 1);
                            } else {
                                self.pattern_mut().drums[row][i] = if on { 0 } else { 100 };
                                self.status = if !on { format!("Step {:02} on - Ctrl+Z to undo", i + 1) } else { format!("Step {:02} off - Ctrl+Z to undo", i + 1) };
                            }
                            changed = true;
                        }
                        if response.secondary_clicked() && on {
                            self.checkpoint();
                            self.pattern_mut().drums[row][i] = 0;
                            self.status = format!("Step {:02} erased - Ctrl+Z to undo", i + 1);
                            changed = true;
                        }
                    }
                    if beat < 3 {
                        ui.add_space(group_gap - gap);
                    }
                }
            });
        }
        ui.add_space(12.0);
        let sampler_ids: Vec<u64> = self
            .song
            .instruments
            .iter()
            .filter(|i| matches!(i.kind, keyframexl::sequence::InstrumentKind::Sampler { .. }))
            .map(|i| i.id)
            .collect();
        for inst_id in sampler_ids {
            self.sampler_row(ui, name_width, grid_width, inst_id);
            ui.add_space(8.0);
        }
        ui.add_space(12.0);
        let sampler_now = self.song.sampler_preset().cloned();
        if let Some(preset) = &sampler_now {
            ui.add_space(10.0);
            ui.collapsing("DWP zones & import report", |ui| {
                egui::Grid::new("zone-table")
                    .striped(true)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        for heading in ["Zone", "Keys", "Velocity", "Loop frames", "ADSR"] {
                            ui.strong(heading);
                        }
                        ui.end_row();
                        for z in &preset.zones {
                            ui.label(
                                std::path::Path::new(&z.name)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            );
                            ui.label(format!("{}–{}", note_name(z.low), note_name(z.high)));
                            ui.label(format!("{}–{}", z.low_velocity, z.high_velocity));
                            ui.label(if z.loop_mode == keyframexl::sampler::LoopMode::None {
                                "Off".into()
                            } else {
                                format!("{}..{}", z.loop_start, z.loop_end)
                            });
                            ui.label(format!(
                                "{:.3}s / {:.3}s / {:.0}% / {:.3}s",
                                z.adsr.attack,
                                z.adsr.decay,
                                z.adsr.sustain * 100.0,
                                z.adsr.release
                            ));
                            ui.end_row();
                        }
                    });
                ui.label(&preset.report);
            });
        }
        if changed {
            self.publish();
        }
    }
    fn roll(&mut self, ui: &mut egui::Ui) {
        let sampler_ids: Vec<(u64, String)> = self
            .song
            .instruments
            .iter()
            .filter(|i| matches!(i.kind, keyframexl::sequence::InstrumentKind::Sampler { .. }))
            .map(|i| (i.id, i.name.clone()))
            .collect();
        if !sampler_ids.iter().any(|(id, _)| *id == self.instrument) {
            self.instrument = sampler_ids.first().map(|(id, _)| *id).unwrap_or(4);
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("PIANO ROLL").font(egui::FontId::new(20.0, egui::FontFamily::Name("plex-semibold".into()))));
            ui.add_space(12.0);
            let iname = sampler_ids
                .iter()
                .find(|(id, _)| *id == self.instrument)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "Sampler".into());
            ui.label(
                egui::RichText::new(format!(
                    "{} / {}",
                    self.pattern_mut().name.to_uppercase(),
                    iname.to_uppercase()
                ))
                .monospace()
                .size(12.0)
                .color(t::MUTED),
            );
            egui::ComboBox::from_id_salt("roll-instrument")
                .selected_text(iname.clone())
                .show_ui(ui, |ui| {
                    for (id, name) in &sampler_ids {
                        ui.selectable_value(&mut self.instrument, *id, name.clone());
                    }
                });
            ui.separator();
            ui.selectable_value(&mut self.slide, false, "Draw note (P)");
            ui.selectable_value(&mut self.slide, true, "Draw slide (S)");
            egui::ComboBox::from_id_salt("snap")
                .selected_text(format!("Snap {}", self.snap.label()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.snap, Snap::Step, "1/16 step");
                    ui.selectable_value(&mut self.snap, Snap::Beat, "1/4 beat");
                    ui.selectable_value(&mut self.snap, Snap::Bar, "1 bar");
                })
                .response
                .on_hover_text("Snap grid for new notes. Hold Alt to bypass.");
            ui.add(
                egui::Slider::new(&mut self.velocity, 1..=127)
                    .text("Velocity")
                    .clamping(egui::SliderClamping::Always),
            );
        });
        let ri = self.instrument;
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("DRAG empty = draw  \u{00b7}  DRAG note = move/resize  \u{00b7}  RIGHT-CLICK erase  \u{00b7}  SPACE play  \u{00b7}  CTRL+Z undo")
                .monospace()
                .size(11.0)
                .color(t::MUTED),
        );
        ui.add_space(4.0);
        let width = (ui.available_width() - 55.0).max(320.0);
        let cell = width / 16.0;
        let row = ((ui.available_height() - 80.0) / 25.0).clamp(16.0, 24.0);
        let size = Vec2::new(width + 52.0, row * 25.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        #[cfg(test)]
        {
            self.probe = Some(rect);
        }
        let p = ui.painter_at(rect);
        let left = rect.left() + 52.0;
        for y in 0..25 {
            let pitch = 72 - y as u8;
            let top = rect.top() + y as f32 * row;
            let black = [1, 3, 6, 8, 10].contains(&(pitch % 12));
            p.rect_filled(
                Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(52.0, row)),
                0.0,
                if black { t::SUNKEN } else { t::SURFACE },
            );
            let names = [
                "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
            ];
            p.text(
                Pos2::new(rect.left() + 6.0, top + 4.0),
                egui::Align2::LEFT_TOP,
                format!("{}{}", names[(pitch % 12) as usize], pitch / 12 - 1),
                egui::FontId::proportional(12.0),
                t::TEXT,
            );
            p.line_segment(
                [Pos2::new(left, top), Pos2::new(left + width, top)],
                Stroke::new(1.0_f32, t::LINE),
            );
        }
        for x in 0..=16 {
            let xx = left + x as f32 * cell;
            p.line_segment(
                [Pos2::new(xx, rect.top()), Pos2::new(xx, rect.bottom())],
                Stroke::new(
                    if x % 4 == 0 { 1.5_f32 } else { 0.5_f32 },
                    if x % 4 == 0 { t::STRONG } else { t::LINE },
                ),
            );
        }
        if let Some(audio) = &self.audio
            && audio.playing.load(Ordering::Relaxed)
        {
            let frame = audio.position.load(Ordering::Relaxed);
            let bar_frames = (keyframexl::sequence::seconds_per_step(self.song.tempo) * 16.0 * RATE as f64) as u64;
            let x = left + (frame % bar_frames) as f32 / bar_frames as f32 * width;
            p.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(2.5_f32, t::BRAND),
            );
        }
        let pat = self
            .song
            .patterns
            .iter()
            .find(|x| x.id == self.selected)
            .unwrap();
        for n in &pat.notes {
            if !(48..=72).contains(&n.pitch) {
                continue;
            }
            let r = Rect::from_min_size(
                Pos2::new(
                    left + n.start as f32 * cell,
                    rect.top() + (72 - n.pitch) as f32 * row + 2.0,
                ),
                Vec2::new(n.length as f32 * cell - 2.0, row - 4.0),
            );
            let own = n.instrument == ri;
            p.rect_filled(r, 3.0, if own { t::BRAND_FILL } else { t::SURFACE });
            if own && r.width() > 35.0 {
                p.text(
                    r.left_center() + Vec2::new(7.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    note_name(n.pitch),
                    egui::FontId::proportional(12.0),
                    t::ON_BRAND,
                );
            }
        }
        for s in &pat.slides {
            if !(48..=72).contains(&s.target) {
                continue;
            }
            let r = Rect::from_min_size(
                Pos2::new(
                    left + s.start as f32 * cell,
                    rect.top() + (72 - s.target) as f32 * row + 2.0,
                ),
                Vec2::new(s.length as f32 * cell - 2.0, row - 4.0),
            );
            let sown = s.instrument == ri;
            p.rect_filled(r, 2.0, if sown { t::WAIT_SOFT } else { t::SURFACE });
            p.rect_stroke(
                r,
                2.0,
                Stroke::new(2.0_f32, if sown { t::WAIT } else { t::LINE }),
                egui::StrokeKind::Inside,
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    r.left_top() + Vec2::new(5.0, 12.0),
                    r.left_top() + Vec2::new(11.0, 3.0),
                    r.left_top() + Vec2::new(17.0, 12.0),
                ],
                t::TEXT,
                Stroke::NONE,
            ));
            if r.width() > 70.0 {
                p.text(
                    r.left_center() + Vec2::new(23.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{} · {} steps", note_name(s.target), s.length),
                    egui::FontId::proportional(12.0),
                    t::TEXT,
                );
            }
            if let Some(n) = pat
                .notes
                .iter()
                .filter(|n| {
                    n.group == s.group
                        && n.instrument == s.instrument
                        && n.start <= s.start
                        && n.start as u16 + n.length as u16 > s.start as u16
                })
                .max_by_key(|n| n.pitch)
                && (48..=72).contains(&n.pitch)
            {
                p.line_segment(
                    [
                        Pos2::new(
                            r.left(),
                            rect.top() + (72 - n.pitch) as f32 * row + row / 2.0,
                        ),
                        Pos2::new(r.right(), r.center().y),
                    ],
                    Stroke::new(1.5_f32, t::WAIT),
                );
            }
        }
        let velocity = self.velocity;
        let alt_bypass = ui.input(|i| i.modifiers.alt);
        let grid = self.snap.grid();
        let locate = |pos: Pos2| -> Option<(usize, u8)> {
            if pos.x < left || pos.x >= left + width || pos.y < rect.top() || pos.y >= rect.bottom()
            {
                return None;
            }
            Some((
                ((pos.x - left) / cell) as usize,
                72 - ((pos.y - rect.top()) / row) as u8,
            ))
        };
        if response.drag_started()
            && let Some((x, y)) = ui
                .input(|input| input.pointer.press_origin())
                .and_then(locate)
        {
            let mut grabbed = None;
            if !self.slide {
                if let Some(pat) = self.song.patterns.iter().find(|p| p.id == self.selected) {
                    if let Some((idx, n)) = pat
                        .notes
                        .iter()
                        .enumerate()
                        .find(|(_, n)| n.instrument == ri && n.pitch == y && (x as u8) >= n.start && (x as u8) < n.start + n.length)
                    {
                        let press_px = ui
                            .input(|input| input.pointer.press_origin())
                            .map(|pos| pos.x)
                            .unwrap_or(left + x as f32 * cell);
                        let note_end_px = left + (n.start + n.length) as f32 * cell;
                        let mode = if note_end_px - press_px < 8.0 && n.length > 1 {
                            MoveMode::ResizeRight
                        } else {
                            MoveMode::Move
                        };
                        grabbed = Some(Moving {
                            idx,
                            mode,
                            grab: x.saturating_sub(n.start as usize),
                            pitch: n.pitch,
                        });
                    }
                }
            }
            if let Some(g) = grabbed {
                self.moving = Some(g);
            } else {
                self.drag = Some((x, y));
            }
        }
        if let (Some(mv), Some(pos)) = (self.moving, response.interact_pointer_pos().and_then(locate)) {
            let (end_step, end_pitch) = pos;
            if let Some(pat) = self.song.patterns.iter().find(|p| p.id == self.selected) {
                if let Some(n) = pat.notes.get(mv.idx) {
                    let (ns, nl, np) = move_target(
                        mv.mode,
                        n.start,
                        n.length,
                        n.pitch,
                        mv.grab,
                        end_step,
                        end_pitch,
                        grid,
                        alt_bypass,
                    );
                    p.rect_stroke(
                        Rect::from_min_size(
                            Pos2::new(
                                left + ns as f32 * cell,
                                rect.top() + (72 - np) as f32 * row + 2.0,
                            ),
                            Vec2::new(nl as f32 * cell - 2.0, row - 4.0),
                        ),
                        2.0,
                        Stroke::new(2.0_f32, t::BRAND),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        if response.dragged()
            && let (Some((x, y)), Some((end, _))) =
                (self.drag, response.interact_pointer_pos().and_then(locate))
        {
            let start = x.min(end);
            let end = x.max(end);
            let r = Rect::from_min_size(
                Pos2::new(
                    left + start as f32 * cell,
                    rect.top() + (72 - y) as f32 * row + 2.0,
                ),
                Vec2::new((end - start + 1) as f32 * cell - 2.0, row - 4.0),
            );
            p.rect_stroke(
                r,
                2.0,
                Stroke::new(2.0_f32, t::BRAND),
                egui::StrokeKind::Inside,
            );
            if self.slide
                && let Some(n) = self
                    .song
                    .patterns
                    .iter()
                    .find(|p| p.id == self.selected)
                    .and_then(|p| {
                        p.notes
                            .iter()
                            .filter(|n| {
                                n.instrument == ri
                                    && (48..=72).contains(&n.pitch)
                                    && n.start as usize <= start
                                    && n.start as usize + n.length as usize > start
                            })
                            .max_by_key(|n| n.pitch)
                    })
            {
                p.line_segment(
                    [
                        Pos2::new(
                            r.left(),
                            rect.top() + (72 - n.pitch) as f32 * row + row / 2.0,
                        ),
                        Pos2::new(r.right(), r.center().y),
                    ],
                    Stroke::new(2.0_f32, t::BRAND),
                );
            }
        }
        if response.drag_stopped() && let Some(mv) = self.moving.take() {
            if let Some(pos) = response.interact_pointer_pos().and_then(locate) {
                let (end_step, end_pitch) = pos;
                let (ns, nl, np) = {
                    let pat = self.pattern_mut();
                    if mv.idx >= pat.notes.len() {
                        self.status = "Move cancelled".into();
                        self.publish();
                        return;
                    }
                    move_target(
                        mv.mode,
                        pat.notes[mv.idx].start,
                        pat.notes[mv.idx].length,
                        pat.notes[mv.idx].pitch,
                        mv.grab,
                        end_step,
                        end_pitch,
                        grid,
                        alt_bypass,
                    )
                };
                let moving_inst = {
                    let pat = self.pattern_mut();
                    pat.notes.get(mv.idx).map(|n| n.instrument).unwrap_or(self.instrument)
                };
                let clash = {
                    let pat = self.pattern_mut();
                    pat.notes.iter().enumerate().any(|(j, n)| {
                        j != mv.idx && n.instrument == moving_inst && n.pitch == np && ns < n.start + n.length && ns + nl > n.start
                    })
                };
                if clash {
                    self.status = "Target occupied - move cancelled".into();
                    self.publish();
                    return;
                }
                self.checkpoint();
                let pat = self.pattern_mut();
                if mv.idx < pat.notes.len() {
                    // ns/nl/np already snapped + clash-checked above; reuse verbatim.
                    match mv.mode {
                        MoveMode::Move => {
                            let n = &mut pat.notes[mv.idx];
                            n.start = ns;
                            n.pitch = np;
                            self.status = format!("Moved {} - Ctrl+Z to undo", note_name(np));
                        }
                        MoveMode::ResizeRight => {
                            let n = &mut pat.notes[mv.idx];
                            n.start = ns;
                            n.length = nl;
                            self.status = format!("Resized {} to {} - Ctrl+Z to undo", note_name(n.pitch), nl);
                        }
                    }
                }
                self.publish();
            }
        }
        if response.drag_stopped()
            && let (Some((x, y)), Some((end, _))) = (
                self.drag.take(),
                response.interact_pointer_pos().and_then(locate),
            )
        {
            let inst = self.instrument;
            let (start, end) = (x.min(end), x.max(end));
            let (start, end) = (
                snap_step(start as u8, grid, alt_bypass) as usize,
                snap_step(end as u8, grid, alt_bypass) as usize,
            );
            let (start, end) = (start.min(end), start.max(end));
            let length = (end - start + 1) as u8;
            let occupied = if self.slide {
                self.pattern_mut().slides.iter().any(|gl| {
                    gl.instrument == ri && gl.target == y
                        && (start as u8) < gl.start + gl.length
                        && (start as u8 + length) > gl.start
                })
            } else {
                self.pattern_mut().notes.iter().any(|n| {
                    n.instrument == ri && n.pitch == y
                        && (start as u8) < n.start + n.length
                        && (start as u8 + length) > n.start
                })
            };
            if occupied {
                self.drag.take();
                self.status = "Cell occupied \u{2014} right-click erases, drag empty area".into();
                return;
            }
            self.checkpoint();
            self.status = "Drawn \u{2014} Ctrl+Z to undo".into();
            if self.slide {
                self.pattern_mut().slides.push(Glide {
                    start: start as u8,
                    length,
                    target: y,
                    group: 0,
                    instrument: inst,
                });
            } else {
                self.pattern_mut().notes.push(Note {
                    start: start as u8,
                    length,
                    pitch: y,
                    velocity,
                    group: 0,
                    instrument: inst,
                });
            }
            self.publish();
        }
        if response.clicked()
            && let Some((cx, y)) = response.interact_pointer_pos().and_then(locate)
        {
            let inst = self.instrument;
            let x = snap_step(cx as u8, grid, alt_bypass) as usize;
            let taken = if self.slide {
                self.pattern_mut().slides.iter().any(|gl| {
                    gl.instrument == ri && gl.target == y && x as u8 >= gl.start && (x as u8) < gl.start + gl.length
                })
            } else {
                self.pattern_mut().notes.iter().any(|n| {
                    n.instrument == ri && n.pitch == y && x as u8 >= n.start && (x as u8) < n.start + n.length
                })
            };
            if taken {
                self.status = "Cell occupied \u{2014} right-click erases, drag empty area".into();
                return;
            }
            self.checkpoint();
            if self.slide {
                self.pattern_mut().slides.push(Glide {
                    start: x as u8,
                    length: 1,
                    target: y,
                    group: 0,
                    instrument: inst,
                });
            } else {
                self.pattern_mut().notes.push(Note {
                    start: x as u8,
                    length: 1,
                    pitch: y,
                    velocity,
                    group: 0,
                    instrument: inst,
                });
            }
            self.publish();
        }
        if response.secondary_clicked()
            && let Some((x, y)) = response.interact_pointer_pos().and_then(locate)
        {
            let slide = self.slide;
            let hit: Option<(bool, usize)> = {
                let pat = self.pattern_mut();
                if slide {
                    pat.slides.iter().position(|s| {
                        s.instrument == ri && s.target == y && x >= s.start as usize && x < (s.start + s.length) as usize
                    }).map(|idx| (true, idx))
                } else {
                    pat.notes.iter().position(|n| {
                        n.instrument == ri && n.pitch == y && x >= n.start as usize && x < (n.start + n.length) as usize
                    }).map(|idx| (false, idx))
                }
            };
            if let Some((is_slide, idx)) = hit {
                self.checkpoint();
                let pat = self.pattern_mut();
                if is_slide {
                    pat.slides.remove(idx);
                    self.status = "Slide removed - Ctrl+Z to undo".into();
                } else {
                    let removed_name = note_name(pat.notes[idx].pitch);
                    pat.notes.remove(idx);
                    self.status = format!("Removed {} - Ctrl+Z to undo", removed_name);
                }
            }
            self.publish();
        }
        ui.add_space(8.0);
        let lane_size = Vec2::new(width + 52.0, 48.0);
        let (lane, response) = ui.allocate_exact_size(lane_size, Sense::click_and_drag());
        let paint = ui.painter_at(lane);
        paint.rect_filled(lane, 2.0, t::SURFACE);
        paint.text(
            lane.left_center() + Vec2::new(3.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "VELOCITY",
            egui::FontId::monospace(11.0),
            t::MUTED,
        );
        for n in &self
            .song
            .patterns
            .iter()
            .find(|p| p.id == self.selected)
            .unwrap()
            .notes
        {
            let x = lane.left() + 52.0 + n.start as f32 * cell;
            paint.rect_filled(
                Rect::from_min_size(
                    Pos2::new(x, lane.bottom() - 4.0 - n.velocity as f32 / 127.0 * 36.0),
                    Vec2::new(7.0, n.velocity as f32 / 127.0 * 36.0),
                ),
                1.0,
                t::BRAND,
            );
        }
        if (response.clicked() || response.drag_started())
            && let Some(pos) = response.interact_pointer_pos()
        {
            let step = ((pos.x - lane.left() - 52.0) / cell).floor() as i32;
            let hit = self
                .song
                .patterns
                .iter()
                .find(|p| p.id == self.selected)
                .is_some_and(|p| p.notes.iter().any(|n| n.instrument == ri && n.start as i32 == step));
            if hit {
                self.checkpoint();
            }
        }
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            let step = ((pos.x - lane.left() - 52.0) / cell).floor() as i32;
            let velocity =
                (((lane.bottom() - pos.y) / lane.height()) * 127.0).clamp(1.0, 127.0) as u8;
            if let Some(top) = self
                .pattern_mut()
                .notes
                .iter()
                .filter(|n| n.instrument == ri && n.start as i32 == step)
                .map(|n| n.pitch)
                .max()
            {
                for n in &mut self.pattern_mut().notes {
                    if n.instrument == ri && n.start as i32 == step && n.pitch == top {
                        n.velocity = velocity;
                    }
                }
            }
            self.publish();
        }
    }
    fn playlist(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("PLAYLIST").font(egui::FontId::new(20.0, egui::FontFamily::Name("plex-semibold".into()))));
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("BAR > SHARED DATA")
                    .monospace()
                    .size(11.0)
                    .color(t::MUTED),
            );
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("CLICK select  \u{00b7}  DOUBLE-CLICK open roll  \u{00b7}  DEL remove clip  \u{00b7}  CTRL+Z undo")
                .monospace()
                .size(11.0)
                .color(t::MUTED),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let add_response = ui.add(
                egui::Button::new("+ Add pattern").min_size(egui::Vec2::new(0.0, 34.0)),
            );
            #[cfg(test)]
            {
                self.probe = Some(add_response.rect);
            }
            if add_response.clicked() {
                self.checkpoint();
                let bar = self
                    .song
                    .bars()
                    .saturating_sub(if self.song.clips.is_empty() { 1 } else { 0 });
                self.song.clips.push(Clip {
                    pattern: self.selected,
                    bar,
                });
                self.status = format!("Added clip at Bar {} \u{2014} Ctrl+Z to undo", bar + 1);
                self.publish();
            }
            let unique_response = ui.add_enabled(
                self.clip.is_some(),
                egui::Button::new("Make unique").min_size(egui::Vec2::new(0.0, 34.0)),
            );
            #[cfg(test)]
            {
                self.probe2 = Some(unique_response.rect);
            }
            if unique_response.clicked()
                && let Some(i) = self.clip
            {
                self.checkpoint();
                match self.song.unique(i) {
                    Ok(id) => {
                        self.selected = id;
                        self.publish()
                    }
                    Err(e) => self.status = e.into(),
                }
            }
        });
        // Timeline: one lane per overlapping stack, 1 bar = 1 cell. Drag moves, double-click opens Roll.
        let n_bars = self.song.clips.iter().map(|c| c.bar + 2).max().unwrap_or(4).max(4).min(64) as usize;
        let bar_w = 96.0_f32;
        let lane_h = 44.0_f32;
        // stack overlapping clips into lanes
        let mut lanes: Vec<Vec<usize>> = vec![];
        let clips = self.song.clips.clone();
        let mut lane_of = vec![0usize; clips.len()];
        for (i, c) in clips.iter().enumerate() {
            let mut lane = 0;
            loop {
                let clash = lanes.get(lane).is_some_and(|l| {
                    l.iter().any(|&j| clips[j].bar == c.bar)
                });
                if clash {
                    lane += 1;
                } else {
                    break;
                }
            }
            lane_of[i] = lane;
            if lanes.len() <= lane {
                lanes.push(vec![]);
            }
            lanes[lane].push(i);
        }
        let lanes_n = lanes.len().max(1);
        let grid_w = 52.0 + n_bars as f32 * bar_w;
        let grid_h = 28.0 + lanes_n as f32 * (lane_h + 6.0);
        egui::ScrollArea::horizontal()
            .id_salt("playlist-timeline")
            .show(ui, |ui| {
                let (grid, _) = ui.allocate_exact_size(Vec2::new(grid_w, grid_h), Sense::hover());
                let paint = ui.painter_at(grid);
                paint.rect_filled(grid, 6.0, t::SUNKEN);
                for b in 0..n_bars {
                    let x = grid.left() + 52.0 + b as f32 * bar_w;
                    paint.text(
                        Pos2::new(x + 4.0, grid.top() + 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("{:02}", b + 1),
                        egui::FontId::monospace(11.0),
                        t::MUTED,
                    );
                    paint.line_segment(
                        [Pos2::new(x, grid.top() + 24.0), Pos2::new(x, grid.bottom())],
                        Stroke::new(1.0_f32, t::LINE),
                    );
                }
                for (i, c) in clips.iter().enumerate() {
                    let name: String = self
                        .song
                        .patterns
                        .iter()
                        .find(|p| p.id == c.pattern)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Missing".into());
                    let r = Rect::from_min_size(
                        Pos2::new(
                            grid.left() + 52.0 + c.bar as f32 * bar_w + 2.0,
                            grid.top() + 28.0 + lane_of[i] as f32 * (lane_h + 6.0),
                        ),
                        Vec2::new(bar_w - 4.0, lane_h),
                    );
                    let cr = ui.interact(r, ui.id().with(("clip", i)), Sense::click_and_drag());
                    #[cfg(test)]
                    if i == 1 {
                        self.probe3 = Some(r);
                    }
                    let selected = self.clip == Some(i);
                    paint.rect_filled(r, 4.0, if selected { t::SELECTION } else { t::SURFACE });
                    paint.rect_stroke(
                        r,
                        4.0,
                        Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, if selected { t::BRAND } else { t::STRONG }),
                        egui::StrokeKind::Inside,
                    );
                    paint.text(
                        r.left_top() + Vec2::new(8.0, 6.0),
                        egui::Align2::LEFT_TOP,
                        name.to_uppercase(),
                        egui::FontId::proportional(13.0),
                        t::TEXT,
                    );
                    paint.text(
                        r.left_bottom() + Vec2::new(8.0, -4.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("BAR {:02}", c.bar + 1),
                        egui::FontId::monospace(11.0),
                        t::MUTED,
                    );
                    if cr.double_clicked() {
                        self.clip = Some(i);
                        self.selected = c.pattern;
                        self.screen = Screen::Roll;
                        self.status = format!("Opened {} - F6 rack / F7 roll", name);
                        self.publish();
                    } else if cr.clicked() {
                        self.clip = Some(i);
                        self.selected = c.pattern;
                        self.publish();
                    }
                    if cr.drag_stopped() {
                        if let Some(pos) = cr.interact_pointer_pos() {
                            let nb = ((pos.x - (grid.left() + 52.0)) / bar_w).floor().max(0.0) as u32;
                            if nb != c.bar {
                                self.checkpoint();
                                if let Some(cc) = self.song.clips.get_mut(i) {
                                    cc.bar = nb;
                                }
                                self.status = format!("Clip to Bar {} - Ctrl+Z to undo", nb + 1);
                                self.publish();
                            }
                        }
                    }
                }
            });
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(result) = self.importing.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.importing = None;
            match result {
                Ok(preset) => {
                    self.status = format!("Imported {} · {}", preset.name, preset.report);
                    self.checkpoint();
                    let iname = preset.name.clone();
                    let nid = self.song.instruments.iter().map(|i| i.id).max().unwrap_or(0) + 1;
                    self.song.instruments.push(keyframexl::sequence::Instrument {
                        id: nid,
                        name: iname,
                        kind: keyframexl::sequence::InstrumentKind::Sampler {
                            preset: Some(Arc::new(preset)),
                        },
                    });
                    self.instrument = nid;
                    self.screen = Screen::Roll;
                    self.publish();
                }
                Err(error) => self.status = format!("Import failed: {error}"),
            }
        }
        let ctrl = ctx.input(|i| i.modifiers.ctrl);
        // FL-like: Space = Play/Stop with rewind to bar start, Ctrl+Space = pause in place.
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) && !ctx.wants_keyboard_input() {
            if let Some(a) = &self.audio {
                let playing = a.playing.load(Ordering::Relaxed);
                if playing {
                    a.playing.store(false, Ordering::Relaxed);
                    if !ctrl {
                        a.position.store(0, Ordering::Relaxed);
                    }
                } else {
                    a.playing.store(true, Ordering::Relaxed);
                }
            }
        }
        // Undo / redo everywhere.
        if !ctx.wants_keyboard_input() && ctrl {
            if ctx.input(|i| i.key_pressed(egui::Key::Z)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Y)) {
                self.redo();
            }
        }
        // Delete selected clip with Del.
        if !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::Delete))
            && self.screen == Screen::Playlist
            && let Some(i) = self.clip
        {
            self.checkpoint();
            self.song.clips.remove(i);
            self.clip = None;
            self.status = "Clip deleted \u{2014} Ctrl+Z to undo".into();
            self.publish();
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::F6)) {
                self.screen = Screen::Rack;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F7)) {
                self.screen = Screen::Roll;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
                self.screen = Screen::Playlist;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::P)) {
                self.slide = false;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::S)) {
                self.slide = true;
            }
            for (n, key) in [(0, egui::Key::Num1), (1, egui::Key::Num2), (2, egui::Key::Num3)] {
                if ctrl && ctx.input(|i| i.key_pressed(key)) {
                    self.checkpoint();
                    let m = &mut self.pattern_mut().muted[n];
                    *m = !*m;
                    self.status = format!("Row {} {} - Ctrl+Z to undo", n + 1, if *m { "muted" } else { "unmuted" });
                    self.publish();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                self.song_mode = !self.song_mode;
                if let Some(a) = &self.audio {
                    a.song_mode.store(self.song_mode, Ordering::Relaxed);
                    a.position.store(0, Ordering::Relaxed);
                }
            }
        }
        egui::TopBottomPanel::top("transport")
            .frame(
                egui::Frame::new()
                    .fill(t::RAISED)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(172.0, 40.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.label(egui::RichText::new("KeyFrameXL").font(egui::FontId::new(20.0, egui::FontFamily::Name("plex-semibold".into()))));
                            ui.label(
                                egui::RichText::new("KFXL v0.4 · PATTERN SEQUENCER")
                                    .size(11.0)
                                    .color(t::MUTED)
                                    .monospace(),
                            );
                        },
                    );
                    let playing = self
                        .audio
                        .as_ref()
                        .is_some_and(|a| a.playing.load(Ordering::Relaxed));
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(if playing { "|| PAUSE" } else { "> PLAY" })
                                    .color(t::ON_BRAND)
                                    .font(egui::FontId::new(14.0, egui::FontFamily::Name("plex-semibold".into()))),
                            )
                            .fill(t::FILL)
                            .min_size(Vec2::new(96.0, 34.0)),
                        )
                        .on_hover_text("Pause in place (Ctrl+Space). Space = stop + rewind.")
                        .clicked()
                        && let Some(a) = &self.audio
                    {
                        if playing {
                            a.playing.store(false, Ordering::Relaxed);
                        } else {
                            a.playing.store(true, Ordering::Relaxed);
                        }
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("\u{25a0} STOP").color(t::TEXT).size(14.0),
                            )
                            .min_size(Vec2::new(82.0, 34.0)),
                        )
                        .on_hover_text("Stop and rewind")
                        .clicked()
                        && let Some(a) = &self.audio
                    {
                        a.playing.store(false, Ordering::Relaxed);
                        a.position.store(0, Ordering::Relaxed);
                    }
                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);
                    let mut mode = self.song_mode;
                    for (value, label) in [(false, "PATTERN"), (true, "SONG")] {
                        let selected = mode == value;
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new(label).size(14.0).color(
                                    if selected {
                                        t::BG
                                    } else {
                                        t::TEXT
                                    },
                                ))
                                .fill(if selected { t::TEXT } else { t::SURFACE })
                                .min_size(Vec2::new(76.0, 34.0)),
                            )
                            .clicked()
                        {
                            mode = value;
                        }
                    }
                    if mode != self.song_mode {
                        self.song_mode = mode;
                        if let Some(a) = &self.audio {
                            a.song_mode.store(mode, Ordering::Relaxed);
                            a.position.store(0, Ordering::Relaxed);
                        }
                    }
                    ui.add_space(12.0);
                    let position = self
                        .audio
                        .as_ref()
                        .map(|a| a.position.load(Ordering::Relaxed))
                        .unwrap_or(0);
                    let step =
                        (position as f64 / RATE as f64 / keyframexl::sequence::seconds_per_step(self.song.tempo))
                            as usize;
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{:02} : {:02}",
                                step / 16 + 1,
                                step % 16 + 1
                            ))
                            .monospace()
                            .size(20.0),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:.2} BPM  -  4/4", self.song.tempo))
                                .color(t::MUTED)
                                .size(12.0)
                                .monospace(),
                        );
                    });
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!("TEMPO {:.0}", self.song.tempo))
                                .monospace()
                                .size(11.0)
                                .color(t::MUTED),
                        );
                        let tr = ui.add(
                            egui::Slider::new(&mut self.song.tempo, 40.0..=300.0)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        if tr.drag_started() {
                            self.checkpoint();
                        }
                        if tr.changed() {
                            self.song.tempo = self.song.tempo.clamp(40.0, 300.0);
                            self.status = format!("Tempo {:.1} BPM", self.song.tempo);
                            self.publish();
                        }
                        let tr2 = tr.on_hover_text("Tempo 40-300 BPM. Drag, or TAP to set.");
                        let _ = tr2;
                    });
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!("SWING {:.0}%", self.song.swing * 100.0))
                                .monospace()
                                .size(11.0)
                                .color(t::MUTED),
                        );
                        let sr = ui.add(
                            egui::Slider::new(&mut self.song.swing, 0.0..=0.6)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        if sr.drag_started() {
                            self.checkpoint();
                        }
                        if sr.changed() {
                            self.song.swing = self.song.swing.clamp(0.0, 0.6);
                            self.status = format!("Swing {:.0}% - Ctrl+Z to undo", self.song.swing * 100.0);
                            self.publish();
                        }
                        sr.on_hover_text("Swing: odd 16ths play late. 0% = straight.");
                    });
                    if ui
                        .add(egui::Button::new("TAP").min_size(Vec2::new(52.0, 34.0)))
                        .on_hover_text("Tap tempo: click 3+ times on the beat")
                        .clicked()
                    {
                        let now = std::time::Instant::now();
                        self.taps.push(now);
                        self.taps.retain(|t| now.duration_since(*t).as_secs_f64() < 2.5);
                        let n_taps = self.taps.len();
                        if n_taps >= 3 {
                            let mut iv: Vec<f64> = self.taps.windows(2).map(|w| w[1].duration_since(w[0]).as_secs_f64()).collect();
                            iv.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                            let med = iv[iv.len() / 2];
                            if med > 0.05 {
                                self.checkpoint();
                                self.song.tempo = (60.0 / med).clamp(40.0, 300.0);
                                self.status = format!("Tempo {:.1} BPM (tap) - Ctrl+Z to undo", self.song.tempo);
                                self.publish();
                                self.taps.clear();
                            }
                        } else {
                            self.status = format!("Tap {n_taps}/3 on the beat...").into();
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("Export WAV").min_size(Vec2::new(0.0, 34.0)))
                            .clicked()
                        {
                            self.export();
                        }
                        if ui
                            .add(egui::Button::new("Open").min_size(Vec2::new(0.0, 34.0)))
                            .clicked()
                        {
                            self.open();
                        }
                        if ui
                            .add(egui::Button::new("Save").min_size(Vec2::new(0.0, 34.0)))
                            .clicked()
                        {
                            self.save();
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let (tone, prefix) = if self.status.starts_with("Import failed")
                || self.status.starts_with("Open failed")
                || self.status.starts_with("Save failed")
                || self.status.starts_with("Export failed")
            {
                (t::BAD, "ERROR: ")
            } else if self.status.starts_with("Importing") {
                (t::WAIT, "... ")
            } else if self.status.starts_with("Undone")
                || self.status.starts_with("Removed")
                || self.status.starts_with("Slide removed")
                || self.status.starts_with("Moved")
                || self.status.starts_with("Resized")
                || self.status.starts_with("Drawn")
                || self.status.starts_with("Note drawn")
                || self.status.starts_with("Step")
                || self.status.starts_with("Added clip")
                || self.status.starts_with("Clip deleted")
                || self.status.starts_with("Cell occupied")
                || self.status.starts_with("Target occupied")
                || self.status.starts_with("Move cancelled")
            {
                (t::TEXT, "* ")
            } else if self.status.starts_with("Imported")
                || self.status.starts_with("Saved")
                || self.status.starts_with("Opened")
                || self.status.starts_with("Exported")
                || self.status.starts_with("Redone")
            {
                (t::OK, "OK: ")
            } else {
                (t::MUTED, "")
            };
            ui.label(egui::RichText::new(format!("{prefix}{}", self.status)).color(tone).size(13.0));
        });
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new("WORKSPACE")
                        .monospace()
                        .size(11.0)
                        .color(t::MUTED),
                );
                ui.add_space(8.0);
                ui.selectable_value(&mut self.screen, Screen::Rack, "RACK");
                ui.selectable_value(&mut self.screen, Screen::Roll, "PIANO ROLL");
                ui.selectable_value(&mut self.screen, Screen::Playlist, "PLAYLIST");
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("PATTERNS")
                        .monospace()
                        .size(11.0)
                        .color(t::MUTED),
                );
                let patterns: Vec<_> = self
                    .song
                    .patterns
                    .iter()
                    .map(|p| (p.id, p.name.clone()))
                    .collect();
                for (id, name) in patterns {
                    if ui.selectable_label(self.selected == id, name).clicked() {
                        self.selected = id;
                        self.publish()
                    }
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t::BG).inner_margin(24))
            .show(ctx, |ui| {
                egui::ScrollArea::both().show(ui, |ui| match self.screen {
                    Screen::Rack => self.rack(ui),
                    Screen::Roll => self.roll(ui),
                    Screen::Playlist => self.playlist(ui),
                });
            });
        if let Some((path, frame)) = &mut self.capture {
            let screenshot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = screenshot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::save_buffer(
                    &path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => eprintln!(
                        "Captured {}x{} to {}",
                        image.size[0],
                        image.size[1],
                        path.display()
                    ),
                    Err(e) => eprintln!("Capture failed: {e}"),
                };
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if *frame == 15 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            *frame += 1;
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        App {
            song: Song::default(),
            history: Vec::new(),
            future: Vec::new(),
            moving: None,
            snap: Snap::default(),
            instrument: 4,
            taps: Vec::new(),
            audio: None,
            selected: 1,
            clip: None,
            screen: Screen::Rack,
            slide: false,
            status: String::new(),
            drag: None,
            song_mode: false,
            velocity: 100,
            capture: None,
            importing: None,
            probe: None,
            #[cfg(test)]
            probe2: None,
            #[cfg(test)]
            probe3: None,
        }
    }
    fn frame(ctx: &egui::Context, app: &mut App, roll: bool, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .show(ctx, |ui| if roll { app.roll(ui) } else { app.rack(ui) });
        });
    }
    fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }
    fn secondary(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }
    fn roll_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        frame(ctx, app, true, events);
    }
    #[test]
    fn move_target_matches_preview_and_commit() {
        // grid 1/4: raw step 5 grabs 0 -> snapped 4 in both paths
        let a = move_target(MoveMode::Move, 0, 2, 48, 0, 5, 60, 4, false);
        assert_eq!(a, (4, 2, 60));
        let b = move_target(MoveMode::ResizeRight, 4, 2, 60, 0, 6, 62, 4, false);
        assert_eq!(b.0, 4);
        assert_eq!(b.1, (snap_step(6, 4, false)).saturating_sub(4) + 1);
        assert_eq!(b.2, 60); // resize never transposes, even if pointer drifts
        // alt bypass: raw values pass through
        assert_eq!(move_target(MoveMode::Move, 0, 2, 48, 0, 5, 60, 4, true), (5, 2, 60));
    }
    #[test]
    fn snap_step_quantizes_and_alt_bypasses() {
        assert_eq!(snap_step(5, 4, false), 4);
        assert_eq!(snap_step(6, 4, false), 8);
        assert_eq!(snap_step(6, 4, true), 6);
        assert_eq!(snap_step(15, 16, false), 16 - 1);
        assert_eq!(snap_step(7, 1, false), 7);
    }
    #[test]
    fn move_commits_single_checkpoint_and_undo_restores() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut app = app();
        roll_frame(&ctx, &mut app, vec![]);
        let rect = app.probe.unwrap();
        let cw = (rect.width() - 52.0) / 16.0;
        let y48 = rect.top() + 24.0 * 24.0 + 12.0;
        // create note steps 0..8 at C3 (pitch 48)
        let a = Pos2::new(rect.left() + 52.0 + cw * 0.5, y48);
        let b = Pos2::new(rect.left() + 52.0 + cw * 7.5, y48);
        roll_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(a), pointer(a, true)]);
        roll_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(b)]);
        roll_frame(&ctx, &mut app, vec![pointer(b, false)]);
        assert_eq!(app.song.patterns[0].notes.len(), 1);
        let h1 = app.history.len();
        // drag the note body to step 2
        let c = Pos2::new(rect.left() + 52.0 + cw * 0.5, y48);
        let d = Pos2::new(rect.left() + 52.0 + cw * 2.5, y48);
        roll_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(c), pointer(c, true)]);
        roll_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(d)]);
        roll_frame(&ctx, &mut app, vec![pointer(d, false)]);
        assert_eq!(app.song.patterns[0].notes[0].start, 2);
        assert_eq!(app.history.len(), h1 + 1);
        assert!(app.undo());
        assert_eq!(app.song.patterns[0].notes[0].start, 0);
    }
    #[test]
    fn right_click_miss_and_empty_velocity_leave_history_untouched() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut app = app();
        roll_frame(&ctx, &mut app, vec![]);
        let h0 = app.history.len();
        let rect = app.probe.unwrap();
        let cw = (rect.width() - 52.0) / 16.0;
        // empty cell at C4 (pitch 60 -> row 12), no notes in default song
        let e = Pos2::new(rect.left() + 52.0 + cw * 5.5, rect.top() + 12.0 * 24.0 + 12.0);
        roll_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(e), secondary(e, true)]);
        roll_frame(&ctx, &mut app, vec![secondary(e, false)]);
        assert_eq!(app.history.len(), h0);
        // velocity lane click where no note sits: below grid, step 5
        let v = Pos2::new(rect.left() + 52.0 + cw * 5.5, rect.bottom() + 8.0 + 24.0);
        roll_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(v), pointer(v, true)]);
        roll_frame(&ctx, &mut app, vec![pointer(v, false)]);
        assert_eq!(app.history.len(), h0);
    }
    #[test]
    fn rack_click_and_piano_drag_create_events() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut app = app();
        frame(&ctx, &mut app, false, vec![]);
        let step = app.probe.unwrap().center();
        frame(
            &ctx,
            &mut app,
            false,
            vec![egui::Event::PointerMoved(step), pointer(step, true)],
        );
        frame(&ctx, &mut app, false, vec![pointer(step, false)]);
        assert_ne!(app.song.patterns[0].drums[0][0], 0);
        frame(&ctx, &mut app, true, vec![]);
        let rect = app.probe.unwrap();
        let width = (rect.width() - 52.0) / 16.0;
        let start = Pos2::new(
            rect.left() + 52.0 + width * 0.5,
            rect.top() + 24.0 * 24.0 + 12.0,
        );
        let end = Pos2::new(start.x + width * 7.0, start.y);
        frame(
            &ctx,
            &mut app,
            true,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
        );
        frame(&ctx, &mut app, true, vec![egui::Event::PointerMoved(end)]);
        frame(&ctx, &mut app, true, vec![pointer(end, false)]);
        assert_eq!(app.song.patterns[0].notes.len(), 1);
        assert_eq!(
            (
                app.song.patterns[0].notes[0].start,
                app.song.patterns[0].notes[0].length,
                app.song.patterns[0].notes[0].pitch
            ),
            (0, 8, 48)
        );
        app.slide = true;
        let slide_start = Pos2::new(
            rect.left() + 52.0 + width * 3.5,
            rect.top() + 12.0 * 24.0 + 12.0,
        );
        let slide_end = Pos2::new(slide_start.x + width * 2.0, slide_start.y);
        frame(
            &ctx,
            &mut app,
            true,
            vec![
                egui::Event::PointerMoved(slide_start),
                pointer(slide_start, true),
            ],
        );
        frame(
            &ctx,
            &mut app,
            true,
            vec![egui::Event::PointerMoved(slide_end)],
        );
        frame(&ctx, &mut app, true, vec![pointer(slide_end, false)]);
        assert_eq!(app.song.patterns[0].slides.len(), 1);
        let slide = &app.song.patterns[0].slides[0];
        assert_eq!((slide.start, slide.length, slide.target), (3, 3, 60));
    }
    fn playlist_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.playlist(ui));
        });
    }
    fn playlist_click(ctx: &egui::Context, app: &mut App, pos: Pos2) {
        playlist_frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        );
        playlist_frame(ctx, app, vec![pointer(pos, false)]);
    }
    #[test]
    fn playlist_clicks_detach_second_occurrence() {
        let ctx = egui::Context::default();
        t::setup(&ctx);
        let mut app = app();
        playlist_frame(&ctx, &mut app, vec![]);
        let add = app.probe.unwrap().center();
        playlist_click(&ctx, &mut app, add);
        playlist_click(&ctx, &mut app, add);
        assert_eq!(app.song.clips.len(), 2);
        playlist_frame(&ctx, &mut app, vec![]);
        let second = app.probe3.unwrap().center();
        playlist_click(&ctx, &mut app, second);
        assert_eq!(app.clip, Some(1));
        playlist_frame(&ctx, &mut app, vec![]);
        let unique = app.probe2.unwrap().center();
        playlist_click(&ctx, &mut app, unique);
        assert_ne!(app.song.clips[0].pattern, app.song.clips[1].pattern);
        app.pattern_mut().drums[0][0] = 100;
        assert_eq!(app.song.patterns[0].drums[0][0], 0);
    }
}
