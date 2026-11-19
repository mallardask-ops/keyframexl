# KeyFrameXL — audit 2 (Muse, 2026-11-15)

Scope: everyday FL workflow vs current code. Read-only audit; no source edits.
Sources: `docs/feedback-2026-11-15.md` (backlog + done-status), `research_notes/feedback-2026-11-15/report-muse.md` (B1–B10), `docs/ui-rules.md` (house style). Each claim verified against the code — file:line cited; nothing listed as missing that exists.

House-style shorthand used below: `sunken` wells, `raised` controls, `kit::knob/mini_knob/fader/chips/dropdown`, `caps` labels ≥12px, statusbar hint per action, no new popup types. Undo rule: `checkpoint()` once per gesture (`first_edit`/`started`/`drag_started`), `publish()` on change, status line "… Ctrl+Z undoes."

---

## 1. Mixer (`src/ui/mixer.rs`, `src/sequence.rs` mixer part, `src/engine.rs` mix path)

| Feature | Status | Evidence | User impact | Size |
|---|---|---|---|---|
| Send routing + levels | ok | Level knob per send `mixer.rs:841-854`; post-fader sum into sends `engine.rs:850-858` | — | — |
| "Where it goes" highlight (selected track's sources/destinations) | ok | `routes()` + blue/orange frames `mixer.rs:100-118,882-901`; `From:`/`To:` inspector `mixer.rs:431-445` | — | — |
| Insert slots count | ok | `FX_SLOTS = 10` `sequence.rs:182`; FL parity (10 slots/track) | — | — |
| Meters (per-track peak, peak-hold, clip latch) | ok | `read_meters` `mixer.rs:208-220`; meter + peak-hold + clip latch `mixer.rs:348-387`; engine peaks `engine.rs:562-564,839` | — | — |
| Stereo separation (SEP) | ok | Knob on strip + inspector `mixer.rs:304-316,461-471`; DSP `engine.rs:818-826` + test `engine.rs:1669-` | — | — |
| Mute / solo (chain-aware) / arm | ok | Lamp + Ctrl/right solo `mixer.rs:281-301`, `solo_keeps/solo_insert` `mixer.rs:124-153`; arm dot `mixer.rs:270-279`, record path `ui/record.rs:93-188` | — | — |
| Docking (dock tab / float, F9) | ok | `Tab::Mixer` is a dock tab `ui/workspace.rs:18,27`; F5–F9 toggle `ui/mod.rs:1198`; fx windows float `mixer.rs:660-732` | — | — |
| Add / remove / reorder / rename inserts | missing | Fixed 17 tracks from `default_mixer` `sequence.rs:252-254`; no insert add/delete UI anywhere in `mixer.rs` (header menu only: preset save/load, reset, route, solo, unmute `mixer.rs:43-82`); `validate` allows ≤128 `sequence.rs:572` but no `remap_inserts` exists (cf. report-muse B5.3) | high | M |
| Sidechain (sidechain sends + SC input on comp/limiter) | missing | `Send{to, level}` only `sequence.rs:228-232`; no `sidechain` flag; no SC selector in `fx_params`/`fx_views::dynamics`; engine has one bus per track, no side bus `engine.rs:730-860` | high | L |
| Sends only to later inserts (no feedback / free routing) | partial | Enforced by construction + validation `sequence.rs:585`, comment `engine.rs:735`; FL allows any route with PDC. Live with it until PDC exists | med | L |
| Track colours / icons | missing | `MixerTrack{name,volume,pan,muted,fx,to_master,sends,eq,sep}` `sequence.rs:190-214` — no colour/icon field; strip shows number + name only `mixer.rs:238-268` | med | S |
| Dynamics feedback (GR meter, I/O waveform) | missing | `fx_views::dynamics` draws static curve only (per feedback-2026-11-15.md "Лічильника GR ще немає"); engine exposes peaks only, no GR tap `engine.rs:562-564` | med | M |

## 2. Playlist (`src/ui/playlist.rs`, `src/sequence.rs` clips/tracks)

| Feature | Status | Evidence | User impact | Size |
|---|---|---|---|---|
| Clip colours (pattern colour + track colour) | ok | Pattern-colour header + tinted body `playlist.rs:567-582`; pattern colour menu `playlist.rs:663-684` + `menu.rs:147-168`; track colour header fill `playlist.rs:405-414`; `Pattern.color` / `Track.color` `sequence.rs:127-132,256-266` | — | — |
| Slicing clips | ok | `Tool::Slice` brush `playlist.rs:868-882` via `slice_lengths` `playlist.rs:43-45` (tested `playlist_tests.rs:244`) | — | — |
| Horizontal zoom + follow + scrollbar | ok | Ctrl+wheel anchored zoom `playlist.rs:323-344`, `zoom_anchor` `playlist.rs:49-51`; follow `playlist.rs:1000-1008`; scrollbar `playlist.rs:1011-1028` | — | — |
| Pattern vs automation clips | ok | Automation envelope painted in clip `playlist.rs:603-609` via `ui/automation.rs:paint_clip`; double-click opens editor `playlist.rs:1040-1044` | — | — |
| Clip move / resize / trim / duplicate / mute / delete | ok | `Pending::{Move,Resize,Trim,Duplicate,…}` `playlist.rs:546-557,1053-` ; right-drag mass delete `playlist.rs:834-850` | — | — |
| Track mute | ok | LED per track `playlist.rs:385-402` + context menu `playlist.rs:493-501` | — | — |
| Time markers (numbered flags, jump, song/loop) | missing | No marker model in `Song`/`Pattern` (`sequence.rs:378-400,118-132`); only "marker" hits are the clip-menu ▾ triangles `playlist.rs:634-645` — a menu handle, not a timeline marker | med | M |
| Time selection / loop region (select bars, loop them) | missing | No time-range state: selection is clip-index list only `playlist.rs:172-191`; no select-drag on empty grid except box-select clips `playlist.rs:911-923`; ruler only seeks `playlist.rs:976-989` | high | M |
| Clip grouping (group select/move/edit) | missing | Multi-select is a flat index list with group-move only via Select-tool drag `playlist.rs:1056-1064`; no named/persistent groups, no group header | med | M |
| Audio clips in playlist | missing | `Clip` plays a pattern window only `sequence.rs:155-172`; audio files load into sampler channels `ui/files.rs:602-630`, never land as playlist clips | high | L |
| Track solo | partial (broken) | Solo row exists but the `soloed` test `playlist.rs:502-503` requires `tracks.len() >= 1000`, which never holds — Solo always mutes-everything-else and can never unsolo | med | S |
| Vertical zoom (lane height) | partial | `LANE_H = 32` fixed `playlist.rs:12`; horizontal zoom only `playlist.rs:263-265`; roll has per-axis zoom, playlist does not | low | S |

## 3. Channel rack (`src/ui/rack.rs`, `src/sequence.rs` instruments)

| Feature | Status | Evidence | User impact | Size |
|---|---|---|---|---|
| Step paint strokes (press-drag paints, right-drag clears, one undo/step value per stroke) | ok | `PadStroke{fill…}` `rack.rs:956-968`; path-covered hit test `rack.rs:386-423` | — | — |
| Channel mute LED / solo / mixer box / Ctrl+wheel step / Ctrl+L route | ok | `channel_strip` `rack.rs:616-739`; `step_channel_insert` `rack.rs:444-462`; `route_selected_channels` `mixer.rs:187-201` | — | — |
| Per-channel Pan/Vol (automatable) | ok | Knobs + `Target::ChannelPan/ChannelVolume` `rack.rs:644-672`; engine `channel_gains` `engine.rs:1251-1260` | — | — |
| Note preview for off-grid channels → roll | ok | `paint_note_strip` `rack.rs:844-881`, click opens roll `rack.rs:358-360` | — | — |
| Clone / move / delete selected, pattern length in bars | ok | `clone/move/delete_channels` `rack.rs:486-549`; length menu `rack.rs:78-90` | — | — |
| Channel filter groups (FL channel filter / groups) | missing | `Instrument{id,name,kind,volume,pan,mixer,muted}` `sequence.rs:269-286` — no group/colour field; no filter UI in `rack_header` `rack.rs:28-121` | med | M |
| Channel colours (+ icons) | missing | Same struct evidence; row head shows name button only `rack.rs:741-788`; FL colours channels — we colour only pads/patterns | med | S |
| Graph editor beyond velocity | partial | Velocity stems only `rack.rs:580-611` (title hardcodes `vel · {name}` `rack.rs:587`); FL graph editor covers pan/vol/pitch/etc. | med | M |
| Step nudge / shift (per-channel or per-selection ±step) | missing | `toggle_step` only adds/removes `rack.rs:141-178`; no shift op; roll has `note_ops` but nothing rack-side consumes them for steps | med | S |
| Randomize / humanize steps | missing | No rack randomize; roll `note_ops::{strum,arpeggiate,randomize,flip,limit}` are roll-menu only (per feedback doc, Terra's dialogs) | low | S |
| Fill each N steps (every 2/3/4, fill, strum-fill) | missing | No fill/cycle-fill op in rack; closest is paint-drag `rack.rs:386-423` | med | S |
| Layer channels (one row plays several channels) | missing | No `Layer` variant in `InstrumentKind` `sequence.rs:288-306`; `clone_channels` copies, never links `rack.rs:505-536` | low | M |
| Step length display / per-step length | partial | Steps are 1-step notes by construction (`Note::new…STEP` `rack.rs:412-421`); non-step notes fall back to preview `rack.rs:348-361` — FL-equivalent, but no per-step slide/length gesture on pads | low | S |

## 4. Browser (`src/ui/browser.rs`, `src/ui/files.rs`)

| Feature | Status | Evidence | User impact | Size |
|---|---|---|---|---|
| Sample/file preview (click plays, Esc stops) | ok | `toggle_preview/poll_preview/stop_preview` `ui/files.rs:530-599`; audio click `ui/files.rs:449-455`, preset/plugin rows separate | — | — |
| Favourites (plugins + files) | ok | `favorites/toggle_favorite/is_favorite` + disk file `browser.rs:25-76`; sections for plugins `browser.rs:123-160,320-409` and files `ui/files.rs:461-466` | — | — |
| Sort (Name/Type/Newest/Largest) + background search with progress | ok | `Sort::ALL/order` + `spawn_index` + progress line `ui/files.rs:37-68,148-181,387-395` | — | — |
| Drag onto channel row / rack empty space / instrument window | ok | Row drop (file → sampler, plugin → replace) `rack.rs:261-310`; rack-empty drop → new channel `rack.rs:431-438`; instrument window drop `instrument.rs:276-279` | — | — |
| Plugin picker (F8) with search, Enter-picks-first | ok | `plugin_picker/toggle_picker` `browser.rs:411-523` | — | — |
| Tags (genre/instrument/type tags, tag filter) | missing | No tag field on files/presets/plugins; search is name-only `browser.rs:10-12` + `files.rs:296-318`; `SHOWN` is extension-only `ui/files.rs:16` | med | M |
| Effect drag-and-drop onto mixer slots | missing | Instrument rows set a `PluginDrag` payload `browser.rs:118`, effects rows are click-only `browser.rs:137-160` (click appends to selected track's end); mixer slots accept clicks/menus, no `dnd_release_payload` in `mixer.rs` | med | S |
| Drop onto piano roll / playlist (create channel+clip) | missing | Drops handled only in rack + instrument window (evidence above); roll/playlist have no `dnd_release_payload` for `FileDrag`/`PluginDrag` | low | S |

## 5. Global (project, render, autosave, shortcuts, MIDI learn, CPU)

| Feature | Status | Evidence | User impact | Size |
|---|---|---|---|---|
| Autosave + recovery | ok | Timed worker autosave `ui/project.rs:126-154`; `Recover autosave` + Open-recent `ui/project.rs:204-247`; settings carry interval `settings.rs:143-144,174` | — | — |
| Unsaved-changes guard | ok | `request/Pending/unsaved_window` + title `*` `ui/project.rs:41-67,106-122` | — | — |
| Render: split stems, tail modes, normalize/dither, progress+cancel | ok | Stems `render.rs:331-`, tail `render.rs:21-46`, dialog `ui/export.rs:205-344` | — | — |
| CPU load + voice count | ok | `cpu {load}% voices {n}` in deck `ui/deck.rs:195-201` from `audio.rs` atomics | — | — |
| Project info (title/author/genre/comments, notes) | missing | `Song{version,tempo,swing,instruments,patterns,clips,tracks,mixer,tempo_map,meters}` `sequence.rs:378-400` — no info fields; no Project/Info settings page (`Page::{Audio,Midi,Plugins,General}` `ui/settings_ui.rs:11-30`) | med | S |
| Render formats beyond WAV/FLAC (MP3/OGG) | missing | `Format::{Wav16,Wav24,Wav32Float,Flac16,Flac24}` only `render.rs:52-`; export dialog chips WAV/FLAC only `ui/export.rs:263-280`; menu says "WAV, FLAC" `ui/menu.rs:82` | med | M |
| MIDI controller learn (knob → hardware) | missing | `Link to controller…` disabled with "Arrives with MIDI" `ui/kit.rs:651-654`; no learn state; `midi_in` handles notes/pedal only (`Event::{On,Off,Control}` consumed for recording, `ui/settings_ui.rs:160-178`) | med | M |
| Xrun / dropout indicator | missing | `xruns: AtomicU64` counted in bridge `bridge/mod.rs:47,109` and `audio.rs` load exists, but deck shows only CPU/voices `ui/deck.rs:199-201` — no underrun counter anywhere in UI | med | S |
| Typing-keyboard octave shift (+ velocity is done) | partial | Fixed two octaves C3/C4 `ui/settings_ui.rs:182-188`; velocity follows the roll encoder (`self.velocity`), but no octave up/down keys; shortcuts list documents only on/off `ui/menu.rs:344` | low | S |
| Tempo / time-signature track UI | missing | Data exists (`tempo_map`, `meters` `sequence.rs:395-399`, `src/tempo.rs`) but no playlist lane/editor UI (gated on Terra's synth per feedback doc); playlist ruler is bars-only `playlist.rs:352-364` | med | M |

---

## Top-12: what to build next (ranked by impact ÷ effort)

### 1. Playlist time selection + loop region
FL users select bars to loop, copy, or mute in one gesture; today selection is clip-indexes only (`playlist.rs:172-191`), so "loop bars 5–9" is impossible. Spec: Shift+drag on empty grid (Draw tool) or on the ruler creates a bar-snapped range (Alt = free ticks), painted as a `TEXT`-outline band in the ruler + shaded grid (`gp` painter, same clip as grid); `L`-style loop toggle in `▾` menu ("Loop selection", shortcut `Ctrl+L` is taken — use `Alt+L`) makes playback cycle the range in Song mode (engine reads a `Song`-level or view-level `(start,len)`; Song mode `loop_frames` already centralises length in `sequence.rs:742-750`, extend there). Click clears; Esc clears; Delete with a range deletes clips inside (one undo step via existing `checkpoint`). Tests: `paint_positions`-style pure helpers for snap/clamp + a `loop_frames`-with-range test in `sequence.rs`.

### 2. Mixer: add / remove / rename inserts (with index remap)
Fixed 17 tracks (`default_mixer`, `sequence.rs:252-254`) blocks every growing project; `validate` already allows ≤128 (`sequence.rs:572`). Spec: `▾` menu rows "Add insert" (appends, selects, one undo) and "Remove this insert" (only when no channel/send points at it, else offer re-point to Master in the same menu — keep it a menu, no popup); rename stays inline like tracks (`mixer.rs:421-430` pattern with `first_edit`/`end_edit`); deletion remaps `Instrument.mixer` + `Send.to` in one new `Song::remap_inserts` helper (same shape as `unique_detaches`, unit-tested with channels + sends on both sides of the gap). Strips scroll already (`mixer.rs:94-120`), so no layout change; house style untouched.

### 3. Track solo repair (playlist) + clip-group move parity
The Solo row can never unsolo (`tracks.len() >= 1000` guard, `playlist.rs:502-512`) — a one-line logic fix with outsized trust value: users who hit Solo get stuck muting everything. Spec: mirror the rack/mixer solo pattern (`solo_keeps`-style: soloed = this track unmuted and all others muted; click again restores), one undo step, status "Track N soloed/unsoloed". Same gesture vocabulary as mute LED (click) so no new affordance. Tests: extend `clip_tests::muted_clips_and_tracks_are_silent` with a solo round-trip asserting render equality with the isolated track.

### 4. Mixer + channel colours (shared palette, FL meaning)
Rack rows, mixer strips and playlist headers each colour independently (pattern colour vs track colour vs none) — FL users colour the *channel* once and see it everywhere. Spec: add `color: Option<u32>` to `Instrument` and `MixerTrack` (serde-defaulted, skipped when `None`, validated as opaque u32 like `Track.color`); rack name button gets a 5px left colour bar + `▾`-style Colour submenu reusing the playlist palette block (`playlist.rs:469-492` — extract to a `kit`/theme helper, no new widget); mixer strip tints its name plate the same way tracks do (`playlist.rs:405-414` pattern); `free_name`/clone preserve colour. Undo: colour pick = one step (existing pattern). Tests: serde round-trip + `validate` acceptance, mirroring `Track` handling.

### 5. Sidechain sends + compressor/limiter SC input
The single most-asked FL mixer feature (ducking, gated verbs); today `Send{to,level}` (`sequence.rs:228-232`) is audio-sum only with no key bus. Spec (staged, engine-gated): model `Send{sidechain: bool}` (default false, skipped in JSON when false) — sidechain sends skip the target's audible bus and feed a per-track side bus (reuse the `scratch`/second-buffer pattern from `engine.rs:mix`, no new allocation on RT); comp/limiter gain a `Sidechain on/off` param (last param slot, default off, so old projects render bit-identically); UI: send row gets an `SC` chip next to the level knob (`sends_ui`, `mixer.rs:837-879`), FX windows show the SC toggle beside On/Bypass. Validation keeps sidechain sends later-insert-only (acyclic). Tests: render test with sidechain send asserting the target's dry bus is silent while the SC detector fires (mirror `sends_reach_a_later_insert…`, `engine.rs:1702-`).

### 6. Playlist markers (numbered flags + jump)
Song arrangement without markers means scrolling bars to find the chorus; the B3 plan already foresees pattern markers, but the *playlist* needs them more. Spec: `Song.markers: Vec<(Tick,String)>` (serde-defaulted, sorted, clamped to `MAX_BARS*BAR`, validated); ruler draws numbered flags above bar numbers (same painter as `END`, `playlist.rs:360-364`); Alt+click flag jumps (existing `seek_tick` path, `playlist.rs:982-989`); `▾` menu Add/Rename/Delete; drag moves (reuse `PlDrag` mode machinery, one undo per drop). No new window; markers render in the muted-ink style of bar numbers. Tests: validation (sorted/in-range/dedup) + `bars()` unaffected + JSON round-trip.

### 7. Rack step tools: nudge, fill-every-N, humanize
Painting 64 steps by hand is the rack's slowest job; FL has shift/fill/humanize and we have none (`toggle_step` only, `rack.rs:141-178`). Spec: `▾` menu section "Steps" (next to "Clear this pattern's notes"): Nudge ◀ ▶ (selected channel or rack selection ±1 step, clamped, one undo), Fill every N (chips 2/3/4 + offset, writes velocity-100 steps where empty — never overwrites, so it composes with paint), Humanize (velocity ±15% + timing ±6 ticks on step notes of selected channels, seeded RNG for determinism, same undo shape as Terra's roll dialogs). All operate on step-grid notes only (skip preview channels). Tests: pure helpers in `note_ops.rs` style + one rack-level undo test.

### 8. Graph editor: pan + volume lanes (not just velocity)
The editor says velocity-only in its own title (`vel · {name}`, `rack.rs:587`); FL users shape hats' pan/vol per-step here. Spec: `▾`/chips above the lane (Velocity/Pan/Volume — `chips` component, house style) switching the lane target; pan lane draws centre-line stems (-1..1), volume draws 0..1.25 stems; drag gesture identical to velocity (`rack.rs:597-610`), Alt-click resets to default (C / 100%); automation targets already exist (`ChannelPan/ChannelVolume`), so lanes write plain values while knobs stay the live surface. One undo per drag (existing `checkpoint` on `drag_started`). Tests: lane value-mapping pure fns + a pattern-level write test.

### 9. Browser tags + effect drag onto mixer slots
Name-only search (`matches`, `browser.rs:10-12`) dies at ~500 plugins/samples; and effects are click-only while instruments drag (`browser.rs:118` vs `137-160`). Spec: free-form tags on favourites (space-separated in the existing `browser-favorites` line format — additive parse, old lines still load); tag chips under the search filter both lists; effect rows get the same `click_and_drag` + `PluginDrag` payload as instruments, and mixer FX slots accept the drop (`dnd_release_payload` in `fx_slot_row`, `mixer.rs:496-594`, falling back to today's click menu). No new windows; drag ghost reuses the file-drag tooltip (`ui/files.rs:437-447`). Tests: tag parse/filter pure tests + favourite round-trip with tags (`browser_tests::favorites_round_trip` extended).

### 10. Project info + MP3/OGG render options
No title/author/comments anywhere (`Song`, `sequence.rs:378-400`; no Info page, `ui/settings_ui.rs:11-30`) means exports leave nameless and collabs lose context; and render stops at WAV/FLAC (`render.rs:52-`, `ui/export.rs:263-280`). Spec: `Song.info: {title,author,genre,comments}` (all default-empty, skipped when empty; title defaults to filename at export); Info page in Settings (5th `Page`, same card/row pattern as General) + FILE-menu "Project info…" shortcut; export dialog gains an MP3 row (VBR preset chips) via an added `Format::Mp3` behind the same `Job`/`Progress` machinery, OGG explicitly deferred with a disabled chip ("Arrives with …" house rule). WAV/FLAC paths render bit-identically. Tests: info round-trip + MP3 encode smoke test on the existing render-test song.

### 11. MIDI controller learn
Every knob advertises a dead control (`Link to controller…` → "Arrives with MIDI", `ui/kit.rs:651-654`) — the highest-visibility stub in the app. Spec: right-click menu on any `kit::knob/mini_knob/fader` gains "Learn…" when idle: next CC received on any enabled port binds `(channel,cc)` → that control's automation target (reuse `AutoTarget` — every learnable knob already declares one via `kit::automatable`); bound knobs show a dot + tooltip "CC 74 · right-click to forget"; bindings persist in settings (additive, default-empty). No MIDI-out yet (explicitly out of scope). Tests: binding map pure tests (learn → apply → forget) + a `midi_in` harness test that a CC moves the bound value.

### 12. Xrun/dropout lamp + dynamics GR plumbing (one observability slice)
Dropouts are invisible: `xruns` counted (`bridge/mod.rs:47,109`) but never shown, and comp/limiter give no gain-reduction feedback (feedback doc confirms "GR still missing"). Spec: deck status right side gains an `xrun N` counter next to `cpu%/voices` (`ui/deck.rs:195-201` pattern, `WAIT` ink when N>0, click resets — same affordance as the mixer's clip latch, `mixer.rs:377-386`); engine exposes per-dynamics-slot GR peak via the existing atomics path used for `peaks` (`engine.rs:562-564` pattern, no new thread machinery); `fx_views::dynamics` draws the GR bar under the curve. No audio-thread allocation; counters relax-ordered. Tests: GR tap stays 0 on unity + ≈ expected on a slammed limiter (mirror `engine.rs:1728-` ceiling tests) + xrun counter increments in the bridge test (`bridge/tests.rs:108-112` already asserts xruns).
