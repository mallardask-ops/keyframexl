# Command line and troubleshooting

## Command line

- `keyframexl` starts the program; `keyframexl song.kfxl` or
  `keyframexl song.flp` opens a project.
- `keyframexl --demo` starts with a short demo beat.
- `keyframexl write-demo demo.kfxl` writes the 32-bar demo song as a
  project; `keyframexl render-demo-song demo.wav` renders it.
- `keyframexl render-demo demo.wav` renders the short demo;
  `keyframexl play-demo` plays it through PipeWire.
- `keyframexl plugin-scan` scans the plugin folders and prints what it
  found.

Environment variables:

- `KFXL_X11=0` runs on pure Wayland on Linux (plugin editors open in their own
  windows).
- `KFXL_NO_BRIDGE=1` loads every plugin inside the program.
- `KFXL_LANG=en|uk` sets the interface language for this run.
- `KEYFRAMEXL_RECORDINGS` is the folder for recorded takes.
- `CLAP_PATH`, `VST3_PATH`, `VST_PATH` add plugin folders.

## Where files are

- **Log** (attach to a bug report; previous run is `keyframexl.log.1`):
  - Linux: `~/.local/state/keyframexl/keyframexl.log`
  - Windows: `%LOCALAPPDATA%\keyframexl\keyframexl.log`
- **Settings and recent projects:**
  - Linux: `~/.config/keyframexl/`
  - Windows: `%APPDATA%\keyframexl\`
- **Autosave and voicebanks:**
  - Linux: `~/.local/share/keyframexl/`
  - Windows: `%LOCALAPPDATA%\keyframexl\`
- **Plugin and vocal cache:**
  - Linux: `~/.cache/keyframexl/`
  - Windows: `%LOCALAPPDATA%\keyframexl\`
- **Your presets, Forge sounds and recordings:**
  - Linux: `~/Music/KeyFrameXL/`
  - Windows: `%USERPROFILE%\Music\KeyFrameXL\`

## Problems

**No sound.** Check the device in *Options → Audio settings…* and press
*Restart audio*. Make sure the master and the channel are not muted and the
channel's mixer insert goes to the master. `Ctrl+H` stops stuck notes.

**Crackles.** Raise the buffer size in the audio settings. A release
build plays much more smoothly than a debug build.

**A plugin is missing.** Look at *Options → Plugins… → Problems*: it
says why a plugin did not load. *Rescan this plugin* tries it again.

**A plugin crashed.** A bridged plugin goes silent and the program keeps
running; *Reload* brings it back from its last saved state.

**A plugin window is blank or does not take clicks.** Run under X11 or
XWayland (the default). Windows plugins need a working yabridge and Wine;
try another Wine build in the plugin settings.

**The vocal is silent.** Load a voicebank in the channel window. A syllable
the voicebank does not have stays silent; the channel window lists it.

**Forge plays silence after a bounce.** Some Windows plugins load their
samples a few seconds after they open: wait and press *Bounce* again.

## Reporting a bug

Open an issue on GitHub (github.com/mallardask-ops/keyframexl) with what you
did, what happened, and the log file.
