# Vocal: singing channels

A Vocal channel sings the notes of the piano roll with the lyrics you
type, using an UTAU voicebank and the WORLD vocoder.

## Set up

1. *Add → Channel → Vocal (singing voice)*. The channel window opens.
2. Choose a voicebank: **Download Kasane Teto** (the official archive,
   about 12 MB, under its own free non-commercial licence),
   **Install zip…** for any UTAU voicebank, or **Folder…** for an
   unpacked one with an `oto.ini`.

The demo song (*Help → Demo song*) writes a small synthetic voice of its
own (CC0) and uses it.

## Lyrics

- Double-click a note, or select it and press `F2`, to type its syllable.
  `Enter` or `Tab` goes to the next note, `Esc` cancels.
- *Tools → Lyrics…* in the piano roll spreads a whole line over the
  selected notes.
- Kana (`か`, `カ`), romaji (`ka`, `shi`, `kya`) and Cyrillic (approximated
  to the nearest kana) are understood. `+` or `ー` holds the previous
  vowel into a new pitch, `-` is a rest.
- A note whose syllable is missing from the voicebank stays silent; the
  channel window lists missing syllables in red.

## Shape the voice

Channel knobs: *Vibrato*, *Rate*, *Delay*, *Fade in*, *Glide* (between
touching notes), *Gender* (formants without changing pitch) and *Breath*.
Piano-roll lanes per note: *Velocity* (loudness), *Mod X* (vibrato depth),
*Mod Y* (gender), *Fine pitch*.

Phrases render in the background. Editing one phrase renders only that
phrase again; moving it in time needs no render. Export waits for every
phrase.

## Limits

Slides and UTAU pitch-bend curves are not supported yet, nor UTAU flags
and external resamplers.
