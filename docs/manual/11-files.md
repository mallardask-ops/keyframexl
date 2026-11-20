# Projects, export and updates

## Projects

- `Ctrl+S` saves a `.kfxl` project, `Ctrl+Shift+S` saves it under a new
  name, `Ctrl+N` saves a new version next to it (`name_2`, `name_3`…).
- `Ctrl+O` opens a `.kfxl` or `.flp` file. Recent projects are on the
  Welcome screen.
- Unsaved work is written to an autosave file every two minutes
  (*Settings → General → Autosave*). After a crash, *File → Recover
  autosave* opens it.
- `F11` (*File → Project info…*) holds the title, author and genre, which
  also go into MP3 tags.

## Bundles

A project refers to samples on your disk. *File → Export → Export
bundle…* writes one `.zip` with the project and every file it uses, so it
opens on another computer. *File → Import → Import bundle…* unpacks one.

## Export audio (Ctrl+R)

- **Source**: the song, the current pattern, or the playlist time
  selection.
- **Formats**: WAV (16-bit, 24-bit, 32-bit float), FLAC (16 or 24-bit) and
  MP3 (128, 192 or 320 kbps; needs the `lame` package). Several formats
  can be written in one pass.
- **Tail**: *Leave remainder* lets reverbs and releases ring out after the
  end (up to 30 s); *Cut remainder* ends at the last bar; *Wrap remainder*
  mixes the ring-out onto the start, for a seamless loop.
- *Normalize to −1 dB peak*, *Dither 16-bit output*.
- *Split mixer tracks* writes one file (stem) per mixer insert.

*File → Export → MIDI* (`Ctrl+Shift+M`) writes the notes as a MIDI file;
*File → Import → MIDI* reads one.

## Settings

*Options* has four pages: **Audio**, **MIDI**, **Plugins** and
**General**. General holds the theme (dark, light, or the desktop's
choice; also in *View → Theme*), the language (English or Ukrainian), the
Welcome screen, the interface size, autosave, *Ask on close*, the window
system (system title bar, X11 window), what a new project loads
(*Starter sounds*), and updates.

## Updates

*Help → Check for updates…* looks for a new version on GitHub. If there is
one, it shows the release notes, downloads the archive, checks its
SHA-256, installs it in place of the running program and restarts.

*Settings → General → Nightly builds* follows the nightly prerelease made
for every change; switch it off to get only stable versions. *Check
automatically* looks at every start. A copy run from a cargo `target/`
folder is not replaced: update the source and build again.
