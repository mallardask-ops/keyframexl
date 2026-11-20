# Forge — Game SFX Designer

**Forge** is a dedicated procedural sound-effect channel built for game audio (jumps, coins, hits, lasers, explosions, UI clicks, ambient hums, and more). Sounds are assembled from up to six layers, shaped with drawn curves and effects, and exported as ready-to-use game assets with automatic variations.

Playing a note on a Forge channel triggers the rendered sound: **C5** plays at its authored pitch and speed, while higher or lower notes pitch-shift and scale playback speed accordingly.

---

## Quick Walkthrough

1. **Create a Forge channel:** Open **Add channel → Forge (game sounds)**. New channels start with the *Jump* recipe.
2. **Pick a starting point (`Start from`):** Choose from 51 built-in recipes grouped by category (UI, Footsteps on seven surfaces, Actions, Weapons & combat, Pickups, Magic, Damage, Doors & mechanisms, Ambience, Creatures & sci-fi) or load one of your own saved sounds from **My sounds**. Each recipe is made at its category's level, so a UI click sits well under a shotgun; UI sounds and the heartbeat take only the light part of a game style (no added weight layer, ring mod or vibrato). Click **▶ PLAY** or click directly on the waveform to audition.
3. **Set a style:**
   - **GAME STYLE** — a project-wide style setting saved with the project. Every recipe in *Start from* and every newly created Forge channel automatically starts in this style (`Any` leaves recipes neutral).
   - **MAKE IT** — transforms the current sound on demand (and can be clicked multiple times to push further):
     - *Retro* — chiptune waveforms, stepped noise, bit reduction
     - *Cartoon* — rounder waveforms, bouncy pitch sweeps, clean output
     - *Heavy* — lower pitch, bigger body, saturation/drive, added sub layer
     - *Sci-fi* — vibrato, phaser, ring modulation, room resonance
4. **Shape with `CHARACTER` macros:**
   - **SIZE** — small and high-pitched ↔ massive, low, and long
   - **ENERGY** — restrained ↔ aggressive (loudness and sweep depth)
   - **DIRT** — clean ↔ saturation, then bitcrushing
   - **SPACE** — dry ↔ cavernous reverb
   - Plus overall **PITCH** and **OUTPUT** controls.
5. **Explore variations (`IDEAS → Mutate`):** Generates 8 deterministic relatives of the current patch. Single-click a card to audition; double-click or click **Use NN** to adopt it (`Ctrl+Z` reverts to the previous patch).
6. **Build with `LAYERS` (up to 6):**
   - Toggle the layer switch to mute, click **S** to solo, or click a layer's name to edit it below.
   - **+ Add layer** supports five source types:
     - **Oscillator** — polyBLEP waveforms
     - **Noise** — selectable noise colors
     - **Sample…** — WAV/FLAC/OGG file embedded directly into the project
     - **Synth note** — renders a note from any KeyFrame Tide preset
     - **Note from a channel** — captures a note from any channel in the current project (VST plugin, Sampler, Tide, or Vocal) through its mixer insert chain, identical to song export. Choose the target channel, **NOTE**, and **HOLD** duration, then click **Bounce** whenever you want to re-capture (for example, after changing a plugin preset).
7. **Edit layer parameters & curves:**
   - Configure the source, **START**, **LENGTH**, **GAIN**, and **PAN**.
   - Draw three time envelopes:
     - **Pitch** — pitch contour in semitones (snaps to semitones; hold `Alt` for free editing)
     - **Volume** — amplitude contour
     - **Filter** — filter mode, cutoff frequency curve, and **RESO**
   - Drag control points to move them, double-click to add or remove points (at least two always remain), and drag the segment between points to bend its curve.
8. **Add effects (`Layer effects` & `Master effects`):** Insert any of 14 built-in effects with full parameter controls (*Bitcrusher, Drive, Wavefolder, Squeeze, Ring Modulator, Phaser, Chorus, Tremolo, Filter, Delay, Reverb, Room Reverb, Compressor, Transient Shaper*).
9. **Save presets (`Save to My sounds`):** Saves the patch to `~/Music/KeyFrameXL/Presets/Forge/<name>.kfxforge`, making it immediately available under **Start from**.
10. **Export for games (`EXPORT FOR GAME`):**
    - Choose **FILES** count to export the authored sound plus subtle randomized variations so repeated in-game triggers never sound robotic (`jump_01.wav`, `jump_02.wav`, …).
    - Select **WAV (16-bit)** or **OGG**, **48 kHz** or **44.1 kHz**, and optional **Normalize** (peak −1 dB; off by default so a set of sounds keeps its balance) and **Mono** (one channel, for sounds a game positions in 3D). Leading and trailing silence is trimmed automatically.

---

## Architecture

| Module | Responsibility |
| --- | --- |
| `src/forge/mod.rs` | Core model (`ForgePatch`, `Layer`, `Source`, `Macros`) and `ForgeSound` (patch + rendered audio buffer). Only the patch is saved in `.kfxl` projects; audio is re-rendered on load. |
| `src/forge/curve.rs` | Curve evaluation and tension shaping (reusing `automation::shape` from automation clips). |
| `src/forge/layer.rs` | Layer sources (polyBLEP oscillators, noise generators, samples, Tide synth notes via `Engine`) and TPT SVF filter. |
| `src/forge/render.rs` | Signal path: per-layer source → layer FX → mix → master FX → `DIRT`/`SPACE` macros → −1 dB limiter → tail trimming. |
| `src/forge/recipes.rs` | The `Recipe` list, levelling by category, style transforms. |
| `src/forge/library.rs` | The recipe catalogue (40 recipes past the first 11) and categories. |
| `src/forge/mutate.rs` | Seeded, deterministic patch mutations and export variations. |
| `src/forge/export.rs` | Batch file export, Lanczos resampling, and OGG encoding. |
| `src/ui/forge.rs` | Channel editor window, interactive curve editor, and background render scheduling. |

### Engine & Threading Notes
- **Shared DSP:** Layer and master effects reuse the exact `FxState` implementations from the mixer. After a layer ends, effects continue running until their tail decays to silence (up to ~2 seconds for delays/reverbs).
- **Playback:** The voice engine plays `ForgeSound::rendered` as a one-shot (`Source::Forge` in `src/engine.rs`), supporting note pitch, fine pitch, slides, and velocity.
- **Non-blocking rendering:** Editing updates the patch immediately (`ForgeSound::edit`) and marks the buffer stale while a background thread renders (`App::poll_forge`, one in-flight job per channel so rapid drags do not queue up). Each revision is numbered so late renders from older edits are discarded. Every drag or edit gesture records a single undo step, and song export flushes any stale Forge channels first (`forge::refresh_all`).
- **Performance (release builds):** Typical render times range from `< 1 ms` (UI click) to `~60 ms` (4-second Hum).

---

## Verification & Testing

- **Library tests:**
  ```sh
  cargo test --lib forge
  ```
  Covers clip-free / NaN-free recipes, determinism, macro behavior, mute/solo, sample and Tide synth layers, malformed patch sanitization, project round-trips, bit-exact engine playback (including octave-up half-length scaling), 5-variation WAV/OGG batch export, resampling, and *My sounds* persistence.
- **UI & integration tests:**
  ```sh
  cargo test --bin keyframexl forge
  ```
  Covers the curve editor (dragging, semitone snap, double-click add/remove, tension bending), channel window, background render coalescing and undo, project game style, and bouncing a Tide channel into a layer.
- **Live VST bounce capture:**
  ```sh
  KFXL_CAPTURE_FORGE=0 \
  KFXL_CAPTURE_PLUGIN=".../Grand Piano.vst3" \
  KFXL_CAPTURE_FORGE_BOUNCE=out.wav \
  KFXL_CAPTURE_FORGE_BOUNCE_AFTER=25000 \
  KFXL_CAPTURE_AFTER_MS=45000 \
  cargo run --release -- --capture-instrument shot.png
  ```
  Bounces C5 from an external VST3 plugin into a Forge layer after 25 s and writes the result to `out.wav`. (Verified 2026-11-19 with Grand Piano: produced a 1.6 s C4/C5 fundamental + harmonics render.)
- **Recipe screenshots:**
  ```sh
  KFXL_CAPTURE_FORGE=N cargo run --release -- --capture-instrument out.png
  ```
  (`N` is the recipe index in `Recipe::ALL`, `0..50`.)

---

## Known Limitations

- **Wine plugin sample loading:** Some Windows VST plugins running under Wine take several seconds after opening to finish loading samples into memory. If you bounce immediately, Forge may report `"played silence"` — wait a moment and click **Bounce** again. (Full song export shares this behavior.)
- **Channel bounce is a snapshot:** A *Note from a channel* layer stores the bounced audio; later edits to the source channel do not update the Forge layer until you click **Bounce** again.
