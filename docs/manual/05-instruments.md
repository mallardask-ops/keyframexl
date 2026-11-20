# Instruments

*Add → Channel* adds an instrument. Click a channel's name in the rack to
open its window.

## Sampler

A multi-zone sampler in the style of DirectWave. Drag a sample or a
`.dwp` preset from the browser onto the channel rack to make a sampler
channel, or import a DirectWave preset
with *File → Import → Sampler preset (.dwp)*.

- **Zones** map a sample to a key range and a velocity range, with a root
  key, fine tune and gain. Zones in the same group can play round-robin or
  at random.
- **Loop type**: off, forward, one shot, sustained (loops while the key
  is down) or bounce (back and forth).
- **Envelopes**: delay, attack, hold, decay, sustain, release for volume
  and for modulation.
- **Filters**: two filters, in series or in parallel (`F1//F2`), with key
  tracking.
- **Two LFOs** and a 16-slot **modulation matrix**.
- **Per-voice effects**: bitcrusher, ring modulator, quantiser, decimator,
  phaser.

- **Time stretch** (Zone page): *Time* changes the length, 25 to 400 %,
  and keeps the pitch; *Beat sync* makes the zone a number of beats long
  at the song's tempo, so a loop follows tempo changes. The stretched
  sound is made in the background; the line under the knob says when it
  is ready.

Hold the zone's audition button to hear it at its root key.

## KeyFrame Tide

The built-in polyphonic synthesiser. Its header has a preset menu with
105 factory presets: *Init* and ten categories (bass, lead, pad, keys,
pluck, bell, arp, FX, drum synth, chiptune). The
browser (`Alt+F8`) lists them by category too, and its search finds a
category by name.

Each voice has two oscillators with unison, noise, a filter with its own
envelope, LFOs, a modulation matrix, glide and effects. *Mutate* makes a
relative of the current sound. *Save…* stores a patch as your own preset
(`Presets/Synth`).

## Built-in drum kits

Two kits are synthesised by Forge, so they need no sample files and a
project stores only their recipes:

- **KFX Core**: 17 sounds in the style of 808 and 909 drum machines
  (kicks, snares, claps, hats, toms, percussion).
- **KFX Chip**: 8 sounds in the style of game consoles.

A new project starts with Kick, Snare and Hat from Core. *Add → Drum kit*
adds a whole kit. *Settings → General → Starter sounds* chooses what a
new project loads.

## Forge and Vocal

Forge makes sound effects for games, Vocal sings lyrics with an UTAU
voicebank. Each has its own chapter.

## Plugins

CLAP, VST3 and VST2 instruments are under *Add → Plugin instrument*. See
*Plugins and FL Studio*.
