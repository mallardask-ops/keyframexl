# Changelog

Every push to `main` ships a nightly prerelease (`v<next>-nightly.<date>`);
versions below are the stable tags. The 0.2.0 plan with its tasks is in
[docs/v0.2.0.md](docs/v0.2.0.md).

## Unreleased (0.2.0)

### Content
- **Built-in drum kits.** KFX Core (17 808/909-style sounds) and KFX Chip
  (8 console sounds), synthesised by Forge from recipes: no sample files,
  projects store only the recipe. New projects get their Kick, Snare and Hat
  from Core instead of borrowing FL Studio's 909 samples. *Add channel →
  Drum kit* adds a whole kit.
- **KeyFrame Tide: 105 factory presets** (was 18) in ten categories: bass,
  lead, pad, keys, pluck, bell, arp, FX, drum-synth, chiptune. Menus and the
  browser group by category; search finds categories too.
- **Forge: 51 recipes** (was 11) in ten categories: UI, footsteps on seven
  surfaces, actions, weapons, pickups, magic, damage, doors, ambience,
  creatures.
- **Genre starters** on a new *Genres* tab of the Welcome screen: lo-fi
  hip-hop, trap, house, synthwave, chiptune, drum & bass, ambient.
- **Demo song** (*Help → Demo song*, `keyframexl write-demo`,
  `render-demo-song`): 32 bars using every part of the program, with a
  vocal on a synthetic CC0 voice the program writes itself.
- **User manual** (*Help → Manual*): 13 chapters built into the program,
  with search; it works offline. The same pages are in `docs/manual/`.
  The old menu item opened a file that did not exist.
- The `F1` list is grouped and complete (`F8`, `Ctrl+Shift+M`, `Alt+E`,
  `Alt+C`, `Alt+↑/↓`, `Ctrl+K` were missing); the manual's shortcut page is
  generated from the same list.
- New README screenshots, all taken from the demo song
  (`scripts/ux/shots.sh`).

### Fixes
- **Forge export levels.** Export no longer normalises every file to −1 dB
  by default; each recipe is made at its category's level, so a UI click
  stays well under a shotgun. *Normalize* is a switch; *Mono* export added.
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
- Clippy-clean codebase; CI checks `cargo fmt` and `cargo clippy -D warnings`.
- `LICENSE` file (MIT).

### Packaging
- The 16 px icon has its own pixel-aligned drawing.
- Release and nightly archives carry the desktop launcher, the icon set and
  `install.sh` (installs into `~/.local`).

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
