# Forge: sounds for games

Forge builds a sound effect from up to six layers and exports it as files
for a game, with variations so a sound repeated in the game does not
sound the same every time.

A note on a Forge channel plays the sound: C5 plays it as it was made,
other notes play it higher or lower (and shorter or longer).

## Five minutes to a game sound

1. *Add → Channel → Forge (game sounds)*. The channel starts on the
   *Jump* recipe.
2. **Start from**: pick one of 51 recipes in ten categories: UI, footsteps
   on seven surfaces, actions, weapons, pickups, magic, damage, doors,
   ambience, creatures. Click **▶ Play** or the waveform to hear it.
3. **Game style** sets a style for the whole project (*Retro*, *Cartoon*,
   *Heavy*, *Sci-fi*, or *Any*). **Make it** applies a style to the current
   sound; click it again to push further.
4. **Character**: four knobs. *Size* (small and high to big and low),
   *Energy* (gentle to aggressive), *Dirt* (clean to distorted), *Space*
   (dry to a large reverb). *Pitch* and *Output* below.
5. **Ideas → Mutate** makes eight relatives of the sound. Click a card to
   hear it, double-click (or *Use NN*) to take it. `Ctrl+Z` goes back.
6. **Export for game**: the number of files, WAV (16-bit) or OGG, 48 or
   44.1 kHz, *Normalize* and *Mono*. Files are named `jump_01.wav`,
   `jump_02.wav`… Silence at the start is trimmed.

Each recipe is made at the level of its category, so a set of exported
sounds keeps its balance: a UI click stays well under a shotgun. Leave
*Normalize* off for that; switch it on to bring every file to −1 dB.

## Layers

**+ Add layer** adds one of five sources: an oscillator, noise, a sample
file (stored in the project), a note of a Tide preset, or a note from any
channel of the project (a plugin, a sampler, a vocal), bounced through
its mixer insert. Click **Bounce** again after you change that channel.

Each layer has a start, length, gain and pan, and three drawn curves:
pitch (in semitones; hold `Alt` for free values), volume, and filter
cutoff. Drag points, double-click to add or remove one, drag a segment
to bend it.

Layer effects and master effects take 14 of the mixer's effects.

## My sounds

*Save to My sounds* writes the sound to
`~/Music/KeyFrameXL/Presets/Forge/<name>.kfxforge`. It then appears in
*Start from*.
