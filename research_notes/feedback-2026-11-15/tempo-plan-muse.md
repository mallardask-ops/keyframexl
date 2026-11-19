# Tempo automation + time-signature plan (researcher: Muse)

Scope: variable tempo over the song (FL tempo automation) and per-bar time signatures (FL markers).
No code changed; all line refs verified 2026-11-15.

## 0. Current state (why this is invasive)

- `Song.tempo: f64`, `Song.validate` range 40..300 (`src/sequence.rs:377,491`).
- Fixed `BAR = PPQ*4` ticks (`src/sequence.rs:17`); `steps()`, `seconds_per_step()` (`:27-34`).
- Engine timebase: `sps()` (`src/engine.rs:802`), `note_frame` (`:807`), `clip_frame` (`:814`),
  `song_tick` (`:821`), all closed-form on constant tempo; `collect_hits` (`:827`), `loop_frames`
  (`src/sequence.rs:726`), offline `render` (`src/engine.rs:1144`).
- Automation: `Target` has no tempo variant; `values_at(song, at_tick, …)` is read **per block**
  in song mode (`src/engine.rs:538-543`) — i.e. tick is *input* to automation.
- UI tick↔frame assumes constant tempo in 6+ places (table §3).

## 1. Data model

Keep `Song.tempo` as the tick-0 fallback (old songs = one segment, §6). Add two additive
`#[serde(default)]` fields so format stays v2 (`src/project_io.rs:41-44`):

```rust
// src/sequence.rs (new; serde defaults on both fields)
pub const TEMPO_MIN: f64 = 40.0; pub const TEMPO_MAX: f64 = 300.0; // same as validate :491

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct TempoPoint { pub tick: Tick, pub bpm: f64, pub shape: TempoShape }

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Default)]
pub enum TempoShape { #[default] Step, Linear }  // FL: jump vs slide

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct TempoMap { pub points: Vec<TempoPoint> }  // sorted by tick; empty = flat Song.tempo

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Meter { pub bar: u32, pub num: u8, pub den: u8 } // den ∈ {1,2,4,8,16}; bar 0 = 4/4 default
```

| Rule | Where to validate (`Song::validate`, `src/sequence.rs:487`) |
|---|---|
| `points` sorted, `tick >= 0`, first `tick == 0` else prepend `Song.tempo` | new |
| each `bpm` in 40..300, finite | mirrors `:491` |
| dedupe same-tick points (last wins, FL) at load, not error | new |
| `meters` sorted, unique `bar`, first `bar == 0` else insert 4/4 | new |
| `den` power-of-two ≤ 16, `num` 1..16 | new |
| bar tick length `num*PPQ*4/den` ≥ 1 step | new |
| pattern-local notes/slides unchanged (ticks are ticks) | existing `:518-540` |

**Why not `Target::Tempo` automation clips:** circular. `values_at` takes a *tick*
(`src/automation.rs:126`), but the tick itself comes from `song_tick(frame)`, which needs the
tempo map (`src/engine.rs:539`). Tempo must therefore be **song-level track data** (FL's tempo
track), editable in the automation *editor UI* if wanted, but stored on `Song`, never in a
looping pattern clip (clip loop/repeat semantics in `values_at` `:128-143` are wrong for tempo).

## 2. Tick↔frame mapping (tempo map)

One new module, e.g. `src/tempo.rs`: pure functions + a precomputed table.

- Seconds per tick at bpm `b`: `60/(b*PPQ)`. Frames per tick: `× rate`.
- **Step segment** `[t_i,t_{i+1})` @ `b_i`: `F(t) = F_i + (t−t_i)·rate·60/(b_i·PPQ)`.
- **Linear ramp** `b_i→b_{i+1}` over `D=t_{i+1}−t_i` ticks, slope `m=(b_{i+1}−b_i)/D`:
  `F(t) = F_i + rate·60/(PPQ·m)·ln(b(t)/b_i)`, `b(t)=b_i+m·(t−t_i)`.
- Inverse (needed: `song_tick`, `frame_tick`, seek): step is linear; ramp uses
  `t(F) = t_i + (b_i/m)·(exp(m·PPQ·(F−F_i)/(rate·60)) − 1)`.
- Guard `|m|→0` (fall back to step form) and clamp `D≥1`, `b` finite.
- Swing stays a **seconds offset added after** map lookup (`swing_offset`,
  `src/sequence.rs:716`; used `src/engine.rs:809,954,1122-1136`) — untouched.

**Table + caching (audio thread: no alloc, amortized O(1)/O(log n)):**

- Build (UI/load thread, on edit): `Vec<CachedSeg> { t0, b0, m, f0 }` with cumulative `f0`
  (f64 frames; round only at the final `as u64`, matching today's `.ceil()` in
  `note_frame`/`clip_frame` (`src/engine.rs:807-818`) — or keep f64 and ceil at use).
- `Engine` holds `tempo_cache: Vec<CachedSeg>` (cap 256) + `tempo_rev: u64`; `Song` carries a
  `#[serde(skip)] tempo_rev` bumped by every tempo/meters edit. Rebuild only on mismatch.
- Lookup: `partition_point` on `t0` (O(log n), no alloc), n is tiny. Per-block work today is
  already per-hit; per-frame `song_tick` is only needed by the metronome (§4) — also O(log n).
- Constant-tempo fast path: single segment ⇒ identical math to today (bit-for-bit, §7 test).

## 3. Call sites that must switch (grep-verified)

"Map" = frames_at(tick) / tick_at(frame); "Sig" = bar_start(bar)/bar_len(bar)/bars().

| File:function:line | Today | Becomes |
|---|---|---|
| `sequence.rs:loop_frames:726-733` | `bars*16*sps*rate` | song mode: `map.frames_at(song.end())`; pattern: `map.frames_at(plen)` |
| `sequence.rs:Song::end:690`, `bars:695` | `BAR` division | `bars()`: count via sig table; `end()` unchanged (ticks) |
| `engine.rs:sps:802-804` | `seconds_per_step(song.tempo)` | delete or keep for pattern-mode compat shim; all uses → map |
| `engine.rs:note_frame:807-810` | closed form | `map.frames_at(t)` + swing seconds (2 call sites: `:843`, slide `:954` via param) |
| `engine.rs:clip_frame:814-818` | bar-truncated form | `map.frames_at(t)` (call sites `:855,:857`); drop BAR-truncation for new content (old-song equivalence holds: single segment + 4/4 ⇒ same, minus the truncation quirk — test it, §7) |
| `engine.rs:song_tick:821-825` | inverse closed form | `map.tick_at(f)` (call site `:539`) |
| `engine.rs:process:545,915` | `Transport{tempo: song.tempo}` | `tempo = map.bpm_at_tick(map.tick_at(pos))` per block; `frame: pos` unchanged |
| `engine.rs:mix:664` | `song.tempo as f32` for FX | block-start tempo (audit `FxState::process(p,buf,rate,tempo)` `src/fx/mod.rs:301` — which kinds actually read tempo? pass-through if none do) |
| `engine.rs:render:1144-1160`, `render.rs:139-141,166` | `loop_frames` | free via `loop_frames` change; tail logic unchanged |
| `audio.rs:Metronome::run:68-69,79` | `beat=60/tempo*rate`, accent `%4` | §4 |
| `audio.rs:377` | `metro.run(…, snap.tempo, …)` | `metro.run(…, &map, &sig, at, wrap)` — needs map snapshot in audio callback: `rx.update()` gives `&Song`; build a stack/borrowed view or pass prebuilt `Arc<TempoTable>` alongside the song snapshot |
| `midi.rs:export_smf:230-238` | single Tempo + hardcoded `TimeSignature(4,2,24,8)` | §5 |
| `midi.rs:import_smf:529,678-682,739` | first tempo only, rest counted in `tempo_changes_flattened` | §5 |
| `ui/mod.rs:frames_per_step:868-870` | `sps(tempo)*rate` | **delete**: every caller converts via map (below). Pattern-mode-only shims keep step grids working |
| `ui/mod.rs:position_step:875-881` | `frames%len/fps` (assumes uniform steps) | `tick = map.tick_at(pos%len)` → display step = tick/`STEP`; bar from sig table |
| `ui/mod.rs:seek_tick:890-896` | `steps(t)*fps` | `map.frames_at(t)` |
| `ui/mod.rs:tap:948` | writes `song.tempo` | writes tick-0 point (`points[0].bpm`, or `song.tempo` if map empty) |
| `ui/deck.rs:lcd:284-285` | `step/16+1` bar, `%16` step | bar/step from sig table (bar index + tick-in-bar ÷ `STEP`) |
| `ui/deck.rs:305,360-362,456` | LCD tempo, steppers, field | display `bpm_at(now_tick)`; edits write/change the tempo point at/past playhead (FL: edits the tempo event), not `song.tempo` |
| `ui/playlist.rs:tick_at:53-54` | `x/bar_w*BAR` | bar-index→tick via sig table (bars stay uniform-width, FL-like): `tick = bar_start(floor(x/bar_w)) + frac*bar_len` |
| `ui/playlist.rs:352-363` | ruler numbers `b+1`, END at `bars()*bar_w` | `bars()` sig-aware; bar numbers same loop, widths uniform |
| `ui/playlist.rs:540-541,595` | clip x/w, note dots `/BAR*bar_w` | bar-index-based x: needs tick→(bar, frac-in-bar); keep uniform `bar_w` |
| `ui/playlist.rs:993-995,1002-1009` | playhead `steps/16*bar_w` | playhead from tick→(bar, frac) |
| `ui/roll.rs:392-394,986-987` | pattern playhead `pos%len/len*plen` | pattern mode: `map.tick_at(pos%len)` (map covers range; sigs irrelevant inside one pattern) |
| `ui/automation.rs:75-79` | `pos/fps*STEP` tick for plugin params | `map.tick_at(pos)` |
| `ui/automation.rs:126-154` | bar ± buttons, `tk%BAR`, `BAR/4` grid | sig-aware grid when bound to song tempo track; pattern clips keep `BAR`-as-4/4 (pattern-local, fine) |
| `ui/settings_ui.rs:frame_tick:224-226` | `frame/fps*STEP` | `map.tick_at(frame)` (call sites `:142,:149` MIDI record; test `:673-675` updated) |
| `ui/menu_tests.rs:253`, `settings_ui` tap tests | `tempo` sets | update to tick-0 point |
| `bridge/mod.rs:140-144`, `bridge/host.rs:115-123` | `tempo_bits`, `frame` header → `Transport{tempo,…}` | §4 |

Pattern-local `BAR` uses stay valid (ticks, 4/4 grid): `ui/roll.rs:329` length rounding,
`ui/rack.rs:84`, `legacy.rs:177,190`, `midi.rs:692-694` length rounding, `note_ops.rs`.

## 4. Plugins, bridge, metronome, ruler

- **Transport** (`src/plugin.rs:30-36`): per block, `tempo = map.bpm_at_tick(map.tick_at(pos))`.
  Consumers: `TestSynth` ignores tempo (`:92-118`); `plugins.rs:381` takes `t` (check what it reads —
  likely rate only); FX via `mix` get block tempo. In-ramp drift within one block (≤~0.5 ms of
  tempo change) is inaudible: document, don't interpolate.
- **Bridge header** (`src/bridge/mod.rs:140-144` → `tempo_bits`, `frame`; child rebuilds
  `Transport` in `src/bridge/host.rs:115-123` via `shm::tempo_of`): no format change needed v1.
  Follow-up (optional): add `tick_bits` so VST3 `ppqPosition` stays exact through ramps; today the
  child can only derive quarters from `frame/rate*tempo/60`, which drifts inside ramps.
- **Metronome** (`src/audio.rs:58-94`, called `:374-378`): needs (a) beat grid in ticks —
  beats are quarter-notes (`PPQ`) regardless of sig; (b) bar starts from sig table for the
  high click (replaces `k%4`, `:79`). Implementation: per block, `t0=tick_at(at)`,
  iterate beat ticks `b in (t0..t1)` via `ceil(t0/PPQ)*PPQ`, click frame = `frames_at(b)−at`;
  accent iff `b ∈ bar_starts`. `wrap` handling (`:72-75`) becomes: compute in the post-wrap
  frame space, same as today.
- **Ruler** (playlist `src/ui/playlist.rs:346-363`): keep uniform `bar_w` per bar (FL-like);
  bar `b` starts at `bar_start_tick(b)`. `tick_at`/`paint`/`seek` all go through
  bar-index ⇄ tick helpers. Sig changes get a marker glyph in the ruler (data: `Song.meters`).
- **LCD** (`src/ui/deck.rs:278-321`): bar = sig bar index+1, step = tick-in-bar/`STEP`+1,
  bpm = current map value. With odd meters (e.g. 3/4: 12 steps) the "step" field just counts to 12.

## 5. MIDI import/export

- **Export** (`src/midi.rs:207-238`): conductor track currently one `Tempo` + fixed 4/4
  (`:230-238`). Emit one `Tempo(mpqn)` per tempo point with `delta = point.tick`
  (ticks already PPQ-based ✓) and one `TimeSignature(num, den_exp, 24, 8)` per meter change
  (`den_exp = log2(den)`), sorted into the existing `finish()` delta-encoding (`:277-304`).
  Notes need no retiming (ticks are absolute).
- **Import** (`src/midi.rs:529,678-682,731,738-740`): today `tempos: Vec<(abs,us)>` keeps only the
  first (`work.tempo = t`, `:739`) and counts the rest as `tempo_changes_flattened` (`:731`).
  Instead: scale each tempo event tick via `scale_tick` (`:499`), clamp 40..300, dedupe,
  build `TempoMap` (shape Step v1 — or Linear if ≥2 events? choose Step, FL default is jump;
  note choice in report). Parse `TimeSignature` metas → `meters` (bar = tick_to_bar at that tick
  under previously parsed sigs; reject den not in {1,2,4,8,16}). Replace
  `tempo_changes_flattened` with `tempos: usize`/`meters: usize` in `ImportReport` (`:57-70`)
  and the status line (`:837+`).
- Round-trip test: export→import preserves point ticks (±1 from PPQ rescale) and sig list.

## 6. Backward compatibility

- New fields `#[serde(default)]`; empty map ⇒ `Song.tempo` everywhere (exact old behavior).
- `Song.tempo` remains the tick-0 value: keep it in sync on save (write `points[0].bpm` into
  `tempo`), so old builds/files still show the right starting tempo.
- `legacy.rs` v1 migration (`:177,190`) yields empty map + empty meters ⇒ unchanged timing.
- Keep `BAR` exported (tests, pattern grid); add `bar_len_ticks(num,den)` alongside.
- `project_io.rs` envelope stays v2 (`:5-6,41-44`): additive fields with defaults don't need v3.

## 7. Test list

| # | Test | Guards |
|---|---|---|
| 1 | `frames_at(tick_at(f)) == f` round-trip, incl. ramp + step mix, property over random maps | §2 inverse |
| 2 | single-segment map ≡ old formulas (`note_frame`, `clip_frame`, `loop_frames`) tick-for-tick | no old-song drift (§3 `clip_frame` truncation quirk: assert equality, change code if mismatch) |
| 3 | ramp song renders expected length: `frames_at(end)` vs `render()` len | engine swap |
| 4 | note lands right across a change: kick at tick past 120→140 step; energy-in-window like `sequence.rs:clip_tests` (`:888-900`) | `collect_hits` + `note_frame` |
| 5 | swing still offsets only odd-16th seconds under varying tempo | swing untouched |
| 6 | `song_tick` monotonic across ramp; `seek_tick`→`frame_tick` inverse (fixes `settings_ui.rs:673-675`) | UI map |
| 7 | MIDI export emits N Tempo + M TimeSig metas; import rebuilds equal map/meters | §5 |
| 8 | old JSON (no new fields) loads, validates, renders bit-identical | §6 |
| 9 | `validate` rejects: bpm 39.9/300.1/NaN, unsorted points, dup bar, `den=3`, `num=0` | §1 |
| 10 | metronome: 3/4 gives accent every 3 beats; click frames match `frames_at(beat_tick)` | §4 |
| 11 | playlist ruler: bar count/END with a 3/4 bar mixed in; playhead x continuous across it | §4 |

## 8. Ordered steps (S <1d, M 1-3d, L >3d)

| Order | Step | Size | Notes |
|---|---|---|---|
| 1 | `sequence.rs`: `TempoPoint/Shape/Map`, `Meter`, defaults, `validate`, `bar_start/bar_len/bars` + tests 8, 9 | S | pure data, no behavior change |
| 2 | `tempo.rs`: segment table, `frames_at/tick_at/bpm_at`, equivalence test 2 + round-trip test 1 | S | pure math; f64 internals; `.ceil()` policy decided here |
| 3 | Engine swap: `sps/note_frame/clip_frame/song_tick/loop_frames`, per-block `Transport.tempo`, FX block tempo; `Engine` cache + `tempo_rev`; tests 3, 4, 5 | M | touches `engine.rs:527-568,802-868,915`; keep `process_live` flat-tempo |
| 4 | Audio metronome rework + `snap` song snapshot carries map (`audio.rs:68-94,341-378`); test 10 | M | needs map access in realtime callback — design the snapshot first |
| 5 | UI position layer: kill `frames_per_step` as the universal converter (`ui/mod.rs:868`), fix seek/playhead/LCD/deck tempo display+edit, `frame_tick`, automation tick; test 6 | M | biggest file churn, mechanical |
| 6 | Playlist ruler sig-aware paint/seek + meter markers add/edit/delete; tempo-track editor (points list w/ step/slide toggle) in automation window; test 11 | M | user-facing; FL parity lives here |
| 7 | MIDI export/import tempo+sig; `ImportReport` change; test 7 | S | isolated in `midi.rs` |
| 8 | Bridge `tick` in header (VST3 ppq exactness); docs + status strings | S | optional follow-up; shm format bump |

Net call-site count: ~25 edits, all enumerated in §3. Riskiest: step 4 (realtime snapshot) and
the `.ceil()`/truncation policy in step 2 — decide by test 2 before touching the engine.
