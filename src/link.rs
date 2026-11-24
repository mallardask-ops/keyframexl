//! UI → audio song hand-off over lock-free ring buffers (`rtrb`).
//!
//! The UI sends each edited song as an `Arc<Song>`; the audio thread takes
//! the newest one at the start of every block. Replaced songs travel back on a
//! second ring so the UI frees them: the audio thread never deallocates.
//!
//! Each song carries a generation number. Live parameter commands (a fader or
//! knob moved between two song snapshots) ride a third ring stamped with the
//! generation of the song they were made against, so the audio thread can
//! tell a command the newest song already contains from one it still lacks.

use crate::automation::Target;
use crate::engine::Engine;
use crate::fx::{FxKind, FxState};
use crate::hit_index::HitIndex;
use crate::sequence::Song;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::Arc;

/// Songs in flight before the audio thread drains them.
const SEND_CAP: usize = 64;
/// Replaced songs waiting for the UI to free them.
const TRASH_CAP: usize = 256;
/// Live parameter commands in flight before the audio thread drains them.
const PARAM_CAP: usize = 512;

/// Song snapshot paired with its prebuilt hit-collection index so the audio
/// thread swaps both atomically without allocating, sorting, or freeing.
struct SongPacket {
    song: Arc<Song>,
    index: Arc<HitIndex>,
}

/// UI side: send songs, free the ones the audio thread has replaced.
pub struct SongSender {
    tx: Producer<(u64, SongPacket)>,
    trash: Consumer<SongPacket>,
    /// The newest song when the ring was full; retried by [`SongSender::tick`].
    pending: Option<(u64, SongPacket)>,
    /// Most recently built index, used to reuse unchanged `Arc<Pattern>` tables.
    last_index: Arc<HitIndex>,
    /// Generation of the newest song handed to `send` (0 = the initial song).
    generation: u64,
}

/// Audio side: the song to play now.
pub struct SongReceiver {
    rx: Consumer<(u64, SongPacket)>,
    trash: Producer<SongPacket>,
    current: SongPacket,
    /// Generation of `current`.
    generation: u64,
    /// Old songs kept because the trash ring was full; handed back later.
    held: Option<SongPacket>,
}

pub fn song_link(song: Song) -> (SongSender, SongReceiver) {
    let (tx, rx) = RingBuffer::new(SEND_CAP);
    let (trash_tx, trash_rx) = RingBuffer::new(TRASH_CAP);
    let song = Arc::new(song);
    let index = Arc::new(HitIndex::build_incremental(&song, None));
    (
        SongSender {
            tx,
            trash: trash_rx,
            pending: None,
            last_index: Arc::clone(&index),
            generation: 0,
        },
        SongReceiver {
            rx,
            trash: trash_tx,
            current: SongPacket { song, index },
            generation: 0,
            held: None,
        },
    )
}

impl SongSender {
    /// Queue `song` for the audio thread. Never blocks; latest wins when full.
    pub fn send(&mut self, song: Arc<Song>) {
        let index = Arc::new(HitIndex::build_incremental(&song, Some(&self.last_index)));
        self.last_index = Arc::clone(&index);
        self.generation += 1;
        self.pending = Some((self.generation, SongPacket { song, index }));
        self.tick();
    }

    /// Generation of the newest song queued so far. Stamp live parameter
    /// commands with it: they apply on top of that song.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Retry a pending song and free replaced ones. Call once per UI frame.
    /// Returns how many songs were freed.
    pub fn tick(&mut self) -> usize {
        if let Some(msg) = self.pending.take()
            && let Err(PushError::Full(msg)) = self.tx.push(msg)
        {
            self.pending = Some(msg);
        }
        let mut freed = 0;
        while self.trash.pop().is_ok() {
            freed += 1;
        }
        freed
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl SongReceiver {
    /// Audio thread, once per block: switch to the newest song. No allocation
    /// and no deallocation happen here.
    pub fn update(&mut self) -> &Song {
        self.update_with_changed().0
    }

    /// [`SongReceiver::update`], also saying whether a newer song arrived in
    /// this call (the caller then drops live overrides the song now carries).
    pub fn update_with_changed(&mut self) -> (&Song, bool) {
        if let Some(old) = self.held.take() {
            self.give_back(old);
        }
        let mut changed = false;
        while let Ok((generation, packet)) = self.rx.pop() {
            let old = std::mem::replace(&mut self.current, packet);
            self.generation = generation;
            changed = true;
            self.give_back(old);
        }
        (&self.current.song, changed)
    }

    /// Generation of the song playing now (0 = the initial song).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn current(&self) -> &Song {
        &self.current.song
    }

    /// Prebuilt hit-collection index matching [`Self::current`].
    pub fn hit_index(&self) -> &HitIndex {
        &self.current.index
    }

    fn give_back(&mut self, old: SongPacket) {
        if self.held.is_some() {
            // The UI is not collecting and one song is already held: keep the
            // newest held, let the older drop. Only reachable if the UI froze
            // for hundreds of edits.
            self.held = Some(old);
            return;
        }
        if let Err(PushError::Full(old)) = self.trash.push(old) {
            self.held = Some(old);
        }
    }
}

/// A control the UI moved, made against the song of `generation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamCmd {
    pub generation: u64,
    pub target: Target,
    pub value: f32,
}

/// UI side of live parameter commands.
pub struct ParamSender {
    tx: Producer<ParamCmd>,
}

/// Audio side: applies the commands the playing song does not contain yet.
pub struct ParamReceiver {
    rx: Consumer<ParamCmd>,
}

pub fn param_link() -> (ParamSender, ParamReceiver) {
    let (tx, rx) = RingBuffer::new(PARAM_CAP);
    (ParamSender { tx }, ParamReceiver { rx })
}

impl ParamSender {
    /// Queue a move of `target` to `value`, made against song `generation`
    /// (`SongSender::generation`). Never blocks; `false` means the ring is
    /// full and the command was dropped.
    pub fn send(&mut self, generation: u64, target: Target, value: f32) -> bool {
        self.tx
            .push(ParamCmd {
                generation,
                target,
                value,
            })
            .is_ok()
    }
}

impl ParamReceiver {
    /// Audio thread, once per block after `SongReceiver::update`: apply every
    /// waiting command. One made against an older song than `current_gen` is
    /// dropped (the newer song already holds its value). Returns how many were
    /// applied. No allocation happens here.
    pub fn drain(&mut self, current_gen: u64, engine: &mut Engine) -> usize {
        let mut applied = 0;
        while let Ok(cmd) = self.rx.pop() {
            if cmd.generation < current_gen {
                continue;
            }
            engine.apply_live_param(cmd.target, cmd.value);
            applied += 1;
        }
        applied
    }
}

/// A slot state built on the UI thread for the engine.
pub struct FxMsg {
    pub track: usize,
    pub slot: usize,
    /// A synth channel's own effect (its id) instead of a mixer slot.
    pub synth: Option<u64>,
    pub state: FxState,
}

const FX_CAP: usize = 256;

/// UI side of effect states: builds a state when a slot's effect changes
/// (delay lines and reverb buffers allocate) and frees replaced ones.
pub struct FxSender {
    tx: Producer<FxMsg>,
    trash: Consumer<FxState>,
    /// Kind and impulse response last sent per mixer track and slot.
    sent: Vec<Vec<Option<(FxKind, Option<std::path::PathBuf>)>>>,
    /// The same for synth channels' own effects: (channel, slot) → kind.
    sent_synth: Vec<(u64, usize, FxKind)>,
    rate: f32,
}

/// Audio side: installs states before the block runs.
pub struct FxReceiver {
    rx: Consumer<FxMsg>,
    trash: Producer<FxState>,
}

pub fn fx_link(rate: u32) -> (FxSender, FxReceiver) {
    let (tx, rx) = RingBuffer::new(FX_CAP);
    let (trash_tx, trash_rx) = RingBuffer::new(FX_CAP);
    (
        FxSender {
            tx,
            trash: trash_rx,
            sent: Vec::new(),
            sent_synth: Vec::new(),
            rate: rate as f32,
        },
        FxReceiver {
            rx,
            trash: trash_tx,
        },
    )
}

impl FxSender {
    /// Call before sending `song`: every slot whose effect changed gets a
    /// fresh state. Returns how many were sent.
    /// Forget the state sent for a slot, so the next sync rebuilds it (a new
    /// impulse response, for example).
    pub fn invalidate(&mut self, track: usize, slot: usize) {
        if let Some(t) = self.sent.get_mut(track)
            && let Some(s) = t.get_mut(slot)
        {
            *s = None;
        }
    }

    pub fn sync(&mut self, song: &Song) -> usize {
        while self.trash.pop().is_ok() {}
        let mut count = 0;
        if self.sent.len() < song.mixer.len() {
            self.sent.resize_with(song.mixer.len(), Vec::new);
        }
        for (t, track) in song.mixer.iter().enumerate() {
            let sent = &mut self.sent[t];
            if sent.len() < track.fx.len() {
                sent.resize(track.fx.len(), None);
            }
            for (k, slot) in track.fx.iter().enumerate() {
                let want = (slot.kind, slot.ir.clone());
                if sent[k].as_ref() == Some(&want) {
                    continue;
                }
                let msg = FxMsg {
                    track: t,
                    slot: k,
                    synth: None,
                    state: FxState::for_slot(slot, self.rate),
                };
                // Ring full: leave it unsent; the next sync retries (the engine
                // builds its own meanwhile).
                if self.tx.push(msg).is_ok() {
                    sent[k] = Some(want);
                    count += 1;
                }
            }
        }
        for inst in &song.instruments {
            let crate::sequence::InstrumentKind::Synth { patch } = &inst.kind else {
                continue;
            };
            for (k, slot) in patch.fx.iter().enumerate() {
                let known = self
                    .sent_synth
                    .iter()
                    .position(|&(i, s, _)| i == inst.id && s == k);
                if known.is_some_and(|n| self.sent_synth[n].2 == slot.kind) {
                    continue;
                }
                let msg = FxMsg {
                    track: 0,
                    slot: k,
                    synth: Some(inst.id),
                    state: FxState::for_slot(slot, self.rate),
                };
                if self.tx.push(msg).is_ok() {
                    match known {
                        Some(n) => self.sent_synth[n].2 = slot.kind,
                        None => self.sent_synth.push((inst.id, k, slot.kind)),
                    }
                    count += 1;
                }
            }
        }
        count
    }
}

impl FxReceiver {
    /// Audio thread: install every waiting state; replaced ones go back.
    pub fn drain(&mut self, engine: &mut Engine) {
        while let Ok(msg) = self.rx.pop() {
            let old = match msg.synth {
                Some(inst) => engine.adopt_synth_fx(inst, msg.slot, msg.state),
                None => engine.adopt_fx(msg.track, msg.slot, msg.state),
            };
            if let Some(old) = old {
                // Trash full means the UI froze; dropping here is the last resort.
                let _ = self.trash.push(old);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(tempo: f64) -> Arc<Song> {
        Arc::new(Song {
            tempo,
            ..Song::default()
        })
    }

    #[test]
    fn newest_song_wins_and_old_ones_come_back_to_the_sender() {
        let (mut tx, mut rx) = song_link(Song::default());
        let first = song(100.0);
        let weak = Arc::downgrade(&first);
        tx.send(first);
        tx.send(song(110.0));
        tx.send(song(120.5));
        assert_eq!(
            rx.update().tempo,
            120.5,
            "the audio thread plays the newest"
        );
        assert!(
            weak.upgrade().is_some(),
            "a replaced song is not freed on the audio side"
        );
        assert_eq!(
            tx.tick(),
            3,
            "the UI frees the initial song and the two replaced"
        );
        assert!(weak.upgrade().is_none());
        assert_eq!(rx.update().tempo, 120.5, "no new song: keep playing");
    }

    #[test]
    fn generations_count_sent_songs_and_follow_the_audio_side() {
        let (mut tx, mut rx) = song_link(Song::default());
        assert_eq!((tx.generation(), rx.generation()), (0, 0));
        tx.send(song(100.0));
        tx.send(song(110.0));
        assert_eq!(tx.generation(), 2);
        assert_eq!(rx.generation(), 0, "nothing popped yet");
        let (current, changed) = rx.update_with_changed();
        assert_eq!(current.tempo, 110.0);
        assert!(changed);
        assert_eq!(rx.generation(), 2);
        let (_, changed) = rx.update_with_changed();
        assert!(!changed, "no new song: unchanged");
        assert_eq!(rx.update().tempo, 110.0);
    }

    #[test]
    fn a_full_ring_still_numbers_the_pending_song() {
        let (mut tx, mut rx) = song_link(Song::default());
        for i in 0..SEND_CAP + 5 {
            tx.send(song(60.0 + i as f64));
        }
        let newest = (SEND_CAP + 5) as u64;
        assert_eq!(tx.generation(), newest);
        rx.update();
        tx.tick();
        rx.update();
        assert_eq!(rx.generation(), newest, "the retried song keeps its number");
    }

    #[test]
    fn param_commands_round_trip_into_the_engine() {
        use crate::automation::Target;
        let (mut tx, mut rx) = param_link();
        let mut engine = Engine::new(48_000);
        engine.reserve(4, 64);
        let t = Target::MixerVolume { track: 1 };
        assert!(tx.send(0, t, 0.25));
        assert!(tx.send(0, Target::MixerPan { track: 1 }, -0.5));
        assert_eq!(rx.drain(0, &mut engine), 2);
        assert_eq!(rx.drain(0, &mut engine), 0, "drained once");
        let mut song = Song::default();
        song.instruments[0].mixer = 1;
        let mut out = vec![0.0f32; 128];
        engine.process_live(&song, &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn a_full_param_ring_refuses_instead_of_blocking() {
        use crate::automation::Target;
        let (mut tx, mut rx) = param_link();
        let t = Target::MixerVolume { track: 0 };
        for _ in 0..PARAM_CAP {
            assert!(tx.send(0, t, 0.5));
        }
        assert!(!tx.send(0, t, 0.5), "full: the command is refused");
        let mut engine = Engine::new(48_000);
        assert_eq!(rx.drain(0, &mut engine), PARAM_CAP);
        assert!(tx.send(0, t, 0.5), "room again after a drain");
    }

    #[test]
    fn commands_made_against_an_older_song_are_discarded() {
        use crate::automation::Target;
        let (mut stx, mut srx) = song_link(Song::default());
        let (mut ptx, mut prx) = param_link();
        let mut engine = Engine::new(48_000);
        engine.reserve(4, 64);
        let old = Target::MixerVolume { track: 1 };
        let new = Target::MixerVolume { track: 2 };
        // A fader moved against song 0, then song 1 (which holds it) is sent,
        // then another fader moved against song 1.
        assert!(ptx.send(stx.generation(), old, 0.1));
        stx.send(song(130.0));
        assert!(ptx.send(stx.generation(), new, 0.9));
        let (current, changed) = srx.update_with_changed();
        assert!(changed);
        assert_eq!(current.tempo, 130.0);
        if changed {
            engine.clear_live_params();
        }
        assert_eq!(
            prx.drain(srx.generation(), &mut engine),
            1,
            "the stale command is dropped, the current one kept"
        );
        assert_eq!(engine.live_param(old), None, "baked into song 1");
        assert_eq!(engine.live_param(new), Some(0.9));
    }

    #[test]
    fn effect_states_are_built_once_per_change_and_installed() {
        use crate::fx::FxSlot;
        let mut song = Song::default();
        song.mixer[1].fx.push(FxSlot::new(FxKind::Reverb));
        let (mut tx, mut rx) = fx_link(48_000);
        let mut engine = Engine::new(48_000);
        engine.reserve(song.mixer.len(), 512);
        assert_eq!(tx.sync(&song), 1);
        assert_eq!(tx.sync(&song), 0, "unchanged slots are not rebuilt");
        rx.drain(&mut engine);
        song.mixer[1].fx[0] = FxSlot::new(FxKind::Delay);
        assert_eq!(tx.sync(&song), 1, "a changed effect gets a new state");
        rx.drain(&mut engine);
        tx.sync(&song);
        // The engine renders with the installed state.
        let mut out = vec![0.0; 1024];
        engine.process(&song, 1, false, 0, &mut out, true);
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn a_full_ring_keeps_the_latest_song_pending() {
        let (mut tx, mut rx) = song_link(Song::default());
        for i in 0..SEND_CAP + 5 {
            tx.send(song(60.0 + i as f64));
        }
        assert!(tx.has_pending(), "ring full: the newest waits");
        rx.update();
        tx.tick();
        assert!(!tx.has_pending());
        assert_eq!(
            rx.update().tempo,
            60.0 + (SEND_CAP + 4) as f64,
            "the newest still arrives"
        );
    }
}

/// Counts heap allocations made on the current thread while armed.
#[cfg(test)]
mod alloc_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    struct Counting;
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            note();
            unsafe { System.alloc(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            note();
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            note();
            unsafe { System.realloc(p, l, n) }
        }
    }
    fn note() {
        let _ = ARMED.try_with(|a| {
            if a.get() {
                let _ = COUNT.try_with(|c| c.set(c.get() + 1));
            }
        });
    }

    #[global_allocator]
    static A: Counting = Counting;

    /// Heap allocations and frees made by `f` on this thread.
    pub fn count(f: impl FnOnce()) -> usize {
        COUNT.with(|c| c.set(0));
        ARMED.with(|a| a.set(true));
        f();
        ARMED.with(|a| a.set(false));
        COUNT.with(|c| c.get())
    }
}

#[cfg(test)]
mod realtime_tests {
    use super::*;
    use crate::fx::FxSlot;
    use crate::sequence::{Note, STEP, Send};

    #[test]
    fn the_audio_block_neither_allocates_nor_frees() {
        let mut song = Song::default();
        for s in 0..16 {
            song.add_note(1, Note::new(1, s * STEP, STEP, 60, 100));
        }
        song.instruments[0].mixer = 1;
        for kind in [
            FxKind::ParamEq,
            FxKind::Reverb,
            FxKind::Delay,
            FxKind::Chorus,
            FxKind::Stereo,
            FxKind::Eq,
        ] {
            song.mixer[1].fx.push(FxSlot::new(kind));
        }
        song.mixer[1].fx[0].params[1] = 6.0;
        song.mixer[1].eq = [3.0, -2.0, 1.0];
        song.mixer[1].sends.push(Send { to: 2, level: 0.5 });
        // A synth channel with its own effects and modulation (Shimmer Bell).
        song.instruments[1].kind = crate::sequence::InstrumentKind::Synth {
            patch: crate::synth::SynthPatch::factory(14),
        };
        for s in 0..8 {
            song.add_note(
                1,
                Note::new(song.instruments[1].id, s * 2 * STEP, STEP, 72, 100),
            );
        }
        song.validate().unwrap();

        let (mut fx_tx, mut fx_rx) = fx_link(48_000);
        let (mut tx, mut rx) = song_link(Song::default());
        let mut engine = Engine::new(48_000);
        engine.reserve(128, 1024);
        let mut out = vec![0.0f32; 1024];
        // UI thread work: build states, send the song.
        fx_tx.sync(&song);
        tx.send(Arc::new(song.clone()));
        // Audio thread: many blocks; a tempo edit from the UI arrives between.
        let mut run =
            |engine: &mut Engine, rx: &mut SongReceiver, fx_rx: &mut FxReceiver, at: &mut i64| {
                alloc_probe::count(|| {
                    for _ in 0..100 {
                        fx_rx.drain(engine);
                        let s = rx.update();
                        *at = engine.process(s, 1, false, *at as u64, &mut out, true) as i64;
                    }
                })
            };
        let mut at = 0i64;
        let first = run(&mut engine, &mut rx, &mut fx_rx, &mut at);
        let mut faster = song.clone();
        faster.tempo = 140.0;
        faster.mixer[1].fx[1] = FxSlot::new(FxKind::Limiter);
        fx_tx.sync(&faster);
        tx.send(Arc::new(faster));
        let second = run(&mut engine, &mut rx, &mut fx_rx, &mut at);
        assert_eq!(
            (first, second),
            (0, 0),
            "allocations or frees in the audio path"
        );
        assert_eq!(rx.current().tempo, 140.0);
        assert_eq!(
            tx.tick(),
            1,
            "the UI frees the song the audio thread replaced"
        );
    }

    #[test]
    fn live_params_cost_the_audio_block_no_allocation() {
        use crate::automation::Target;
        let mut song = Song::default();
        for s in 0..16 {
            song.add_note(1, Note::new(1, s * STEP, STEP, 60, 100));
        }
        song.instruments[0].mixer = 1;
        song.mixer[1].fx.push(FxSlot::new(FxKind::Reverb));
        song.mixer[1].fx.push(FxSlot::new(FxKind::Delay));
        let (mut fx_tx, mut fx_rx) = fx_link(48_000);
        let (mut tx, mut rx) = song_link(song.clone());
        let (mut ptx, mut prx) = param_link();
        let mut engine = Engine::new(48_000);
        engine.reserve(128, 1024);
        fx_tx.sync(&song);
        fx_rx.drain(&mut engine);
        let mut out = vec![0.0f32; 1024];
        let mut at = 0u64;
        // A fader, a pan, an effect mix and a knob move while blocks render;
        // a new song then supersedes them.
        let allocations = alloc_probe::count(|| {
            for block in 0..200u32 {
                if block == 20 {
                    ptx.send(tx.generation(), Target::MixerVolume { track: 1 }, 0.4);
                    ptx.send(tx.generation(), Target::MixerPan { track: 1 }, 0.3);
                    ptx.send(tx.generation(), Target::FxMix { track: 1, slot: 0 }, 0.5);
                    ptx.send(
                        tx.generation(),
                        Target::FxParam {
                            track: 1,
                            slot: 1,
                            param: 0,
                        },
                        3.0,
                    );
                }
                // The returned song borrows the receiver, so read the
                // generation and take the song again afterwards.
                if rx.update_with_changed().1 {
                    engine.clear_live_params();
                }
                prx.drain(rx.generation(), &mut engine);
                at = engine.process(rx.current(), 1, false, at, &mut out, true);
            }
        });
        assert_eq!(allocations, 0, "allocations or frees in the audio path");
        assert_eq!(
            engine.live_param(Target::MixerVolume { track: 1 }),
            Some(0.4)
        );
        tx.send(Arc::new(song));
        let (_, changed) = rx.update_with_changed();
        assert!(changed);
        engine.clear_live_params();
        assert_eq!(engine.live_param(Target::MixerVolume { track: 1 }), None);
    }

    #[test]
    fn the_indexed_song_mode_audio_block_neither_allocates_nor_frees() {
        use crate::sequence::{BAR, Clip};
        let mut song = Song::default();
        for s in 0..16 {
            song.add_note(1, Note::new(1, s * STEP, STEP, 60, 100));
        }
        for bar in 0..8 {
            song.clips.push(Clip::new(1, bar * BAR, 0));
        }
        let (mut tx, mut rx) = song_link(Song::default());
        let mut engine = Engine::new(48_000);
        engine.reserve(128, 1024);
        let mut out = vec![0.0f32; 1024];
        tx.send(Arc::new(song.clone()));
        let mut at = 0u64;
        let first = alloc_probe::count(|| {
            for _ in 0..100 {
                rx.update();
                at = engine.process_with_index(
                    rx.current(),
                    rx.hit_index(),
                    1,
                    true,
                    at,
                    &mut out,
                    true,
                );
            }
        });
        let mut edited = song.clone();
        edited
            .pattern_mut(1)
            .unwrap()
            .notes
            .push(Note::new(1, STEP / 2, STEP, 67, 100));
        tx.send(Arc::new(edited));
        let second = alloc_probe::count(|| {
            for _ in 0..100 {
                rx.update();
                at = engine.process_with_index(
                    rx.current(),
                    rx.hit_index(),
                    1,
                    true,
                    at,
                    &mut out,
                    true,
                );
            }
        });
        assert_eq!(
            (first, second),
            (0, 0),
            "allocations or frees in indexed song-mode audio path"
        );
        assert_eq!(tx.tick(), 1);
    }
}
