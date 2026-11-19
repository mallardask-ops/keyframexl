# MIDI + audio export/import research for KeyFrameXL

Date: 2026-11-15. Language: English.
Scope: research only. No repo code was changed by this task.

Codebase state this document refers to (files read in full or in the cited
regions on 2026-11-15):

- `src/sequence.rs` — `Song` (format version 2, `PPQ = 480`, `STEP = 120`,
  `BAR = 1920`), `Note { id, instrument, start, length, pitch, velocity,
  pan, release, fine, mod_x, mod_y, muted, color 0..15, group, porta }`,
  `Glide { start, length, target, group, instrument }`, `Clip { pattern,
  start, track, offset, length: Option<Tick>, muted }`, `InstrumentKind::
  Drum { voice 0..2 } | Sampler`, mixer (`INSERTS = 16`, `FADER_MAX = 1.9`).
- `src/project_io.rs` — `.kfxl` JSON envelope `{app:"keyframexl", doc:2,
  song}`, atomic save via temp file + `persist` + directory `fsync`;
  format-1 migration; `ensure_note_ids` + `validate`.
- `src/lib.rs` — `write_wav` writes **16-bit stereo only** at
  `SAMPLE_RATE = 48_000` via `hound`.
- `src/engine.rs:884-900` — `render()`: one pass of pattern or playlist,
  then continues while `active_voices() > 0`, capped by
  `MAX_TAIL_SECONDS = 30.0` (`src/engine.rs:16`).
- `src/ui/menu.rs:65-99` — FILE menu: New `Ctrl+N`, Open `Ctrl+O`,
  Open recent, Recover autosave, Save `Ctrl+S`, Save as `Ctrl+Shift+S`,
  Export → Wave `Ctrl+R` (enabled) / MIDI `Ctrl+Shift+M` (disabled,
  `LATER_MIDI`), Import → `.dwp` (enabled) / MIDI (disabled).
- `src/ui/mod.rs:773-789` — `export()`: `rfd` save dialog with fixed name
  `song.wav`, picks **playlist automatically when any clips exist**,
  renders synchronously on the UI thread, then writes 16-bit WAV.
- `src/ui/project.rs` — `Pending::{New, Open, Quit}`, "Save changes?"
  dialog, recent list (max 10, `recent.ron`), `autosave.kfxl` every 120 s
  off-thread, window title `"{name} *" / " — KeyFrameXL"`, dirty marker
  `· unsaved` in the menu bar (`src/ui/menu.rs:59-61`).
- `Cargo.toml` — `hound = "3.5"` (lock resolves 3.5.x), `rfd = "0.15"`,
  `cpal = "0.15"`.

---

## Executive summary

1. **MIDI library: use `midly 0.5.3`** (`midly = "0.5"`,
   https://docs.rs/midly, https://github.com/kovaxis/midly, MIT-ish — see
   §2). It is the only Rust crate that reads *and* writes SMF, handles
   running status, SMPTE timing, multi-track files and tempo maps, and is
   benchmarked an order of magnitude faster than the alternatives. `nodi`
   is a player built *on top of* `midly`, not a replacement; `rimd 0.0.1`
   is abandoned/low-level; `augmented-midi 1.8.0` is a message-level
   parser, not an SMF file library.
2. **Audio formats, in ship order: WAV (done, extend) → FLAC (`flacenc
   0.5.1`, pure Rust, Apache-2.0) → OGG Vorbis (`vorbis_rs 0.5.6`,
   BSD-3-Clause C bindings) → MP3 last (`mp3lame-encoder 0.3.0`, LGPL-3.0,
   bundled LAME 4.0).** MP3 is the only option with license consequences
   for a MIT project; keep it an optional feature or a documented
   follow-up. A pure-Rust Vorbis encoder (`rusty_vorbis 0.1.2`,
   Apache-2.0, released 2026-10-22) exists but is ~3 weeks old — watch it,
   do not depend on it yet.
3. **Biggest UX gaps vs FL Studio:** KeyFrameXL's export has no dialog at
   all (no mode/tail/bit-depth/normalize/dither/stems choice), renders on
   the UI thread (UI freezes on long songs), and auto-picks playlist vs
   pattern. Save lacks "Save new version" (`Ctrl+N` in FL, `name_2`),
   per-project backup history, and a zipped-with-samples archive. §4 gives
   concrete dialog specs; §5 stages the work (MIDI import+export first,
   then render dialog + worker thread, then FLAC/OGG, then MP3/stems/save
   extras).

---

## 1. FL Studio behaviour (ground truth from the manual + community docs)

Primary source: Image-Line manual "Exporting Audio & MIDI"
(https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/fformats_save_export.htm),
"Menu Bar / File" (…/html/menu_file.htm), "MIDI Import tool"
(…/html/automation_midiimport.htm), "Piano roll Import MIDI Data Tool"
(…/html/pianoroll_midi.htm), "File Search & Browser Settings" (…/html/envsettings_files.htm),
"Zipped Project File" (…/html/fformats_save_zip.htm). Shortcuts below are Windows.

### 1a. FILE → Export → audio/MIDI and the Rendering dialog

Entry points: `File > Export > Wave file` (`Ctrl+R`), `MP3 file`
(`Ctrl+Shift+R`), `OGG file`, `FLAC file`, `MIDI file…`; or `Tools >
Macros > Prepare for MIDI export` + Export dialog → MIDI. Flow is always
**file-save dialog first (proposes a name), then the Rendering settings
dialog, then `Start` with a progress bar**. Proposed name defaults to the
project name with the format extension
(e.g. community guide recommends `BeatName_BPM_Key_v1.wav`;
https://www.audeobox.com/learn/fl-studio/how-to-export-bounce-in-fl-studio).

Dialog sections (manual + GratuiTous/SoundShaper walkthroughs,
https://itsgratuitous.com/ultimate-guide-to-exporting-audio-in-fl-studio/,
https://www.soundshaper.org/blog/best-export-settings-fl-studio):

- **Project type / Mode:** `Full song` (renders the Playlist) vs `Pattern`
  (renders the currently selected Channel-Rack pattern). A Playlist
  time-selection overrides both (hierarchy: time selection > last
  time-marker past data > end of last bar containing data + Tail).
- **Tail** (what happens to decaying audio after the end point):
  - `Leave remainder` (default) — extends the render until decaying
    sounds (reverb/delay/release) die out; if it misses, add a Playlist
    Repeat marker to force a longer end.
  - `Wrap remainder` — renders starting from the last bar and mixes audio
    decaying past the end back into the *start*; for seamless loops. Only
    captures decay originating in the last bar.
  - `Cut remainder` — stops exactly at the end (loops without tails, or
    abrupt cuts).
  - Loop advice from the manual: use **WAV for loops**; MP3 encoders add a
    short silence at the start that breaks looping.
- **Output format (multi-select checkboxes, one render pass can write
  several formats at once):** WAV, MP3, OGG, FLAC, M4A/M4A variants per
  manual. Reported `Length` (bars), `Total Time` (H:M:S) and `Disk Space`
  (combined total across selected formats).
  - **WAV bit depth dropdown:** `16-Bit int` (CD-compatible, widest device
    support; for CD use 44.1 kHz), `24-Bit int` (DAW hardware/older DAWs),
    `32-Bit float` (native FL mix-engine format; for continued mixing /
    stems that need >0 dB headroom).
  - **MP3:** bitrate slider (guides recommend 320 kbps CBR for finals).
  - **FLAC bit depth:** 16 or 24 bit.
  - **Sample rate is NOT in the export dialog** — render uses the Mixer
    rate from `Options > Audio settings (F10)`.
- **Quality:** Resampling interpolation (`64-point sinc` default-ish up to
  `512-point sinc` for finals), `HQ for all plugins` (native plugins with
  an HQ render mode; VSTs via wrapper "notify about rendering mode"),
  `Disable maximum polyphony` (ignore per-channel Max-poly caps on
  render, but not Mono), `Dithering` — shaped dither, hiss mostly above
  10 kHz, applies to **16-bit WAV and MP3 only, and only once on the
  final render** (never stack with a limiter's dither).
- **Miscellaneous / Include:** `Split mixer tracks` (stems: one file per
  mixer track routed, directly or indirectly, to Master; Master effects
  are bypassed on the stems; each stem keeps its insert FX/fader),
  `Enable insert effects`, `Enable master effects`, `Trim PDC silence`
  (default on — removes plugin-delay-compensation latency silence at the
  head), `Normalize` (peak-normalize option in newer FL render dialogs;
  community mastering guides explicitly say *off* for premasters, peaks
  around −6…−3 dBFS), `Save project bones`-style extras.
  Second stems route: `File > Export > All playlist tracks` (per-playlist-
  track files; `From song start` so all files line up).
- **Render progress UI:** modal progress bar with Cancel; render is a
  non-real-time ("rendering") process, slower than playback, quality
  higher than live monitoring. On completion FL offers the containing
  folder / plays the result depending on version.

Filename/rules to copy: propose `<project>[_pattern|_song][_24bit].wav`,
never silently overwrite — the OS save dialog already confirms; when
writing stems, `one file per mixer track named "<MixerTrackName>"` in the
chosen folder plus optional `Master` rough mix.

### 1b. MIDI import — dialogs and mapping

Two distinct routes (manual "MIDI Import tool" + "Piano roll Import MIDI
Data Tool"; corroborated by flstudiopro.com 2026-09-10 and myloops.net
guides):

1. **Piano roll route (single part):** open the target channel's Piano
   roll → menu `File > Import MIDI file` (`Ctrl+M`). Adds notes to *that*
   Piano roll; **controller data is dropped** on this route. Options:
   `Which Tracks to Import`, `Which Channels to Import` (1–16). `Blend
   with existing data` on = merge, off = replace.
2. **Project route (multitrack):** `File > Import > MIDI file`, or drop
   the `.mid` onto the FL desktop / empty Channel-Rack area (Shift =
   skip dialog, Alt = force dialog). Dialog options:
   - `Which Tracks to Import` (named tracks / instruments in the file),
     `Which Channels to Import` (red = included),
   - `Channel type`: `FLEX` (General-MIDI bank), `MIDI Out`
     (per-track channel), `MIDI Out + Fruity LSD` (Windows GM synth),
   - `Blend with existing data`, **`Start new project`** (with it off,
     existing instrument data in the target pattern is **overwritten** —
     warn!),
   - **`Create one channel per track`** (one Instrument channel per MIDI
     channel/track; only on this route, not on drops onto a single
     channel), `Set mixer tracks for new channels` (route new channels to
     consecutive free mixer tracks),
   - `Realign events` (strip leading silence; keep consistent between a
     notes pass and a CC pass or they desync),
   - `Import time signatures` (adds markers; does **not** import the tempo
     map — FL import is documented as not reliably importing tempo; verify
     by ear),
   - `Import zero velocity notes` (treat vel-0 Note-On as Note-On instead
     of Note-Off).

Mapping notes → FL model:

- Note-On pitch → piano-roll key; velocity → note velocity; delta-time →
  position via PPQ (FL internal PPQ is 96; SMF division is converted).
- **Note color ↔ MIDI channel:** each of the 16 piano-roll colors is
  locked to one MIDI channel (hover shows the channel; piano-roll manual).
  Multichannel data in one roll = colored overlaps.
- **Channel 10 → drums:** GM convention; FL guides say consolidate drums
  onto one FPC channel set to MIDI channel 10 for correct GM playback.
- Program changes select GM patches per channel (Acoustic Grand = 0
  default; guides map bass ≈ 39–40, pads/strings ≈ 49–55/89–96).
- Sustain (CC64) and pitch-bend/mod-wheel live in the Piano-roll event
  editor lanes; velocity, fine pitch (±100 cents), release, pan per note
  exist as note properties (channelrack manual) — CC lanes that have no
  per-note equivalent (mod wheel) stay as automation, not note data.
- Time signature events → playlist/piano-roll markers; tempo map is
  *not* trusted on import (see above).

### 1c. MIDI export

- **Single pattern:** Piano-roll menu `File > Export as MIDI file…`
  (`Shift+Ctrl+M`); default folder `Documents/Image-Line/FL
  Studio/Presets/Scores` (or the project data folder). Exports note data
  only — no sounds, no audio clips.
- **Whole project:** destructive prep required: **save first**, then
  `Tools > Macros > Prepare for MIDI export` converts every channel to an
  auto-configured `MIDI Out` (no undo — project no longer plays its own
  instruments), set per-channel MIDI channel/patch (drums → channel 10),
  then Export dialog → MIDI → `Start`. Manual warns: *don't save the
  project in this state*; save to a new project if needed.
- Lesson for KeyFrameXL: never do destructive in-place conversion —
  build the SMF from the song model directly (§2 mapping), no model
  mutation.

### 1d. Save / Save as / Save new version / backups / unsaved marker / zipped package

- `Save (Ctrl+S)` overwrites; `Save as (Shift+Ctrl+S)` new name/format;
  **`Save new version (Ctrl+N)`** — auto-increments: `MyProj` →
  `MyProj_2` → `MyProj_3`, for permanent sequential manual backups
  (manual "Menu Bar / File"; support article ans=647 explicitly recommends
  Ctrl+S habit + Ctrl+N versions).
- **Backup/Autosave** (`Options > File settings > Backup`): Never (remind
  every 10 min) / Rarely (15 min) / Occasionally (10 min) / Regularly
  (5 min) / Frequently (5 min + before risky ops, even while playing) /
  Very frequently (1 min + risky ops). Backups of current + overwritten
  files go to `Documents/Image-Line/FL Studio/Projects/Backup` (global) or
  the project data folder; capped by `Maximum` count (oldest deleted);
  `Revert to last autosave` in File menu; autosave postponed while
  playing except in Frequently/Very-frequently modes.
- **Title/unsaved marker:** FL shows `*` / modified state and project name
  in the title and hint bar; KeyFrameXL already has `"{name} *" + " —
  KeyFrameXL"` (`src/ui/project.rs:115`) and `· unsaved` in the menu bar
  (`src/ui/menu.rs:59-61`) — keep both, add `*` semantics parity check in
  tests.
- **Zipped loop package** (`File > Export > Zipped loop package`, `.zip`):
  the `.flp` + all samples/presets used (sampler channels + audio clips;
  *not* 3rd-party sampler content unless monolithic, *not* Slicex/Edison
  loop state). Opens directly from FL's File menu without extracting.
  Alternative: per-project **data folder** (`Project settings > Data
  folder`, `Auto` = beside the `.flp`) + `Copy used samples to the
  project folder`. KeyFrameXL equivalent: zip `.kfxl` + referenced sample
  files (sampler presets embed PCM today, so the zip is mostly future-
  proofing for external sample references).

---

## 2. Rust crates for Standard MIDI Files

### Recommendation: `midly 0.5.3`

`midly = "0.5"` — https://docs.rs/midly (crate page
https://docs.rs/crate/midly/latest), repo
https://github.com/kovaxis/midly, ~231k recent downloads, the fastest
option in the author's benchmark table (Pi.mid 24 MB: midly 60 ms vs
augmented-midi 214 ms vs rimd 20 s). Features used: default (`std`,
`alloc`, `parallel`/rayon — keep default; `parallel` only affects large
parses).

API surface that matters (all verified on docs.rs/midly/latest):

- `midly::Smf<'a> { header: Header { format: Format, timing: Timing },
  tracks: Vec<Track<'a>> }`; `Smf::parse(&bytes)`, `smf.save(path)`,
  `smf.write(&mut vec)` / `write_std(w)`; zero-copy borrows of the input
  buffer (read file fully, then parse — two steps).
- `Timing::Metrical(u15)` = ticks/beat (**PPQ**); `Timing::Timecode(Fps,
  u8)` = SMPTE ticks/second (Fps ∈ {Fps24, Fps25, Fps29, Fps30}).
  Running status is handled inside the parser (writer emits it
  automatically); you never touch it.
- `TrackEvent { delta: u28, kind: TrackEventKind }`,
  `TrackEventKind::{Midi{channel: u4, message}, SysEx(&[u8]),
  Escape(&[u8]), Meta(MetaMessage)}`.
- `MidiMessage::{NoteOn{key: u7, vel: u7}, NoteOff{key, vel},
  ProgramChange{program}, PitchBend{value}, Controller{controller, value},
  …}` — note **`NoteOn` with `vel == 0` is delivered as `NoteOn`, and per
  the SMF spec (https://midi.community/docs/smf/) it MEANS Note-Off**;
  you must normalize it yourself (FL even has an "Import zero velocity
  notes" toggle for the opposite choice).
- `MetaMessage::{Tempo(u24)` = µs/quarter, `TimeSignature(n, d, clocks,
  thirtyseconds)`, `TrackName(&[u8])`, `InstrumentName`, `EndOfTrack`
  (obligatory, writer does not add it for you — push it), `SmpteOffset`,
  `KeySignature`, `MidiChannel`, `MidiPort`, `Text/Lyric/Marker/CuePoint`,
  `SetTempo` alias docs…`}`.
- `Format::{SingleTrack, Parallel, Sequential}`; `Header::new(format,
  timing)`; `u4/u7/u15/u24/u28` newtypes with `::from(u8)` + `.as_int()`.
- `TrackEventKind::as_live_event()` for converting file events to live
  (lossy: drops meta/escapes) — useful if KeyFrameXL ever forwards MIDI to
  `midir`.

Why not the alternatives:

- **`nodi`** (https://github.com/insomnimus/nodi, https://docs.rs/nodi,
  MIT) — a *player/abstraction* (time-map, join/merge tracks, play via
  `midir`) that re-exports and depends on `midly`. Use it only if/when a
  "play external MIDI clock"-style feature needs scheduling; not an SMF
  read/write replacement.
- **`rimd 0.0.1`** (https://docs.rs/rimd) — stores messages as raw
  `Vec<u8>`, manual meta builders, no meaningful maintenance; author's own
  benchmark: slowest by 2 orders of magnitude and fails on several test
  files. Reject.
- **`augmented-midi 1.8.0`** (https://docs.rs/augmented-midi, part of
  augmented-audio) — nom/cookie-factory combinators for *single-message*
  parse/serialize, no SMF container model, only ~15% documented. Reject
  for file I/O (fine if someone later needs `no_alloc` wire parsing).

### Mapping SMF ↔ KeyFrameXL model (PPQ 480)

Import (`Timing::Metrical(ppq)`; reject or special-case
`Timing::Timecode` — see edge cases):

```rust
use midly::{Smf, Timing, TrackEventKind, MidiMessage, MetaMessage};

// ticks_src -> ticks_kfxl, exact for whole steps at common PPQs:
let kfxl_tick = |t: u64, ppq_src: u16| -> Tick {
    (t as i64 * PPQ as i64) / ppq_src as i64   // PPQ=480 divisible by 2,3,4,5,6,8
};
```

- Walk every track accumulating absolute ticks (`abs += ev.delta`).
- **Tempo map:** collect all `MetaMessage::Tempo` events with their
  absolute tick into a map; KeyFrameXL `Song` has one global `tempo`
  (`src/sequence.rs:330`). Rule: `Song.tempo = first tempo event`
  (default 500 000 µs/qn = 120 BPM when absent, matching the SMF default);
  if later tempo events exist, either (a) warn "tempo changes flattened"
  or (b) refuse multitrack-tempo files in v1 — FL itself does not
  reliably import tempo maps (§1b), so flattening with a warning is
  industry-consistent. Store nothing else; do not invent automation.
- **Notes:** state machine per (channel, key): `NoteOn vel>0` opens;
  matching `NoteOff` *or* `NoteOn vel==0` closes (duration = close −
  open). Overlapping same-key Note-Ons: close the previous note at the new
  note's start (FIFO, per https://midi.community/docs/smf/ pitfalls) so
  `validate()` (no zero/negative lengths, `1..=127` velocity) always
  passes. Unclosed notes at EndOfTrack: close at track end. Stray
  Note-Offs: ignore.
- **Velocity** maps 1:1 (`u7` → `u8`); KeyFrameXL rejects velocity 0
  (`src/sequence.rs:482`), so a zero-length guard (`length.max(1)`) plus
  skipping vel-0-noteon-as-note is required.
- **Channels:** FL parity — `Create one channel per track` equivalent =
  one KeyFrameXL `Instrument`/pattern-notes group per SMF track (or per
  MIDI channel when a track mixes channels; merge rule: group by
  (track, channel)). New channels named from `TrackName`/`InstrumentName`
  meta events, else `"MIDI Ch N"`. **`channel 9 (0-based) = GM drums`** →
  route to a `Drum` channel or a sampler kit, never to a pitched sampler
  (GM convention, §1b; KeyFrameXL `drum_voice` is 0..2 kick/snare/hat —
  map GM 36/38/42 primarily, everything else to nearest voice or a new
  sampler channel per pitch class).
- **ProgramChange:** recorded per channel; seed the new channel's name
  (`"GM 39 Synth Bass"`) — KeyFrameXL has no GM engine, so this is a label
  only, exactly like FL's post-`Prepare for MIDI export` state.
- **PitchBend/CC:** `PitchBend` → ignored in v1 (or averaged into
  `Note.fine` ±100 cents for single-bend notes — lossy, so prefer ignore +
  count in the import report); CC64 sustain extends note lengths;
  ModWheel (CC1) → `mod_y` is *not* a faithful mapping (FL keeps it as
  automation), so v1: ignore with a report count. Never silently drop —
  the import report must say "ignored N pitch-bend / M CC events".
- **TimeSignature:** import as informational only (KeyFrameXL has no meter
  model; everything is 4/4 bars) — same spirit as FL's `Import time
  signatures` toggle, minus the destination.
- **SMPTE:** `Timing::Timecode(fps, subframes)` files (absolute-time,
  rare in music) → v1: reject with "SMPTE-timing MIDI not supported"
  (midly parses them fine; *we* choose not to map them to bars/beats).
  `MetaMessage::SmpteOffset` → ignore with report count.

Export (KeyFrameXL → SMF, format 1, `Timing::Metrical(480)`):

```rust
use midly::{Smf, Header, Format, Timing, TrackEvent, TrackEventKind,
            MidiMessage, MetaMessage, num::{u4, u7, u28}};

let header = Header::new(Format::Parallel, Timing::Metrical(480.into()));
let mut smf = Smf::new(header);
// track 0: tempo map (convention: first track holds tempo, §1b/§2 spec notes)
let mut tempo_track: Vec<TrackEvent> = vec![
    TrackEvent { delta: 0.into(), kind: TrackEventKind::Meta(MetaMessage::Tempo((60_000_000.0 / song.tempo) as u32 .into())) },
    TrackEvent { delta: 0.into(), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) },
];
// one track per instrument; muted notes/channels skipped (or velocity 0? NO — skip, FL mutes are not MIDI)
for (ch, inst) in song.instruments.iter().enumerate() {
    let mut evs: Vec<(u64, TrackEventKind)> = vec![];
    for n in pattern.notes.iter().filter(|n| n.instrument == inst.id && !n.muted) {
        evs.push((n.start as u64, TrackEventKind::Midi { channel: (ch as u8 % 16).into(),
            message: MidiMessage::NoteOn { key: n.pitch.into(), vel: n.velocity.into() } }));
        evs.push(((n.start + n.length) as u64, TrackEventKind::Midi { channel: (ch as u8 % 16).into(),
            message: MidiMessage::NoteOff { key: n.pitch.into(), vel: 64.into() } }));
    }
    // sort by abs tick, NoteOff before NoteOn at equal ticks, then delta-encode:
    evs.sort_by_key(|(t, k)| (*t, matches!(k, TrackEventKind::Midi { message: MidiMessage::NoteOn {..} }) as u8));
    // ... delta-encode, push EndOfTrack, smf.tracks.push(evs)
}
smf.save(path)?; // or write_std for Vec<u8> then atomic persist like project_io::save
```

- `Note.color 0..15` → MIDI channel when exporting a *single* pattern
  (FL parity: colors appear to plugins as channels, §1b); when exporting
  the whole song, channel = instrument index (≤16 instruments → direct;
  more → wrap or split to extra tracks with a warning; KeyFrameXL default
  song has 4, so this is an edge case).
- Slides (`Glide`)/`porta`/`mod_x/y`, swing, mixer, clips arrangement:
  not representable — document as export limitations; song-mode export
  flattens clips to absolute ticks (same flattening the audio renderer
  does in `engine.rs`).
- Filenames: FL proposes project-named `.mid`; KeyFrameXL should propose
  `<project>.mid` (pattern) / `<project>_song.mid` (song).

---

## 3. Audio export formats in Rust

Current state: `keyframexl::write_wav` = 16-bit int stereo @ 48 kHz via
`hound` (`src/lib.rs:58-75`). `hound 3.5.1`
(https://docs.rs/crate/hound/latest, https://crates.io/crates/hound,
Apache-2.0) already supports **8/16/24/32-int + 32-float** read/write
(`WavSpec { channels, sample_rate, bits_per_sample, sample_format }`;
`SampleFormat::Int/Float`) — so WAV 16/24/32f needs **no new dependency**,
only writer-parameter plumbing plus dither for 16-bit.

| Format | Crate (version, license, link) | Status / notes for KeyFrameXL |
|---|---|---|
| WAV 16/24/32f | `hound 3.5.1`, Apache-2.0, https://docs.rs/hound | Already in tree. Add spec param; 24-bit via `i32` samples shifted (hound writes 24-bit from `i32`); 32f via `f32` samples + `SampleFormat::Float`. |
| FLAC 16/24 | `flacenc 0.5.1`, Apache-2.0, https://docs.rs/crate/flacenc/latest, repo https://github.com/yotarok/flacenc-rs | **Pure Rust, recommended.** `MemSource::from_samples(&[i32] interleaved, channels, bits, rate)` + `encode_with_fixed_block_size` + `ByteSink`; `par`+`log` default features (multithreaded). Decode feature is experimental — only need encode. MSRV 1.83. |
| OGG Vorbis q4–q6 | `vorbis_rs 0.5.6`, BSD-3-Clause, https://crates.io/crates/vorbis_rs (bindings to aoTuV+Lancer-patched libvorbis + libogg; MSRV 1.87) | **Recommended for OGG.** `VorbisEncoderBuilder::new(rate, channels, &mut out)` → `encode_audio_block(planar f32)` loop → `finish()` on drop. C deps via vendored sys crates (`aotuv_lancer_vorbis_sys`, `ogg_next_sys`); cross-compile/static-link story is fine on Linux, verify before promising. |
| OGG Vorbis (pure Rust, future) | `rusty_vorbis 0.1.2`, Apache-2.0, https://crates.io/crates/rusty_vorbis (2026-10-22) + `ogg` crate https://github.com/RustAudio/ogg for the container | Encoder validated vs `lewton`/ffmpeg; API is packet-level (`push_pcm_f32` → `next_packet`), **caller must mux Ogg pages** — more integration work, brand-new crate. Watch; revisit in 6–12 months. (`oxideav-vorbis` is a framework piece, not a drop-in.) |
| MP3 CBR/VBR | `mp3lame-encoder 0.3.0` + `mp3lame-sys 0.2.0` (bundled LAME 4.0, July 2026), **LGPL-3.0**, https://crates.io/crates/mp3lame-encoder, https://github.com/DoumanAsh/mp3lame-encoder | Works (`Builder::new → set_num_channels/rate/brate/quality → build → encode(DualPcm) → flush::<FlushNoGap>`), but **LGPL infects the binary's licensing story** (dynamic-link + attribution + publish modifications; https://github.com/lameproject/lame/blob/master/LICENSE). KeyFrameXL is MIT (`Cargo.toml`). Options: (a) optional cargo feature `mp3` (default off) with a build-time notice; (b) defer MP3 until after WAV/FLAC/OGG; (c) shell out to user-installed `lame` (no). Recommend (a)+(b): gate behind feature, document. |

Dither, normalize, tail (all format-independent, implement once in a shared
`export` module):

- **Dither (TPDF) for 16-bit only:** engine renders `f32` in −1..1.
  Quantizing to 16-bit without dither = truncation distortion. Add
  triangular-probability-density-function dither: `out = (s * 32768 +
  (rand1 − rand2)) as i16` with two uniform `fastrand`/std RNG draws per
  sample, *before* clamping; shaped dither (like FL's above-10 kHz shape)
  is a non-goal for v1. Apply **exactly once, only for 16-bit int**
  (FL manual rule, §1a); never for 24/32f/FLAC; never stack with a
  limiter-dither (we have no mastering dither, so no conflict).
- **Normalize:** two options, FL-style. (1) Peak normalize to e.g. −1 dBFS
  (find |peak|, gain = 10^(−1/20)/peak, apply if peak > 0). (2) Off
  (default — mastering guides in §1a say off for premasters). No loudness
  (LUFS) normalization in v1; that needs a K-weighting filter + gated
  integration (EBU R128) — list as follow-up.
- **Tail rendering:** `engine::render` already does the right thing —
  render the loop length, then keep processing while `active_voices() > 0`
  up to a 30 s cap (`src/engine.rs:884-900`). Map FL tails onto it:
  `Leave remainder` = current behaviour; `Cut remainder` = stop at loop
  end (skip the tail loop); `Wrap remainder` = render last-bar + tail,
  then mix the tail audio into the head (implement as post-process on the
  `Vec<[f32;2]>`). Add a *silence threshold* so a 30 s whisper doesn't
  bloat files: stop the tail when the last N ms (e.g. 100 ms) peak below
  −90 dBFS — currently any active voice (even inaudible release) holds the
  tail open. `max_release` (`src/sequence.rs:420-423`, ≥15 ms) is the
  minimum tail floor.
- **Sample rate:** KeyFrameXL is fixed at 48 kHz (`RATE`, status bar shows
  `48 khz · ppq 480`). FL renders at the mixer rate. Keep 48 kHz for all
  formats in v1 (CD-compat resampling to 44.1 k is a separate feature;
  note it in the dialog as informational text).

---

## 4. What "normal" save/export UX should look like for KeyFrameXL

Conventions to adopt (FL + desktop norms): `Ctrl+S` save,
`Ctrl+Shift+S` save as, **`Ctrl+N` save-new-version** (note: KeyFrameXL
currently uses `Ctrl+N` for *New project* — FL uses it for *Save new
version*; resolve by moving New-project to nothing/elsewhere or picking
`Ctrl+Alt+S` for new-version; decision needed, default to FL parity:
`Ctrl+N` = new version, New project keeps menu item + no shortcut or
`Ctrl+Shift+N`), `Ctrl+R` export audio, `Ctrl+Shift+M` export MIDI
(already reserved in the menu, `src/ui/menu.rs:86`), `F1` shortcuts list
updated for every new binding.

### 4a. Save / Save as / Save new version / backups

- **Save (`Ctrl+S`):** current behaviour is correct (atomic temp+persist,
  `src/project_io.rs:16-25`). Add: error dialog with Retry/Choose-folder
  on failure (disk full/permissions), not just a status line.
- **Save as (`Ctrl+Shift+S`):** keep `rfd` dialog, filter `*.kfxl`,
  propose current file name. After save: `path` + recent + title update
  (already done in `save_to`, `src/ui/mod.rs:724-734`).
- **Save new version (`Ctrl+N` proposed):** if `path` is `foo.kfxl`,
  save `foo_2.kfxl` (then `_3`, …; FL rule, §1d); if untitled, fall back
  to Save as. Status: `Saved foo_2.kfxl — Ctrl+Z does not undo saves`
  (saves are intentionally outside the undo stack; say so).
- **Backups:** extend the existing 120 s autosave (`src/ui/project.rs`)
  with: (1) keep N (default 5) timestamped copies
  `~/.local/share/keyframexl/backup/<proj>-<ts>.kfxl` instead of one
  `autosave.kfxl` (single file loses history; FL keeps `Maximum` N);
  (2) `FILE > Revert to last backup…` listing them by age (reuse
  `recover_item` pattern, `src/ui/project.rs:197-218`); (3) never autosave
  while rendering/exporting (FL postpones while playing).
- **Unsaved marker:** keep both existing indicators (title `*`,
  menu-bar `· unsaved`); add "Save changes?" coverage for Export/MIDI
  destructive ops (none are destructive, so no new prompts needed —
  assert this in tests).
- **Zipped project with samples:** `FILE > Export > Zipped project
  (.zip)…` producing `<name>.zip` with `project.kfxl` + `samples/…`
  (today presets embed PCM, so the zip initially just future-proofs
  external sample references; still worth it for sharing). Needs `zip`
  crate (pure Rust, Apache-2.0) — new dependency, small.

### 4b. Audio export dialog (replaces today's one-click `export()`)

Modal `egui::Window` after the `rfd` save-file dialog (FL order: file
first, settings second — keep it, users already accepted a path):

- **Source:** radio `Pattern "…" | Song (playlist)` — default Song iff
  `!song.clips.is_empty()` (today's heuristic, `src/ui/mod.rs:779`),
  user-overridable; shows bars + `M:SS.mmm` + estimated frames for each.
- **Tail:** radio `Leave remainder (default) | Cut | Wrap (loops)` with
  one-line explanations (§1a).
- **Format (multi-select like FL):** checkboxes `WAV`, `FLAC`, `OGG`;
  `MP3` visible but disabled with tooltip until the `mp3` cargo feature
  is enabled. Per-format controls appear when checked: WAV bit depth
  `16 (CD) | 24 | 32 float (default 24 for stems? default 16 for finals —
  pick 24 default, CD note for 16)`; FLAC bits `16|24`; OGG quality
  slider `q −1…10 (default 4)`; MP3 bitrate `192|256|320 (default 320)`.
- **Options:** `Normalize to −1 dBFS peak [off default]`, `Dither
  (16-bit WAV only) [on, auto-disabled for other depths]`,
  `Split mixer tracks (stems) [off]` → writes `<name>_<Track>.wav` per
  non-muted insert + Master (FL rule: Master FX bypassed on stems — for
  us, render each insert pre-master; document), `Include metronome/click
  [absent — no click exists; omit, don't stub]`.
- **Footer:** `Disk space` estimate (frames × bytes-per-sample × formats),
  `Start` (primary) / `Cancel`. Validation: unwritable path, disk-full
  estimate, clipping indicator (peak ≥ 0 dBFS pre-normalize → warning
  text, not a blocker).
- **Progress:** render runs on a **worker thread** (`std::thread` +
  `mpsc` progress every N blocks, like `autosave`'s spawn pattern in
  `src/ui/project.rs:139-146`); dialog shows determinate bar (0–100%),
  current stage (`Rendering… 42%`, `Encoding FLAC…`), `Cancel` (drops the
  receiver, worker checks an `AtomicBool`). Result/failure returns to a
  status message (`Tone::Done/Bad`, existing `say()` pattern) + offer
  "Show in folder" (open parent via `xdg-open`; omit if brittle — decide
  in implementation).
- **Error states:** render panic → `Not exported: render failed: …`;
  encode error → which format failed, partial files listed (and kept —
  FL keeps completed formats); dialog stays open on failure with `Retry`.

### 4c. MIDI import/export dialogs

- **Import (`FILE > Import > MIDI file…`, enable the disabled item,
  `src/ui/menu.rs:93`):** `rfd` open (`*.mid, *.midi`) → options window:
  `Create one channel per track [on]` (FL parity; off = merge all into
  current pattern's first channel), `Which tracks [all checked listbox
  from TrackName metas]`, `Start new project [off, danger-styled with
  "current work is kept in undo" note — better than FL's overwrite!]`,
  `Target: current pattern | new pattern [default new]`, `Flatten tempo
  to first (BPM shown) [on, disabled+tooltip when single tempo]`,
  `Drums: channel 10 → Drum channels [on]`. Preview line: `12 tracks, 4
  tempo changes, 1 204 notes → 3 new channels`. Import report afterwards
  (status + details popup): created channels, flattened tempos, ignored
  pitch-bend/CC counts, unclosed-note repairs. Invalid file →
  `Cannot import …: Not a MIDI file (bad header)` etc.
- **Export (`FILE > Export > MIDI file…`, `Ctrl+Shift+M`):** `rfd` save
  (`*.mid`, propose `<project>.mid`/`_song.mid`) → tiny options window:
  `Source: current pattern | song (flatten clips)`, `One track per
  channel [on]`, `Include muted: no [locked off in v1]`, `Channel 10 for
  drum channels [on]`; `Start`. Non-destructive by construction (§1c
  lesson) — assert song equality after export in tests.

### 4d. Testing each automatically

- Headless golden tests (no GUI): build `Song::demo()` variants,
  run the same pure functions the dialogs call (`build_export_plan`,
  `render_and_encode`, `import_smf`, `export_smf`), assert on bytes/files
  in `tempfile` dirs. MIDI round-trip: `export → import → export`
  byte- or event-compare (§5). Audio: decode our own WAV/FLAC/OGG back
  (via `hound`/`claxon`-side decode or `symphonia`) and compare peak/RMS
  within lossy tolerance; assert tail length vs `Cut`; assert
  `Wrap` energy conservation (head+tail ≈ leave-remainder total ±1%).
- GUI-level: existing `KFXL_CAPTURE_*` screenshot harness (`App::new`
  capture branches, `src/ui/mod.rs:259-402`) gains
  `KFXL_CAPTURE_EXPORT=1` (open export dialog on demo song) and
  `KFXL_CAPTURE_MIDI_IMPORT=path.mid`; scripts in `scripts/` drive
  `r.*click()` like current live checks; `docs/ui-rules.md` screenshot
  rules apply. Shortcut tests: simulate `Ctrl+R`/`Ctrl+Shift+M` key
  events in `menu_tests.rs`-style unit tests asserting the dialog state
  opens (not the `rfd` native part — gate native dialogs behind a
  `#[cfg(test)]`-injectable file-picker trait or `KFXL_NO_NATIVE_DIALOG`
  env in tests).
- Error-state tests: read-only dir, zero-byte `.mid`, truncated track,
  SMPTE-timing file, >16-channel song, 30 s-tail song with cap assertion.

---

## 5. Staged implementation plan (estimates, risks, tests)

Assumptions: single dev, familiar with the codebase, Linux/Wayland/X11
egui as today. Estimates in focused work-blocks; testing included per
stage. Order minimizes user-visible breakage (additive menus/dialogs;
`export()` stays until the new dialog ships).

### Stage 1 — MIDI export (pattern + song), no import (3–5 days)

- New module `src/midi.rs` (pure, GUI-free): `song_to_smf(&Song,
  Scope, MidiExportOpts) -> midly::Smf<'static>`-ish (own the bytes:
  build with `Arena` or construct owned then `write_std`; midly events
  borrow — simplest: build `Smf` then `write_std` into `Vec<u8>`
  immediately, return bytes) + `save_mid(path, bytes)` reusing the
  atomic temp+persist pattern from `project_io::save`.
- Wire `FILE > Export > MIDI file…` (`Ctrl+Shift+M`): `rfd` save +
  minimal options (scope radio, ch10-drums checkbox) → worker thread
  (even though fast — establishes the pattern for stage 2) → status.
- Tests: golden `.mid` fixtures (export demo pattern, compare event
  list via `midly` re-parse: pitches/velocities/ticks exact); PPQ-480
  identity test; vel-0 never emitted; `EndOfTrack` present per track;
  drum-channel-10 test; song-flattening test (clip offset/length →
  absolute ticks; muted clips/notes excluded); **non-mutation test**
  (song JSON identical before/after export).
- Risks: low. midly API churn — pin `midly = "=0.5.3"` initially, relax
  later. Lifetime ergonomics — contained by bytes-out design.

### Stage 2 — MIDI import + round-trip (4–6 days)

- `parse_mid(bytes) -> ImportPlan { channels, notes per pattern,
  tempo_first, warnings }` + `apply_import(&mut Song, plan, ImportOpts)`
  (undoable via existing `checkpoint()`); enable `FILE > Import > MIDI
  file…` with the §4c dialog; import report window.
- Tests: fixtures — single-track melody, multitrack (incl. ch10 drums),
  vel-0-noteoffs, overlapping same-key, multi-tempo (flatten warning),
  SMPTE-timing (rejected), corrupt/truncated (error strings);
  **round-trip**: `demo → export → import into fresh Song → export →
  event-equal` (modulo channel naming + ignored CC); PPQ conversion
  property test (ticks scale exactly for PPQ ∈ {96, 192, 384, 480, 960});
  import-then-`validate()` must pass on all fixtures.
- Risks: medium. Tempo-map flattening surprises → mitigate with explicit
  preview line + report. >16 channels → defined wrap rule + warning.
  Malformed files must never panic — fuzz `parse_mid` with 1–2 h of
  `cargo fuzz` if available, else a corpus of truncated mutations.

### Stage 3 — Render dialog + worker thread + WAV depths/dither/normalize (5–8 days)

- Extract `export()` into `src/export.rs`: `ExportPlan`, `render_stems`,
  `apply_tail(Tail)`, `tpdf_dither`, `peak_normalize`, `write_wav_depth`
  (16/24/32f via hound), progress callback; UI dialog per §4b (WAV only);
  worker thread + cancel; keep old `Ctrl+R` one-click path delegating to
  dialog-with-defaults until users approve removal.
- Tests: bit-depth goldens (read back with hound: spec + frame count);
  dither-once property (16-bit output differs from truncation on
  low-level sine; 24-bit path bit-exact to float×2²³ rounding);
  normalize peak == −1 dBFS ±0.01 dB; tail tests — `Leave` length >
  loop, `Cut` length == loop exactly, `Wrap` energy conservation;
  silence-threshold test (sub −90 dBFS tail doesn't extend file);
  cancel test (worker stops, no partial file left); progress monotonic.
- Risks: medium. Threading + egui state — keep worker 100% song-owned
  (clone `Song` in, like autosave does). Long renders block-free but use
  CPU — cap: single thread, no SIMD work in v1.

### Stage 4 — FLAC (+ OGG if smooth) (3–5 days FLAC; +3–4 OGG)

- `flacenc 0.5.1` behind the dialog checkbox; map `f32 → i32` (16/24-bit
  range check + clip count in report); verify output decodes (test-only
  dep on a decoder, e.g. `claxon`, or `flacenc`'s own `decode` feature in
  dev-dependencies). OGG via `vorbis_rs 0.5.6` (bundled C build — confirm
  `cargo build` on the CI image + Wayland/x11 target first; if the C
  toolchain bites, defer OGG to the `rusty_vorbis`-matures track).
- Tests: FLAC round-trip bit-exact (decode → sample-equal); OGG
  decode-tolerance (RMS error < −40 dB vs source, duration ±1 block);
  multi-format single render (WAV+FLAC in one pass — shared PCM buffer).
- Risks: medium-low for FLAC (pure Rust); medium for OGG (C deps, MSRV
  1.87 vs project's toolchain — check `rustc --version` in CI).

### Stage 5 — MP3 (gated), stems polish, save extras (4–7 days, deferrable)

- MP3 behind default-off cargo feature `mp3` (`mp3lame-encoder 0.3.0`,
  LGPL-3.0): dialog enables the checkbox only with the feature;
  document license implication in README + dialog tooltip; bitrate
  320 default. Stems: per-insert render loop + `<name>_<Track>.wav`
  naming + Master-bypass rule + stems-in-one-folder; playlist-track
  stems explicitly out of scope (FL has them; we have no audio clips).
  Save extras: `Ctrl+N` new-version (shortcut decision, §4a), N-file
  backup rotation + revert UI, zipped project export (`zip` crate).
- Tests: MP3 smoke (file decodes, duration sane — no exactness claim);
  feature-off build test (`--no-default-features`-ish matrix);
  stems file-count/naming + Master-bypass (stem mix ≠ master mix when
  Master FX present); new-version increment sequence; backup rotation
  cap; zip contains `project.kfxl` + manifest.
- Risks: **licensing (MP3/LGPL)** — the one non-technical risk; get an
  explicit decision before merging. Stems CPU cost — sequential renders,
  progress per stem.

### Cross-stage test ideas (recap for planning)

- Round-trip MIDI export→import equality (event-level, §5 stages 1–2).
- Render length/tail tests (`Leave/Cut/Wrap`, silence threshold, 30 s
  cap from `engine.rs:16`).
- Bit-depth tests (spec goldens, dither-once, 24-bit exactness).
- Dialog/UX tests via `KFXL_CAPTURE_EXPORT`/`KFXL_CAPTURE_MIDI_IMPORT`
  screenshots + `menu_tests.rs`-style shortcut tests with native dialogs
  stubbed.
- Fuzz/corpus tests for `parse_mid`; read-only-dir and disk-full error
  paths; `validate()` green on every import fixture.

---

## Sources

- Codebase: `src/sequence.rs`, `src/project_io.rs`, `src/lib.rs`,
  `src/engine.rs` (`render`, `MAX_TAIL_SECONDS`), `src/ui/menu.rs`,
  `src/ui/mod.rs` (`export()`), `src/ui/project.rs`, `Cargo.toml` /
  `Cargo.lock` (`hound 3.5.x` in tree).
- FL manual: fformats_save_export.htm (render dialog, tails, bit depths,
  dither, stems), menu_file.htm (save variants), automation_midiimport.htm
  + pianoroll_midi.htm (import options), envsettings_files.htm (backups),
  fformats_save_zip.htm (zipped package), pianoroll_menu.htm (export as
  MIDI, `Shift+Ctrl+M`), pianoroll.htm (colors = MIDI channels),
  channelrack.htm (per-note pan/release/fine/mod).
- Guides: audeobox.com/learn/fl-studio/how-to-export-bounce-in-fl-studio
  (dialog flow, naming), itsgratuitous.com (render walkthrough),
  soundshaper.org/blog/best-export-settings-fl-studio (starting preset),
  flstudiopro.com/fl-studio-import-midi (2026 import routes),
  audioproductionroom.com (full-project MIDI export + `Prepare for MIDI
  export` macro), midimighty.com (one-channel-per-track).
- Crates: `midly 0.5.3` (docs.rs/midly, github.com/kovaxis/midly),
  `nodi` (github.com/insomnimus/nodi), `rimd 0.0.1` (docs.rs/rimd),
  `augmented-midi 1.8.0` (docs.rs/augmented-midi), `hound 3.5.1`
  (docs.rs/hound), `flacenc 0.5.1` (docs.rs/crate/flacenc, Apache-2.0),
  `vorbis_rs 0.5.6` (crates.io/crates/vorbis_rs, BSD-3-Clause),
  `rusty_vorbis 0.1.2` (crates.io, 2026-10-22, Apache-2.0),
  `mp3lame-encoder 0.3.0` / `mp3lame-sys 0.2.0` (LGPL-3.0, bundled LAME
  4.0), SMF spec notes (midi.community/docs/smf/ — running status,
  vel-0 Note-On, tempo-map track convention, FIFO overlap pairing).
