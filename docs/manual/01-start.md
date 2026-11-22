# Getting started

KeyFrameXL is a pattern-based music sequencer for Linux and Windows. You build
short **patterns** (a drum loop, a bass line), arrange them on the **playlist**
into a song, and mix the channels on the **mixer**. Every editor is a
window you can dock, stack or float.

This manual is built into the program (*Help → Manual*) and works
offline. The same pages are in the repository under `docs/manual/`.

## Install

### Linux

1. Download `keyframexl-linux-x86_64.tar.gz` from the GitHub releases page.
2. Unpack it and run `./install.sh`. The program goes to `~/.local/bin`,
   the launcher and icons to `~/.local/share`. Set `PREFIX` to install
   somewhere else.
3. Start *KeyFrameXL* from the application menu, or run `keyframexl`.

You need 64-bit Linux, OpenGL, and PipeWire or ALSA for sound. The program
runs through X11 (XWayland on a Wayland desktop) so plugin windows can be
embedded; `KFXL_X11=0` runs it on pure Wayland without plugin embedding.

### Windows

1. Download `keyframexl-windows-x86_64-setup.exe` from the GitHub releases
   page and run it (installs to `%LOCALAPPDATA%\Programs\KeyFrameXL` without
   administrator rights, adds a Start Menu shortcut and `.kfxl` association).
   Or unpack the portable `keyframexl-windows-x86_64.zip` anywhere and run
   `keyframexl.exe`.
2. You need 64-bit Windows 10/11 and OpenGL drivers; sound works via WASAPI
   out of the box or ASIO with an audio interface.

On both platforms, the program updates itself after installation (see
*Projects, export and updates*).

## The screen

- **Menu bar** at the top: File, Edit, Add, Patterns, View, Options,
  Tools, Help.
- **Deck** under it: transport (play, stop, record), Pattern / Song switch,
  tempo, the current pattern, the master meter and a small oscilloscope or
  spectrum (*View → Deck monitor*).
- **Workspace** below: the editors. `F5` Playlist, `F6` Channel rack,
  `F7` Piano roll, `F9` Mixer, `Alt+F8` Browser. Pressing the key again
  closes the window. A closed window opens as a tab in the window that has
  the focus, or in a window of its own: *Settings → General → Open
  panels*.
- **Status bar** at the bottom: what just happened, and hints. Most
  messages that change the project end with "Ctrl+Z undoes".

There are three workspace layouts: `Alt+1` *Arrange*, `Alt+2` *Compose*,
`Alt+3` *Edit*. Drag a tab by its name or by the grip `⋮⋮` to dock it
elsewhere; drop it outside the window to float it. Layouts and window
positions are remembered. *View → Reset this workspace* puts one back.

## The Welcome screen

It opens when the program starts (switch it off in
*Settings → General*). From it you can open a recent project, start from a
template, start from a **genre** (lo-fi, trap, house, synthwave, chiptune,
drum & bass, ambient), or search the Tide and Forge presets. *Help → Demo
song* opens a 32-bar song that uses every part of the program.

## Interface size

`Ctrl+=` and `Ctrl+-` make everything larger or smaller, `Ctrl+0` resets
to 100%. *View → Interface size* has fixed steps.
