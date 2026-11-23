# Changelog

Every push to `main` ships a nightly prerelease (`v<version>-nightly.<date>` or `v<version>-nightly.rc.<candidate>.<date>`);
versions below are the stable tags. The 0.2.0 plan with its tasks is in
[docs/v0.2.0.md](docs/v0.2.0.md).

## Unreleased (0.2.1)

See [docs/v0.2.1.md](docs/v0.2.1.md) for the Windows port and 0.2.1 plan.

### Engine and internals
- **Cross-OS `.kfxl` compatibility, native Windows FL Studio lookup & docs (W5)**:
  `resolve_plugin_path` now resolves hosted plugins across Linux and Windows by
  CLAP `#id`, cross-OS bundle/library stem (`.dll` ↔ `.so`, `.vst3`, `.clap`),
  or display name/vendor when opening a project saved on another OS; bundle and
  convolution IR paths normalize `/` ↔ `\`; `flp::resolve_path` and FLP sample /
  DirectWave lookup work natively on Windows without a Wine prefix; Steinberg
  ASIO trademark attribution added to *About* and `README.md`; English and
  Ukrainian manuals updated for Linux and Windows.
- **Out-of-process plugin bridge on Windows (W4)**: abstracted `src/bridge/ipc/`
  with a native Windows backend using pagefile-backed `CreateFileMappingW` /
  `MapViewOfFile`, auto-reset named `CreateEventW`, full-duplex overlapped Named
  Pipes (`\\.\pipe\kfxl-bridge-...`), `CREATE_NO_WINDOW`, and a Job Object
  (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), with the full `bridge::tests` suite
  running on `windows-latest`.
- **Native Windows plugin hosting (W3)**: modularized `src/plugins/` into
  `mod`, `scan`, `host`, `wine`, `editor_x11`, and `editor_win32`; migrated
  `src/vst2.rs` and `src/clap.rs` to `libloading`; enabled VST3, CLAP (`win32`
  GUI + DPI scale), and VST2 (`.dll`) scanning and hosting on Windows with
  embedded child `HWND` editors (`SetWindowRgn` clipping under egui popups,
  Per-Monitor v2 DPI tracking, and DAW hotkey forwarding).
- **Windows self-updater & packaging (W2)**: `src/updater.rs` now picks the
  target-OS archive (`keyframexl-windows-x86_64.zip` + `.sha256` on Windows,
  ignoring `-setup.exe`), extracts `.zip` archives with the `zip` crate, checks
  PE (`MZ`) magic, replaces the running executable via `keyframexl.old.exe`
  (cleaned up on next launch), and ships portable `.zip` plus a user-scope Inno
  Setup installer (`assets/keyframexl.iss`, `.kfxl` association, Start Menu
  shortcut) in both nightly and stable releases.
- **Static MP3 export (LAME)**: `mp3lame-sys` is now statically compiled into
  KeyFrameXL on Linux and Windows, so MP3 export works out of the box without
  installing an external `lame` package or DLL.
- **Windows desktop & audio layer (W1)**: `src/platform/{linux,windows}.rs`
  (ShellExecuteW, Windows Registry dark/light theme, `GetUserDefaultLocaleName`,
  Windows CJK fonts, `%LOCALAPPDATA%` log file, `ExitProcess`, hidden console
  for `ffmpeg`, embedded `.ico` icon and `windows_subsystem = "windows"`), plus
  WASAPI & ASIO driver selection with multi-channel / `I32` / `I16` output
  support.
- **MIDI export with mix, automation, slides and markers**: `File → Export → MIDI` now writes channel + mixer volume (CC 7) and pan (CC 10), song-mode volume/pan automation clips, piano-roll slides and fine pitch as `PitchBend` (with RPN 0,0 bend sensitivity when over ±2 semitones), per-note pan and Mod X / Mod Y (CC 74 / CC 71), and playlist section markers; melodic tracks skip GM drum channel 10, and MIDI import now also restores volume/pan on split drum channels and recognises RPN 0,0.
- **Native PipeWire output**: the program is a PipeWire node of its own
  (cpal 0.18), with ALSA as the fallback; the buffer setting is the node's
  quantum, xruns are counted, and the device list shows hardware outputs
  and *System default* instead of ALSA plug names. `realtime-dbus` is kept
  off so `RLIMIT_RTTIME` is never lowered to one quantum (which killed
  yabridge/Wine plugins during initialization).
- **Windows foundation (W0)**: platform-gated dependencies and stubs so the
  library and application build and pass tests on `windows-latest` in CI.

## 0.2.0 — 2026-11-21

Everything since 0.1.2, gathered from the nightlies and release
candidates up to `v0.2.0-nightly.rc.4`. Native PipeWire output (E1) is
still in review and not part of this release.

### New features
- **Ukrainian interface.** Every window, menu, tooltip, dialog and status
  message is in English and Ukrainian, with proper plural forms. The
  language follows the system locale; change it in *Settings → General* or
  run with `KFXL_LANG=en|uk`. Effect, preset and recipe names stay as they
  are.
- **User manual** (*Help → Manual*): 13 chapters in English and Ukrainian,
  built into the program with search and links; it works offline and opens
  in the interface language. The same pages are in `docs/manual/` and
  `docs/manual/uk/`. The old menu item opened a file that did not exist.
- **Light theme** (*Settings → General → Theme*, *View → Theme*): Dark,
  Light, or System (follows the desktop through xdg-desktop-portal). All
  text keeps at least 4.5:1 contrast; Tide and the Sampler stay dark, like
  hardware on a light desk.
- **Demo mode** (*View → Demo mode*, `keyframexl --show`): a full-window
  Arrange overview and a Falling notes view with vocal pitch contours, live
  lyrics, rhythm pads and a lit keyboard, with deterministic video
  recording.
- **Project info** (*File → Project info*, `F11`): a resizable window with
  tabs for details, notes and musical settings. Album, year,
  copyright/licence (with presets), website, genre presets, *Show on open*,
  a live project summary, and tempo, swing, time signature, key (synced
  with the Piano roll) and Forge sound style in one place. MP3 export
  writes these fields as ID3v2.4 tags, previewed in the window.
- **Import dialog** (*File → Import…*): one movable dialog for audio,
  DirectWave presets, MIDI, FL Studio projects and bundles, showing where
  each file will go.
- **Tap tempo window** (*Tools → Tap tempo*, **Tap** on the deck): tap with
  the mouse, `Space`/`Enter`/`T`, or any MIDI note or sustain pedal, timed
  by the device's own clock. It shows the tempo and the spread of recent
  taps, averages 4/8/16 taps, rejects stray taps and sets the project
  tempo as one undo step. While it is open, `Space` taps instead of
  playing.
- **Sampler time stretch.** A zone can stretch its sample with the pitch
  kept: by a ratio (*Time*, 25–400 %) or to a number of beats at the
  song's tempo (*Beat sync*), using Signalsmith Stretch (MIT, vendored).
  Renders are deterministic, so playback and export match.
- **Vocal pitch curves.** The piano roll's *Pitch* lane draws each sung
  note's pitch: points with smooth, linear or eased segments (a scoop, a
  fall, a bend), and a note's own vibrato instead of the channel's.
- **FL Studio live link window**: a state (not running, starting,
  connected, check, error), the measured delay, gaps and queue, whether FL
  has the MIDI input open, a *Reconnect* button, and a check of what FLEX
  needs in FL's Wine prefix (vkd3d-proton, Verdana) with a warning for
  FL 2026 on stock Wine. One link channel at a time. See
  [docs/fl-link.md](docs/fl-link.md).

### Content
- **Built-in drum kits.** KFX Core (17 808/909-style sounds) and KFX Chip
  (8 console sounds), synthesised by Forge from recipes: no sample files,
  projects store only the recipe. New projects get their Kick, Snare and Hat
  from Core instead of borrowing FL Studio's 909 samples. *Add channel →
  Drum kit* adds a whole kit.
- **KeyFrame Tide: 105 factory presets** (was 18) in ten categories: bass,
  lead, pad, keys, pluck, bell, arp, FX, drum-synth, chiptune. Menus and the
  browser group by category, search finds categories, factory presets can
  be favourites.
- **Forge: 51 recipes** (was 11) in ten categories: UI, footsteps on seven
  surfaces, actions, weapons, pickups, magic, damage, doors, ambience,
  creatures. *Mono* export added.
- **Genre starters** on a new *Genres* tab of the Welcome screen: lo-fi
  hip-hop, trap, house, synthwave, chiptune, drum & bass, ambient.
- **Demo song** (*Help → Demo song*, `keyframexl write-demo`,
  `render-demo-song`): 32 bars using every part of the program, with a
  vocal on a synthetic CC0 voice the program writes itself.
- New README screenshots, dark and light, all taken from the demo song
  (`scripts/ux/shots.sh`).

### Interface
- A new install of a stable release gets only stable updates; a nightly build
  keeps following nightlies. Existing settings keep their choice.
- **Browser**: *All*, *Library* and *Files* sources with one search field,
  a Stop button while a sample previews, favourites and root folders in
  your own order (dragged, remembered), folders sorted by name, type, date
  or size. The plugin scan notice can be dismissed or opens the Problems
  list.
- **Channel rack**: drag a channel's name to move it; the step grid scrolls
  sideways while the channel controls stay in place.
- **Deck**: the mouse wheel over the insert or pattern selector changes
  it; right-click the pattern selector for rename, colour, clone and
  delete. *File → Welcome screen* reopens the Welcome screen.
- **Colours** of tracks and patterns are picked from a grid of swatches
  (they were a list of hex codes).
- **Pattern list** on the deck: larger rows in each pattern's colour.
- **Opening panels**: a closed Playlist, Channel rack, Piano roll or Mixer
  opens as a tab in the window that has the focus (it went to the first
  pane that happened to be open), or in a window of its own:
  *Settings → General → Open panels*.
- **Knobs, faders and number fields** capture the mouse, so a long drag
  does not stop at the screen edge.
- **Docked panels**: dragging a shared edge resizes both panels. Embedded
  X11 plugin editors are hidden where an app window covers them.
- **Export audio** is a movable, resizable window: its settings scroll,
  the size estimate, progress and buttons stay pinned at the bottom, and
  the file name is an editable field.
- **Shortcuts**: the `F1` list is grouped and complete (`F8`,
  `Ctrl+Shift+M`, `Alt+E`, `Alt+C`, `Alt+↑/↓`, `Ctrl+K` were missing); the
  manual's shortcut page is generated from the same list, and the View menu
  shows `Ctrl+=` instead of `Ctrl++`.
- **Version**: nightly builds show their own version (for example
  `v0.2.0-nightly.rc.3.…`) with a *Nightly* badge on the Welcome screen, in
  About and in *Settings → General*; they showed 0.1.2.
- **Window size**: the main window reopens at its last size, and maximised
  if it was closed maximised.
- The dark theme's Play button text is a little more readable.

### Fixes
- MIDI import reads pitch bends back: a bend ramp becomes a piano-roll slide
  and a bend held from a note's start becomes its fine pitch, so slides survive
  a MIDI export and re-import (before, every bend was dropped).
- The updater no longer offers a release that has no build for your system
  (0.2.1 stable is Linux only; Windows stays on nightly builds).
- Settings, Export and Project info windows can no longer be shrunk until
  their content is cut off; long hints on the Plugins page wrap.
- Plugins page: the Ukrainian "ready" count shows its number.
- **Crash** when a tab group or a floating window was dragged by its grip
  over another panel (since 0.1.1).
- **Export and FL Studio channels' pitch.** An export started with the pitch
  the live engine had left on FL Studio channels, and afterwards the
  project played with the pitch the export ended on, so slides came out
  wrong, mostly at the start. Every FL note now sends its pitch.
- **Export tail** ran 30 s past the end when a plugin (an amp simulator)
  hissed above −80 dBFS: the tail now ends 60 dB under the song's peak, so
  the file is as long as the estimate.
- **Switch highlights** (sample rate, buffer…) no longer trail behind a
  window that is dragged quickly.
- **Window height creep**: Export audio and Project info lost a few pixels
  of height on every drag of a side edge.
- **Maximised start on X11** was ignored: the request was sent before the
  window was mapped.
- **Forge export levels.** Export no longer normalises every file to −1 dB
  by default; each recipe is made at its category's level, so a UI click
  stays well under a shotgun. *Normalize* is a switch.
- **Vocal timing.** Notes inside a phrase follow the tempo map (they used
  the constant song tempo); a tempo-map edit renders the phrase again. A
  playlist clip cut in the middle of a phrase now sings from its first note
  (it was silent), and a clip's end cuts the phrase.
- **FL Studio import.**
  - Audio clip offsets are milliseconds (they were read as beats, so clips
    started past the end of their sample).
  - Clip mute is bit `0x2000` (`0x1000` muted the wrong clips).
  - The playlist record size follows the layout event, with FL 25.2+
    88-byte records.
  - Channel stretch pitch and clip fades are kept.
  - Without FL Studio VSTi, 3x Osc channels play on KeyFrame Tide with
    their three oscillators, and FLEX channels on the Tide preset of the
    same role (bass, pad, pluck…), instead of staying silent.
  - Fruity Squeeze and Hyper Chorus keep their settings (they were
    imported switched off); Squeeze with no Impact no longer clicks once
    per cycle.
  - Automation of effect slot on/off and of send levels comes across.
- **Denormals.** DSP threads flush denormals to zero: reverb and filter
  tails no longer spike the CPU in silence.
- **Tide DC offset.** A per-voice DC blocker removes the offset of sync and
  PWM waves (it ate headroom and clicked at note on/off). Hollow Choir no
  longer clips on chords.

### Engine and internals
- Patterns are shared between song snapshots and copied on write: an edit
  in a large project costs ~50 µs instead of 2–5 ms, and undo keeps only
  changed patterns.
- The Tide header no longer rebuilds every factory preset each frame.
- Styling and behaviour no longer depend on English labels, so translation
  cannot change what a control does; a test fails on any new untranslated
  UI text.
- Clippy-clean codebase; CI checks `cargo fmt` and `cargo clippy -D warnings`.
- `LICENSE` file (MIT).

### Packaging
- The 16 px icon has its own pixel-aligned drawing.
- Release and nightly archives carry the desktop launcher, the icon set and
  `install.sh` (installs into `~/.local`).
- Nightlies are tagged `v<version>-nightly.<date>`, and release candidates
  `v0.2.0-nightly.rc.<n>.<date>`.

## 0.1.2 — 2026-11-19
- In-place self-updater: downloads the release archive, checks its SHA-256,
  installs it next to the running program and restarts.
- Release notes render as Markdown in the update dialog.
- Nightly channel: a prerelease for every push to `main`, followed by the
  updater unless *Settings → General → Nightly builds* is off.

## 0.1.1 — 2026-11-19
- UI overhaul; Playlist and Piano Roll selection and mass-move fixes; group
  tab docking; window positions remembered.

## 0.1.0 — 2026-11-19
- First public build: channel rack, piano roll, playlist, mixer with 32
  built-in effects, sampler with a native DirectWave reader, KeyFrame Tide,
  Forge, UTAU vocal channels, VST3/VST2/CLAP hosting, FL Studio import,
  MIDI in/out and clock, project bundles, audio export, Welcome screen,
  English/Ukrainian UI, GitHub CI and releases.
