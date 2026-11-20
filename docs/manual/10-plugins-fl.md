# Plugins and FL Studio

## Plugin hosting

KeyFrameXL hosts **CLAP**, **VST3** and **VST2** plugins, instruments and
effects. *Options → Plugins…* lists them, with a search by name or vendor.

Folders scanned:

- CLAP: `~/.clap`, `/usr/lib/clap`, `/usr/local/lib/clap`, `$CLAP_PATH`.
- VST3: `~/.vst3`, `/usr/lib/vst3`, and more in the list.
- VST2: `~/.vst`, `/usr/lib/vst`, and more in the list.

*Add folder…* adds your own. *Rescan all* checks every plugin again;
*Rescan this plugin* probes only one. A plugin that failed is listed under
*Problems* with the reason.

**Process.** By default every plugin runs in its own process (*Bridged*):
a plugin that crashes goes silent but the program keeps running. *Inside
the program* is faster to load but a crash closes KeyFrameXL. Set it per
plugin or in the settings.

**Editors** open embedded in the channel window (X11 or XWayland). On
pure Wayland (`KFXL_X11=0`) an editor opens in its own window. *Show
editor* / *Hide editor* toggle it.

A plugin's state is saved in the project. *Reload* loads it again from
its last saved state; *Reset to defaults* forgets its settings.
A plugin with several outputs can give every output pair its own mixer
insert (*Map to inserts*).

## Windows plugins (Wine, yabridge)

Windows VST2 and VST3 plugins run through yabridge and Wine. Install
yabridge, run `yabridgectl sync`, and the bridged plugins appear in the
normal folders. *Options → Plugins → Wine* picks which Wine to use:
*System Wine* or a folder with a `wine` binary (a Proton or Wine-GE
build, for example). Plugins restart with the chosen Wine.

Some Windows plugins take a few seconds after loading to read their
samples; an export or a bounce made at once can come out silent. Wait
and try again.

## FL Studio projects

*File → Import → Import FL Studio project (.flp)…* (or open the `.flp` directly)
brings in:

- channels with their notes, patterns and the playlist (pattern, audio and
  automation clips, with clip mute, fades and audio offsets);
- samples, with the channel's stretch pitch;
- the mixer: inserts, routing, levels and the built-in effects that have a
  KeyFrameXL counterpart;
- the tempo;
- plugins that are installed here, with their state.

The status bar and the import report say what could not come across.
FL Studio's own generators and FLEX are not native KeyFrameXL
instruments; see below.

## FL Studio live link

For Image-Line instruments that do not run inside FL Studio VSTi (such
as FLEX), a channel can play FL Studio itself: *FL Studio (live link)* in
the plugin list. Its notes go to FL's MIDI input `KeyFrameXL - FL link`,
and FL's sound comes back into the channel. *Open FL Studio* starts it
under Wine; in FL, add the instrument to the channel rack and keep its
channel selected.

The link plays in real time only: it is not in WAV export yet.
