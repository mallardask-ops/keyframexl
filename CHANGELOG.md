# Changelog

Every push to `main` ships a nightly prerelease (`v<version>-nightly.<date>` or `v<version>-nightly.rc.<candidate>.<date>`);
versions below are the stable tags. The 0.2.0 plan with its tasks is in
[docs/v0.2.0.md](docs/v0.2.0.md).

## Unreleased

### Changed
- **Piano roll performance**: large patterns with thousands of notes now scroll, zoom, and edit smoothly by rendering and hit-testing only the visible viewport.

### Fixed
- Piano roll playhead in Song mode: skips clips on muted playlist tracks, stays
  visible when paused at a playlist position, and works with *Follow playhead*.
- Piano roll selection: `Ctrl+drag` box selection now works in Paint and Select
  tools as well as Draw, `Ctrl+Shift+click` toggles individual notes in the
  selection, and releasing a marquee outside the window no longer clears or
  corrupts the selected notes.
- Playlist live editing: adding, moving, resizing, or deleting clips during Song
  playback no longer cuts ringing voices or causes the playhead to jump when
  the song length changes.
- Export audio: the file name field can now be cleared completely while typing;
  an error is shown only if you try to start exporting with an empty name.
- Cloning a plugin channel in the Channel Rack now gives the copy its own
  independent plugin instance instead of routing both channels' notes into the
  first plugin; existing projects saved with duplicate plugin IDs are repaired
  automatically on open.
- Native Linux plugin windows built on VSTGUI (MT-PowerDrumKit and others) no
  longer go black after you switch to another tab and back, and react to the
  mouse without delay.
- Audio playback no longer frees replaced song snapshots or effect states on
  the real-time audio thread even if the interface stalls during rapid edits,
  preventing sudden dropouts.

## 0.2.1 — 2026-11-24

Everything since 0.2.0, gathered from the nightlies and release candidates up
to `v0.2.1-rc.3`. Stable 0.2.1 ships Linux (`keyframexl-linux-x86_64.tar.gz`)
only; the Windows port continues on the nightly channel until verified on real
Windows hardware ([docs/v0.2.1.md](docs/v0.2.1.md)).

### New features
- **Native PipeWire output.** KeyFrameXL runs as its own PipeWire node
  (cpal 0.18), with ALSA as the fallback. The buffer setting controls the
  node's quantum, xruns are counted, and the device list shows hardware
  outputs and *System default* instead of ALSA plug names. `realtime-dbus` is
  kept off so Wine/yabridge plugins remain stable.
- **Universal automation & MIDI controller linking.** Right-click almost any
  knob or control to create a Playlist automation clip or link it to a MIDI
  controller: Song BPM (tempo automation clips), Channel Rack Swing, Mixer
  stereo separation / 3-band strip EQ / FX mix / sends, all Sampler program
  and zone knobs, all KeyFrame Tide and built-in FX knobs, Vocal parameters,
  and Forge character knobs.
- **Keyframe Forge Studio rework.** Expanded to an 8-layer DSP engine with
  2-op FM, unison and shape on oscillators, a physical *Modal* material
  resonator (metal, wood, glass, stone, membrane), a *Grain* procedural and
  sample-cloud texture engine, stereo/burst noise, non-destructive sample
  trimming, *Pan* and *Morph* curves, and equal-power *Seamless Loops* with
  integrated LUFS metering. The editor is now a tabbed Studio workspace
  (*Design*, *Presets & Blocks*, *Variations*, *Export*) with an interactive
  multi-layer timeline (drag to move/trim, drop audio files, lock layers),
  live playhead, *Loop* / *Round-Robin* / *Auto-Audition* / *A–B* controls,
  searchable presets and Layer Building Blocks, WYSIWYG / Sound Pack 24-bit
  WAV export with peak/LUFS loudness targets, one-click *→ Playlist* rendering,
  and factory presets rebuilt on the new engine.
- **MP3 / M4A / AAC / AIFF decoding & Audio Clip patterns.** Load MP3, M4A,
  AAC and AIFF files directly alongside WAV/FLAC/OGG, and place FL-style
  Audio Clip patterns directly on the Playlist.
- **Built-in static MP3 export.** LAME (`mp3lame-sys`) is statically compiled
  into KeyFrameXL, so MP3 export works out of the box without installing an
  external `lame` package or DLL.
- **Full MIDI export** (*File → Export → MIDI*): writes channel and mixer
  volume (CC 7) and pan (CC 10), song-mode automation clips, piano-roll slides
  and fine pitch as `PitchBend` (with RPN 0,0 bend sensitivity when over ±2
  semitones), per-note pan and Mod X / Mod Y (CC 74 / CC 71), and playlist
  section markers; melodic tracks skip GM drum channel 10.

### Interface
- **Playlist time selection**: drag either edge on the ruler to resize it.
- **Playlist start marker**: a ruler click sets it; *Stop* returns there and
  *Play* starts there until it is moved or removed (ruler right-click).
- **Mixer effect reordering & replacement**: drag an effect slot onto another
  or right-click → *Move up* / *Move down* (automation and open effect windows
  follow the slot), and right-click → *Replace* to swap an effect in place for
  another built-in or plugin.
- **Mixer route button**: shows where each insert is routed (`→ M`, `→ 2`,
  `→ M+1`), adds an outline when routed outside Master, and lists every
  destination with its send level in the tooltip.
- **Live MIDI key highlight**: held MIDI keys light up live on the Piano Roll
  and Sampler keyboards.
- **Update channel defaults**: new installs of a stable release follow stable
  updates only; nightly builds keep following nightlies. Existing settings
  keep their choice.

### Fixes
- **Playlist loop drag**: the playhead no longer slides along while a time
  selection is dragged during playback; the new loop takes effect on release.
- **Interaction & playback fixes**: prevented click-through behind floating
  controls, refined Piano Roll UX, fixed lead-channel behaviour, and fixed
  playlist clip memory retention (`ed42070`).
- **Offline plugin export**: improved reliability when rendering hosted and
  bridged plugins offline, including SSD5.
- **MIDI import**: reads pitch bends back as piano-roll slides and fine pitch
  (so slides survive a MIDI export and re-import), restores volume and pan on
  split drum channels, and recognises RPN 0,0 bend sensitivity.
- **Window focus & toolbar buttons**: clicking a toolbar button brings its
  covered floating window to the front, and clicking a docked panel's button
  when hidden under a floating window folds that window to its title bar.
- **Minimum window sizes & layout**: Settings, Export and Project info windows
  can no longer be shrunk until controls are clipped, long hints on the
  Plugins page wrap, and the Ukrainian "ready" count shows its number.
- **OS-aware updater**: the updater skips releases that have no build for the
  current OS (keeping Windows installs on nightlies while 0.2.1 stable is
  Linux-only).
- **Wine/yabridge plugin stability on Linux**: fixed plugin initialization
  crashes by dropping `cpal/realtime-dbus` and restoring inherited
  `RLIMIT_RTTIME`.

### Windows port (nightly preview) & cross-OS compatibility
- **Cross-OS `.kfxl` compatibility**: opening a project saved on another OS
  resolves plugins by CLAP `#id`, cross-OS bundle/library stem (`.dll` ↔
  `.so`/`.vst3`/`.clap`), or display name/vendor; bundle and convolution IR
  paths normalize `/` ↔ `\`; FL Studio sample and DirectWave lookup works
  natively on Windows without a Wine prefix.
- **Native Windows support (nightly channel)**: WASAPI and ASIO audio output
  (with Steinberg ASIO trademark attribution in *About* and `README.md`),
  native VST3, CLAP (`win32` GUI + DPI scale) and VST2 (`.dll`) hosting with
  embedded Win32 `HWND` editors and Per-Monitor v2 DPI, out-of-process plugin
  bridge via Named Pipes / `CreateFileMappingW` / Job Objects, `.zip` +
  `.old.exe` self-updater, portable `.zip` and user-scope Inno Setup installer,
  and updated English and Ukrainian manuals.
- **Release & CI**: daily scheduled nightlies instead of per-push releases,
  `vX.Y.Z-rc.N` release candidates, `Cargo.toml` as the single version source,
  and full Linux + Windows integration test suites in CI.

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
