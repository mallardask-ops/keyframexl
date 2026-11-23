# Forge — Layered Game SFX Studio

**Forge** is a dedicated procedural and sample-layered sound-effect workstation built for game audio and modern sound design (tactile UI, footsteps, impacts, weapons, magic, creatures, seamless ambient loops, and more). Sounds are assembled from up to **eight layers** on an interactive visual timeline, shaped with drawn curves and mixer effects, auditioned in Round-Robin or Seamless Loop mode, and exported as ready-to-use game assets or multi-channel Sound Packs.

Playing a note on a Forge channel triggers the rendered sound: **C5** plays at its authored pitch and speed, while higher or lower notes pitch-shift and scale playback speed accordingly.

---

## Studio Walkthrough

1. **Create a Forge channel:** Open **Add → Channel → Forge (game sounds)**.
2. **Transport & Studio Deck:**
   - **▶ PLAY / STOP** (or `Space`, or clicking the master waveform) auditions the sound with a live playhead.
   - **Loop** switches the patch into **Seamless Loop** rendering (tail wrapping + equal-power loop-seam splice) and plays continuously on repeat.
   - **RR** (Round-Robin) cycles through your approved variations on every Play / `Space` trigger so you hear how repeated triggers behave in a game engine.
   - **Auto** automatically auditions the sound when you release a knob, curve, or timeline drag.
   - **A / B** swaps between two patch snapshots for instant comparison.
   - **Init**, **Save to My sounds**, and **→ Playlist** (renders the Forge sound directly onto the Playlist as an audio clip).
3. **Interactive Layer Timeline (up to 8 layers):**
   - Each lane displays a color-coded clip positioned by `start` and `length`, with inline mute LED, **S**olo, and a **Lock** toggle (locked layers stay untouched during variations).
   - Drag a clip body to move its start time, drag its left/right edges to trim length, or drag-and-drop WAV/FLAC/OGG files directly from the Files browser onto the timeline.
   - **+ Add layer** supports six source types:
     - **Oscillator (`Osc+`)** — polyBLEP waveforms with 2-op FM (`FM Idx`, `Ratio`), `Unison` (1–4 voices), `Detune`, `Shape` (PWM / wavefold), and vibrato.
     - **Modal** — 8-mode physical material resonator (`Metal`, `Wood`, `Glass`, `Stone`, `Membrane`) with `Damp`, `Inharm`, and `Strike` exciter shaping.
     - **Grain** — deterministic granular/texture cloud over procedural micro-grains or an embedded sample (`Density`, `Grain ms`, `Scatter`, `Spread`, `Spray`).
     - **Noise** — five noise colors (`White`, `Pink`, `Brown`, `Crackle`, `Chip noise`) plus stereo `Width` and stochastic `Burst` gating.
     - **Sample… / Note from a channel** — embedded WAV/FLAC/OGG or bounced project channel (VST, Sampler, Tide, Vocal) with non-destructive `Trim In` / `Trim Out`, `Reverse`, and one-click `Bounce`.
     - **Synth note** — renders a note from any KeyFrame Tide preset.
4. **Curve Editor (`Pitch`, `Volume`, `Filter`, `Pan`, `Morph`):**
   - Five time curves per layer over a subtle ghost waveform reference, plus one-click shape presets (**Drop, Pluck, Ramp, Bell, Stutter, Flat**).
   - Drag control points (`Alt` disables semitone snap on Pitch), double-click to add/remove points, and drag between points to bend tension.
   - **Morph** modulates each source's primary expressive parameter over time (FM/shape on Osc+, brightness/ring on Modal, density on Grain, burst on Noise).
5. **Character Macros & FX:**
   - **SIZE**, **ENERGY**, **DIRT**, **SPACE**, **PITCH**, and **OUTPUT** shape the entire sound, alongside per-layer and master FX chains drawn from 14 built-in mixer effects.
6. **Presets & Layer Blocks (`Presets & Blocks` tab):**
   - Searchable tagged browser across 51 built-in sounds (with Modal/Grain/FM synthesis and Seamless Loop ambience), **My sounds** (`~/Music/KeyFrameXL/Presets/Forge/*.kfxforge`), and reusable **Layer Building Blocks** (*Sub Drop, Metal Strike, Wood Knock, Glass Ping, Gravel Debris, Plasma Crackle, Stereo Whoosh, Air Tail*) that append directly to the current sound, plus **Inspire** for instant creative mutations.
7. **Live Variation Rack (`Variations` tab):**
   - Configure **Count** (1–12) and **Spread** (`Subtle` ↔ `Wild`), inspect which layers are locked, audition each variation card (`▶`), reroll a single slot (`↻`), pin keepers (`★`), or promote a card to the main patch (`Use`).
8. **WYSIWYG & Sound Pack Export (`Export` tab):**
   - Exports the exact auditioned variations for the current channel — or batch-exports **all Forge channels in the project** as a **Sound Pack**.
   - Supports **WAV 16-bit**, **WAV 24-bit**, and **OGG** at **48 / 44.1 kHz**, **Stereo / Mono**, **As mixed / −1 dB peak / −14 LUFS** loudness targets, and custom filename templates (`{name}_{nn}`). Leading silence is trimmed on one-shots and preserved on seamless loops.

---

## Architecture

| Module | Responsibility |
| --- | --- |
| `src/forge/mod.rs` | Core data model (`ForgePatch`, `Layer`, `Source`, `ModalMaterial`, `GrainSample`, `Macros`, `ForgeSound`). Only the patch is saved in `.kfxl` projects (100% backward-compatible `#[serde(default)]`); audio re-renders on load. |
| `src/forge/curve.rs` | Curve evaluation, tension shaping (`automation::shape`), and `CurvePreset` builders. |
| `src/forge/modal.rs` | 8-mode physical material resonator DSP with coupled-form rotation and subtle stereo mode spread. |
| `src/forge/granular.rs` | Seeded granular/texture cloud engine (procedural micro-grains & sample clouds). |
| `src/forge/layer.rs` | Per-layer dry renderer (`Osc+`, `Modal`, `Grain`, `Noise+`, `Sample+`, `Synth`), TPT SVF filter, `pan_curve`, and `morph`. |
| `src/forge/render.rs` | Mix pipeline, per-layer & master FX chains, `DIRT`/`SPACE` macros, `Seamless Loop` tail wrap & equal-power splice, warmed-up −1 dB limiter, and ITU-R BS.1770 K-weighted `integrated_lufs`. |
| `src/forge/recipes.rs` | The 51 built-in `Recipe` entries, category levelling, and legacy style compatibility. |
| `src/forge/library.rs` | Rebuilt recipe catalogue using Modal, Grain, FM/Unison, Stereo Noise, Pan curves, and Seamless Loops. |
| `src/forge/mutate.rs` | Deterministic variations & mutations respecting `Layer::locked`, plus `variation_at` single-slot rerolls. |
| `src/forge/export.rs` | WYSIWYG `export_patches`, batch `export_sound_pack`, Lanczos resampling, peak/LUFS loudness targets, 16/24-bit WAV & OGG encoding. |
| `src/ui/forge/` | Modular Studio UI (`mod.rs`, `timeline.rs`, `inspector.rs`, `curves.rs`, `tabs.rs`). |

---

## Verification & Testing

- **Library tests:** `cargo test --lib forge`
- **UI & i18n tests:** `cargo test --bin keyframexl forge` and `cargo test --bin keyframexl i18n::scan_tests`
- **Live VST bounce capture:**
  ```sh
  KFXL_CAPTURE_FORGE=0 \
  KFXL_CAPTURE_PLUGIN=".../Grand Piano.vst3" \
  KFXL_CAPTURE_FORGE_BOUNCE=out.wav \
  KFXL_CAPTURE_FORGE_BOUNCE_AFTER=25000 \
  KFXL_CAPTURE_AFTER_MS=45000 \
  cargo run --release -- --capture-instrument shot.png
  ```

## Known Limitations

- **Wine plugin sample loading:** Some Windows VST plugins running under Wine take several seconds after opening to finish loading samples. If bounce reports silence, wait a moment and click **Bounce** again.
- **Channel bounce is a snapshot:** A *Note from a channel* layer stores bounced audio; click **Bounce** again after modifying the source channel.
