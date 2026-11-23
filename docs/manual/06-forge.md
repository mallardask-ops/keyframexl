# Forge: layered game SFX studio

Forge builds sound effects from up to **eight layers** on an interactive
timeline, auditions variations in Round-Robin or Seamless Loop mode, and
exports ready-to-use files (or an entire multi-channel Sound Pack) for games.

A note on a Forge channel plays the sound: C5 plays it as it was made,
other notes play it higher or lower (and shorter or longer).

## Designing a sound in Forge

1. *Add → Channel → Forge (game sounds)*.
2. **Transport & quick tools**: click **▶ Play** (or press `Space`, or click
   the waveform) to hear the sound with a moving playhead. Toggle **Loop**
   for seamless looping, **RR** to audition approved variations in
   Round-Robin order, **Auto** to play automatically after every edit, and
   **A / B** to compare two versions of your patch. Use **→ Playlist** to
   render the sound directly onto the Playlist as an audio clip.
3. **Presets & Blocks**: search 51 built-in sounds (including seamless
   ambient loops) and your own *My sounds*, filter by category or *Seamless
   loops*, or switch to **Layer blocks** to append ready-made building blocks
   (*Sub Drop*, *Metal Strike*, *Wood Knock*, *Glass Ping*, *Gravel Debris*,
   *Plasma Crackle*, *Stereo Whoosh*, *Air Tail*) straight into your sound,
   or click **Inspire** to mutate the current sound.
4. **Master & Layer Timeline**: drag clip blocks horizontally to move when a
   layer starts, drag clip edges to trim length, toggle mute, solo (**S**),
   or **Lock** (keeps a layer unchanged in variations), or drag-and-drop WAV,
   FLAC, or OGG files straight from the Files browser onto the timeline.
5. **Layer Inspector & Curves**:
   - Six sources: **Oscillator** (with 2-op FM, Unison, Detune, Shape/PWM),
     **Modal** (physical Metal, Wood, Glass, Stone, and Membrane resonators),
     **Grain** (procedural or sample-based granular texture cloud), **Noise**
     (stereo width & burst gating), **Sample** (non-destructive Trim In/Out,
     Reverse, and *Note from a channel* bounce), and **Tide** synth notes.
   - Five curves per layer (**Pitch**, **Volume**, **Filter**, **Pan**, and
     **Morph**) with one-click shapes (*Drop, Pluck, Ramp, Bell, Stutter,
     Flat*), drawn over a ghost waveform. Drag points (`Alt` frees Pitch from
     semitones), double-click to add or remove points, and drag between
     points to bend tension.
   - **Character** macros (*Size*, *Energy*, *Dirt*, *Space*, *Pitch*,
     *Output*) plus per-layer and master chains from 14 mixer effects.
6. **Variations**: set the number of variations (1–12) and **Spread**
   (subtle to wild). Audition each card (**▶**), reroll a single slot
   (**↻**), pin keepers (**★**), or click **Use** to promote one to the main
   patch. Locked layers stay untouched.
7. **Export**: write the exact approved variations for this channel — or
   switch Scope to **All Forge channels** to export an entire Sound Pack at
   once. Choose WAV 16-bit, WAV 24-bit, or OGG; 48 or 44.1 kHz; Stereo or
   Mono; *As mixed*, *−1 dB peak*, or *−14 LUFS*; and a filename template
   such as `{name}_{nn}`.

## My sounds

*Save to My sounds* writes the patch to
`~/Music/KeyFrameXL/Presets/Forge/<name>.kfxforge`, where it immediately
appears in **Presets & Blocks → My sounds**.
