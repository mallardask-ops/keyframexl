# Plugin hosting research for KeyFrameXL (Linux)

Date: 2026-11-15. Status: research only, no code changes.
Codebase grounding: `src/engine.rs` (Engine::process block rendering), `src/fx.rs`
(FxKind/FxSlot/FxState), `src/sequence.rs` (Instrument/InstrumentKind, MixerTrack,
FX_SLOTS=10, project format v2), `src/link.rs` (rtrb Song/Fx handoff, allocation
probe test), `Cargo.toml` (eframe/egui 0.31, cpal 0.15, rtrb 0.3).

## Executive summary

- Support **CLAP first, VST3 second, LV2 third**. Skip VST2 (licensing dead since
  Oct 2018; no new licences). CLAP is MIT, has the cleanest host API of the three,
  and the best Rust story (`clack`). VST3 reaches the most plugins but its Rust
  hosting story is younger; since VST 3.8 (Oct 2025) the SDK is **MIT-licensed**,
  so the old GPLv3/proprietary dual-licence blocker is gone. LV2 matters only for
  the long tail of Linux-native plugins (Calf, x42, Zyn/Yoshimi); its Rust hosting
  crates are thin wrappers over the C `lilv` library.
- Recommended stack: `clack-host` 0.2.0 + `clack-extensions` 0.2.0 for CLAP now;
  `vst3-host` 0.11.x (HelgeSverre, MIT) or `truce-rack` 1.1.5 for VST3 as stage 2;
  defer LV2 (`livi` 0.9.0 over system `lilv`) to stage 3, if ever.
- Editors on Linux are **X11 windows** in practice. Minimal working approach from
  an egui/winit app on Wayland: open a floating top-level X11 window (XWayland;
  force winit to the X11 backend or use raw x11rb/xcb), pass its window ID to the
  plugin, pump idle/timer callbacks on the UI thread. Do not try to embed into an
  egui panel in v1. VST 3.8 adds native Wayland interfaces
  (`IWaylandHost`/`IWaylandFrame`, `kPlatformTypeWaylandSurfaceID`), but almost no
  plugin implements them yet and current Rust host crates accept X11 only.
- Crash isolation: v1 = **in-process DSP + out-of-process scanner** with a scan
  cache. Full sandboxing (Bitwig-style per-vendor bridge processes) is a later
  stage; Carla's `carla-bridge-*` helpers are the reference design.
- Test plugins are all in Arch `extra` (same for CachyOS, which tracks Arch repos
  + AUR): instrument `surge-xt-clap`/`surge-xt-vst3`, `dexed-clap`/`dexed-vst3`;
  effect `zam-plugins-*`, `lsp-plugins-*`, `dragonfly-reverb-*`.

---

## 1. Formats worth supporting on Linux in 2026

### 1a. VST2: dead, do not implement

- Steinberg closed VST2 to new licensees in **October 2018**, removed it from the
  SDK, and has issued no licences since. Sources: sonore.dev plugin-formats guide
  (Oct 2026), Steinberg VST3 portal licence pages.
- Consequence for KeyFrameXL: no `InstrumentKind::Plugin` backend should speak
  VST2. Some Arch packages still ship VST2 builds as legacy (e.g.
  `cardinal-vst`, `dpf-plugins-vst`, `dragonfly-reverb-vst`), but these are
  leftovers, not a target.

### 1b. VST3: the reach format, licence blocker now gone

- Licence: **since VST 3.8 (20 Oct 2025, press release 29 Oct 2025) the SDK is MIT**,
  no agreement to sign, no fees; GPLv3/proprietary dual licence no longer offered.
  Current SDK is **3.8.1 (Aug 2026)**. Trademark/logo use is optional but must
  follow Steinberg's usage guidelines if used.
  (https://steinbergmedia.github.io/vst3_dev_portal/pages/VST+3+Licensing/VST3+License.html)
- Reach: ~13,370 VST3 plugins in KVR's database vs ~1,230 CLAP (Mar 2026 poll
  numbers quoted by sonore.dev). Nearly every commercial Linux plugin ships VST3.
- Cost: the hardest API of the three (COM-style C++ interfaces, separate
  processor/controller objects, complex lifetime rules). Rust support exists but is
  young (see §2).

### 1c. CLAP: the recommended first format

- Licence: **MIT headers** (free-audio/clap), current version **1.2.10 (Jul 2026)**.
  Plain-C ABI, single `.clap` shared object per plugin — much easier to `dlopen`
  than VST3 bundles.
- Host support: Bitwig 4.3+, REAPER 6.71+, FL Studio 24.1.1+ (2024), Studio One /
  Fender Studio Pro 7+, Qtractor, MultitrackStudio. Not in Cubase, Ableton Live,
  Logic, Pro Tools (irrelevant on Linux), Ardour, Renoise (as of Oct 2026).
- Why first for KeyFrameXL: (1) simplest correct host implementation; (2) best
  Rust crate (`clack`); (3) per-note modulation/expression and sample-accurate
  automation fit a step-sequencer's piano roll better than VST3's equivalents;
  (4) thread-pool and latency/tail/state extensions cover everything §4 needs.
- Plugin coverage on Linux is growing fast: Surge XT, Dexed (1.0.x), Odin2,
  OB-Xf, Cardinal, DPF collection, LSP 1.2.37, Zam, Dragonfly Reverb, u-he Linux
  betas, Airwindows Consolidated, Vital (via AUR) all ship CLAP.

### 1d. LV2: the Linux-native long tail, defer

- Licence: ISC/permissive headers. The only format Ardour fully embraces;
  REAPER loads LV2 since 6.24; Bitwig does **not** host LV2.
- Strengths: RDF metadata (plugin parameters are machine-readable — unique among
  the formats), mature extension set (worker, state, options, buf-size,
  latency), split DSP/UI binaries (good for embedded/headless).
- Weaknesses as a host target: standard evolves slowly, learning curve steep,
  commercial plugin coverage weakest, UI embedding requires the C `suil` library
  (cross-toolkit embedding), no pure-Rust UI story.
- LV2-only plugins worth noting: **x42-plugins** (Robin Gareus meters/EQ/tuner —
  LV2 + JACK only), **Calf** 0.90.9 (unmaintained, GTK2 UI problems),
  **ZynAddSubFX** (DSSI/LV2/VST2; `yoshimi-lv2` fork in extra). Everything else
  relevant ships CLAP or VST3 too.

### 1e. What popular Linux plugins actually ship (verified Nov 2026)

| Plugin | CLAP | VST3 | LV2 | Notes / source |
|---|---|---|---|---|
| Surge XT 1.3.4 | yes | yes | JUCE6-community build only, not in Arch pkgs | Arch: `surge-xt-clap`, `surge-xt-vst3`; downloads page lists Standalone/CLAP/LV2/VST3 |
| Dexed 1.0.1 | yes | yes | no (Arch) | Arch: `dexed-clap`, `dexed-vst3`, `dexed-standalone` |
| Odin2 2.4.1 | yes | yes | yes | Arch: `odin2-synthesizer-{clap,vst3,lv2,standalone}` |
| OB-Xf 1.0.3 (successor of OB-Xd) | yes | yes | yes | Arch: `ob-xf-{clap,vst3,lv2,standalone}` |
| Cardinal 26.02 | yes | yes | yes (+legacy VST2) | Arch: `cardinal-{clap,vst3,lv2,vst,standalone}` |
| DPF plugins 1.7 | yes | yes | yes (+VST2/LADSPA/DSSI) | Arch: `dpf-plugins-*` |
| LSP 1.2.37 | yes | yes | yes | Arch: `lsp-plugins-{clap,vst3,lv2,standalone}` |
| Zam 4.5 | yes | yes | yes | Arch: `zam-plugins-*` |
| Dragonfly Reverb 3.2.10 | yes | yes | yes | Arch: `dragonfly-reverb-*` |
| Vital | yes (AUR `vital-synth-clap-bin`) | yes (VST3) | yes (upstream builds; `vitalium` fork in some distros) | Official installer is a manual-download .deb wrapped by AUR `vital-synth`; CLAP-on-Linux has a known black-window bug report — test before relying on it |
| u-he (Diva/Hive/…) Linux betas | yes | yes | no | u-he product pages: "Linux (64-bit): CLAP, VST3"; still labelled beta |
| Airwindows Consolidated | yes (CLAP-native) | yes | yes | free, AU too; single plugin hosting 500+ algorithms |
| x42-plugins 20260908 | no | no | **yes only** (+JACK apps) | Arch: `x42-plugins-lv2` |
| Calf 0.90.9 | no | no | **yes only** | Arch: `calf`; unmaintained |
| ZynAddSubFX 3.0.6 / Yoshimi 2.3.6.5 | no | no (VST2 legacy) | yes (+DSSI) | Arch: `zynaddsubfx`, `yoshimi-lv2` |
| sfizz 1.2.3 (SFZ sampler) | no | yes | yes | Arch: `sfizz-vst3`, `sfizz-lv2` |
| mda.lv2 1.2.12 | no | no | yes | Arch: `mda.lv2` — useful as a tiny LV2 test plugin |

Arch group pages consulted (2026-11 state): `clap-plugins` (25 pkgs),
`vst3-plugins` (29 pkgs), `lv2-plugins` (73 pkgs), `pro-audio` (~296 pkgs).
CachyOS tracks Arch `extra` plus the AUR, so the same names apply; CachyOS adds
its own kernels/schedulers but no separate pro-audio repo to care about.

### 1f. Recommendation (order)

1. **CLAP** — implement first (simplest, MIT, best Rust crates, covers all test
   instruments/effects above).
2. **VST3** — implement second (reach; MIT since 3.8 removes the licence risk).
3. **LV2** — only if users demand x42/Calf/Zyn; budget for C dependencies
   (`lilv`, `suil`) and RDF/Turtle parsing.
4. Never VST2.

---

## 2. Rust hosting crates

Versions checked against crates.io API on 2026-11-15. Key: "host still implements"
= what the crate does *not* do for you.

### 2a. CLAP

- **`clack-host` 0.2.0 (2026-09-12) + `clack-extensions` 0.2.0 + `clap-sys` 0.5.0**,
  repo `prokopyl/clack` (244 stars). Licence **MIT OR Apache-2.0**. MSRV 1.85.
  The crate to build on.
  - Maturity: author declares "feature-complete but still under active development
    and refinement; breaking changes possible". 0.x versioning. ~2,900 lines of
    host wrappers + ~9,500 lines of extension wrappers. No higher-level CLAP-host
    alternative exists in Rust (author's own assessment).
  - What the host still implements itself: `HostHandlers` (shared/main-thread/audio
    impls: `request_restart/process/callback`), the event loop, audio-port setup,
    every extension you use (params, state, gui, timer, posix-fd, latency, tail,
    thread-check, thread-pool, note-ports, voice-info…), scanning/caching, and the
    audio↔main thread parameter bridges (see §4).
  - Example host: **`host/examples/cpal`** in the same repo — a CPAL-based host.
    Directly relevant: same audio backend as KeyFrameXL (cpal 0.15 at 48 kHz).
    Start by reading that example, not the raw docs.
  - GUI extension included in `clack-extensions` (`gui` feature): `GuiExtension`
    with `guiApiType` `"x11"` on Linux.
  - Related: `clack-finder` (in-repo finder helper). For plugin *creation*
    (not needed here) there is `nih-plug` (VST3+CLAP, higher level) — mentioned
    only to avoid confusion: it does not host.

### 2b. VST3

- **`vst3-host` 0.11.1 (2026-11-08, HelgeSverre/rust-vst3-host), MIT.**
  The pragmatic choice. Self-description: "Safe, simple VST3 plugin hosting with
  audio playback, MIDI, and crash protection. Zero unsafe in the public API."
  ~25k lines, very active (0.1.0 → 0.11.1 between Jun–Nov 2026).
  - Features map to our needs: `process-isolation` (default on), `cpal-backend`
    (default on), `egui-widgets` (optional), `midi-input`. Ships
    `vst3-host-helper` and `vst3-host-probe` binaries (probe = out-of-process
    scanner primitive, §3).
  - GUI: `vst3_host::embed` module — **implemented on macOS/Windows/Linux-X11; a
    Wayland `RawWindowHandle` is explicitly rejected** ("VST 3.8 requires the host
    to provide a compositor connection…"). So X11-via-XWayland is the supported
    Linux path today.
  - Caveat: young and single-maintainer; audit the `unsafe` internals and pin the
    version. Docs note real-world plugin quirks (e.g. a licensed Waves plugin
    aborting with "Rust cannot catch foreign exceptions").
- **`vst3` 0.3.0 (2025-12-07, coupler-rs / micahrj), MIT OR Apache-2.0.**
  Raw, complete-ish Rust port of the VST3 API (~15.6k lines, no abstractions).
  Use only if `vst3-host` proves insufficient — you would be writing the host
  layer yourself on top of it.
- **`vst3-sys` (RustAudio, GitHub only, not on crates.io), GPL-3.0, last touched
  Jun 2023.** COM-vtable port spawned from archived `m-hilgendorf/cli-host`.
  Avoid: GPL-3.0 licence + stale.
- **`truce-rack` 1.1.5 (2026-06-19, truce-audio, MIT OR Apache-2.0)** — "Cross-format
  audio plugin host (CLAP + VST3 by default; AU/AU3/LV2 behind features)".
  Facade over `truce-rack-clap` / `truce-rack-vst3` (the VST3 side builds on the
  community `vst3` crate). ~550–700 downloads/version. Worth evaluating as an
  alternative to combining `clack-host` + `vst3-host` yourself: one engine-facing
  abstraction for both formats. Younger and less proven than either half alone —
  spike both and compare before committing.
- **`plugin_host` 0.1.0 (2026-02-25), MIT** — "VST3 and CLAP plugin host for DAW —
  scanner, sandboxing, parameter automation, preset management", ~1.1k lines, one
  release, no repo link, 237 downloads. Immature; watch, don't adopt.
- Old licence note (no longer blocking): pre-3.8 VST3 SDK was GPLv3-or-proprietary
  dual licence, which is why older Rust VST3 crates are GPL (`vst3-sys`) or
  carefully avoid shipping SDK code. Under the 3.8 MIT licence this concern is
  gone for new work — but verify any crate you pick doesn't *bundle* old
  GPL-marked SDK headers.

### 2c. LV2

- **`livi` 0.9.0 (2026-10-25, wmedrano/livi-rs), MIT.** "A library for hosting LV2
  plugins." Thin-ish safe wrapper over the system C **`lilv`** library (LV2 host
  discovery/instantiation library) — so the build needs `liblilv` + LV2 headers
  on the system, and UI embedding additionally needs C **`suil`**. Example
  `livi-jack` runs an LV2 URI (`http://drobilla.net/plugins/mda/EPiano`).
  Small API surface (~3.4k lines); LV2 feature coverage (worker, state, options,
  port groups) is whatever Lilv exposes — expect to drop to `lilv-sys`/raw LV2
  for advanced needs.
- **`lilv-rs` (wmedrano)** — even thinner Rust wrapper around Lilv; author points
  host builders at `livi` instead.
- **`lv2` crate (rust-lv2, marcan/RustAudio)** — plugin-side only ("hosting
  plugins is not supported"). Not usable for hosting.
- No pure-Rust LV2 host stack exists (RDF/Turtle parsing, `serd`/`sord`
  equivalents, and `suil` embedding are all C). Budget C-dependency pain if LV2
  is ever scheduled.

### 2d. Recommendation

- Stage 1 (CLAP): depend on `clack-host` + `clack-extensions` (+ `clack-finder`
  if useful). Read `host/examples/cpal` first.
- Stage 2 (VST3): spike `vst3-host` vs `truce-rack` on 3–4 real plugins
  (Surge XT VST3, Dexed VST3, Zam/LSP VST3) before choosing. Default to
  `vst3-host` unless the unified abstraction of `truce-rack` survives contact
  with those plugins.
- Stage 3 (LV2, optional): `livi` + system `lilv`; accept C deps.

---

## 3. Plugin scanning

### 3a. Standard search paths on Linux

**CLAP** (normative, from `free-audio/clap` `include/clap/entry.h`, v1.2.x):

- `~/.clap`, `/usr/lib/clap`, plus **`CLAP_PATH`** env var (same syntax as `PATH`,
  `:`-separated). Recursively scan each dir for `*.clap` (DSO on Linux, bundle
  on macOS). `init`/`deinit` are cheap-scan safe: no GUI, no user interaction,
  callable on any thread (1.2.x requires plugins to tolerate repeated init calls).

**VST3** (normative, Steinberg "Plug-in Locations" doc; priority order,
first-found Processor UID wins):

1. `$HOME/.vst3/` (user)
2. `/usr/lib64/vst3/` (Fedora convention)
3. `/usr/lib/vst3/`
4. `/usr/local/lib64/vst3/`
5. `/usr/local/lib/vst3/`
6. `$APPFOLDER/vst3/` (application-local)

A VST3 "plugin" is a **bundle directory** `Name.vst3/` containing
`Contents/x86_64-linux/*.so` (+ `Resources/`), *not* a bare `.so` (Ardour forum
threads confirm hosts must walk bundles). There is **no `VST3_PATH` in the spec**
— Steinberg SDK issue #67 explicitly notes the missing concept (requests it for
Linux, citing legacy `VST_PATH`). In practice: REAPER/Carla/Bitwig each add their
own user-configurable extra paths; KeyFrameXL should too (a `plugin_paths` list
in settings + the six defaults).

**LV2** (normative, lv2plug.in Filesystem Hierarchy Standard):

- Bundles `name.lv2/` (directories with `manifest.ttl`) in `$HOME/.lv2`,
  `$PREFIX/lib/lv2` (`/usr[/local]/lib/lv2`), searched via **`LV2_PATH`**
  (`:`-delimited; Unix default
  `$HOME/.lv2:/usr/local/lib/lv2:/usr/lib/lv2`). Save files must reference
  plugins by **URI, never by filesystem path** (bundle names are not stable
  identifiers).

### 3b. Scanning out-of-process + cache

- Rationale: `dlopen`ing dozens of unknown DSOs in the UI process risks hangs
  (network-licensed plugins), crashes (see Vital LV2 freeze reports), and
  versioned-symbol conflicts. Every serious host scans in a child process:
  Bitwig's `BitwigPluginHost` processes, Carla's `carla-discovery` +
  `carla-bridge-*` helpers, `vst3-host`'s `vst3-host-probe` binary.
- Design for KeyFrameXL: a `keyframexl-scan` helper binary (or reuse
  `vst3-host-probe` for VST3) that takes a path list, loads each candidate,
  prints one JSON line per plugin (format, path/bundle, id/URI, name, vendor,
  version, audio/port layout, param count, has editor, latency/tail if cheap),
  and exits with a status. The UI process spawns it per directory (or per file
  with a timeout, e.g. 10 s), never `dlopen`s plugins itself during scan.
- Cache: persist results (path → mtime/size → descriptor + failures with
  negative caching) in the user cache dir (`dirs::cache_dir()/keyframexl/` —
  `dirs 6` is already a dependency). Rescan on startup only for changed files;
  full rescan on demand from the browser. LV2 additionally needs a Turtle-parse
  step (via Lilv in the helper), and LV2 save files use URIs (§4 state).
- Freshness key must include the *file* identity (mtime+size), not just the path,
  since package upgrades replace files in place.

---

## 4. Threading / realtime

KeyFrameXL constraints (from the code read): the cpal audio callback must not
allocate (`src/link.rs` `alloc_probe` test asserts zero allocs/frees across 100
blocks; `Engine::process` pre-grows buses in `reserve`/`grow_buses`,
`src/engine.rs:477`); UI→audio data travels as `Arc<Song>` + prebuilt
`FxState` over `rtrb` rings (`SongSender`/`FxSender`, `src/link.rs:1-176).

### 4a. The rules per format (audio thread vs main thread)

**CLAP** (threading is explicit via `clap.thread-check` + docs):

- Audio thread: `process()` only. Must be lock-free, no allocation, no syscalls
  that can block. Parameter changes arrive as **sample-accurate events** in the
  input event list (param-value events carry sample offsets); plugin outputs
  parameter changes the same way.
- Main thread: instantiation, `activate/deactivate`, `start/stop_processing`,
  GUI creation, state save/load, param flush (`params->flush`, callable from
  audio thread only during process for output events).
- Optional but expected extensions: `thread-pool` (host offers worker threads —
  nice fit: offer the engine's voice-render threads later), `timer-support`
  + `posix-fd-support` (main-thread callbacks; drive them from the UI tick),
  `track-info` (transport/tempo), `latency` (get ms → delay compensation),
  `tail` (ring-out time for export), `state`/`state-context` (project blobs),
  `note-ports`/`voice-info` (polyphonic expression), `render` (offline mode hint),
  `audio-ports-config` (mono/stereo/sidechain negotiation).
- Transport/tempo: CLAP transport events (`clap.event.transport`) delivered per
  process call — song position in beats, tempo, time signature, loop points,
  playing/recording flags. KeyFrameXL already computes loop position per block
  (`Engine::process`, `src/engine.rs:425`); convert to beats via
  `steps()/PPQ` and feed the event list each block.

**VST3**:

- Audio (process) thread: `IAudioProcessor::process` with `ProcessData`
  (input/output buffers, `IParameterChanges` in/out — sample-offset-accurate,
  `IEventList` for note on/off, `ProcessContext` for tempo/time-sig/position).
  Same no-alloc/no-lock rules.
- UI/main thread: `IEditController` (parameter get/set/text, normalized 0..1 —
  needs the plugin's `ParamSpec`-like title/unit mapping for display),
  `setComponentState`/`getState` (`IBStream` blobs for project save),
  `setBusArrangements`, `setupProcessing`, `setProcessing(true/false)`,
  `restartComponent` callbacks (must be honoured: latency or layout changes).
- Latency: `getLatencySamples()` → KeyFrameXL-side delay compensation (see 4c).
- Note expression and per-note controllers exist (`INoteExpressionController`)
  but map poorly onto the piano-roll's per-note fields (pan/release/fine/modX/Y);
  v1 should map Note → note-on/off + velocity, and expose plugin params through
  automation lanes later.

**LV2**:

- Audio thread: `LV2_Descriptor::run()`; control via input control ports
  (floats, block-rate, not sample-accurate) + **atom ports** (MIDI + `patch:Set`
  messages). Non-RT work via **worker** extension (`schedule_work` → background
  thread → `work_response` on audio thread — preallocated responses only).
- Main thread: instantiation, `activate/deactivate`, `state` extension
  save/restore, `options`/`buf-size` negotiation (`boundedBlockLength`,
  `maxBlockLength` — must be given cpal's real block size), `port-groups`,
  UI via `suil` (separate process or same-process instance access).
- Latency: dedicated `lv2:latency` control output port.

### 4b. Parameter changes

- All three formats separate the *gesture* (UI thread, human units) from the
  *event* (audio thread, sample offset). KeyFrameXL's current model
  (`FxSlot.params: Vec<f32>` + `validate()`, `src/fx.rs:192-226`) already looks
  like normalized plugin params — extend it, don't replace it: a
  `PluginParams { values: Vec<f64>, automation: … }` with the same validation
  discipline (finite, in-range), plus an rtrb event queue UI→audio carrying
  `(param_id, sample_offset, value)` per block, merged into the CLAP input
  events / VST3 `IParameterChanges` each `process` call.
- Automation recording/playback can reuse the existing per-block parameter
  smoothing pattern (cf. fader ramp in `Engine::mix`, `src/engine.rs:574-585`).

### 4c. Latency reporting + plugin delay compensation (PDC)

- Query per instance: CLAP `latency.get()` (samples), VST3 `getLatencySamples()`,
  LV2 `lv2:latency` port. Re-query after `restartComponent`/latency-changed.
- KeyFrameXL has no PDC today (all built-ins are zero-latency; mix is a single
  forward pass inserts→Master). Minimum viable: per-track integer delay lines
  (reuse the `Delay`-style buffer pattern, `src/fx.rs:462`) sized to the max
  latency on the *other* tracks feeding the same summing point, so parallel
  inserts stay phase-aligned; report total chain latency in the mixer UI.
  Full PDC (delaying the transport grid, lookahead) is out of scope for v1.

### 4d. Sample-accurate note events

- KeyFrameXL's engine already collects hits per block with frame offsets
  (`Hit.at`, `src/engine.rs:312-317, 458-473`). For plugin instruments, translate
  each hit into a timestamped note-on (CLAP `NOTE_ON` event / VST3
  `NoteOnEvent` in `IEventList`) at `hit.at - block_start`, and schedule
  note-offs from `note.end()` (gate length; feeds CLAP `NOTE_OFF` / VST3
  `NoteOffEvent`). The piano roll's per-note fields map as: velocity→velocity,
  fine→pitch-bend or CLAP note expression, pan/modX/modY→(stage 2) per-note
  modulation (CLAP's strong suit; VST3 note expression stage 2).
- Voice management: plugin instruments bypass the internal `Voice` pool —
  `render_voices` (`src/engine.rs:815`) gains a branch that routes a channel's
  hits to the plugin instance instead of spawning `Voice`s. Polyphony/mono/
  legato semantics then belong to the plugin.

### 4e. State save/restore (project file)

- Project format is serde JSON v2 (`Song`, `src/sequence.rs:327`). Plugin state
  must be an opaque blob: CLAP `state.save/load` (binary stream), VST3
  `getState/setState` (`IBStream`), LV2 `state` extension (a *bundle* of
  files — store as base64 map in JSON or sidecar files next to the project).
- Data model: `PluginState { blob: Vec<u8> /* or base64 String for JSON */, … }`
  with a schema version + plugin id + format tag so stale blobs fail soft
  (plugin missing → channel renders silence + keeps the blob, never deletes it).
- Size discipline: blobs can be megabytes (sample-based plugins); cap inline-JSON
  size, spill to sidecar (`.kfx-data/`) above threshold. Validate like everything
  else (`Song::validate`, `src/sequence.rs:440`).

### 4f. Block sizes from cpal

- cpal block size is host/buffer-size dependent and can change between runs (and
  theoretically between callbacks on some backends). All three formats handle
  arbitrary block sizes *if* told: CLAP `process` gets `frames_count` per call
  (no fixed size; respect min/max from `audio-ports-config`); VST3
  `setupProcessing` + `processSetup.maxSamplesPerBlock` / `process` `numSamples`
  (call `setupProcessing` again when the device block size changes);
  LV2 `options` `maxBlockLength` + `boundedBlockLength` (re-instantiate or
  re-negotiate when it changes; never assume 512).
- KeyFrameXL's `reserve(tracks, frames)` / `grow_buses` pattern
  (`src/engine.rs:377-380, 477-494`) extends naturally: grow plugin scratch
  buffers the same way, never shrink on the audio thread. The existing rule
  "build states on the UI thread, `adopt_fx` into the engine"
  (`src/link.rs:103-108`, `src/engine.rs:382-392`) is exactly how plugin
  instances (which allocate on creation) must enter the audio graph.

---

## 5. GUIs (plugin editors on Linux)

### 5a. The ground truth in 2026

- **CLAP GUI**: plugins expose the `clap.gui` extension with API types; on Linux
  this is `"x11"`. The host creates a parent X window and the plugin embeds into
  it (via `gui.create` with an X11 window handle / `set_parent`). Some plugins
  also offer a `"wayland"` stub, but coverage is negligible — target X11.
- **VST3 views**: `IPlugView` attached with platform type
  `kPlatformTypeX11EmbedWindowID` (parent X window ID as `void*`). VST **3.8 adds
  native Wayland**: `IWaylandHost`/`IWaylandFrame` +
  `kPlatformTypeWaylandSurfaceID` (host acts as both Wayland client and
  compositor). As of 2026 this is implemented by essentially no third-party
  plugins; treat as future-proofing, not a v1 path.
- **LV2 UIs**: separate binaries in the bundle; hosts embed cross-toolkit via
  **`suil`** (Ardour's approach: any toolkit UI inside a GTK host window).
  Requires linking C `suil`; no Rust equivalent.

### 5b. What the reference hosts do

- **Bitwig** (all formats): floating top-level windows for plugin editors; crash
  protection via separate `BitwigPluginHost` processes per sandbox mode
  (together / by manufacturer / individually). Never embeds into thearranger.
- **REAPER** (Linux): floating editor windows (X11 via XWayland under Wayland);
  also runs bridged Windows plugins out-of-process (built-in `lin-bridge`
  lineage + optional yabridge). Editor embedding is not attempted.
- **Ardour** (LV2+VST3): embeds LV2 UIs with `suil` into the editor window;
  VST3 editors open as separate X11 windows. `Help > About > Config` even reports
  "LV2 UI embedding: True/False" depending on suil availability — embedding is a
  build-time capability, not a given.
- **Carla**: plugin UIs run in `carla-bridge-ui` helper processes (per
  format/arch/toolkit), shown as standalone windows; the main window only hosts
  generic sliders. This is the design to copy for sandboxing (§6).

### 5c. Minimal working approach for KeyFrameXL (egui/winit on Wayland)

KeyFrameXL is eframe/egui 0.31 with `wayland`+`x11` features (`Cargo.toml:10`).
Recommended v1:

1. **Floating top-level X11 window per open editor** (REAPER/Bitwig behaviour),
   not embedding. Force that window onto X11: either create it with `winit`
   using the X11 backend explicitly (`x11` feature is already enabled), or — more
   robustly — create it with raw **`x11rb`**/`xcb` so there is zero doubt about
   which display server owns it under a Wayland session (it will go through
   XWayland automatically).
2. Pass the X window ID to the plugin (CLAP `gui.set_parent` with the X11 window;
   VST3 `IPlugView::attached((void*)xid, kPlatformTypeX11EmbedWindowID)`).
3. **Pump callbacks on the UI thread**: register the plugin's fds with the egui
   update loop — VST3 `IRunLoop` (`registerTimer`/`registerEventHandler`) on
   Linux; CLAP `timer-support` (`register_timer`, ~30 Hz) and
   `posix-fd-support` (`register_fd` + poll in the UI tick). eframe's
   `request_repaint` / a small poll in the app update is sufficient; do not run a
   second event loop thread if avoidable.
4. **Resize**: CLAP `gui.can_resize` + `get_size`/`set_size` (+
   `suggest_size`); VST3 `IPlugView::canResize`/`onSize` + host-side
   `IPlugFrame::resizeView` handshake. Implement host→plugin size push and
   plugin→host resize request (clamp to sane min/max; re-set the X window size).
5. HiDPI: read the XWayland scale via winit's scale factor for the editor window
   and pass `gui.set_scale` (CLAP) / `IPlugView::setScaleFactor` if available;
   document that XWayland editors may render at 1x and look soft on HiDPI —
   accepted limitation in v1 (same as REAPER-on-Wayland today).
6. Fallback that must always exist: **generic parameter sliders** in an egui
   window (CLAP `params` info: name, min/max, default, units; VST3
   `IEditController` + `ParameterInfo`). Editors crash or fail far more often
   than DSP — the generic UI keeps the plugin usable when its editor won't open.

Wayland-native path (VST 3.8 `IWaylandHost`, possible future CLAP `wayland`
api): revisit when plugins implement it; the floating-window architecture
doesn't change, only the handle type.

---

## 6. Crash isolation

### 6a. Reference designs

- **Bitwig sandboxing** (documented in "Plug-in Hosting & Crash Protection in
  Bitwig Studio"): per-plugin choice of *Together* (one shared process),
  *By manufacturer*, or *Individually* (own process each). A crashed plugin shows
  as a reloadable placeholder; audio engine and UI never go down. This is the UX
  bar.
- **Carla bridges** (`falkTX/Carla`, `source/bridges-plugin` + `bridges-ui`):
  small per-format/per-arch helper binaries (`carla-bridge-lv2-*`,
  `carla-bridge-vst3-*`, …) that host the plugin and talk audio/MIDI/params over
  shared memory + a control socket; UIs run in `carla-bridge-ui-*`. Proves the
  SHM-transport design works for all three formats on Linux with modest latency
  cost. Also how Carla runs 32-bit and Windows plugins.
- **yabridge** (robbert-vdh): out-of-process Wine hosts for *Windows* VST2/VST3/
  CLAP on Linux, with user-defined *plugin groups* sharing one process. Relevant
  as (a) proof that process-per-group scales, and (b) the escape hatch for
  Windows-only plugins — KeyFrameXL should not build this, but should not
  preclude users from loading yabridge-hosted plugins either (they look like
  normal Linux VST3s once set up).

### 6b. Recommendation

- **v1 (ship with CLAP support): in-process DSP + out-of-process scanner.**
  Rationale: keeps the audio path identical to today's zero-alloc `Engine`
  (no SHM transport to design, no latency accounting yet); the scanner helper
  (§3b) already establishes the child-process pattern and eliminates the most
  common crash vector (scan-time `dlopen` of 100+ unknown binaries). A crashing
  *instrument* still takes down the engine — accepted and documented risk for v1,
  mitigated by autosave + the generic-UI fallback.
- **v2 (with VST3): per-vendor bridge processes for DSP**, Carla-style:
  `keyframexl-bridge-{clap,vst3}` helpers, SHM double-buffered stereo audio +
  event queues reusing the existing `rtrb` discipline, UI-side supervisor that
  restarts a dead bridge and re-instantiates its plugins (state blobs from §4e
  make this lossless). Offer Bitwig-like granularity later (together/by vendor/
  individual); start with one bridge per plugin instance (simplest correct).
- What v1 must not paint over: engine APIs that assume `&mut self` plugin DSP
  inline (keep a trait boundary: `PluginVoice : process_block(&mut, …)` with an
  in-process impl now, SHM impl later); project blobs must already be
  bridge-agnostic (§4e).

---

## 7. Free test plugins on Arch/CachyOS (extra repo + AUR)

All installable with `pacman -S` (extra) or an AUR helper (CachyOS users: same
names; `paru`/`yay` from CachyOS repos). Versions as of Nov 2026.

**Instruments (channels)** — each ships both CLAP and VST3:

| Use | Package (CLAP) | Package (VST3) | Version |
|---|---|---|---|
| Workhorse subtractive/wavetable (default instrument test) | `surge-xt-clap` | `surge-xt-vst3` | 1.3.4-7 |
| DX7 FM (params + sysex state blobs) | `dexed-clap` | `dexed-vst3` | 1.0.1-2 |
| Analog-style poly (editor-resize test) | `odin2-synthesizer-clap` | `odin2-synthesizer-vst3` | 2.4.1-2 |
| Oberheim-style (OB-Xf, successor of OB-Xd) | `ob-xf-clap` | `ob-xf-vst3` | 1.0.3-1 |
| Modular (extreme port-count/param-count case) | `cardinal-clap` | `cardinal-vst3` | 26.02-1 |
| SFZ sampler (state-blob + file-reference case) | — (use VST3) | `sfizz-vst3` | 1.2.3-19 |
| Wavetable flagship (manual QA only) | AUR `vital-synth-clap-bin` | AUR `vital-synth` (wraps upstream .deb; manual download required) | 1.5.5 |

**Effects (mixer slots)** — each ships CLAP + VST3 (+LV2 where noted):

| Use | Packages | Version |
|---|---|---|
| Bread-and-butter FX assortment (default effect tests) | `zam-plugins-clap`, `zam-plugins-vst3` (+`-lv2`) | 4.5-1 |
| Full mixing suite (latency-reporting plugins included) | `lsp-plugins-clap`, `lsp-plugins-vst3` (+`-lv2`) | 1.2.37-1 |
| Reverb incl. standalone for A/B | `dragonfly-reverb-clap`, `dragonfly-reverb-vst3` (+`-lv2`) | 3.2.10-5 |
| DPF collection (all-formats CI matrix incl. LV2 later) | `dpf-plugins-clap`, `dpf-plugins-vst3`, `dpf-plugins-lv2` | 1.7-3 |
| 300+ algorithms in one binary (param-count stress) | Airwindows Consolidated (direct download, CLAP/VST3/LV2) | rolling |
| u-he quality bar (manual QA; Linux still "beta") | direct download from u-he (CLAP+VST3 64-bit) | rolling |

**LV2-only spares** (deferred to stage 3, but cheap to install now):
`x42-plugins-lv2` (20260908-1), `calf` (0.90.9-2), `yoshimi-lv2` (2.3.6.5-1),
`mda.lv2` (1.2.12-1, tiny — good first LV2 instantiation test).

**Suggested automated-test set (CI, headless, no GUI):**
instrument `surge-xt-clap` + `dexed-vst3` (or `-clap`); effect `zam-plugins`
(CLAP + VST3) and `dragonfly-reverb`. Assert: scan finds them, instantiate at
48 kHz stereo, process 512-frame blocks with a note-on/off pair, params
set/get round-trip, state save→new instance→load produces identical params,
latency/tail query doesn't crash. Gate VST3 tests on the stage-2 crate choice.

---

## 8. Staged implementation plan for KeyFrameXL

### Data model (touches `src/sequence.rs`, `src/fx.rs`)

- `InstrumentKind::Plugin { format: PluginFormat /* Clap(+Vst3 later) */, id: String /* CLAP id / VST3 UID / LV2 URI */, name: String, state: PluginState, params: Vec<PluginParam> }`.
  Keep `Drum`/`Sampler` untouched so old projects (format v2) load after a
  version bump to v3 with graceful fallback (unknown-plugin → muted channel that
  keeps its blob, §4e).
- `FxSlot`: add `FxKind::Plugin { … }`-style variant or a parallel
  `PluginSlot { … }` (cleaner: `enum Slot { Builtin(FxSlot), Plugin(PluginSlot) }`
  to avoid breaking the 10 `FxKind::ALL` UI paths). Slot keeps `mix` + `enabled`
  semantics; plugin replaces the DSP, not the slot machinery.
- `PluginState { format, id, blob: Base64String|sidecar, schema: u32 }` + per-param
  cache for the generic UI. All validated in `Song::validate` (size caps, id
  charset, param ranges).

### Engine (touches `src/engine.rs`, `src/link.rs`)

- New `plugin.rs` module: `PluginEngine` trait (`instantiate`, `process_block`,
  `set_param`, `save_state`, `latency`, `tail_samples`), in-process CLAP impl.
  Engine holds instances in a side table keyed by (channel-id | track,slot),
  created/destroyed on the UI thread, handed over via the existing
  `adopt_fx`-style rtrb pattern (extend `FxMsg`/`FxSender::sync` — the "build on
  UI thread, install before block" discipline in `src/link.rs:111-176` is exactly
  right and must be preserved to keep the `alloc_probe` test green).
- `collect_hits`/`render_voices`: plugin channels consume hits as timestamped
  note events (§4d) instead of spawning `Voice`s; plugin FX slots run in `mix()`
  where `FxState::process` runs today, with PDC delay lines (§4c) around them.
- Transport: build the CLAP transport event / VST3 `ProcessContext` per block
  from the existing `pos`/`looping` bookkeeping in `Engine::process`
  (`src/engine.rs:425-454`).

### Scanner + browser

- `keyframexl-scan` helper binary + cache in `dirs::cache_dir()` (§3b); settings
  for extra paths; "Plugins" browser section grouped by format, with
  effect/instrument badges from the scan descriptors; drag-to-channel (instrument)
  / drag-to-slot (effect); missing-plugin placeholder on load.

### Editor windows

- Floating X11 windows per §5c + generic param UI fallback. No egui embedding in
  v1.

### Stages, risks, estimates

Assumes one experienced Rust/audio dev, part-time-friendly slices. Estimates are
wall-clock for design+implement+test, excluding open-ended plugin-compat QA.

| Stage | Scope | Risks | Estimate |
|---|---|---|---|
| 0. Harness | `PluginEngine` trait + null/mock plugin; `adopt`-style handoff for plugin instances; `alloc_probe`-style test extended to plugin blocks; scan-cache schema | Trait too narrow (VST3 restart/latency callbacks don't fit) — mitigate by designing against both CLAP *and* VST3 needs up front | 1–2 weeks |
| 1. CLAP instruments | clack-host integration; note-on/off from hits; params via rtrb events; transport/tempo; state blobs; generic UI; in-process | clack 0.x API churn (pin versions); thread-check violations (run with CLAP validator); per-note expression mapping | 4–6 weeks |
| 2. CLAP effects | Slot variant; PDC delay lines; latency/tail handling; offline-export tail (MAX_TAIL_SECONDS interplay) | PDC correctness on sends/parallel inserts; tail cutoffs in export | 2–3 weeks |
| 3. Scanner + browser | helper binary, cache, Plugins section, missing-plugin path | Slow/network plugin stalls (timeouts), malformed bundles | 2 weeks |
| 4. Editors (X11 float) | x11rb window, parent-ID attach, timer/fd pumping, resize, generic-UI parity | Wayland/XWayland quirks per compositor; HiDPI softness; editor crashes (fallback covers) | 3–4 weeks |
| 5. VST3 | vst3-host or truce-rack spike → integrate; COM lifetime bugs; restartComponent handling; IRunLoop pump | Crate immaturity (audit/fix upstream); per-plugin quirks (Surge/Dexed first, then u-he/Vital) | 5–8 weeks |
| 6. Hardening | out-of-process DSP bridges (§6b v2); fuzz scan corpus; plugin-compat matrix in CI | SHM transport design; restart UX | 6+ weeks, optional |
| 7. LV2 (optional) | livi + lilv/suil C deps; Turtle/URI handling; suil embedding or float UI | C dependency hell; suil+egui coexistence | 4–6 weeks, optional |

Biggest overall risks: (1) Rust VST3 host crates are young — budget spike time
and upstream fixes; (2) editor windowing is where Linux hosts historically bleed
(XWayland, toolkit mismatches) — the generic-UI fallback is what makes this
shippable; (3) state-blob compatibility across plugin upgrades — version and
fail-soft from day one.

## Sources (all URLs verified 2026-11-15)

- Codebase: `src/engine.rs:1-1240`, `src/fx.rs:1-764`, `src/sequence.rs:1-847`,
  `src/link.rs:1-333`, `Cargo.toml`.
- CLAP scan paths + init rules: free-audio/clap `include/clap/entry.h` (main).
- CLAP version 1.2.10 / host list / KVR counts: sonore.dev "plugin-formats"
  (2026-10-27); jacklab.org format guide (2026-09-07).
- VST3 MIT licence since 3.8: Steinberg VST3 portal
  (`…/VST+3+Licensing/VST3+License.html`, `…/Versions/Version+3.8.0.html`),
  press release 2025-10-29; SDK 3.8.1 Aug 2026; repo `steinbergmedia/vst3sdk`
  (MIT). VST3 locations: `…/Locations+Format/Plugin+Locations.html`. Missing
  `VST3_PATH`: `steinbergmedia/vst3sdk#67`.
- VST3 Wayland (3.8 `IWaylandHost`/`IWaylandFrame`): VST3 portal change history.
- LV2 paths: lv2plug.in "Filesystem Hierarchy Standard"; Lilv docs
  (lv2plug.in/pages/developing.html).
- clack: `prokopyl/clack` repo (README: cpal host example path
  `host/examples/cpal`, "no higher-level alternative" note); crates.io
  `clack-host` 0.2.0, `clack-extensions` 0.2.0, `clap-sys` (via deps).
- VST3 crates: crates.io `vst3-host` 0.11.1 (+ `embed` docs on docs.rs:
  X11-only, Wayland handle rejected), `vst3` 0.3.0 (coupler-rs),
  `truce-rack` 1.1.5 (truce-audio), `plugin_host` 0.1.0; `RustAudio/vst3-sys`
  (GPL-3.0, stale Jun 2023); `m-hilgendorf/cli-host` (archived).
- LV2 crates: crates.io `livi` 0.9.0 (`wmedrano/livi-rs`, example `livi-jack`
  with mda EPiano URI), `lilv-rs`; `lv2` crate is plugin-side only.
- Sandboxing: Bitwig "Plug-in Hosting & Crash Protection"; `falkTX/Carla`
  (`source/bridges-plugin`, `bridges-ui`); yabridge (`yabridge.org/how-does-yabridge-work`,
  plugin groups).
- Plugin format matrices: Surge downloads page + manual; u-he product pages
  ("Linux (64-bit): CLAP, VST3", beta notice); airwindows.com/consolidated;
  `zynaddsubfx` README (DSSI/LV2/VST); x42-plugins.com (LV2+JACK).
- Arch packages: archlinux.org groups `clap-plugins` (25), `vst3-plugins` (29),
  `lv2-plugins` (73), `pro-audio` (~296); AUR `vital-synth`,
  `vital-synth-clap-bin` (Package Base `vital-synth-bin`).
