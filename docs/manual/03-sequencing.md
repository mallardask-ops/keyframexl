# Patterns, channels and the playlist

## Patterns

A pattern holds notes for any number of channels. The deck shows the
current pattern: scroll over it to switch, double-click it (or press `F2`)
to rename it. The *Patterns* menu adds (`Ctrl+Shift+Ins`), finds the next
empty one (`F4`), clones, colours and deletes patterns. The last pattern
cannot be deleted.

**Pattern mode** plays the current pattern in a loop. **Song mode** plays
the playlist. `L` switches between them.

## Channel rack (F6)

Every sound source is a channel: a sampler, a Tide synth, a Forge sound,
a vocal, or a plugin.

- Steps: click on, click again off, `Shift+click` velocity, right-click
  clear. A channel whose notes are not simple steps shows them as a small
  piano-roll preview instead.
- Left of each row: the mute lamp (click mutes, right-click or
  `Ctrl+click` solos), pan and volume knobs, and the mixer insert the
  channel plays into (click to choose, `Ctrl+wheel` to step). Click the
  channel's name to open its window; right-click it for more.
- Select channels with the bar at the right end of each row (`Ctrl`
  toggles, `Shift` extends). `Alt+↑` / `Alt+↓` move them,
  `Alt+C` clones them with their notes, `Alt+Delete` removes them.
- `Ctrl+L` routes the selected channels to the first free mixer inserts.
- `Ctrl+K` opens the velocity graph under the steps.
- `Ctrl+1 … 0` mutes channels 1 to 10, `Ctrl+Shift+1 … 0` solos them.
- *Add → Drum kit* adds a whole built-in kit at once.

## Piano roll (F7)

The piano roll edits the notes of one channel in the current pattern.

Tools: `P` draw, `B` paint (draws as you drag), `D` delete, `T` mute,
`C` slice, `E` select, `Z` zoom, `S` slide (a note that glides into the
next one).

- Snap is in the toolbar (1/16, 1/4, bar…). Hold `Alt` while dragging to
  ignore it.
- `Ctrl+A` selects all, `Ctrl+D` deselects; drag on empty space to select
  with a box.
- Arrows move the selection: `↑` / `↓` a semitone, `Ctrl+↑` / `Ctrl+↓` an
  octave, `←` / `→` a grid step, `Alt+←` / `Alt+→` one tick.
  `Shift+←` / `Shift+→` change the length.
- `Ctrl+C`, `Ctrl+X`, `Ctrl+V` copy, cut and paste; `Ctrl+B` duplicates
  the selection after itself.
- `Ctrl+Q` quantises, `Alt+Q` opens the quantise dialog. `Ctrl+L` makes
  notes legato, `Ctrl+U` chops them, `Ctrl+G` glues them.
- `Alt+V` shows the notes of other channels as ghost notes.
- The lane under the notes edits velocity and the other note properties.
- On a vocal channel, `F2` edits the lyric of the selected note.

## Riff Machine (Alt+E)

*Tools → Riff Machine* writes a riff for the active channel. It works in
eight stages, listed on the left: Progression, Chords, Arpeggio, Mirror,
Levels, Articulation, Groove and Fit. Each stage has its own controls
(key and scale, chord voices, rhythm, swing, note range…) and can be
reset or varied on its own. *Recipes* loads a ready setting. The preview
plays the riff in a loop; *Apply* writes the notes into the pattern and
`Ctrl+Z` undoes it.

## Playlist (F5)

The playlist arranges clips on tracks. A clip is a pattern, an audio file
or an automation curve.

- Pick a pattern and click in a track to place it. Drag clips to move
  them, drag their right edge to shorten or lengthen them.
- Right-drag selects clips with a box; a quick right-click deletes the
  clip under the mouse. `Ctrl+A` selects all, `Esc` deselects,
  `Delete` removes the selection.
- Drag an audio file from the browser onto a track to make an audio clip.
- Click the ruler to move the playhead. Drag on the ruler to make a time
  selection the song loops; double-click the ruler to clear it.
- **Markers** name parts of the song: right-click the ruler to add one.
  `Ctrl+Alt+←` / `Ctrl+Alt+→` jump between them; double-click a marker
  (or *Loop to the next marker* in its menu) loops that section.
- The **tempo lane** at the top changes the tempo over the song.

## Undo

Almost every edit is undoable: `Ctrl+Z` undoes, `Ctrl+Y` or
`Ctrl+Shift+Z` redoes.
