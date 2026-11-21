//! Demo / Showcase mode ("Для Демонстрації"): full-canvas read-only
//! presentation overlay with two recording-ready views:
//! 1. **Arrange**: entire arranged Playlist fitted to screen with playhead,
//!    section markers, repeat seams, note previews, and automation curves.
//! 2. **Falling notes**: notes, vocals (pitch contour + live lyrics), and
//!    percussion falling onto a play line and lit keyboard.
//!
//! Includes a deterministic frame-accurate video recorder (`ffmpeg` pipe with
//! PNG sequence fallback) and works in both Dark and Light themes without
//! mutating project history or `workspace.ron`.

use super::{App, kit};
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use keyframexl::sequence::{
    BAR, ClipNote, InstrumentKind, MAX_BARS, Pattern, STEP, Song, Tick, clip_notes, pattern_notes,
    seconds_per_step, song_max_note_len, steps, swing_offset,
};
use keyframexl::tempo::TempoTable;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

/// Active visual view in Demo mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShowView {
    Arrange,
    Notes,
}

/// Song timeline range shown and played in Demo mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShowFit {
    /// Whole song from bar 1 to `end_bar_tick()` (ignores `demo::build()`'s chorus loop).
    Song,
    /// The song's saved `loop_range`, if it had one when entering Demo mode.
    Loop,
}

/// Which channels are visible in the Falling notes view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShowChannels {
    All,
    Selected(u64),
}

/// Clock source driving Show playback and rendering.
#[derive(Clone, Debug)]
pub(crate) enum ShowClock {
    /// Follows `App::audio` when available, or a wall-clock fallback when no audio device exists.
    Live {
        fallback_playing: bool,
        fallback_frames: u64,
        last_time: Option<f64>,
    },
    /// Fixed audio frame (used by `--capture-show` and deterministic tests).
    Fixed(u64),
    /// Frame-stepped deterministic clock owned by an active video recording job.
    Recording(u64),
}

/// Background video recording state.
struct RecordJob {
    phase: RecordPhase,
    cancel: Arc<AtomicBool>,
}

enum RecordPhase {
    RenderingAudio {
        out_path: PathBuf,
        fps: u32,
        start_frame: u64,
        end_frame: u64,
        rx: mpsc::Receiver<Result<PathBuf, String>>,
    },
    Capturing {
        out_path: PathBuf,
        audio_wav: PathBuf,
        fps: u32,
        start_frame: u64,
        total_frames: u64,
        next_frame: u64,
        warmup: u8,
        in_flight: bool,
        expected_size: Option<[usize; 2]>,
        tx: Option<mpsc::SyncSender<Vec<u8>>>,
        done_rx: mpsc::Receiver<Result<String, String>>,
        started_at: std::time::Instant,
    },
    Done(String),
    Error(String),
}

/// Complete transient overlay state for Demo mode.
pub(crate) struct ShowState {
    pub view: ShowView,
    pub fit: ShowFit,
    pub channels: ShowChannels,
    pub hud: bool,
    pub lookahead_s: f32,
    pub fps: u32,
    pub fullscreen: bool,
    pub clock: ShowClock,
    record: Option<RecordJob>,
    // Snapshot restored verbatim on exit (never creates undo history or marks dirty).
    saved_song_mode: bool,
    saved_loop_range: Option<(Tick, Tick)>,
    saved_metronome: bool,
    saved_position: u64,
    saved_playing: bool,
    cursor_hidden: bool,
    was_hud_before_rec: bool,
    /// Optional frame limit for automated recording checks (`KFXL_SHOW_RECORD_LIMIT`).
    record_limit: Option<u64>,
}

impl ShowState {
    pub(crate) fn new(
        view: ShowView,
        saved_song_mode: bool,
        saved_loop_range: Option<(Tick, Tick)>,
        saved_metronome: bool,
        saved_position: u64,
        saved_playing: bool,
    ) -> Self {
        Self {
            view,
            fit: ShowFit::Song,
            channels: ShowChannels::All,
            hud: true,
            lookahead_s: 3.0,
            fps: 30,
            fullscreen: false,
            clock: ShowClock::Live {
                fallback_playing: false,
                fallback_frames: saved_position,
                last_time: None,
            },
            record: None,
            saved_song_mode,
            saved_loop_range,
            saved_metronome,
            saved_position,
            saved_playing,
            cursor_hidden: false,
            was_hud_before_rec: true,
            record_limit: std::env::var("KFXL_SHOW_RECORD_LIMIT")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&n| n > 0),
        }
    }

    pub(crate) fn is_recording(&self) -> bool {
        matches!(
            self.record.as_ref().map(|r| &r.phase),
            Some(RecordPhase::RenderingAudio { .. } | RecordPhase::Capturing { .. })
        )
    }

    fn cancel_recording(&mut self) {
        if let Some(job) = self.record.take() {
            job.cancel.store(true, Ordering::Relaxed);
            if let RecordPhase::Capturing { tx, audio_wav, .. } = job.phase {
                drop(tx);
                let _ = std::fs::remove_file(audio_wav);
            }
        }
        self.hud = self.was_hud_before_rec;
        if let ShowClock::Recording(f) = self.clock {
            self.clock = ShowClock::Live {
                fallback_playing: false,
                fallback_frames: f,
                last_time: None,
            };
        }
    }
}

/// Fitted geometry for the Arrange overview.
#[derive(Clone, Debug)]
pub(crate) struct ArrangeLayout {
    pub start_tick: Tick,
    pub end_tick: Tick,
    pub tracks: Vec<u32>,
    pub gutter_w: f32,
    pub grid: Rect,
    pub lane_h: f32,
    pub dense: bool,
}

impl ArrangeLayout {
    pub(crate) fn compute(song: &Song, rect: Rect, range: (Tick, Tick)) -> Self {
        let max_span = MAX_BARS as Tick * BAR;
        let start_tick = range.0.max(0);
        let end_tick = range.1.max(start_tick + BAR).min(start_tick + max_span);

        let mut tracks: Vec<u32> = song.clips.iter().map(|c| c.track).collect();
        tracks.sort_unstable();
        tracks.dedup();

        let ruler_h = if rect.height() < 160.0 { 22.0 } else { 30.0 };
        let grid_h = (rect.height() - ruler_h).max(0.0);
        let rows = tracks.len().max(1) as f32;
        let lane_h = if grid_h <= 0.0 {
            0.0
        } else {
            (grid_h / rows).clamp(4.0, 120.0)
        };
        let gutter_w = if rect.width() >= 640.0 && lane_h >= 16.0 && !tracks.is_empty() {
            112.0_f32.min(rect.width() * 0.16)
        } else {
            0.0
        };
        let grid = Rect::from_min_max(
            pos2(
                (rect.left() + gutter_w).min(rect.right()),
                (rect.top() + ruler_h).min(rect.bottom()),
            ),
            rect.max,
        );
        let bars = ((end_tick - start_tick) as f32 / BAR as f32).max(1.0);
        let bar_px = grid.width() / bars;
        let dense = bar_px < 10.0 || lane_h < 14.0 || tracks.len() > 36;
        Self {
            start_tick,
            end_tick,
            tracks,
            gutter_w,
            grid,
            lane_h,
            dense,
        }
    }

    #[inline]
    pub(crate) fn x_of(&self, tick: Tick) -> f32 {
        let span = (self.end_tick - self.start_tick).max(1) as f64;
        let frac = ((tick - self.start_tick) as f64 / span).clamp(0.0, 1.0) as f32;
        self.grid.left() + frac * self.grid.width()
    }

    #[inline]
    pub(crate) fn tick_at(&self, x: f32) -> Tick {
        if self.grid.width() <= 0.0 {
            return self.start_tick;
        }
        let frac = ((x - self.grid.left()) / self.grid.width()).clamp(0.0, 1.0) as f64;
        let span = (self.end_tick - self.start_tick).max(1) as f64;
        self.start_tick + (frac * span).round() as Tick
    }

    pub(crate) fn track_y(&self, track: u32) -> Option<(f32, f32)> {
        let idx = self.tracks.binary_search(&track).ok()?;
        let y0 = self.grid.top() + idx as f32 * self.lane_h;
        let y1 = (y0 + self.lane_h).min(self.grid.bottom());
        (y1 > y0).then_some((y0, y1))
    }
}

/// Geometry for the Falling notes view (optional Rhythm strip + Piano waterfall + Keyboard).
#[derive(Clone, Debug)]
pub(crate) struct FallLayout {
    pub lo_pitch: u8,
    pub hi_pitch: u8,
    pub perc_channels: Vec<u64>,
    pub rhythm_rect: Rect,
    pub stage_rect: Rect,
    pub keyboard_rect: Rect,
    pub rhythm_pad_rect: Rect,
    pub play_y: f32,
    pub key_w: f32,
}

impl FallLayout {
    pub(crate) fn compute(song: &Song, rect: Rect, channels: ShowChannels) -> Self {
        let perc_channels = detect_percussion_channels(song, channels);
        let (lo_pitch, hi_pitch) = stable_pitch_range(song, channels, &perc_channels);

        let key_h = if rect.height() < 180.0 {
            (rect.height() * 0.22).clamp(18.0, 40.0)
        } else {
            (rect.height() * 0.13).clamp(52.0, 86.0)
        };
        let play_y = (rect.bottom() - key_h).max(rect.top());

        // Allocate a compact left-side Rhythm Strip only when unpitched percussion exists
        // and the window has room.
        let rhythm_w = if !perc_channels.is_empty() && rect.width() >= 360.0 {
            let per_col = 36.0_f32;
            let want = perc_channels.len() as f32 * per_col + 12.0;
            want.clamp(56.0, (rect.width() * 0.22).max(56.0))
        } else {
            0.0
        };

        let rhythm_rect = Rect::from_min_max(rect.left_top(), pos2(rect.left() + rhythm_w, play_y));
        let rhythm_pad_rect = Rect::from_min_max(
            pos2(rect.left(), play_y),
            pos2(rect.left() + rhythm_w, rect.bottom()),
        );
        let keys_left = (rect.left() + rhythm_w).min(rect.right());
        let stage_rect =
            Rect::from_min_max(pos2(keys_left, rect.top()), pos2(rect.right(), play_y));
        let keyboard_rect =
            Rect::from_min_max(pos2(keys_left, play_y), pos2(rect.right(), rect.bottom()));

        let count = (hi_pitch.saturating_sub(lo_pitch) as usize + 1).max(1) as f32;
        let key_w = (stage_rect.width() / count).max(1.0);

        Self {
            lo_pitch,
            hi_pitch,
            perc_channels,
            rhythm_rect,
            stage_rect,
            keyboard_rect,
            rhythm_pad_rect,
            play_y,
            key_w,
        }
    }

    #[inline]
    pub(crate) fn pitch_x(&self, pitch: u8) -> Option<(f32, f32)> {
        if pitch < self.lo_pitch || pitch > self.hi_pitch {
            return None;
        }
        let i = (pitch - self.lo_pitch) as f32;
        let x0 = self.stage_rect.left() + i * self.key_w;
        Some((x0, x0 + self.key_w))
    }

    #[inline]
    pub(crate) fn perc_x(&self, inst_id: u64) -> Option<(f32, f32)> {
        if self.rhythm_rect.width() <= 2.0 || self.perc_channels.is_empty() {
            return None;
        }
        let idx = self.perc_channels.iter().position(|&id| id == inst_id)?;
        let inner_l = self.rhythm_rect.left() + 6.0;
        let inner_w = (self.rhythm_rect.width() - 12.0).max(1.0);
        let col_w = inner_w / self.perc_channels.len() as f32;
        let x0 = inner_l + idx as f32 * col_w;
        Some((x0, x0 + col_w))
    }

    /// Exact Y coordinate on screen for an event at `frame`.
    /// When `frame == now_frame`, returns `play_y`. Future frames sit above `play_y`.
    #[inline]
    pub(crate) fn y_of_frame(&self, frame: f64, now_frame: f64, lookahead_frames: f64) -> f32 {
        let span = lookahead_frames.max(1.0);
        let px_per_frame = self.stage_rect.height() as f64 / span;
        (self.play_y as f64 - (frame - now_frame) * px_per_frame) as f32
    }
}

/// One falling note ready to paint in the Falling notes view.
#[derive(Clone, Debug)]
pub(crate) struct FallNote {
    pub clip_note: ClipNote,
    pub start_frame: f64,
    pub end_frame: f64,
    pub color: Color32,
    pub is_vocal: bool,
    pub is_perc: bool,
    pub active: bool,
}

/// Deterministic channel color from KeyframeXL's palette (`t::*`).
/// Vocals always receive the distinct vocal accent (`t::HUE_PINK`).
pub(crate) fn channel_color(song: &Song, inst_id: u64) -> Color32 {
    let Some(idx) = song.instruments.iter().position(|i| i.id == inst_id) else {
        return t::enc_white();
    };
    let inst = &song.instruments[idx];
    if matches!(inst.kind, InstrumentKind::Vocal { .. }) {
        return t::HUE_PINK;
    }
    let palette = [
        t::enc_blue(),
        t::enc_green(),
        t::enc_orange(),
        t::HUE_CYAN,
        t::wait(),
        t::HUE_PURPLE,
        t::HUE_LIME,
        t::HUE_ROSE,
        t::HUE_TEAL,
        t::HUE_INDIGO,
    ];
    palette[idx % palette.len()]
}

/// Unpitched percussion channels: `Drum` always, plus `Forge` channels whose notes
/// all sit on a single trigger pitch (e.g. C4 / 60).
fn detect_percussion_channels(song: &Song, channels: ShowChannels) -> Vec<u64> {
    let mut out = Vec::new();
    for inst in &song.instruments {
        if inst.muted {
            continue;
        }
        if let ShowChannels::Selected(id) = channels
            && inst.id != id
        {
            continue;
        }
        match inst.kind {
            InstrumentKind::Drum { .. } => out.push(inst.id),
            InstrumentKind::Forge { .. } => {
                let mut first_pitch: Option<u8> = None;
                let mut multi = false;
                for p in &song.patterns {
                    for n in &p.notes {
                        if n.instrument == inst.id && !n.muted {
                            match first_pitch {
                                None => first_pitch = Some(n.pitch),
                                Some(prev) if prev != n.pitch => {
                                    multi = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    if multi {
                        break;
                    }
                }
                if first_pitch.is_some() && !multi {
                    out.push(inst.id);
                }
            }
            _ => {}
        }
    }
    out
}

/// Stable octave-snapped pitch range across the whole song so the keyboard never
/// jumps horizontally between frames.
fn stable_pitch_range(song: &Song, channels: ShowChannels, perc: &[u64]) -> (u8, u8) {
    let mut lo = u8::MAX;
    let mut hi = 0u8;
    for p in &song.patterns {
        for n in &p.notes {
            if n.muted || perc.contains(&n.instrument) {
                continue;
            }
            if let ShowChannels::Selected(id) = channels
                && n.instrument != id
            {
                continue;
            }
            if song.instrument(n.instrument).is_none_or(|i| i.muted) {
                continue;
            }
            lo = lo.min(n.pitch);
            hi = hi.max(n.pitch);
        }
    }
    if lo > hi {
        return (36, 84); // C2..C6 fallback
    }
    // Snap down to C and up to C with comfortable breathing room, minimum 2 octaves.
    let lo_c = ((lo.saturating_sub(2)) / 12) * 12;
    let mut hi_c = ((hi.saturating_add(2)) / 12 + 1) * 12;
    if hi_c.saturating_sub(lo_c) < 24 {
        hi_c = (lo_c + 24).min(108);
    }
    (lo_c.clamp(12, 84), hi_c.clamp(36, 108))
}

#[inline]
fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

#[inline]
fn tick_to_frame(song: &Song, table: Option<&TempoTable>, tick: Tick, rate: u32) -> f64 {
    match table {
        Some(m) => m.frames_at(tick),
        None => steps(tick) * seconds_per_step(song.tempo) * rate as f64,
    }
}

#[inline]
fn frame_to_tick(song: &Song, table: Option<&TempoTable>, frame: f64, rate: u32) -> Tick {
    let f = frame.max(0.0);
    match table {
        Some(m) => m.tick_at_frame(f.round() as u64),
        None => {
            let fps = seconds_per_step(song.tempo) * rate as f64;
            if fps <= 0.0 {
                0
            } else {
                (f / fps * STEP as f64).round() as Tick
            }
        }
    }
}

/// Collect all notes visible in `[now_frame, now_frame + lookahead_frames]` with
/// exact tempo map, swing alignment, and seamless loop-wrap lookahead.
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_fall_window(
    song: &Song,
    song_mode: bool,
    selected_pattern: u64,
    table: Option<&TempoTable>,
    rate: u32,
    loop_bounds: (u64, u64),
    now_frame: f64,
    lookahead_frames: f64,
    channels: ShowChannels,
    perc_channels: &[u64],
) -> Vec<FallNote> {
    let (loop_start, loop_end) = (
        loop_bounds.0 as f64,
        loop_bounds.1.max(loop_bounds.0 + 1) as f64,
    );
    let loop_len = (loop_end - loop_start).max(1.0);
    let max_lookback = song_max_note_len(song);

    // Split `[now_frame, now_frame + lookahead_frames]` across loop boundaries so
    // upcoming notes from the next loop pass fall in smoothly from the top.
    let mut segments: Vec<(f64, f64, f64)> = Vec::new(); // (seg_start, seg_end, frame_shift)
    let mut cursor = now_frame;
    let mut remaining = lookahead_frames.max(1.0);
    let mut shift = 0.0;
    for _ in 0..8 {
        if remaining <= 0.5 {
            break;
        }
        let seg_end = if cursor < loop_end {
            (cursor + remaining).min(loop_end)
        } else {
            cursor + remaining
        };
        if seg_end > cursor {
            segments.push((cursor, seg_end, shift));
            let consumed = seg_end - cursor;
            remaining -= consumed;
            shift += consumed;
            cursor = loop_start;
            // After wrapping, `shift` maps `loop_start` to `now_frame + shift_so_far`.
        } else {
            break;
        }
    }

    let mut out = Vec::new();
    for (seg_idx, (seg_a, seg_b, elapsed_before)) in segments.into_iter().enumerate() {
        let t0 = frame_to_tick(song, table, seg_a, rate);
        let t1 = frame_to_tick(song, table, seg_b, rate).max(t0 + 1);
        let raw = if song_mode && !song.clips.is_empty() {
            clip_notes(song, t0, t1, max_lookback)
        } else {
            pattern_notes(song, selected_pattern, t0, t1)
        };
        for cn in raw {
            if let ShowChannels::Selected(id) = channels
                && cn.note.instrument != id
            {
                continue;
            }
            let bpm = table.map_or(song.tempo, |m| m.bpm_at(cn.start));
            let sps = seconds_per_step(bpm);
            let swing_f = swing_offset(cn.local_tick, sps, song.swing) * rate as f64;
            let raw_start = tick_to_frame(song, table, cn.start, rate) + swing_f;
            let raw_end = (tick_to_frame(song, table, cn.end, rate) + swing_f).max(raw_start + 1.0);

            // For wrapped segments (seg_idx > 0), ignore notes that started before loop_start
            // (engine does not retrigger pre-loop notes on loop wrap).
            if seg_idx > 0 && raw_start < seg_a - 0.5 {
                continue;
            }
            let delta_offset = if seg_idx == 0 {
                0.0
            } else {
                (now_frame + elapsed_before) - seg_a
            };
            let start_frame = raw_start + delta_offset;
            let end_frame = raw_end + delta_offset;
            if end_frame <= now_frame - 1.0 || start_frame >= now_frame + lookahead_frames + 1.0 {
                continue;
            }
            let is_vocal = song
                .instrument(cn.note.instrument)
                .is_some_and(|i| matches!(i.kind, InstrumentKind::Vocal { .. }));
            let is_perc = perc_channels.contains(&cn.note.instrument);
            let color = channel_color(song, cn.note.instrument);
            let active = start_frame <= now_frame && now_frame < end_frame;
            out.push(FallNote {
                clip_note: cn,
                start_frame,
                end_frame,
                color,
                is_vocal,
                is_perc,
                active,
            });
        }
        // Prevent infinite duplicates on degenerate zero-length loops.
        if loop_len < 1.0 {
            break;
        }
    }
    out
}

impl App {
    /// Enter Demo / Showcase mode without mutating project history or dirty flag.
    pub(crate) fn enter_show(&mut self, view: ShowView, ctx: Option<&egui::Context>) {
        if self.show.is_some() {
            if let Some(s) = &mut self.show {
                s.view = view;
            }
            return;
        }
        let saved_song_mode = self.song_mode;
        let saved_loop_range = self.song.loop_range;
        let saved_metronome = self
            .audio
            .as_ref()
            .map_or(self.settings.general.metronome, |a| {
                a.metronome.load(Ordering::Relaxed)
            });
        let saved_position = self.position_frames();
        let saved_playing = self.is_playing();

        let state = ShowState::new(
            view,
            saved_song_mode,
            saved_loop_range,
            saved_metronome,
            saved_position,
            saved_playing,
        );

        // Switch to Song mode when the project has playlist clips, and clear the
        // temporary chorus loop range (e.g. demo song's bars 12..20) so full-song
        // playback works out of the box without marking the project dirty.
        if !self.song.clips.is_empty() {
            self.song_mode = true;
            if let Some(a) = &self.audio {
                a.song_mode.store(true, Ordering::Relaxed);
            }
        }
        if self.song.loop_range.is_some() {
            self.song.loop_range = None;
            self.publish();
        }
        if let Some(a) = &self.audio {
            a.metronome.store(false, Ordering::Relaxed);
            a.count_in.store(0, Ordering::Relaxed);
            a.position.store(0, Ordering::Relaxed);
        }
        self.waiting_input = false;
        self.welcome_open = false;
        self.show = Some(state);
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    }

    /// Exit Demo mode and restore every transport/song/viewport setting verbatim.
    pub(crate) fn exit_show(&mut self, ctx: Option<&egui::Context>) {
        let Some(mut st) = self.show.take() else {
            return;
        };
        st.cancel_recording();
        if st.cursor_hidden
            && let Some(ctx) = ctx
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
        }
        if st.fullscreen
            && let Some(ctx) = ctx
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        let loop_changed = self.song.loop_range != st.saved_loop_range;
        self.song.loop_range = st.saved_loop_range;
        self.song_mode = st.saved_song_mode;
        if let Some(a) = &self.audio {
            a.song_mode.store(st.saved_song_mode, Ordering::Relaxed);
            a.metronome.store(st.saved_metronome, Ordering::Relaxed);
            a.playing.store(st.saved_playing, Ordering::Relaxed);
            a.position.store(st.saved_position, Ordering::Relaxed);
        }
        if loop_changed {
            self.publish();
        }
    }

    /// Stop any active show recording on app shutdown.
    pub(crate) fn shutdown_show(&mut self) {
        if let Some(st) = &mut self.show {
            st.cancel_recording();
        }
    }

    fn show_apply_fit(&mut self, fit: ShowFit) {
        let Some(st) = &mut self.show else { return };
        let effective = match fit {
            ShowFit::Loop if st.saved_loop_range.is_some() => ShowFit::Loop,
            _ => ShowFit::Song,
        };
        st.fit = effective;
        let target_loop = match effective {
            ShowFit::Song => None,
            ShowFit::Loop => st.saved_loop_range,
        };
        if self.song.loop_range != target_loop {
            self.song.loop_range = target_loop;
            self.publish();
        }
        let (start_f, _) = self.loop_bounds();
        self.show_seek_frame(start_f);
    }

    fn show_range_ticks(&self) -> (Tick, Tick) {
        if self.song_mode && !self.song.clips.is_empty() {
            if let Some((a, b)) = self.song.loop_range
                && b > a
            {
                return (a, b);
            }
            let end = self.song.end_bar_tick().max(BAR);
            (0, end)
        } else {
            let plen = self.pattern().length.max(BAR);
            (0, plen)
        }
    }

    fn show_is_playing(&self) -> bool {
        let Some(st) = &self.show else {
            return false;
        };
        match st.clock {
            ShowClock::Live {
                fallback_playing, ..
            } => self
                .audio
                .as_ref()
                .map_or(fallback_playing, |a| a.playing.load(Ordering::Relaxed)),
            ShowClock::Fixed(_) => false,
            ShowClock::Recording(_) => true,
        }
    }

    fn show_play_pause(&mut self) {
        if self.show.as_ref().is_some_and(|s| s.is_recording()) {
            return;
        }
        let (start, end) = self.loop_bounds();
        if let Some(a) = &self.audio {
            let on = !a.playing.load(Ordering::Relaxed);
            if on {
                let at = keyframexl::sequence::wrap_frame(
                    a.position.load(Ordering::Relaxed),
                    (start, end),
                );
                if !(start..end).contains(&at) {
                    a.position.store(start, Ordering::Relaxed);
                }
            }
            a.count_in.store(0, Ordering::Relaxed);
            a.playing.store(on, Ordering::Relaxed);
        } else if let Some(st) = &mut self.show {
            match &mut st.clock {
                ShowClock::Live {
                    fallback_playing,
                    last_time,
                    ..
                } => {
                    *fallback_playing = !*fallback_playing;
                    *last_time = None;
                }
                ShowClock::Fixed(f) => {
                    st.clock = ShowClock::Live {
                        fallback_playing: true,
                        fallback_frames: *f,
                        last_time: None,
                    };
                }
                ShowClock::Recording(_) => {}
            }
        }
    }

    fn show_stop(&mut self) {
        if self.show.as_ref().is_some_and(|s| s.is_recording()) {
            return;
        }
        let (start, _) = self.loop_bounds();
        if let Some(a) = &self.audio {
            a.count_in.store(0, Ordering::Relaxed);
            a.playing.store(false, Ordering::Relaxed);
            a.position.store(start, Ordering::Relaxed);
        }
        if let Some(st) = &mut self.show {
            st.clock = ShowClock::Live {
                fallback_playing: false,
                fallback_frames: start,
                last_time: None,
            };
        }
    }

    fn show_seek_frame(&mut self, frame: u64) {
        if let Some(a) = &self.audio {
            a.position.store(frame, Ordering::Relaxed);
        }
        if let Some(st) = &mut self.show {
            match &mut st.clock {
                ShowClock::Live {
                    fallback_frames,
                    last_time,
                    ..
                } => {
                    *fallback_frames = frame;
                    *last_time = None;
                }
                ShowClock::Fixed(f) => *f = frame,
                ShowClock::Recording(_) => {}
            }
        }
    }

    fn show_seek_tick(&mut self, tick: Tick) {
        let clamped = tick.max(0);
        let f = self.tick_frame_at(clamped);
        self.show_seek_frame(f);
    }

    fn show_now_frame(&mut self, ctx: &egui::Context) -> u64 {
        let bounds = self.loop_bounds();
        let rate = self.rate() as f64;
        let now_s = ctx.input(|i| i.time);
        let Some(st) = &mut self.show else {
            return self.loop_pos();
        };
        match &mut st.clock {
            ShowClock::Fixed(f) => *f,
            ShowClock::Recording(f) => *f,
            ShowClock::Live {
                fallback_playing,
                fallback_frames,
                last_time,
            } => {
                if let Some(a) = &self.audio {
                    let pos = a.position.load(Ordering::Relaxed);
                    keyframexl::sequence::wrap_frame(pos, bounds)
                } else {
                    if *fallback_playing {
                        if let Some(prev) = *last_time {
                            let dt = (now_s - prev).clamp(0.0, 0.25);
                            *fallback_frames =
                                fallback_frames.saturating_add((dt * rate).round() as u64);
                        }
                        *last_time = Some(now_s);
                    } else {
                        *last_time = None;
                    }
                    keyframexl::sequence::wrap_frame(*fallback_frames, bounds)
                }
            }
        }
    }

    /// Start deterministic video recording into `out_path`.
    fn start_show_record(&mut self, out_path: PathBuf, ctx: &egui::Context) {
        let (start_frame, end_frame) = self.loop_bounds();
        let fps = self.show.as_ref().map_or(30, |s| s.fps).clamp(15, 60);
        let song = self.song.clone();
        let selected = self.selected;
        let song_mode = self.song_mode;
        let range = if song_mode && !song.clips.is_empty() {
            song.loop_range
        } else {
            None
        };
        let rate = self.rate();

        // Pause live audio during offline recording so output stays silent and deterministic.
        if let Some(a) = &self.audio {
            a.playing.store(false, Ordering::Relaxed);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = cancel.clone();
        let ctx_bg = ctx.clone();
        let (tx, rx) = mpsc::channel();
        let wav_path = std::env::temp_dir().join(format!(
            "kfxl-show-audio-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        ));
        let wav_out = wav_path.clone();
        let tr = crate::i18n::strings(self.settings.general.language);

        std::thread::spawn(move || {
            let mut noop = |_: f32| {};
            let rendered = keyframexl::render::render_range(
                &song,
                selected,
                song_mode,
                range,
                keyframexl::render::Tail::Cut,
                rate,
                &keyframexl::render::Plugins::default(),
                &cancel_bg,
                &mut noop,
            );
            let res = match rendered {
                None => Err(tr.show_rec_cancelled.into()),
                Some(frames) => keyframexl::render::write_wav_at(
                    &wav_out,
                    &frames,
                    keyframexl::render::Format::Wav16,
                    false,
                    rate,
                )
                .map(|()| wav_out)
                .map_err(|e| {
                    crate::i18n::fill(tr.show_rec_audio_write_err, &[("err", &e.to_string())])
                }),
            };
            let _ = tx.send(res);
            ctx_bg.request_repaint();
        });

        if let Some(st) = &mut self.show {
            st.was_hud_before_rec = st.hud;
            st.record = Some(RecordJob {
                phase: RecordPhase::RenderingAudio {
                    out_path,
                    fps,
                    start_frame,
                    end_frame,
                    rx,
                },
                cancel,
            });
        }
    }

    fn tick_show_record(&mut self, ctx: &egui::Context) {
        let tr = crate::i18n::strings(self.settings.general.language);
        let rate = self.rate() as u64;
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });

        let Some(st) = &mut self.show else { return };
        let Some(job) = &mut st.record else { return };

        let mut transition: Option<RecordPhase> = None;
        let mut set_clock: Option<ShowClock> = None;
        let mut hide_hud_cursor = false;
        let mut restore_hud_cursor = false;
        let mut request_shot = false;
        let mut request_repaint = false;

        match &mut job.phase {
            RecordPhase::RenderingAudio {
                out_path,
                fps,
                start_frame,
                end_frame,
                rx,
            } => {
                request_repaint = true;
                if let Ok(res) = rx.try_recv() {
                    match res {
                        Ok(audio_wav) => {
                            let span_frames = end_frame.saturating_sub(*start_frame).max(1);
                            let mut total_frames = ((span_frames as f64 / rate.max(1) as f64)
                                * (*fps as f64))
                                .ceil() as u64;
                            total_frames = total_frames.max(1);
                            if let Some(limit) = st.record_limit {
                                total_frames = total_frames.min(limit);
                            }
                            // Start the encoder lazily on the first captured frame once physical size is known.
                            let (_done_tx, done_rx) = mpsc::channel::<Result<String, String>>();
                            hide_hud_cursor = true;
                            set_clock = Some(ShowClock::Recording(*start_frame));
                            transition = Some(RecordPhase::Capturing {
                                out_path: out_path.clone(),
                                audio_wav,
                                fps: *fps,
                                start_frame: *start_frame,
                                total_frames,
                                next_frame: 0,
                                warmup: 3,
                                in_flight: false,
                                expected_size: None,
                                tx: None,
                                done_rx,
                                started_at: std::time::Instant::now(),
                            });
                        }
                        Err(e) => {
                            restore_hud_cursor = true;
                            transition = Some(RecordPhase::Error(e));
                        }
                    }
                }
            }
            RecordPhase::Capturing {
                out_path,
                audio_wav,
                fps,
                start_frame,
                total_frames,
                next_frame,
                warmup,
                in_flight,
                expected_size,
                tx,
                done_rx,
                started_at,
            } => {
                request_repaint = true;
                if *warmup > 0 {
                    *warmup -= 1;
                    set_clock = Some(ShowClock::Recording(*start_frame));
                } else if *in_flight {
                    if let Some(image) = shot {
                        *in_flight = false;
                        let sz = image.size;
                        if let Some(exp) = *expected_size {
                            if exp != sz {
                                drop(tx.take());
                                let _ = std::fs::remove_file(&*audio_wav);
                                restore_hud_cursor = true;
                                transition =
                                    Some(RecordPhase::Error(tr.show_rec_resized_err.into()));
                            }
                        } else {
                            *expected_size = Some(sz);
                            let (ftx, frx) = mpsc::sync_channel::<Vec<u8>>(4);
                            let (dtx, drx) = mpsc::channel();
                            *tx = Some(ftx);
                            *done_rx = drx;
                            spawn_video_encoder(
                                out_path.clone(),
                                audio_wav.clone(),
                                sz[0] as u32,
                                sz[1] as u32,
                                *fps,
                                frx,
                                dtx,
                                job.cancel.clone(),
                                ctx.clone(),
                                tr,
                            );
                        }
                        if transition.is_none() {
                            let rgba: Vec<u8> =
                                image.pixels.iter().flat_map(|p| p.to_array()).collect();
                            if let Some(sender) = tx.as_ref()
                                && sender.send(rgba).is_err()
                            {
                                let msg = done_rx
                                    .try_recv()
                                    .ok()
                                    .and_then(Result::err)
                                    .unwrap_or_else(|| tr.show_rec_encoder_stopped.into());
                                drop(tx.take());
                                restore_hud_cursor = true;
                                transition = Some(RecordPhase::Error(msg));
                            } else {
                                *next_frame += 1;
                                if *next_frame >= *total_frames {
                                    // Close channel to flush encoder.
                                    drop(tx.take());
                                } else {
                                    let f = *start_frame
                                        + ((*next_frame as f64 * rate as f64) / *fps as f64).round()
                                            as u64;
                                    set_clock = Some(ShowClock::Recording(f));
                                }
                            }
                        }
                    } else if started_at.elapsed().as_secs() > 15 && *next_frame == 0 {
                        drop(tx.take());
                        let _ = std::fs::remove_file(&*audio_wav);
                        restore_hud_cursor = true;
                        transition =
                            Some(RecordPhase::Error(tr.show_rec_screenshot_timeout.into()));
                    }
                } else if *next_frame < *total_frames {
                    let f = *start_frame
                        + ((*next_frame as f64 * rate as f64) / *fps as f64).round() as u64;
                    set_clock = Some(ShowClock::Recording(f));
                    *in_flight = true;
                    request_shot = true;
                } else if let Ok(res) = done_rx.try_recv() {
                    let _ = std::fs::remove_file(&*audio_wav);
                    restore_hud_cursor = true;
                    transition = Some(match res {
                        Ok(msg) => RecordPhase::Done(msg),
                        Err(e) => RecordPhase::Error(e),
                    });
                }
            }
            RecordPhase::Done(_) | RecordPhase::Error(_) => {}
        }

        if let Some(c) = set_clock {
            st.clock = c;
        }
        if hide_hud_cursor {
            st.hud = false;
            if !st.cursor_hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(false));
                st.cursor_hidden = true;
            }
        }
        if restore_hud_cursor {
            st.hud = st.was_hud_before_rec;
            if st.cursor_hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
                st.cursor_hidden = false;
            }
            if let ShowClock::Recording(f) = st.clock {
                st.clock = ShowClock::Live {
                    fallback_playing: false,
                    fallback_frames: f,
                    last_time: None,
                };
            }
        }
        if let Some(next_phase) = transition
            && let Some(job) = &mut st.record
        {
            job.phase = next_phase;
        }
        if request_shot {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if request_repaint {
            ctx.request_repaint();
        }
    }

    /// Render the full-screen Demo mode overlay. Returns `true` when Demo mode
    /// owns the frame (caller must skip normal menus, deck, browser, and docks).
    pub(crate) fn show_overlay(&mut self, ctx: &egui::Context) -> bool {
        if self.show.is_none() {
            return false;
        }

        self.tick_show_record(ctx);
        let recording = self.show.as_ref().is_some_and(|s| s.is_recording());

        // Keyboard shortcuts dedicated to Demo mode.
        let mut exit_now = false;
        let mut toggle_play = false;
        let mut stop_now = false;
        let mut toggle_view = false;
        let mut toggle_hud = false;
        let mut toggle_full = false;
        let mut toggle_chan = false;
        let mut toggle_fit = false;
        let mut toggle_rec = false;
        let mut seek_bars: i64 = 0;
        let mut seek_home = false;

        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                exit_now = true;
            }
            if !recording {
                if i.key_pressed(egui::Key::Space) {
                    if i.modifiers.shift {
                        stop_now = true;
                    } else {
                        toggle_play = true;
                    }
                }
                if i.key_pressed(egui::Key::Tab) {
                    toggle_view = true;
                }
                if i.key_pressed(egui::Key::H) {
                    toggle_hud = true;
                }
                if i.key_pressed(egui::Key::F) {
                    toggle_full = true;
                }
                if i.key_pressed(egui::Key::C) {
                    toggle_chan = true;
                }
                if i.key_pressed(egui::Key::L) {
                    toggle_fit = true;
                }
                if i.key_pressed(egui::Key::R) {
                    toggle_rec = true;
                }
                if i.key_pressed(egui::Key::Home) {
                    seek_home = true;
                }
                let step = if i.modifiers.shift { 4 } else { 1 };
                if i.key_pressed(egui::Key::ArrowLeft) {
                    seek_bars -= step;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    seek_bars += step;
                }
            }
        });

        if exit_now {
            if recording {
                if let Some(st) = &mut self.show {
                    st.cancel_recording();
                }
                if let Some(st) = &mut self.show
                    && st.cursor_hidden
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
                    st.cursor_hidden = false;
                }
            } else {
                self.exit_show(Some(ctx));
                return true;
            }
        }
        if toggle_play {
            self.show_play_pause();
        }
        if stop_now {
            self.show_stop();
        }
        if toggle_view && let Some(st) = &mut self.show {
            st.view = match st.view {
                ShowView::Arrange => ShowView::Notes,
                ShowView::Notes => ShowView::Arrange,
            };
        }
        if toggle_hud && let Some(st) = &mut self.show {
            st.hud = !st.hud;
        }
        if toggle_full && let Some(st) = &mut self.show {
            st.fullscreen = !st.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(st.fullscreen));
        }
        if toggle_chan && let Some(st) = &mut self.show {
            st.channels = match st.channels {
                ShowChannels::All => ShowChannels::Selected(self.instrument),
                ShowChannels::Selected(_) => ShowChannels::All,
            };
        }
        if toggle_fit && let Some(fit) = self.show.as_ref().map(|s| s.fit) {
            let next = match fit {
                ShowFit::Song => ShowFit::Loop,
                ShowFit::Loop => ShowFit::Song,
            };
            self.show_apply_fit(next);
        }
        if seek_home {
            let (a, _) = self.show_range_ticks();
            self.show_seek_tick(a);
        }
        if seek_bars != 0 {
            let cur_frame = self.show_now_frame(ctx);
            let cur_tick = self.frame_tick_at(cur_frame);
            let (a, b) = self.show_range_ticks();
            let target = (cur_tick + seek_bars * BAR).clamp(a, b.saturating_sub(1).max(a));
            self.show_seek_tick(target);
        }
        if toggle_rec {
            self.trigger_show_record_dialog(ctx);
        }

        let now_frame = self.show_now_frame(ctx);
        let table = self.tempo_table();
        let rate = self.rate();
        let now_tick = frame_to_tick(&self.song, table.as_ref(), now_frame as f64, rate);
        let bpm_now = table
            .as_ref()
            .map_or(self.song.tempo, |m| m.bpm_at(now_tick));

        let show_hud = self.show.as_ref().is_some_and(|s| s.hud) && !recording;
        if show_hud {
            egui::TopBottomPanel::top("show-hud")
                .exact_height(42.0)
                .frame(egui::Frame::new().fill(t::surface()))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    self.paint_show_hud(ui, now_tick, bpm_now);
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t::sunken()))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let view = self.show.as_ref().map_or(ShowView::Arrange, |s| s.view);
                match view {
                    ShowView::Arrange => {
                        self.paint_show_arrange(ui, rect, now_tick);
                    }
                    ShowView::Notes => {
                        self.paint_show_fall(ui, rect, now_frame as f64, table.as_ref(), rate);
                    }
                }
                self.paint_show_record_status(ui, rect);
            });

        self.capture_frame(ctx);
        let fast = self.show_is_playing() || self.show.as_ref().is_some_and(|s| s.is_recording());
        ctx.request_repaint_after(std::time::Duration::from_millis(if fast {
            16
        } else {
            120
        }));
        true
    }

    fn trigger_show_record_dialog(&mut self, ctx: &egui::Context) {
        if let Ok(env_path) = std::env::var("KFXL_SHOW_RECORD") {
            self.start_show_record(PathBuf::from(env_path), ctx);
            return;
        }
        let stem = self
            .song
            .info
            .title
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let file = if stem.is_empty() {
            "keyframexl-demo.mp4".to_string()
        } else {
            format!("{stem}-demo.mp4")
        };
        let tr = crate::i18n::strings(self.settings.general.language);
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&file)
            .add_filter(tr.show_mp4_filter, &["mp4"])
            .save_file()
        {
            self.start_show_record(path, ctx);
        }
    }

    fn paint_show_hud(&mut self, ui: &mut Ui, now_tick: Tick, bpm_now: f64) {
        let tr = crate::i18n::strings(self.settings.general.language);
        let rect = ui.max_rect();
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0_f32, t::edge()),
        );

        let view = self.show.as_ref().map_or(ShowView::Arrange, |s| s.view);
        let fit = self.show.as_ref().map_or(ShowFit::Song, |s| s.fit);
        let has_saved_loop = self
            .show
            .as_ref()
            .is_some_and(|s| s.saved_loop_range.is_some());
        let channels = self.show.as_ref().map_or(ShowChannels::All, |s| s.channels);
        let playing = self.show_is_playing();
        let fps = self.show.as_ref().map_or(30, |s| s.fps);
        let fullscreen = self.show.as_ref().is_some_and(|s| s.fullscreen);

        let mut next_view = None;
        let mut do_play = false;
        let mut do_stop = false;
        let mut next_fit = None;
        let mut next_channels = None;
        let mut toggle_fps = false;
        let mut do_record = false;
        let mut do_hide_hud = false;
        let mut do_full = false;
        let mut do_exit = false;

        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            // Segmented view switch: Arrange | Falling notes
            if hud_tab(ui, tr.show_arrange, view == ShowView::Arrange) {
                next_view = Some(ShowView::Arrange);
            }
            if hud_tab(ui, tr.show_notes, view == ShowView::Notes) {
                next_view = Some(ShowView::Notes);
            }

            ui.add_space(10.0);
            hud_divider(ui);
            ui.add_space(8.0);

            // Transport
            if hud_btn(
                ui,
                if playing { tr.show_pause } else { tr.show_play },
                playing,
            ) {
                do_play = true;
            }
            if hud_btn(ui, tr.show_stop, false) {
                do_stop = true;
            }

            ui.add_space(8.0);
            // Live position + BPM readout
            let pos_str = if self.song_mode && !self.song.clips.is_empty() {
                super::playlist::song_bar_pos(&self.song.meters, now_tick)
            } else {
                super::roll::bar_pos(now_tick)
            };
            ui.label(
                RichText::new(pos_str)
                    .font(t::mono_medium(13.0))
                    .color(t::text()),
            );
            ui.label(
                RichText::new(format!("{bpm_now:.0} BPM"))
                    .font(t::mono(12.0))
                    .color(t::muted()),
            );

            // Song title & active section marker (only if room)
            if rect.width() >= 900.0 {
                let title = self.show_title();
                let section = active_marker_name(&self.song, now_tick);
                ui.add_space(8.0);
                hud_divider(ui);
                ui.add_space(8.0);
                ui.label(RichText::new(title).font(t::medium(13.0)).color(t::text()));
                if let Some(sec) = section {
                    ui.label(RichText::new(sec).font(t::medium(12.0)).color(t::wait()));
                }
            }

            // Right-aligned controls
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if hud_btn(ui, tr.show_exit, false) {
                    do_exit = true;
                }
                if hud_btn(ui, tr.show_fullscreen, fullscreen) {
                    do_full = true;
                }
                if hud_btn(ui, tr.show_hud, false) {
                    do_hide_hud = true;
                }
                if hud_accent_btn(ui, tr.show_record) {
                    do_record = true;
                }
                if rect.width() >= 760.0 && hud_btn(ui, &format!("{fps} fps"), false) {
                    toggle_fps = true;
                }
                if view == ShowView::Notes {
                    let all = matches!(channels, ShowChannels::All);
                    let lbl = if all {
                        tr.show_all_channels
                    } else {
                        tr.show_selected_channel
                    };
                    if hud_btn(ui, lbl, !all) {
                        next_channels = Some(if all {
                            ShowChannels::Selected(self.instrument)
                        } else {
                            ShowChannels::All
                        });
                    }
                }
                if has_saved_loop {
                    let is_loop = fit == ShowFit::Loop;
                    let lbl = if is_loop {
                        tr.show_range_loop
                    } else {
                        tr.show_range_song
                    };
                    if hud_btn(ui, lbl, is_loop) {
                        next_fit = Some(if is_loop {
                            ShowFit::Song
                        } else {
                            ShowFit::Loop
                        });
                    }
                }
            });
        });

        if let Some(v) = next_view
            && let Some(st) = &mut self.show
        {
            st.view = v;
        }
        if do_play {
            self.show_play_pause();
        }
        if do_stop {
            self.show_stop();
        }
        if let Some(f) = next_fit {
            self.show_apply_fit(f);
        }
        if let Some(c) = next_channels
            && let Some(st) = &mut self.show
        {
            st.channels = c;
        }
        if toggle_fps && let Some(st) = &mut self.show {
            st.fps = if st.fps == 30 { 60 } else { 30 };
        }
        if do_record {
            self.trigger_show_record_dialog(ui.ctx());
        }
        if do_hide_hud && let Some(st) = &mut self.show {
            st.hud = false;
        }
        if do_full && let Some(st) = &mut self.show {
            st.fullscreen = !st.fullscreen;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(st.fullscreen));
        }
        if do_exit {
            self.exit_show(Some(ui.ctx()));
        }
    }

    fn show_title(&self) -> String {
        let info_title = self.song.info.title.trim();
        if !info_title.is_empty() {
            return info_title.to_string();
        }
        if let Some(p) = &self.path
            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
            && !stem.is_empty()
        {
            return stem.to_string();
        }
        crate::i18n::strings(self.settings.general.language)
            .untitled
            .to_string()
    }

    fn paint_show_arrange(&mut self, ui: &mut Ui, rect: Rect, now_tick: Tick) {
        if rect.width() <= 2.0 || rect.height() <= 2.0 {
            return;
        }

        if self.song.clips.is_empty() {
            self.paint_show_empty(ui, rect, true);
            return;
        }

        let range = self.show_range_ticks();
        let layout = ArrangeLayout::compute(&self.song, rect, range);
        let p = ui.painter().with_clip_rect(rect);

        // Background: ruler + track lanes
        let ruler_rect = Rect::from_min_max(rect.left_top(), pos2(rect.right(), layout.grid.top()));
        p.rect_filled(ruler_rect, 0.0, t::surface());
        p.rect_filled(layout.grid, 0.0, t::bg());

        if layout.gutter_w > 0.0 {
            let gutter = Rect::from_min_max(
                pos2(rect.left(), layout.grid.top()),
                pos2(layout.grid.left(), rect.bottom()),
            );
            p.rect_filled(gutter, 0.0, t::side());
            p.line_segment(
                [
                    pos2(layout.grid.left(), rect.top()),
                    pos2(layout.grid.left(), rect.bottom()),
                ],
                Stroke::new(1.0_f32, t::edge()),
            );
        }
        p.line_segment(
            [
                pos2(rect.left(), layout.grid.top()),
                pos2(rect.right(), layout.grid.top()),
            ],
            Stroke::new(1.0_f32, t::edge()),
        );

        // Alternating lane stripes + gutter track names
        for (row, &track_idx) in layout.tracks.iter().enumerate() {
            let y0 = layout.grid.top() + row as f32 * layout.lane_h;
            let y1 = (y0 + layout.lane_h).min(layout.grid.bottom());
            if row % 2 == 1 {
                p.rect_filled(
                    Rect::from_min_max(pos2(layout.grid.left(), y0), pos2(layout.grid.right(), y1)),
                    0.0,
                    t::surface().gamma_multiply(0.32),
                );
            }
            p.line_segment(
                [pos2(rect.left(), y1), pos2(rect.right(), y1)],
                Stroke::new(1.0_f32, t::line().gamma_multiply(0.45)),
            );
            if layout.gutter_w > 0.0 {
                let name = self.song.track_name(track_idx);
                let muted = self.song.track_muted(track_idx);
                let shown = kit::elide(&p, &name, t::sans(12.0), layout.gutter_w - 16.0);
                p.text(
                    pos2(rect.left() + 10.0, (y0 + y1) * 0.5),
                    Align2::LEFT_CENTER,
                    shown,
                    t::sans(12.0),
                    if muted { t::faint() } else { t::muted() },
                );
            }
        }

        // Saved loop band indicator when viewing full song
        if let Some((la, lb)) = self.show.as_ref().and_then(|s| s.saved_loop_range)
            && lb > la
            && self.song.loop_range.is_none()
        {
            let x0 = layout.x_of(la);
            let x1 = layout.x_of(lb);
            if x1 > x0 + 1.0 {
                p.rect_filled(
                    Rect::from_min_max(pos2(x0, ruler_rect.top()), pos2(x1, ruler_rect.bottom())),
                    0.0,
                    t::brand_soft().gamma_multiply(0.55),
                );
            }
        }

        // Bar lines + time signature flags + bar numbers
        let gp = ui.painter().with_clip_rect(
            Rect::from_min_max(pos2(layout.grid.left(), rect.top()), rect.max).intersect(rect),
        );
        let mut bar = 0u32;
        let total_bars = ((layout.end_tick - layout.start_tick) as f32 / BAR as f32).max(1.0);
        let bar_px = layout.grid.width() / total_bars;
        let label_step: u32 = if bar_px >= 28.0 {
            1
        } else if bar_px >= 14.0 {
            2
        } else if bar_px >= 7.0 {
            4
        } else if bar_px >= 3.5 {
            8
        } else {
            16
        };

        loop {
            let tick = self.song.bar_tick(bar);
            if tick > layout.end_tick || Tick::from(bar) > MAX_BARS {
                break;
            }
            if tick >= layout.start_tick {
                let x = layout.x_of(tick);
                let strong = bar.is_multiple_of(4);
                gp.line_segment(
                    [pos2(x, layout.grid.top()), pos2(x, layout.grid.bottom())],
                    Stroke::new(
                        1.0_f32,
                        if strong {
                            t::line()
                        } else {
                            t::line().gamma_multiply(0.45)
                        },
                    ),
                );
                if bar.is_multiple_of(label_step) && x < layout.grid.right() - 14.0 {
                    gp.text(
                        pos2(x + 4.0, ruler_rect.bottom() - 4.0),
                        Align2::LEFT_BOTTOM,
                        format!("{}", bar + 1),
                        t::mono(11.0),
                        if strong { t::text() } else { t::faint() },
                    );
                }
                if let Some(m) = self
                    .song
                    .meters
                    .iter()
                    .find(|m| m.bar == bar && (bar > 0 || (m.num, m.den) != (4, 4)))
                    && rect.height() >= 180.0
                {
                    let flag_txt = format!("{}/{}", m.num, m.den);
                    let fw = 8.0 + flag_txt.len() as f32 * 6.5;
                    let flag =
                        Rect::from_min_size(pos2(x + 2.0, ruler_rect.top() + 2.0), vec2(fw, 12.0));
                    gp.rect_filled(flag, 2.0, t::wait());
                    gp.text(
                        flag.center(),
                        Align2::CENTER_CENTER,
                        flag_txt,
                        t::mono(10.0),
                        t::sunken(),
                    );
                }
            }
            bar += 1;
        }

        // Section markers on ruler
        if rect.height() >= 160.0 {
            let mut last_chip_right = layout.grid.left() - 10.0;
            for m in &self.song.markers {
                if m.tick < layout.start_tick || m.tick >= layout.end_tick {
                    continue;
                }
                let x = layout.x_of(m.tick);
                if x < last_chip_right + 4.0 {
                    continue;
                }
                let shown = kit::elide(&gp, &m.name, t::medium(10.5), 96.0);
                let w = (shown.chars().count() as f32 * 6.4 + 10.0).min(106.0);
                let chip =
                    Rect::from_min_size(pos2(x + 2.0, ruler_rect.top() + 2.0), vec2(w, 13.0));
                gp.rect_filled(chip, 2.5, t::wait_soft());
                gp.rect_stroke(
                    chip,
                    2.5,
                    Stroke::new(1.0_f32, t::wait().gamma_multiply(0.75)),
                    egui::StrokeKind::Inside,
                );
                gp.text(
                    chip.center(),
                    Align2::CENTER_CENTER,
                    shown,
                    t::medium(10.0),
                    t::wait(),
                );
                last_chip_right = chip.right();
            }
        }

        // Clips
        let clip_painter = ui.painter().with_clip_rect(layout.grid.intersect(rect));
        for c in &self.song.clips {
            let Some(pat) = self.song.pattern(c.pattern) else {
                continue;
            };
            let len = self.song.clip_len(c);
            let c_end = c.start.saturating_add(len);
            if c_end <= layout.start_tick || c.start >= layout.end_tick {
                continue;
            }
            let Some((y0, y1)) = layout.track_y(c.track) else {
                continue;
            };
            let x0 = layout.x_of(c.start.max(layout.start_tick));
            let x1 = layout.x_of(c_end.min(layout.end_tick)).max(x0 + 2.0);
            let pad = if layout.lane_h >= 18.0 { 2.0 } else { 0.5 };
            let r =
                Rect::from_min_max(pos2(x0, y0 + pad), pos2(x1, (y1 - pad).max(y0 + pad + 2.0)));

            let muted = c.muted || self.song.track_muted(c.track);
            let active = !muted && now_tick >= c.start && now_tick < c_end;
            let base = t::pattern_color(pat.id, pat.color);
            let fill = if muted {
                t::surface().gamma_multiply(0.6)
            } else if active {
                kit::mix(t::surface(), base, 0.36)
            } else {
                kit::mix(t::surface(), base, 0.22)
            };
            let radius = if r.height() >= 14.0 { 4.0 } else { 1.5 };
            clip_painter.rect_filled(r, radius, fill);

            // Top accent header bar on clips tall enough
            let head_h = if r.height() >= 22.0 && r.width() >= 8.0 {
                14.0
            } else {
                (r.height() * 0.25).min(3.0)
            };
            let head = Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + head_h));
            clip_painter.rect_filled(
                head,
                radius,
                if muted {
                    t::faint().gamma_multiply(0.35)
                } else {
                    base.gamma_multiply(if active { 0.92 } else { 0.65 })
                },
            );

            // Pattern title inside header
            if head_h >= 12.0 && r.width() >= 28.0 {
                let shown = kit::elide(&clip_painter, &pat.name, t::medium(10.5), r.width() - 8.0);
                let ink = if muted {
                    t::faint()
                } else if t::theme() == t::Theme::Dark {
                    t::edge()
                } else {
                    t::on_brand()
                };
                clip_painter.text(
                    pos2(r.left() + 4.0, head.center().y),
                    Align2::LEFT_CENTER,
                    shown,
                    t::medium(10.5),
                    ink,
                );
            }

            // Repeat seams + content preview (skipped in ultra-dense mode)
            if !layout.dense && r.width() >= 6.0 {
                let plen = pat.length.max(1);
                let mut k = c.offset.div_euclid(plen) + 1;
                let mut seam_guard = 0;
                while k * plen < c.offset + len && seam_guard < 128 {
                    let seam_tick = c.start + (k * plen - c.offset);
                    if seam_tick > layout.start_tick && seam_tick < layout.end_tick {
                        let sx = layout.x_of(seam_tick);
                        clip_painter.line_segment(
                            [pos2(sx, head.bottom()), pos2(sx, r.bottom() - 1.0)],
                            Stroke::new(
                                1.0_f32,
                                base.gamma_multiply(if muted { 0.2 } else { 0.45 }),
                            ),
                        );
                    }
                    k += 1;
                    seam_guard += 1;
                }

                let body = Rect::from_min_max(
                    pos2(r.left() + 1.0, head.bottom() + 1.5),
                    pos2(r.right() - 1.0, r.bottom() - 1.5),
                );
                if body.height() >= 5.0 && body.width() >= 4.0 {
                    if let Some(env) = &pat.automation {
                        let ap =
                            clip_painter.with_clip_rect(body.intersect(clip_painter.clip_rect()));
                        let ink = if muted { t::faint() } else { t::enc_green() };
                        super::automation::paint_clip(
                            &ap, body, env, &self.song, c.offset, len, plen, ink,
                        );
                    } else {
                        paint_clip_note_preview(
                            &clip_painter,
                            body,
                            pat,
                            c.start,
                            c.offset,
                            len,
                            &layout,
                            if muted {
                                t::faint().gamma_multiply(0.6)
                            } else {
                                t::text().gamma_multiply(0.85)
                            },
                        );
                    }
                }
            }

            let border = if active {
                Stroke::new(1.5_f32, base)
            } else {
                Stroke::new(
                    1.0_f32,
                    base.gamma_multiply(if muted { 0.25 } else { 0.55 }),
                )
            };
            clip_painter.rect_stroke(r, radius, border, egui::StrokeKind::Inside);
        }

        // Playhead
        let px = layout.x_of(now_tick.clamp(layout.start_tick, layout.end_tick));
        gp.line_segment(
            [pos2(px, ruler_rect.top()), pos2(px, layout.grid.bottom())],
            Stroke::new(2.0_f32, t::brand()),
        );
        let tri = vec![
            pos2(px - 6.0, ruler_rect.top()),
            pos2(px + 6.0, ruler_rect.top()),
            pos2(px, ruler_rect.top() + 9.0),
        ];
        gp.add(Shape::convex_polygon(tri, t::brand(), Stroke::NONE));

        // Click / drag on timeline to seek (read-only: never edits project)
        if !self.show.as_ref().is_some_and(|s| s.is_recording()) {
            let seek_rect = Rect::from_min_max(pos2(layout.grid.left(), rect.top()), rect.max);
            let resp = ui.interact(
                seek_rect,
                ui.id().with("show-arrange-seek"),
                Sense::click_and_drag(),
            );
            if (resp.clicked() || resp.dragged())
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let tick = layout.tick_at(pos.x);
                self.show_seek_tick(tick);
            }
        }
    }

    fn paint_show_fall(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        now_frame: f64,
        table: Option<&TempoTable>,
        rate: u32,
    ) {
        if rect.width() <= 2.0 || rect.height() <= 2.0 {
            return;
        }
        let channels = self.show.as_ref().map_or(ShowChannels::All, |s| s.channels);
        let lookahead_s = self.show.as_ref().map_or(3.0, |s| s.lookahead_s);
        let show_hud = self
            .show
            .as_ref()
            .is_some_and(|s| s.hud && !s.is_recording());

        // Mouse wheel adjusts lookahead duration smoothly
        if !self.show.as_ref().is_some_and(|s| s.is_recording()) {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.5
                && ui.rect_contains_pointer(rect)
                && let Some(st) = &mut self.show
            {
                st.lookahead_s = (st.lookahead_s - scroll * 0.004).clamp(1.2, 6.0);
            }
        }

        let layout = FallLayout::compute(&self.song, rect, channels);
        let lookahead_frames = lookahead_s as f64 * rate.max(1) as f64;
        let bounds = self.loop_bounds();
        let notes = collect_fall_window(
            &self.song,
            self.song_mode,
            self.selected,
            table,
            rate,
            bounds,
            now_frame,
            lookahead_frames,
            channels,
            &layout.perc_channels,
        );

        let has_any_audible = self.song.patterns.iter().any(|p| {
            p.notes.iter().any(|n| {
                !n.muted
                    && self.song.instrument(n.instrument).is_some_and(|i| !i.muted)
                    && match channels {
                        ShowChannels::All => true,
                        ShowChannels::Selected(id) => n.instrument == id,
                    }
            })
        });

        let p = ui.painter().with_clip_rect(rect);

        // Stage pitch columns (alternating white/black key lanes + C octave guide lines)
        for pitch in layout.lo_pitch..=layout.hi_pitch {
            let Some((x0, x1)) = layout.pitch_x(pitch) else {
                continue;
            };
            let bg = if is_black_key(pitch) {
                t::row_black()
            } else {
                t::row_white()
            };
            p.rect_filled(
                Rect::from_min_max(pos2(x0, layout.stage_rect.top()), pos2(x1, layout.play_y)),
                0.0,
                bg,
            );
            if pitch % 12 == 0 {
                p.line_segment(
                    [pos2(x0, layout.stage_rect.top()), pos2(x0, layout.play_y)],
                    Stroke::new(1.0_f32, t::line().gamma_multiply(0.55)),
                );
            }
        }

        // Rhythm strip backdrop
        if layout.rhythm_rect.width() > 2.0 {
            p.rect_filled(layout.rhythm_rect, 0.0, t::side());
            for &inst_id in &layout.perc_channels {
                if let Some((x0, x1)) = layout.perc_x(inst_id) {
                    p.rect_filled(
                        Rect::from_min_max(
                            pos2(x0 + 2.0, layout.rhythm_rect.top()),
                            pos2(x1 - 2.0, layout.play_y),
                        ),
                        0.0,
                        t::sunken().gamma_multiply(0.65),
                    );
                }
            }
            p.line_segment(
                [
                    pos2(layout.rhythm_rect.right(), rect.top()),
                    pos2(layout.rhythm_rect.right(), rect.bottom()),
                ],
                Stroke::new(1.0_f32, t::edge()),
            );
        }

        // Falling horizontal bar / beat grid lines synced with audio frames
        let now_tick = frame_to_tick(&self.song, table, now_frame, rate);
        let end_tick_look = frame_to_tick(&self.song, table, now_frame + lookahead_frames, rate);
        let first_beat = (now_tick / (BAR / 4)).max(0);
        let last_beat = (end_tick_look / (BAR / 4) + 1).min(first_beat + 64);
        for beat in first_beat..=last_beat {
            let bt = beat * (BAR / 4);
            let bf = tick_to_frame(&self.song, table, bt, rate);
            let y = layout.y_of_frame(bf, now_frame, lookahead_frames);
            if y < layout.stage_rect.top() || y > layout.play_y {
                continue;
            }
            let is_bar = beat % 4 == 0;
            p.line_segment(
                [pos2(rect.left(), y), pos2(rect.right(), y)],
                Stroke::new(
                    1.0_f32,
                    if is_bar {
                        t::line().gamma_multiply(0.7)
                    } else {
                        t::line().gamma_multiply(0.25)
                    },
                ),
            );
        }

        // Track active keys & percussion pads for keyboard lighting + active vocal lyric
        let mut active_keys: [Option<Color32>; 128] = [None; 128];
        let mut active_perc: Vec<(u64, Color32)> = Vec::new();
        let mut active_lyric: Option<(String, Color32)> = None;

        let stage_clip = ui.painter().with_clip_rect(
            Rect::from_min_max(rect.left_top(), pos2(rect.right(), layout.play_y)).intersect(rect),
        );

        for fn_note in &notes {
            let y_bot = layout
                .y_of_frame(fn_note.start_frame, now_frame, lookahead_frames)
                .min(layout.play_y);
            let y_top = layout
                .y_of_frame(fn_note.end_frame, now_frame, lookahead_frames)
                .min(y_bot - 4.0);
            if y_bot <= rect.top() || y_top >= layout.play_y {
                continue;
            }
            let vel = (fn_note.clip_note.note.velocity as f32 / 127.0).clamp(0.35, 1.0);
            let fill = if fn_note.active {
                kit::mix(fn_note.color, Color32::WHITE, 0.22)
            } else {
                fn_note.color.gamma_multiply(0.68 + 0.32 * vel)
            };

            if fn_note.is_perc {
                if let Some((x0, x1)) = layout.perc_x(fn_note.clip_note.note.instrument) {
                    // Crisp horizontal percussion bar for drum/Forge hits
                    let bar_h = (y_bot - y_top).clamp(5.0, 14.0);
                    let r =
                        Rect::from_min_max(pos2(x0 + 4.0, y_bot - bar_h), pos2(x1 - 4.0, y_bot));
                    stage_clip.rect_filled(r, 3.0, fill);
                    if fn_note.active {
                        stage_clip.rect_stroke(
                            r,
                            3.0,
                            Stroke::new(1.5_f32, t::text()),
                            egui::StrokeKind::Inside,
                        );
                        active_perc.push((fn_note.clip_note.note.instrument, fn_note.color));
                    }
                }
                continue;
            }

            let pitch = fn_note.clip_note.note.pitch;
            if let Some((x0, x1)) = layout.pitch_x(pitch) {
                let inset = (layout.key_w * 0.08).clamp(0.5, 2.5);
                let r = Rect::from_min_max(pos2(x0 + inset, y_top), pos2(x1 - inset, y_bot));
                let radius = if r.width() >= 8.0 { 3.5 } else { 1.5 };
                stage_clip.rect_filled(r, radius, fill);
                if fn_note.active {
                    stage_clip.rect_stroke(
                        r,
                        radius,
                        Stroke::new(1.5_f32, Color32::WHITE.gamma_multiply(0.9)),
                        egui::StrokeKind::Inside,
                    );
                    active_keys[pitch as usize] = Some(fn_note.color);
                } else {
                    stage_clip.rect_stroke(
                        r,
                        radius,
                        Stroke::new(1.0_f32, t::edge().gamma_multiply(0.55)),
                        egui::StrokeKind::Inside,
                    );
                }

                // Vocal pitch curve & syllable lyric inside block
                if fn_note.is_vocal
                    && let Some(pat) = self.song.pattern(fn_note.clip_note.pattern)
                {
                    let nid = fn_note.clip_note.note.id;
                    if let Some(np) = pat.pitch.get(&nid)
                        && !np.points.is_empty()
                    {
                        let nlen = (fn_note.clip_note.end - fn_note.clip_note.start).max(1);
                        let samples = ((r.height() / 4.0) as usize).clamp(4, 32);
                        let mut pts = Vec::with_capacity(samples + 1);
                        for si in 0..=samples {
                            let frac = si as f64 / samples as f64;
                            let cents = np.cents_at(frac * nlen as f64, nlen);
                            let px = r.center().x + (cents / 100.0) as f32 * layout.key_w;
                            let py = y_bot - frac as f32 * (y_bot - y_top);
                            pts.push(pos2(px, py));
                        }
                        stage_clip.add(Shape::line(pts, Stroke::new(1.8_f32, t::text())));
                    }
                    if let Some(lyr) = pat.lyrics.get(&nid) {
                        let text = lyr.trim();
                        if !text.is_empty() {
                            if fn_note.active {
                                active_lyric = Some((text.to_string(), fn_note.color));
                            }
                            if r.height() >= 16.0 && r.width() >= 18.0 {
                                let shown =
                                    kit::elide(&stage_clip, text, t::medium(11.0), r.width() - 4.0);
                                stage_clip.text(
                                    pos2(r.center().x, r.bottom() - 8.0),
                                    Align2::CENTER_CENTER,
                                    shown,
                                    t::medium(11.0),
                                    t::on_brand(),
                                );
                            }
                        }
                    }
                }
            }
        }

        // Crisp Play Line separating falling notes from keyboard/pads
        p.line_segment(
            [
                pos2(rect.left(), layout.play_y),
                pos2(rect.right(), layout.play_y),
            ],
            Stroke::new(2.0_f32, t::brand()),
        );

        // Live Vocal Lyric Banner right above play line when a syllable is sung
        if let Some((lyr, col)) = active_lyric
            && layout.stage_rect.height() >= 90.0
        {
            let w = (lyr.chars().count() as f32 * 9.5 + 24.0).clamp(64.0, 260.0);
            let pill = Rect::from_center_size(
                pos2(layout.stage_rect.center().x, layout.play_y - 24.0),
                vec2(w, 28.0),
            );
            p.rect_filled(pill, 6.0, t::surface().gamma_multiply(0.94));
            p.rect_stroke(
                pill,
                6.0,
                Stroke::new(1.5_f32, col),
                egui::StrokeKind::Inside,
            );
            p.text(
                pill.center(),
                Align2::CENTER_CENTER,
                lyr,
                t::semibold(15.0),
                t::text(),
            );
        }

        // Bottom Rhythm Pads (for percussion channels)
        if layout.rhythm_pad_rect.width() > 2.0 {
            p.rect_filled(layout.rhythm_pad_rect, 0.0, t::side());
            for &inst_id in &layout.perc_channels {
                let Some((x0, x1)) = layout.perc_x(inst_id) else {
                    continue;
                };
                let lit = active_perc
                    .iter()
                    .find(|(id, _)| *id == inst_id)
                    .map(|(_, c)| *c);
                let pad = Rect::from_min_max(
                    pos2(x0 + 3.0, layout.play_y + 4.0),
                    pos2(x1 - 3.0, layout.rhythm_pad_rect.bottom() - 4.0),
                );
                let col = channel_color(&self.song, inst_id);
                let fill = match lit {
                    Some(c) => c,
                    None => kit::mix(t::surface(), col, 0.14),
                };
                p.rect_filled(pad, 4.0, fill);
                p.rect_stroke(
                    pad,
                    4.0,
                    Stroke::new(1.0_f32, col.gamma_multiply(0.6)),
                    egui::StrokeKind::Inside,
                );
                if pad.width() >= 22.0 && pad.height() >= 22.0 {
                    let name = self.instrument_name(inst_id);
                    let short = kit::elide(&p, &name, t::medium(10.0), pad.width() - 4.0);
                    p.text(
                        pad.center(),
                        Align2::CENTER_CENTER,
                        short,
                        t::medium(10.0),
                        if lit.is_some() {
                            t::on_brand()
                        } else {
                            t::muted()
                        },
                    );
                }
            }
        }

        // Bottom Piano Keyboard (exact column alignment with falling pitches)
        p.rect_filled(layout.keyboard_rect, 0.0, t::edge());
        for pitch in layout.lo_pitch..=layout.hi_pitch {
            let Some((x0, x1)) = layout.pitch_x(pitch) else {
                continue;
            };
            let black = is_black_key(pitch);
            let lit = active_keys[pitch as usize];
            if !black {
                let kr = Rect::from_min_max(
                    pos2(x0 + 0.5, layout.play_y + 1.0),
                    pos2(x1 - 0.5, layout.keyboard_rect.bottom() - 1.0),
                );
                p.rect_filled(kr, 2.0, lit.unwrap_or_else(t::key_white));
                p.rect_stroke(
                    kr,
                    2.0,
                    Stroke::new(0.8_f32, t::key_white_edge()),
                    egui::StrokeKind::Inside,
                );
                if pitch % 12 == 0 && kr.width() >= 11.0 && kr.height() >= 26.0 {
                    let oct = (pitch as i32 / 12) - 1;
                    p.text(
                        pos2(kr.center().x, kr.bottom() - 8.0),
                        Align2::CENTER_CENTER,
                        format!("C{oct}"),
                        t::mono(9.5),
                        if lit.is_some() {
                            t::on_brand()
                        } else {
                            t::key_ink()
                        },
                    );
                }
            } else {
                let bh = layout.keyboard_rect.height() * 0.64;
                let kr = Rect::from_min_max(
                    pos2(x0 + 0.5, layout.play_y + 1.0),
                    pos2(x1 - 0.5, layout.play_y + bh),
                );
                p.rect_filled(kr, 2.0, lit.unwrap_or_else(t::key_black));
                if lit.is_some() {
                    p.rect_stroke(
                        kr,
                        2.0,
                        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.8)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }

        // Channel legend strip (top-left of stage when HUD is visible)
        if show_hud && layout.stage_rect.width() >= 360.0 && layout.stage_rect.height() >= 140.0 {
            let mut next_filter: Option<ShowChannels> = None;
            let legend_rect = Rect::from_min_size(
                pos2(
                    layout.stage_rect.left() + 10.0,
                    layout.stage_rect.top() + 8.0,
                ),
                vec2((layout.stage_rect.width() - 20.0).max(1.0), 26.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(legend_rect), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for inst in self.song.instruments.iter().filter(|i| !i.muted).take(10) {
                        let col = channel_color(&self.song, inst.id);
                        let selected =
                            matches!(channels, ShowChannels::Selected(id) if id == inst.id);
                        let resp = legend_chip(ui, &inst.name, col, selected);
                        if resp.clicked() {
                            next_filter = Some(if selected {
                                ShowChannels::All
                            } else {
                                ShowChannels::Selected(inst.id)
                            });
                        }
                    }
                });
            });
            if let Some(f) = next_filter
                && let Some(st) = &mut self.show
            {
                st.channels = f;
            }
        }

        if !has_any_audible {
            self.paint_show_empty(ui, layout.stage_rect, false);
        }
    }

    fn paint_show_empty(&mut self, ui: &mut Ui, rect: Rect, arrange: bool) {
        let tr = crate::i18n::strings(self.settings.general.language);
        let msg = if arrange {
            tr.show_empty_arrange
        } else {
            tr.show_empty_notes
        };
        let box_w = (rect.width() - 40.0).clamp(240.0, 460.0);
        let box_r = Rect::from_center_size(rect.center(), vec2(box_w, 118.0));
        let p = ui.painter();
        p.rect_filled(box_r, 8.0, t::surface());
        p.rect_stroke(
            box_r,
            8.0,
            Stroke::new(1.0_f32, t::line()),
            egui::StrokeKind::Inside,
        );

        let mut switch_to_notes = false;
        let mut show_all = false;
        let mut exit_now = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(box_r.shrink(16.0)), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(msg).font(t::sans(13.0)).color(t::text()));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if arrange && hud_accent_btn(ui, tr.show_notes) {
                        switch_to_notes = true;
                    }
                    if !arrange
                        && self
                            .show
                            .as_ref()
                            .is_some_and(|s| !matches!(s.channels, ShowChannels::All))
                        && hud_accent_btn(ui, tr.show_all_channels)
                    {
                        show_all = true;
                    }
                    if hud_btn(ui, tr.show_exit, false) {
                        exit_now = true;
                    }
                });
            });
        });
        if switch_to_notes && let Some(st) = &mut self.show {
            st.view = ShowView::Notes;
        }
        if show_all && let Some(st) = &mut self.show {
            st.channels = ShowChannels::All;
        }
        if exit_now {
            self.exit_show(Some(ui.ctx()));
        }
    }

    fn paint_show_record_status(&mut self, ui: &mut Ui, rect: Rect) {
        let tr = crate::i18n::strings(self.settings.general.language);
        let Some(job) = self.show.as_ref().and_then(|s| s.record.as_ref()) else {
            return;
        };
        // During active frame capture we keep the screen 100% clean so no status banner
        // leaks into the recorded video frames! Only show status while preparing, done, or failed.
        let (title, tone, dismissible, cancellable) = match &job.phase {
            RecordPhase::RenderingAudio { .. } => {
                (format!("{}...", tr.show_recording), t::wait(), false, true)
            }
            RecordPhase::Capturing { .. } => return,
            RecordPhase::Done(msg) => (msg.clone(), t::ok(), true, false),
            RecordPhase::Error(msg) => (msg.clone(), t::bad(), true, false),
        };

        let box_w = (rect.width() - 40.0).clamp(260.0, 480.0);
        let box_r = Rect::from_center_size(
            pos2(rect.center().x, rect.bottom() - 44.0),
            vec2(box_w, 46.0),
        );
        let p = ui.painter();
        p.rect_filled(box_r, 6.0, t::surface());
        p.rect_stroke(
            box_r,
            6.0,
            Stroke::new(1.5_f32, tone),
            egui::StrokeKind::Inside,
        );

        let mut cancel_now = false;
        let mut dismiss_now = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(box_r.shrink(8.0)), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new(title).font(t::medium(12.5)).color(t::text()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if cancellable && hud_btn(ui, tr.show_cancel, false) {
                        cancel_now = true;
                    }
                    if dismissible && hud_btn(ui, tr.common_close, false) {
                        dismiss_now = true;
                    }
                });
            });
        });

        if cancel_now && let Some(st) = &mut self.show {
            st.cancel_recording();
        }
        if dismiss_now && let Some(st) = &mut self.show {
            st.record = None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_clip_note_preview(
    p: &egui::Painter,
    area: Rect,
    pat: &Pattern,
    clip_start: Tick,
    clip_offset: Tick,
    clip_len: Tick,
    layout: &ArrangeLayout,
    ink: Color32,
) {
    let mut low = u8::MAX;
    let mut high = 0u8;
    for n in pat.notes.iter().filter(|n| !n.muted) {
        low = low.min(n.pitch);
        high = high.max(n.pitch);
    }
    if low > high {
        return;
    }
    let plen = pat.length.max(1);
    let pass0 = clip_offset.div_euclid(plen);
    let pass1 = (clip_offset + clip_len - 1)
        .div_euclid(plen)
        .min(pass0 + 128);
    let p2 = p.with_clip_rect(area.intersect(p.clip_rect()));
    for nn in pat.notes.iter().filter(|n| !n.muted) {
        let y = area.top()
            + (f32::from(high - nn.pitch) + 0.5) / (f32::from(high - low) + 1.0) * area.height();
        for pass in pass0..=pass1 {
            let s0 = nn.start + pass * plen;
            let s1 = nn.end() + pass * plen;
            if s1 <= clip_offset || s0 >= clip_offset + clip_len {
                continue;
            }
            let t0 = clip_start + (s0.max(clip_offset) - clip_offset);
            let t1 = clip_start + (s1.min(clip_offset + clip_len) - clip_offset);
            let x0 = layout.x_of(t0);
            let x1 = layout.x_of(t1).max(x0 + 1.5);
            p2.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.5_f32, ink));
        }
    }
}

fn active_marker_name(song: &Song, tick: Tick) -> Option<&str> {
    song.markers
        .iter()
        .rfind(|m| m.tick <= tick)
        .map(|m| m.name.as_str())
        .filter(|s| !s.is_empty())
}

fn hud_divider(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().line_segment(
        [r.center_top(), r.center_bottom()],
        Stroke::new(1.0_f32, t::line()),
    );
}

fn hud_tab(ui: &mut Ui, label: &str, active: bool) -> bool {
    let b = egui::Button::new(
        RichText::new(label)
            .font(if active {
                t::semibold(12.5)
            } else {
                t::sans(12.5)
            })
            .color(if active { t::text() } else { t::muted() }),
    )
    .fill(if active {
        t::selection()
    } else {
        Color32::TRANSPARENT
    })
    .stroke(Stroke::new(
        1.0_f32,
        if active {
            t::line()
        } else {
            Color32::TRANSPARENT
        },
    ))
    .corner_radius(5.0);
    ui.add(b).clicked()
}

fn hud_btn(ui: &mut Ui, label: &str, active: bool) -> bool {
    let b = egui::Button::new(RichText::new(label).font(t::medium(12.0)).color(if active {
        t::text()
    } else {
        t::muted()
    }))
    .fill(if active {
        t::selection()
    } else {
        t::ctl_bottom()
    })
    .stroke(Stroke::new(
        1.0_f32,
        if active { t::strong() } else { t::line() },
    ))
    .corner_radius(5.0);
    ui.add(b).clicked()
}

fn hud_accent_btn(ui: &mut Ui, label: &str) -> bool {
    let b = egui::Button::new(
        RichText::new(label)
            .font(t::semibold(12.0))
            .color(t::on_brand()),
    )
    .fill(t::brand_fill())
    .stroke(Stroke::new(1.0_f32, t::brand_edge()))
    .corner_radius(5.0);
    ui.add(b).clicked()
}

fn legend_chip(ui: &mut Ui, name: &str, color: Color32, selected: bool) -> egui::Response {
    let text = RichText::new(format!("   {name}"))
        .font(t::medium(11.0))
        .color(if selected { t::text() } else { t::muted() });
    let fill = if selected {
        kit::mix(t::surface(), color, 0.28)
    } else {
        t::surface().gamma_multiply(0.86)
    };
    let resp = ui.add(
        egui::Button::new(text)
            .fill(fill)
            .stroke(Stroke::new(
                1.0_f32,
                if selected { color } else { t::line() },
            ))
            .corner_radius(4.0),
    );
    let dot = pos2(resp.rect.left() + 8.0, resp.rect.center().y);
    ui.painter().circle_filled(dot, 3.0, color);
    resp
}

#[allow(clippy::too_many_arguments)]
fn spawn_video_encoder(
    out_path: PathBuf,
    audio_wav: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    rx: mpsc::Receiver<Vec<u8>>,
    done_tx: mpsc::Sender<Result<String, String>>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
    tr: &'static crate::i18n::Strings,
) {
    std::thread::spawn(move || {
        let res = encode_stream(&out_path, &audio_wav, width, height, fps, rx, &cancel, tr);
        let _ = done_tx.send(res);
        ctx.request_repaint();
    });
}

#[allow(clippy::too_many_arguments)]
fn encode_stream(
    out_path: &Path,
    audio_wav: &Path,
    width: u32,
    height: u32,
    fps: u32,
    rx: mpsc::Receiver<Vec<u8>>,
    cancel: &AtomicBool,
    tr: &'static crate::i18n::Strings,
) -> Result<String, String> {
    let size_arg = format!("{width}x{height}");
    let fps_arg = fps.to_string();
    let ffmpeg = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &size_arg,
            "-framerate",
            &fps_arg,
            "-i",
            "pipe:0",
            "-i",
        ])
        .arg(audio_wav)
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
        ])
        .arg(out_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();

    match ffmpeg {
        Ok(mut child) => {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| tr.show_rec_ffmpeg_stdin.to_string())?;
            for frame in rx {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(out_path);
                    return Err(tr.show_rec_cancelled.into());
                }
                if let Err(e) = stdin.write_all(&frame) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(crate::i18n::fill(
                        tr.show_rec_ffmpeg_write_err,
                        &[("err", &e.to_string())],
                    ));
                }
            }
            drop(stdin);
            let output = child.wait_with_output().map_err(|e| {
                crate::i18n::fill(tr.show_rec_ffmpeg_wait_err, &[("err", &e.to_string())])
            })?;
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                return Err(crate::i18n::fill(
                    tr.show_rec_ffmpeg_err,
                    &[("err", err.trim())],
                ));
            }
            Ok(crate::i18n::fill(
                tr.show_rec_saved_video,
                &[("path", &out_path.display().to_string())],
            ))
        }
        Err(_) => {
            // Fallback when `ffmpeg` is not installed: write PNG frames + audio.wav + encode.sh.
            let dir = out_path.with_extension("frames");
            std::fs::create_dir_all(&dir).map_err(|e| {
                crate::i18n::fill(tr.show_rec_frames_dir_err, &[("err", &e.to_string())])
            })?;
            let mut idx = 0u64;
            for frame in rx {
                if cancel.load(Ordering::Relaxed) {
                    return Err(tr.show_rec_cancelled.into());
                }
                let p = dir.join(format!("frame-{idx:06}.png"));
                image::save_buffer(&p, &frame, width, height, image::ColorType::Rgba8).map_err(
                    |e| crate::i18n::fill(tr.show_rec_png_save_err, &[("err", &e.to_string())]),
                )?;
                idx += 1;
            }
            let copied_wav = dir.join("audio.wav");
            let _ = std::fs::copy(audio_wav, &copied_wav);
            let script = format!(
                "#!/bin/sh\nffmpeg -y -framerate {fps} -i frame-%06d.png -i audio.wav -c:v libx264 -pix_fmt yuv420p -vf \"scale=trunc(iw/2)*2:trunc(ih/2)*2\" -c:a aac -b:a 192k -shortest \"{}\"\n",
                out_path.display()
            );
            let _ = std::fs::write(dir.join("encode.sh"), script);
            Ok(tr.show_rec_saved_frames.fmt_with(
                tr.lang,
                idx as usize,
                &[("path", &dir.display().to_string())],
            ))
        }
    }
}
