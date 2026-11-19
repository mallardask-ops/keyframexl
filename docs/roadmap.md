# KeyFrameXL — Feature Roadmap

This document tracks what has already been built in KeyFrameXL and what remains planned.

**Guiding principle:** Build an honest data and audio model first, then build UI on top of it. Verify features with automated tests and native framebuffer captures.

---

## Implemented

### Core Engine, Sequencing & Workspaces
- **Unified Voice Engine (`src/engine.rs`, `src/link.rs`, `src/audio.rs`)** — stateful polyphonic voice pool (128 voices, oldest-voice stealing, allocation-free note-on in the audio callback) with sample-accurate event dispatch, lock-free `rtrb` **live parameter streaming** (generation-tagged mixer/channel/FX parameter tweaks without cloning `Song`), PipeWire low-latency quantum hints (`PIPEWIRE_LATENCY`), backend detection, and automatic **audio-device hot-reconnection & stall recovery** with exponential backoff. Real-time playback and offline audio export execute the exact same render path (verified byte-identical).
- **Channel Rack (`F6`)** — every channel supports both step-sequencer pads (grouped in 4-beat trays) and full Piano Roll notes, per-channel mute/solo (`Ctrl+1..0`), mini Pan/Volume knobs, target mixer track selector, and global Swing.
- **Piano Roll (`F7`)** — chromatic editor with Draw (`P`), Paint (`B`), Delete (`D`), Mute (`T`), Slice (`C`), Select (`E`), Zoom (`Z`), and Slide (`S`) tools; Ghost notes, scale highlighting, independent X/Y zoom, multi-target bottom control lane (*Velocity, Pan, Release, Fine, Mod X, Mod Y*), portamento notes, note colors, and a built-in **Riff Machine** (`src/riff.rs`).
- **Playlist (`F5`)** — multi-track arrangement for pattern and audio clips, shared clip instances with **Make unique**, **Tempo automation lane** (`src/tempo.rs`), and **Automation clips** (`src/automation.rs`).
- **Dockable Shell (`src/ui/workspace.rs`)** — hybrid docked + floating tab layout (`egui_dock`) with three saved presets (`Arrange`, `Compose`, `Edit` via `Alt+1..3`) persisted to `~/.config/keyframexl/workspace.ron`, plus a **Browser** (`Alt+F8`) with Current Project and Files tabs, background search, and click-to-audition.

### Built-in Instruments
- **Sampler v2 (`src/sampler.rs`, `src/dwp.rs`)** — DirectWave-inspired multi-zone sampler with:
  - *Program:* Volume, Pan, Transpose, Play Mode (`Poly` / `Mono` / `Legato`), Polyphony limit, Glide Time (`Fixed` / `Auto`), Pitch Bend range, two **Program LFOs** (`ProgLfo1` / `ProgLfo2`), and per-program **FX sends** (Drive, Chorus, Stereo Delay, Reverb).
  - *Zone:* Semi `±36`, Fine `±50`, Key Tracking `±100%`, Gain, Pan, Velocity range, Amp DAHDSR, Loop modes (`Disabled`, `Forward`, `One Shot`, `Sustained`, `Bounce`), Reverse, Start offset, Mute group, **Round-robin trigger groups** (`Normal` / `Cycle` / `Random` / `AvoidPrevious`), and **per-voice FX** (Ringmod, Decimator, Quantizer, 4-stage Phaser).
  - *Filters & Modulation:* Dual TPT SVF filters (`Off`, `LP`, `HP`, `BP`, `Notch`, `Allpass`, Serial/Parallel), `ENV 1/2`, `LFO 1/2` (8 waveforms, Rate, Attack, Phase, Sync/Free Run), and a 16-slot Modulation Matrix.
  - *Native DirectWave `.dwp` Reader (`src/dwp.rs`):* Reads `DwPr` binary presets directly (v36/v37/v38, embedded FLAC & planar PCM monoliths, external WAV/FLAC/OGG, dual filters, `ENV 1/2`, `LFO 1/2`, Program LFOs, and full 16-slot modulation matrix) with zero external tools required, plus ConvertWithMoss fallback for non-`DwPr` files (see [dwp-validation.md](dwp-validation.md)).
- **KeyFrame Tide (`src/synth.rs`)** — native polyphonic synthesizer with 18 factory presets.
- **Forge (`src/forge/`)** — 6-layer procedural game SFX designer with recipes, project game styles, macros, deterministic mutations, channel bouncing, and batch WAV/OGG export (see [forge.md](forge.md)).
- **Vocal (`src/vocal/`)** — UTAU singing voice channels using an embedded WORLD vocoder, per-note lyrics in the Piano Roll, and background phrase rendering/caching (see [vocal.md](vocal.md)).

### Mixer, Effects & Plugin Hosting
- **Mixer (`F9`)** — scrollable insert strips with Master bus, arm-for-record, mute/solo, pan/stereo separation, dB faders with meters, 3-band strip EQ, loop-free sends, and a 10-slot effect rack inspector.
- **32 Built-in DSP Effects (`src/fx/`)** — custom UI faces and A/B presets covering EQ/filters, dynamics, reverbs/delays (including convolution reverb), distortion, modulation, stereo tools, and a vocoder.
- **VST3, VST2 & CLAP Plugin Hosting (`src/bridge/`, `src/plugins.rs`, `src/vst2.rs`, `src/clap.rs`)** — native Linux CLAP 1.2 host (multi-plugin bundles, stereo/multi-out/sidechain buses, parameters, state chunks, latency/PDC, X11 embedded GUI, timers & POSIX fd support) alongside VST3 and VST2, with out-of-process crash-isolated plugin hosting over shared memory, background plugin scanning, and Wine/yabridge support for Windows plugins.

### MIDI, Interop, Recording & Export
- **MIDI & Recording (`src/midi_in.rs`, `src/midi_out.rs`, `src/midi.rs`, `src/ui/record.rs`)** — live ALSA sequencer MIDI input & hardware output, 24 PPQN **MIDI Clock Master & Slave** synchronization (`0xF8`/`0xFA`/`0xFB`/`0xFC`), MIDI Learn (`Link to controller`), **Multilink to controllers** (`Ctrl+J`), typing-keyboard-to-piano, MIDI file import/export, and in-app audio recording.
- **Unified Parameter Automation (`src/automation.rs`, `src/ui/automation.rs`, `src/engine.rs`)** — `TOOLS → Last tweaked` actions (*Create automation clip*, *Link to controller…*, *Copy/Paste value*, *Reset to default*), live automation recording during `Record + Play` with Ramer–Douglas–Peucker point thinning and single-step undo, and click-free 5 ms per-sample/per-block parameter smoothing in the audio engine.
- **FL Studio Interop (`src/flp.rs`, `src/flp_fx.rs`, `src/fl_link.rs`, `src/flp_freeze.rs`)** — `.flp` project importer plus experimental FL Link and channel freezing helpers.
- **Project I/O, Packaging & Audio Export (`src/project_io.rs`, `src/bundle.rs`, `src/render.rs`, `src/ui/export.rs`)** — self-contained `.kfxl` projects (Save, Save As, Recent files, unsaved-change guard, autosave recovery), **Project bundle (`.zip`, "Collect all & save")** export & import with external IR packaging, plugin/voicebank inventory (`manifest.json` + `README.txt`), and multi-format rendering to **WAV** (16/24-bit, 32-bit float), **FLAC** (16/24-bit), and **MP3** (128/192/320 kbps).
- **Deck Monitor & Update Checker (`src/ui/deck.rs`, `src/updater.rs`, `src/ui/updater_ui.rs`)** — optional recessed Master **Oscilloscope / Spectrum** monitor in the top transport deck (`VIEW → Deck monitor`), and a non-intrusive, zero-telemetry GitHub Releases/Tags **Update Checker** (`HELP → Check for updates…`, daily rate-limited background check with `KFXL_NO_UPDATE_CHECK=1` opt-out and skip-version support).

---

## Planned / Open Work

### Audio Backend & Realtime Polish
- **Native PipeWire streams** — move from the PipeWire-ALSA bridge (`cpal 0.15` + `PIPEWIRE_LATENCY` tuning) to direct native PipeWire (`cpal 0.18+` or `pipewire-rs`) graph nodes.
- **Lock-free sequence edit streaming** — extend the `rtrb` live parameter queue to stream pattern note/clip mutations without snapshotting `Song`.

### Sampler & DirectWave Fidelity
- **Time Stretch / Beat Sync / Tick Slice** — tempo-synced granular/phase-vocoder sample stretching and slice playback modes.

### Workflow & UI Polish
- **Vocal engine upgrades** — dynamic mid-phrase tempo-map stretching, clip-offset vocal slicing, and per-note pitch-bend curves.
- **UI polish** — optional Light theme, toolbar/deck customization, and potential Ukrainian UI localization.

---

## Explicitly Out of Scope

- **Windows & macOS support** — KeyFrameXL targets Linux exclusively (ALSA/PipeWire, X11/XCB plugin embedding, UNIX IPC, Wine/yabridge).
- **Online / account ecosystems** — no cloud sync, built-in store, news feeds, or DRM/licensing dialogs.
