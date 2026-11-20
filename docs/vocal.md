# Vocal — UTAU Singing Channels

A **Vocal** channel sings piano-roll notes with lyrics using any **UTAU** voicebank, powered by an embedded **WORLD** vocoder. Kasane Teto is supported with one-click official download, and any standard UTAU voicebank can be loaded from a `.zip` archive or unpacked folder.

---

## Quick Start

1. **Create a Vocal channel:** Open **Add channel → Vocal (singing voice)**. The channel window opens automatically.
2. **Load a voicebank:** In the **Voicebank** section, choose one of:
   - **Download Kasane Teto** — downloads the official archive from `kasaneteto.jp` (~12 MB) into `~/.local/share/keyframexl/voicebanks/Kasane Teto`.
   - **Install zip…** — imports any downloaded UTAU `.zip` voicebank.
   - **Folder…** — points to an unpacked voicebank directory containing `oto.ini`.
3. **Write notes & lyrics in the Piano Roll:**
   - **Double-click a note** (or select it and press `F2`) to type its syllable.
   - Press `Enter` or `Tab` to commit and advance to the next note; press `Esc` to cancel.
   - Use **Tools → Lyrics…** in the Piano Roll to paste a full line or verse — syllables are distributed sequentially across the selected notes (or all notes of that channel in the pattern).
4. **Shape the voice:**
   - **Channel knobs:** `VIBRATO`, `RATE`, `DELAY`, `FADE IN`, `GLIDE` (pitch transition between touching notes), `GENDER` (formant shift without changing pitch), and `BREATH`.
   - **Per-note Piano Roll lanes:**
     - `Velocity` — syllable loudness (also influences consonant speed)
     - `Mod X` — per-note vibrato depth (`0.5` = channel default)
     - `Mod Y` — per-note gender/formant offset (`0.5` = neutral)
     - `Fine pitch` — micro-tuning offset
5. **Live auditioning:** Clicking piano-roll keys or playing a MIDI keyboard sings the channel's `default lyric` (initially `あ`) at the played pitch.

---

## Lyric Input Syntax

| Input | Behavior |
| --- | --- |
| `か`, `カ`, `きゃ` | Sings the matching voicebank entry (Katakana is automatically normalized to Hiragana). |
| `ka`, `shi`, `tsu`, `kya` | Romaji is converted to Kana. Multi-syllable words like `konnichiha` split into 5 syllables across 5 notes during bulk paste. |
| `привіт` | Cyrillic is approximated to the closest Japanese Kana (note: Japanese lacks `l`, `v`, and consonant clusters). |
| `+` or `ー` | Sustains the previous note's vowel while smoothly gliding to the new pitch. |
| `-` | Rest / phrase break (the note produces silence and splits the phrase). |
| Multiple kana in one note | Splits the note's duration evenly across the syllables. |
| Raw voicebank aliases (`ka`, `a ka`) | Looked up verbatim first, so Latin-aliased and VCV voicebanks work out of the box. |

---

## Architecture

| Module | Responsibility |
| --- | --- |
| `src/vocal/oto.rs` | Parses `oto.ini` (Shift-JIS and UTF-8), `prefix.map`, and `character.txt`. |
| `src/vocal/lyric.rs` | Syllable tokenizer and Katakana / Romaji / Cyrillic transliteration. |
| `src/vocal/analysis.rs` | WORLD vocoder analysis (`Dio` + `StoneMask`, `CheapTrick`, `D4C`) for each `.wav` sample, cached on disk under `~/.cache/keyframexl/vocal`. |
| `src/vocal/render.rs` | Applies UTAU timing rules (`offset`, `consonant`, `cutoff`, `preutterance`, `overlap`, velocity-scaled consonant speed), builds pitch/vibrato contours, extends vowels via attack → ping-pong sustain → tail, crossfades adjacent syllables in the spectral envelope domain, and runs one WORLD synthesis pass per phrase. |
| `src/vocal/plan.rs` | Groups notes into phrases (splitting on gaps `> 30 ms`). Phrases are content-hashed so editing one phrase re-renders only that phrase, and moving a phrase in time requires no re-render. |
| `src/vocal/host.rs` | Background render worker pool (up to 4 threads), ~200 MB in-memory phrase cache (for fast undo/repeats), lock-free `VocalMap` publication to the audio thread, and synchronous `render_all` for export. |
| `src/engine.rs` | Triggers phrases at the first note's sample position shifted early by `preutterance`. Live keyboard playback uses pre-rendered C2–C6 preview buffers. |

Lyrics are stored in `Pattern.lyrics` keyed by note ID, so copying notes, duplicating patterns, **Make unique**, and Undo/Redo all preserve lyrics cleanly. Audio export waits for all pending phrase renders before writing output.

---

## Timing and clips

In song mode every Playlist clip gets its own phrases, timed through the song
tempo map exactly as the engine times the notes (swing included); a tempo
change inside a phrase renders it again. A clip sings the notes that start
inside it (a clip cut mid-phrase starts singing at its first note) and its end
cuts the phrase. Pattern mode plays each pattern as a loop at the song tempo.

## Known Limitations

- **Pitch slides:** Piano-roll slide notes are not used; draw the pitch in the piano roll's *Pitch* lane instead (per-note points and vibrato, stored in `Pattern.pitch`; without a curve, transitions use a cosine glide controlled by `GLIDE`). UST import is not there yet.
- **UTAU flags & external resamplers:** UTAU flags (`g`, `B`, `H`, …) and external resampler binaries are not supported — synthesis always uses the built-in WORLD engine.
- **Live key preview:** Audition buffers are capped at 2.5 seconds per note and use the single default syllable.
- **Missing syllables:** If a syllable has no matching entry in the loaded voicebank, that note stays silent and the missing lyric is listed in red in the Vocal channel window.

---

## Voicebank Licensing & Tests

- **Kasane Teto** is distributed under the creator's free non-commercial license ([guidelines](https://kasaneteto.jp/guideline/voice.html)). No voicebank files are bundled in this repository; the in-app button downloads the official archive only when requested by the user.
- Automated tests (`tests/vocal_synthetic.rs`) run against a generated synthetic voicebank. Real-voicebank tests (`tests/vocal_teto.rs`) are opt-in via the `KFXL_VOICEBANK` environment variable.
