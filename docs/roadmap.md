# KeyFrameXL — Feature Roadmap

This document tracks what has already been built in KeyFrameXL and what remains planned.

**Guiding principle:** Build an honest data and audio model first, then build UI on top of it. Verify features with automated tests and native framebuffer captures.

---

## Implemented

### Core Engine, Sequencing & Workspaces
- **Unified Voice Engine (`src/engine.rs`, `src/link.rs`, `src/audio.rs`)** — stateful polyphonic voice pool (128 voices, oldest-voice stealing, allocation-free note-on in the audio callback) with sample-accurate event dispatch, lock-free `rtrb` **live parameter streaming** (generation-tagged mixer/channel/FX parameter tweaks without cloning `Song`), native **PipeWire** graph node (with ALSA fallback) on Linux and **WASAPI + ASIO** on Windows via `cpal 0.18`, xrun tracking, and automatic **audio-device hot-reconnection & stall recovery** with exponential backoff. Real-time playback and offline audio export execute the exact same render path (verified byte-identical).
- **Channel Rack (`F6`)** — every channel supports both step-sequencer pads (grouped in 4-beat trays) and full Piano Roll notes, per-channel mute/solo (`Ctrl+1..0`), mini Pan/Volume knobs, target mixer track selector, and global Swing.
- **Piano Roll (`F7`)** — chromatic editor with Draw (`P`), Paint (`B`), Delete (`D`), Mute (`T`), Slice (`C`), Select (`E`), Zoom (`Z`), and Slide (`S`) tools; Ghost notes, scale highlighting, independent X/Y zoom, multi-target bottom control lane (*Velocity, Pan, Release, Fine, Mod X, Mod Y, Pitch*), portamento notes, note colors, and a built-in **Riff Machine** (`src/riff.rs`).
- **Playlist (`F5`)** — multi-track arrangement for pattern and audio clips, shared clip instances with **Make unique**, **Tempo automation lane** (`src/tempo.rs`), and **Automation clips** (`src/automation.rs`).
- **Dockable Shell & Themes (`src/ui/workspace.rs`, `src/theme.rs`)** — hybrid docked + floating tab layout (`egui_dock`) with three saved presets (`Arrange`, `Compose`, `Edit` via `Alt+1..3`), Dark / Light / System themes, Demo mode (`--show`), and a **Browser** (`Alt+F8`) with Current Project and Files tabs, background search, and click-to-audition.

### Built-in Instruments
- **Sampler v2 (`src/sampler.rs`, `src/dwp.rs`)** — DirectWave-inspired multi-zone sampler with:
  - *Program:* Volume, Pan, Transpose, Play Mode (`Poly` / `Mono` / `Legato`), Polyphony limit, Glide Time (`Fixed` / `Auto`), Pitch Bend range, two **Program LFOs** (`ProgLfo1` / `ProgLfo2`), and per-program **FX sends** (Drive, Chorus, Stereo Delay, Reverb).
  - *Zone:* Semi `±36`, Fine `±50`, Key Tracking `±100%`, Gain, Pan, Velocity range, Amp DAHDSR, Loop modes (`Disabled`, `Forward`, `One Shot`, `Sustained`, `Bounce`), **Time Stretch & Beat Sync** (Signalsmith Stretch), Reverse, Start offset, Mute group, **Round-robin trigger groups** (`Normal` / `Cycle` / `Random` / `AvoidPrevious`), and **per-voice FX** (Ringmod, Decimator, Quantizer, 4-stage Phaser).
  - *Filters & Modulation:* Dual TPT SVF filters (`Off`, `LP`, `HP`, `BP`, `Notch`, `Allpass`, Serial/Parallel), `ENV 1/2`, `LFO 1/2` (8 waveforms, Rate, Attack, Phase, Sync/Free Run), and a 16-slot Modulation Matrix.
  - *Native DirectWave `.dwp` Reader (`src/dwp.rs`):* Reads `DwPr` binary presets directly (v36/v37/v38, embedded FLAC & planar PCM monoliths, external WAV/FLAC/OGG, dual filters, `ENV 1/2`, `LFO 1/2`, Program LFOs, and full 16-slot modulation matrix) with zero external tools required, plus ConvertWithMoss fallback for non-`DwPr` files (see [dwp-validation.md](dwp-validation.md)).
- **KeyFrame Tide (`src/synth.rs`, `src/synth_presets.rs`)** — native polyphonic synthesizer with 105 factory presets in ten categories, each checked by a render test (silence, clipping, DC, subnormals, determinism).
- **Forge (`src/forge/`)** — 6-layer procedural game SFX designer with 51 recipes in ten categories, each made at its category's level, project game styles, macros, deterministic mutations, channel bouncing, and batch stereo/mono WAV/OGG export (see [forge.md](forge.md)).
- **Built-in drum kits (`src/kits.rs`)** — KFX Core (808/909-style) and KFX Chip, synthesised from Forge recipes; new projects start on Core.
- **Vocal (`src/vocal/`)** — UTAU singing voice channels using an embedded WORLD vocoder, per-note lyrics and pitch/vibrato curves in the Piano Roll, and background phrase rendering/caching (see [vocal.md](vocal.md)).

### Mixer, Effects & Plugin Hosting
- **Mixer (`F9`)** — scrollable insert strips with Master bus, arm-for-record, mute/solo, pan/stereo separation, dB faders with meters, 3-band strip EQ, loop-free sends, and a 10-slot effect rack inspector.
- **32 Built-in DSP Effects (`src/fx/`)** — custom UI faces and A/B presets covering EQ/filters, dynamics, reverbs/delays (including convolution reverb), distortion, modulation, stereo tools, and a vocoder.
- **Cross-Platform VST3, VST2 & CLAP Plugin Hosting (`src/bridge/`, `src/plugins/`, `src/vst2.rs`, `src/clap.rs`)** — native Linux and Windows CLAP 1.2, VST3, and VST2 host (multi-plugin bundles, stereo/multi-out/sidechain buses, parameters, state chunks, latency/PDC, embedded X11 and Win32 `HWND` editors), out-of-process crash-isolated plugin bridge (`src/bridge/ipc/{linux,windows}.rs`), background plugin scanning, cross-OS plugin resolution in `.kfxl` projects, and Wine/yabridge support on Linux.

### MIDI, Interop, Recording & Export
- **MIDI & Recording (`src/midi_in.rs`, `src/midi_out.rs`, `src/midi.rs`, `src/ui/record.rs`)** — live MIDI input & hardware output (`midir 0.11`), 24 PPQN **MIDI Clock Master & Slave** synchronization (`0xF8`/`0xFA`/`0xFB`/`0xFC`), Tap Tempo (`Tools → Tap tempo`), MIDI Learn (`Link to controller`), **Multilink to controllers** (`Ctrl+J`), typing-keyboard-to-piano, rich MIDI file import/export (volume/pan CC 7/10, automation, slides via PitchBend + RPN 0,0, markers), and in-app audio recording.
- **Unified Parameter Automation (`src/automation.rs`, `src/ui/automation.rs`, `src/engine.rs`)** — `TOOLS → Last tweaked` actions (*Create automation clip*, *Link to controller…*, *Copy/Paste value*, *Reset to default*), live automation recording during `Record + Play` with Ramer–Douglas–Peucker point thinning and single-step undo, and click-free 5 ms per-sample/per-block parameter smoothing in the audio engine.
- **FL Studio Interop (`src/flp.rs`, `src/flp_fx.rs`, `src/fl_link.rs`, `src/flp_freeze.rs`)** — `.flp` project importer with native Windows and Linux/Wine sample/pack lookup, plus FL Link status UI and channel freezing workflows on Linux.
- **Project I/O, Packaging & Audio Export (`src/project_io.rs`, `src/bundle.rs`, `src/render.rs`, `src/mp3.rs`, `src/ui/export.rs`)** — cross-OS self-contained `.kfxl` projects (Save, Save As, Recent files, unsaved-change guard, autosave recovery), **Project bundle (`.zip`, "Collect all & save")** export & import with external IR packaging, plugin/voicebank inventory (`manifest.json` + `README.txt`), and multi-format rendering to **WAV** (16/24-bit, 32-bit float), **FLAC** (16/24-bit), and **MP3** (128/192/320 kbps via statically linked LAME).
- **Deck Monitor & Cross-Platform Self-Updater (`src/ui/deck.rs`, `src/updater.rs`, `src/ui/updater_ui.rs`)** — optional recessed Master **Oscilloscope / Spectrum** monitor in the top transport deck (`VIEW → Deck monitor`), and a zero-telemetry GitHub Releases updater (`HELP → Check for updates…`) that downloads `.tar.gz` (Linux) or `.zip` (Windows), SHA-256-verifies and installs a release in place, then restarts; it follows nightly builds or stable releases only.
- **Welcome Screen, Localization, Manual & Dual-Platform CI (`src/ui/welcome.rs`, `src/i18n/`, `docs/manual/`, `.github/workflows/`)** — FL-style startup hub, full **English / Ukrainian** UI and built-in 13-chapter offline user manual, and automated Linux + Windows GitHub Actions CI (`ci.yml`) + dual-platform nightly & release publishing (`keyframexl-linux-x86_64.tar.gz`, `keyframexl-windows-x86_64.zip`, and user-scope Inno Setup `keyframexl-windows-x86_64-setup.exe`).

---

## Planned / Open Work

### Engine & Performance
- **Per-note edit streaming** — song snapshots already share unchanged patterns (an edit copies one pattern); a note overlay queue would make frame-by-frame note edits free as well.

### Sampler & DirectWave Fidelity
- **Slice playback mode** — transient/beat slicing in the Sampler (remainder of E3).

### Workflow & Interop
- **FL Studio import** — FL native generators (3x Osc, Sytrus, Harmor) and remaining effects mapped to built-ins; slot enable/send automation.
- **Toolbar / deck customization** — configurable deck actions (remainder of P5).

---

## Explicitly Out of Scope (for now)

- **macOS support** — deferred, not refused (would need CoreAudio, NSView embedding, Audio Units, and Apple notarization; see [porting.md](porting.md)).
- **Online / account ecosystems** — no cloud sync, built-in store, news feeds, or DRM/licensing dialogs.
