<div align="center">
  <img src="assets/icon.svg" width="84" height="84" alt="KeyFrameXL icon">
  <h1>KeyFrameXL</h1>
  <p>Native Linux pattern-based music sequencer and audio workstation written in Rust</p>
</div>

> [!IMPORTANT]
> **This application is vibecoded.**
> KeyFrameXL is an experimental native Linux DAW and pattern sequencer built through AI-assisted vibecoding. Expect rough edges, rapid iteration, and unconventional internals — use it for experimenting, making beats, and exploring ideas, not as a battle-tested studio replacement.

KeyFrameXL is built with `eframe`/`egui` + `egui_dock`, inspired by classic pattern-centric DAW workflows and tactile hardware sequencers.

![KeyFrameXL workspace](docs/screenshots/workspace.png)

## Features

### Sequencing & Arrangement
- **Channel Rack (`F6`)** — step sequencer with beat groups, per-step velocity (Shift+click), mute/solo, step operations, and inline note previews.
- **Piano Roll (`F7`)** — chromatic MIDI note editor with Draw (`P`), Paint (`B`), Delete (`D`), Mute (`T`), Slice (`C`), Select (`E`), Zoom (`Z`), and Slide (`S`) tools, snapping (`1/16`, `1/4`, `Bar`, or free with `Alt`), scale highlighting, scroll-safe marquee selection, velocity/property lanes, note tools, and a built-in **Riff Machine**.
- **Playlist (`F5`)** — multi-track arrangement for pattern, audio, and automation clips, **Tempo automation lane**, right-drag marquee selection (quick right-click deletes), multi-clip move/duplicate, and song markers.
- **Transport Deck & Live Monitor** — pattern selector with mouse-wheel switching and double-click rename, master VU meter, and live **Oscilloscope / Spectrum Analyzer** (`VIEW → Deck monitor`).
- **Dockable Workspaces** — every editor is a tab that can be docked side-by-side, stacked, or floated into its own window. Combined tab groups and floating windows can be dragged and docked together via their grip handle (`⋮⋮`) or empty tab bar. Three workspace presets (`Arrange`, `Compose`, `Edit` via `Alt+1..3`), floating window relative positions across UI scaling, and main window size persist automatically.
- **Welcome Screen & Localization** — startup hub with recent projects, 6 starter templates, and searchable KeyFrame Tide / Forge preset launchers. Full **English & Ukrainian** UI localization (`Settings → General → Language`).

### Built-in Instruments
- **Multi-zone Sampler** — DirectWave-inspired sampler with **native `.dwp` reader** (no external converter required), key/velocity zone mapping, round-robin / random trigger groups, DAHDSR envelopes, forward/sustain/ping-pong loops, dual filters, dual LFOs, program LFOs & sends, per-voice FX (Bitcrusher, Ring Mod, Quantizer, Decimator, Phaser), and a 16-slot modulation matrix.
- **KeyFrame Tide** — built-in polyphonic synthesizer with 18 factory presets.
- **Forge (Game SFX Designer)** — procedural sound designer for jumps, coins, hits, lasers, and explosions with up to 6 layers (oscillators, noise, samples, Tide synth), drawn pitch/volume/filter curves, 4 character knobs (*Size, Energy, Dirt, Space*), style presets (*Retro, Cartoon, Heavy, Sci-fi*), **Mutate** variations, and batch WAV/OGG export. See [docs/forge.md](docs/forge.md).
- **Vocal (UTAU Singing Channels)** — singing synthesis powered by the WORLD vocoder (`rsworld-sys`) using UTAU voicebanks (including one-click official Kasane Teto download) with per-note lyrics directly in the piano roll. See [docs/vocal.md](docs/vocal.md).

### Mixer & 32 Built-in Effects
The Mixer (`F9`) ships with **32 native DSP effects** with custom visual faces and A/B presets:
- **EQ & Filters:** Parametric EQ 2, 3-band EQ, Tilt EQ, Filter
- **Dynamics:** Compressor, Multiband Compressor, Limiter, Squeeze, Soft Clipper, Noise Gate, Transient Shaper
- **Delay & Reverb:** Reverb, Room Reverb, Convolution Reverb, Bloom, Delay, Stereo Delay
- **Distortion & Color:** Drive, Wavefolder, Bitcrusher
- **Modulation & Time:** Chorus, Hyper Chorus, Phaser, Tremolo, Auto Pan, Ring Modulator, Time
- **Stereo & Vocal:** Stereo, Stereo Shaper, Mid / Side, Balance, Vocoder

### Plugin Hosting & FL Studio Interop
- **CLAP 1.2, VST3 & VST2 Hosting** — native Linux CLAP 1.2 support alongside VST3 & VST2, out-of-process plugin bridge (`src/bridge/`) to isolate crashes, embedded X11 editor windows, and Wine/yabridge support for Windows plugins.
- **FL Studio Interop** — imports `.flp` projects (`src/flp.rs`, `src/flp_fx.rs`), plus optional FL Link / channel freezing workflows.

### Audio, MIDI, Export & Updates
- **Shared Real-Time Engine** — real-time playback (PipeWire/ALSA via `cpal` with low-latency PipeWire tuning, automatic hot-reconnect, and a lock-free `rtrb` live parameter queue) and offline rendering share one voice engine (`src/engine.rs`).
- **MIDI & Recording** — live MIDI input (`midir`), MIDI Clock Master/Slave & MIDI Out, MIDI file import/export, and in-app audio recording.
- **Portable Projects, Bundles & Export** — saves self-contained `.kfxl` projects and shareable `.zip` **Project Bundles** (*Collect all & save*), exports to **WAV** (16/24-bit, 32-bit float), **FLAC** (16/24-bit), and **MP3** (128/192/320 kbps), plus WAV/OGG batch export in Forge.
- **Self-Updater** — non-blocking GitHub Releases check (`HELP → Check for updates…`) that downloads, SHA-256-verifies and installs the new build in place, then restarts. Follows **Nightly builds** (a prerelease for every push to `main`) or stable releases only (*Settings → General*). Builds run from a cargo `target/` directory are updated from source instead.

## Quick Start

### Platform Support

> [!WARNING]
> **Linux only — Windows and macOS are NOT supported.**
> KeyFrameXL **will not work properly on Windows or macOS**, and support for those platforms is **not planned at this time**. The codebase relies directly on Linux-specific APIs and subsystems (`libc` / file-descriptor manipulation, ALSA/PipeWire audio, X11/XCB plugin window embedding, UNIX IPC/process supervision, and Wine/yabridge bridging).

### System Requirements

- **OS:** 64-bit Linux (`x86_64`)
- **Display:** X11, or Wayland with **XWayland** (runs on X11/XWayland by default so embedded VST/CLAP plugin GUIs work; pure Wayland is supported via `KFXL_X11=0` without X11 plugin embedding)
- **Graphics:** OpenGL-capable GPU & drivers (for `eframe` / `glow`)
- **Audio:** Working **PipeWire** (via PipeWire-ALSA) or **ALSA** output
- **Display Resolution:** `1180×680` minimum (`1366×768` or higher recommended)
- **Build & System Libraries:**
  - **Rust** toolchain (Edition 2024 / recent stable `rustc` + `cargo`)
  - **C/C++ compiler** (`gcc` or `clang`) and `pkg-config` (for native dependencies like `rsworld-sys`)
  - **ALSA, X11/XCB & Fontconfig dev libraries:**
    - Debian / Ubuntu: `sudo apt install build-essential pkg-config libasound2-dev libxcb1-dev libx11-dev fontconfig`
    - Arch Linux: `sudo pacman -S base-devel pkgconf alsa-lib libxcb libx11 fontconfig`
    - Fedora: `sudo dnf install gcc gcc-c++ pkgconf-pkg-config alsa-lib-devel libxcb-devel libX11-devel fontconfig`
  - **File dialogs:** XDG desktop portal (`xdg-desktop-portal`) or GTK3/zenity/kdialog for `rfd` open/save dialogs

### Build & Run

```sh
# Run with the demo beat & bass slide
cargo run -- --demo

# Run an optimized release build (recommended for live playback & DSP)
cargo run --release

# Open an existing KeyFrameXL or FL Studio project directly
cargo run --release -- path/to/project.kfxl
cargo run --release -- path/to/project.flp
```

### Useful CLI Commands

```sh
# Render the built-in demo directly to WAV
cargo run -- render-demo demo.wav

# Play the built-in demo via PipeWire (pw-cat)
cargo run -- play-demo

# Scan CLAP / VST3 / VST2 folders and print plugin status
cargo run -- plugin-scan
```

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Space` | Play / Stop (with rewind) |
| `Ctrl+Space` | Pause in place |
| `L` | Switch Pattern / Song mode |
| `F5` / `F6` / `F7` / `F9` | Open or focus Playlist / Channel Rack / Piano Roll / Mixer |
| `Alt+F8` | Show / hide Browser |
| `Alt+1` / `Alt+2` / `Alt+3` | Switch Workspace (`Arrange` / `Compose` / `Edit`) |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+Shift+N` / `Ctrl+O` / `Ctrl+S` | New / Open / Save project |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+Shift+Ins` / `F4` | New pattern / Next empty pattern |
| `F2` / Double-click Pattern | Rename current pattern (scroll wheel switches patterns) |
| `P` / `S` / `E` (Piano Roll & Playlist) | Draw / Slide / Select tool (`RMB drag` selects in Playlist) |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Scale UI larger / smaller / reset |

## Optional Integrations

- **DirectWave `.dwp` presets:** parsed natively in Rust (`src/dwp.rs`) with embedded samples and zone/DSP parameters. [ConvertWithMoss](https://www.mossgrabers.de/Software/ConvertWithMoss/ConvertWithMoss.html) (20.2+) is supported as an optional fallback via `PATH` or `KEYFRAMEXL_CONVERTER`.
- **Windows VST plugins:** configure Wine / yabridge paths in **Options → Settings → Plugins**. Native X11 window embedding is enabled automatically (via XWayland on Wayland sessions; override with `KFXL_X11=0`).
- **UTAU voicebanks:** install `.zip` voicebanks or use the built-in one-click installer inside a Vocal channel.

## Project Structure

```text
assets/          App icon, brand lockup SVG, bundled IBM Plex fonts
docs/            Feature guides (Forge, Vocal, UI rules, roadmap, DWP validation)
scripts/         Fixture generators and live/UX helper scripts
src/
  bridge/        Out-of-process CLAP / VST3 / VST2 plugin host & shared memory protocol
  clap/          Native CLAP 1.2 C ABI loader & host callbacks
  forge/         Procedural game SFX synthesizer & batch exporter
  fx/            32 built-in native audio effects + presets
  ui/            egui/eframe shell, dockable editors, welcome screen, custom widgets
  vocal/         UTAU voicebank parser, lyric mapper, WORLD vocoder renderer
  dwp.rs         Native DirectWave .dwp preset & sample parser
  engine.rs      Shared real-time & offline audio engine (lock-free rtrb param queue)
  flp.rs         FL Studio .flp project importer
  i18n.rs        English & Ukrainian UI localization
  sampler.rs     Multi-zone sampler engine (DirectWave DSP, round-robin, per-voice FX)
  synth.rs       KeyFrame Tide synthesizer
  updater.rs     GitHub Releases update checker and in-place self-installer
tests/           Integration tests and audio fixtures
vendor/          Patched local dependencies (cpal, egui_dock, vst3-host, winit)
```

## Documentation

- [Forge — Game SFX Designer](docs/forge.md)
- [Vocal — UTAU Singing Channels](docs/vocal.md)
- [UI Rules & Component Reference](docs/ui-rules.md)
- [Roadmap](docs/roadmap.md)
- [DirectWave (DWP) Validation](docs/dwp-validation.md)

## License

MIT — see `Cargo.toml`.
