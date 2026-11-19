# Plugin process bridge plan

## Scope and current behavior

A 256-frame block at 48 kHz lasts 5.333 ms. The figures below describe added bridge latency, not the plugin's own processing time or device buffering. No bridge has been benchmarked here.

The scan already runs `plugin-probe` in a child (`src/plugins.rs:184`, `src/plugins.rs:246`), but normal hosting puts all plugins on a host thread in the DAW and calls them from `Vst3Proc::process` under a mutex (`src/plugins.rs:282`, `src/plugins.rs:314`, `src/plugins.rs:545`). An abort in yabridge's native library therefore still kills the DAW. `Processor` takes interleaved stereo, timed events and transport (`src/plugin.rs:28`, `src/plugin.rs:38`); the engine calls it for instruments and effects (`src/engine.rs:609`, `src/engine.rs:682`). `PluginCmd` currently installs or removes an instance (`src/audio.rs:106`). The existing project plan expects state persistence, sample-timed notes, editors and export (`docs/plan-2026-11-15.md:164`).

## Options

| Option | Added latency and CPU per 256-frame block | Containment, state, parameters and events | Complexity |
| --- | --- | --- | --- |
| (a) `vst3-host` process isolation as-is | Synchronous round trip; no fixed extra block, but serialization, copies, scheduling and plugin time all consume the same 5.333 ms budget. JSON/base64 and allocation add CPU and jitter; measure rather than assume a duration. | A helper crash contains the native yabridge library. Save/load state, parameter operations, timed MIDI and output events exist on the protocol, but integration and parity still need testing. Its editor is helper-owned, not embedded. | Lowest prototype effort. Unsuitable unchanged for the realtime callback: `process` clones inputs and synchronously sends a command (`vendor/vst3-host/src/internal/isolated_plugin_impl.rs:428`); response waiting has a deadline (`vendor/vst3-host/src/process_isolation.rs:1544`). Recovery can block control operations and must not run in the callback. |
| (b) Our child, shared audio memory and socket control | Recommend a one-block pipeline: 5.333 ms per bridge at this block size, plus scheduling and plugin execution within that deadline. Copies and atomics are bounded; worker wakeups and extra processes add CPU. No measured overhead yet. | A child per instance contains faults to that instance; groups share a failure domain. Explicit checkpoint/restore, parameter feedback, transport, timed events and output events are needed. | Highest initial effort; explicit protocol, lifecycle, audio graph latency compensation and tests. |
| (c) yabridge groups alone | No new KeyframeXL bridge block; yabridge's existing IPC and Wine scheduling remain. Grouping can reduce process overhead, but CPU and round-trip time depend on workload. | Multiple Windows plugins share a Wine group host. Its death can affect all members, and the in-process native bridge can still throw or terminate the DAW, as observed here. Grouping is not a KeyframeXL crash boundary. Existing state, parameter and MIDI paths remain unchanged, with no recovery guarantee after failure. | Configuration only; no reliable containment. [yabridge architecture](https://github.com/robbert-vdh/yabridge/blob/master/docs/architecture.md) describes group host reuse. |
| (d) Catch abort or install `std::terminate` hook | No planned bridge latency; overhead is not the issue. | A signal handler cannot safely unwind or repair arbitrary corrupted plugin state, locks and foreign threads; async-signal-safe operations are severely restricted. `std::terminate` is process-wide, may be invoked on any thread, and a returning terminate handler aborts; it cannot resume the failed call safely. No reliable state, parameter or event recovery. | Superficially small, but cannot deliver containment. A diagnostic handler may record minimal crash information, not recover. |

The vendored protocol includes state, parameter, MIDI and audio commands (`vendor/vst3-host/src/process_isolation.rs:435`, `vendor/vst3-host/src/process_isolation.rs:527`, `vendor/vst3-host/src/process_isolation.rs:570`, `vendor/vst3-host/src/process_isolation.rs:619`). Its existing timeout and fake-helper tests are useful references (`vendor/vst3-host/src/process_isolation.rs:2677`). It may serve as a control-protocol prototype, but its helper editor ignores the supplied parent (`vendor/vst3-host/src/internal/isolated_plugin_impl.rs:916`) and its audio transport needs replacement.

## Editor across processes

X11 window IDs name server resources across clients on the same display, subject to X server access and lifetime. Today `embed()` creates an XCB child directly under the passed parent XID, maps it and supplies the new child XID to `open_editor` (`src/plugins.rs:428`). This is already a plain child window, not an XEmbed implementation or a reparent of an existing top-level window. A child process can do the same using its own X connection and the parent's XID; reparenting is another possible mechanism. XEmbed is optional and requires explicit focus/protocol support, not merely a parent XID.

Keep UI layout and `Embed { id, parent, rect, visible }` decisions in the DAW (`src/plugins.rs:369`). Send them to the child, which owns the plugin, X connection, editor window, map/configure/unmap, plugin run loop and resize requests. Return `EditorSize`, open/close and error notifications. The current loop services the plugin and applies requested sizes (`src/plugins.rs:698`). Test focus, keyboard input, clipping, scaling, parent destruction and XWayland; a shared X server alone does not guarantee all editor behavior. On child death the X server destroys its windows; the UI must clear stale editor state.

## Recommended design

Start with one `keyframexl plugin-host` child per instance. The DAW must never load that instance's native plugin or yabridge library. Retain the existing probe as a separate process. Consider optional grouping only after measuring resource cost, with explicit group-wide crash semantics. The supervisor owns child lifecycle on a non-audio thread; a proxy implements `Processor` in the DAW.

Use a versioned Unix socket for control and a pre-sized memfd mapping passed by file descriptor. Negotiate protocol version, instance/generation ID, format, maximum block size, channel/event capacities and features before publishing the proxy to the audio engine. Validate lengths, IDs and offsets on both sides. Do not put pointers, Rust types with unspecified layout, or process-local mutexes in shared memory. Use fixed-width fields, defined alignment and verified cross-process lock-free atomics; publish with release/acquire ordering. Each generation gets a new mapping, so a late child cannot write into a replacement instance.

Control messages, each with request ID and instance generation:

- DAW to child: `Load(path, class, rate, max_frames, state)`, `Start`, `Stop`, `SetRate`, `SaveState`, `LoadState`, `GetParams`, `SetParam`, `Embed`, `HideEditor`, `Unload`, `Shutdown`.
- Child to DAW: `Ready(info, latency, capabilities)`, `Ack` or `Error`, `State(blob, revision)`, `Params`, `ParamChanged` including edit gesture, `EditorSize`, `EditorClosed`, and diagnostics. The supervisor reports `Crashed` with exit status separately.
- State blobs and formatting belong on the control socket, with bounded lengths and deadlines. Do not perform socket I/O on the callback. Maintain the last successfully acknowledged project checkpoint in the DAW; failed saves must preserve it and report failure. A crash loses changes since that checkpoint. Reload applies the checkpoint before starting audio, reports restore failure, and never silently substitutes defaults.

The mapping contains a versioned header and fixed-capacity single-producer/single-consumer input and output slots. Each input slot has generation, sequence, frame count, transport, bounded sample-offset MIDI/note and automation events, and stereo input samples. Each output slot has the matching generation/sequence, frame count, stereo output and bounded output events/parameter feedback. Give each slot explicit ownership; neither side overwrites a slot until its consumer releases it. On exhaustion, fail the block explicitly and count it; never silently drop a note-off. Event capacity overflow must trigger a visible diagnostic and a defined all-notes-off/resynchronization policy. Timed automation travels with its block; UI parameter edits and gestures use control messages and must be ordered against processing at a defined block boundary. Preserve event offsets and ordering; route plugin output events when engine support exists, otherwise report that limitation.

For realtime playback, the callback publishes block N and consumes completed block N-1 without waiting. A child worker may sleep on eventfd/futex; the callback uses only bounded, nonblocking operations and never waits for it. Startup, a missed completion deadline, a full queue, or a dead child produces silence for that block, including an effect's wet output. Existing effect mix can still contribute dry audio (`src/engine.rs:682`). Never consume a late result for a different sequence; discard it only when ownership permits. A watchdog on the supervisor detects a child stuck beyond a separate bounded health deadline, terminates/reaps it off-thread, and marks it crashed. A single missed audio deadline is an xrun, not proof of death. No allocations, locks, blocking syscalls, logging, spawning, destruction, or unbounded retries on the audio thread. The current engine locks `pl.proc` (`src/engine.rs:634`, `src/engine.rs:687`); change that ownership path as part of implementation, rather than claiming the proxy alone is realtime safe.

Pipeline delay is one block per bridge, not a constant 5.333 ms if callback sizes vary. Fix a negotiated quantum or design timestamped buffering for variable callback sizes. Account for delay across serial effects and parallel dry/instrument paths with graph-wide latency compensation, including plugin-reported latency; preallocate delay buffers. Define latency changes and transport discontinuities at safe graph transitions. Offline export needs a separate non-realtime mode that can wait with deadlines, flush pipeline tails and align output; it must not inherit live silence-on-scheduling behavior silently.

On EOF, exit or watchdog failure, the supervisor atomically marks that generation crashed, wakes the UI, and reaps the child. The callback emits silence without waiting; the UI shows “Plugin crashed — Reload”. Reload is explicit and off-thread: launch a fresh generation, load the last saved state, prepare resources, then replace the proxy through the engine's command path. Reset held-note bookkeeping and send a panic or resynchronize notes so stale notes do not stick. Keep the crash indicator if reload fails.

## Phases and verification

1. Build a minimal fake `plugin-host` and supervisor. Exercise handshake/version errors, fd lifetime, process exit during load and mid-block, hangs, repeated reload, stale messages and cleanup. Assert the DAW stays alive and other instances continue.
2. Implement bounded shared slots and the proxy, initially with a deterministic fake gain/synth child. Test sequence ownership, ordering, overflow, input/output events, silence on missed deadlines, variable-size rejection or buffering, and late completions. Instrument callback allocation and blocking; stress CPU scheduling and collect xruns and CPU at 48 kHz / 256 frames.
3. Move VST3 load, processing, transport, state and parameter operations into the child. Test state round trips and failed saves, sample offsets, automation feedback, note-off/panic, effect mix, multiple instances, sample-rate change and plugin-reported latency. Use a deliberately crashing plugin or fake child exiting inside processing; test a hung child too.
4. Move editor creation and servicing. Under X11 and XWayland check embedded resize, visibility, focus, repeated open/close, DAW parent destruction and child crash while open. Check graceful behavior with no display.
5. Implement compensation and offline render flushing; compare deterministic bridged and unbridged renders with delay accounted for, including serial effects and parallel dry paths. Verify project save/autosave, undo and reload against the last acknowledged state.
6. Live-test native VST3 and yabridge instruments and effects, including Evergreen with a working display and the headless OpenGL failure. Kill the Wine host and the KeyframeXL child separately during audio and editor use. Confirm silence, surviving audio/UI, crash indicator, reload and restored checkpoint. Record actual latency, xrun counts and CPU under several instance counts and buffer sizes.

## Risks and open questions

- One-block scheduling and latency compensation across arbitrary routing, live monitoring and variable callback sizes need a concrete engine design; multiple serial bridges accumulate delay. Define acceptable latency and measure before rollout.
- VST3 controller and editor thread affinity, reentrant callbacks, state capture while processing and yabridge's own synchronization need validation. Control must not block audio through a shared child-side mutex.
- Cross-process atomics, wakeup behavior, sandbox permissions, FD inheritance and abnormal shutdown need Linux-specific tests. A child can hang in plugin teardown; enforce deadlines and reap it without stalling the UI.
- X11/XWayland support does not imply native Wayland embedding. Some plugins may require focus or graphics behavior unavailable in headless sessions.
- Decide checkpoint cadence and user-visible dirty-state behavior, event capacity, overflow recovery, plugin output MIDI routing and compatibility policy for plugins requiring more than stereo buses.
- Measure per-process memory and CPU before allowing groups. A Wine group shared across KeyframeXL children can still make several instances fail together; yabridge group configuration must be documented and tested.

## Status (2026-11-15)

Implemented (`src/bridge/`):

- **Mechanics.**
  - Each plugin runs in its own `keyframexl plugin-host` child (Settings → Plugins → "Run each plugin in its own process", on by default; `KFXL_NO_BRIDGE=1` forces in-process).
  - The child runs the unchanged plugin host thread. Requests travel over the socket.
  - Audio goes through a `/dev/shm` block. A futex wakes the child, and the `in_seq`/`out_seq` hand-off gives one block of latency through a preallocated FIFO.
  - The editor embeds by parent XID from the child.
- **Crash handling.**
  - A crash or hang turns into silence plus `Reply::Failed`. The wrapper shows "crashed" and a Reload button, which reloads from the last saved state.
  - Export sets `plugin::OFFLINE`, so bridged plugins answer each block at once, with no latency.
- **Tests.** Real child processes:
  - gain latency, sample-accurate notes;
  - crash mid-block, hang → watchdog;
  - load error, long blocks, offline.
- **Live checks** (headless KWin + XWayland, yabridge):
  - Grand Piano: editor embedded across processes; output matches in-process (RMS 0.070 vs 0.0707, spectral similarity 0.953).
  - `kill -9` of the child: the app keeps running, "crashed" + Reload, and the reload works.
  - Evergreen's editor OpenGL failure used to abort the program (exit 134). It now only marks that plugin crashed.
- **Bug found and fixed:** the child started from `in_seq` and could skip a block published before its loop ran. It now starts from `out_seq`.

Not done yet:

- latency compensation across chains of sends between inserts (channels, effect dry/wet and insert outputs are compensated: see docs/feedback-2026-11-15.md);
- xrun counter in the UI;
- grouping several plugins per process;
- a no-allocation test of `BridgeProc::process`.
