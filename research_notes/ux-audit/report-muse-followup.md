# KeyFrameXL — follow-up audit (Muse, 2026-11-15 night)

Scope: `/tmp/kfxl-final/*.png` (7 new shots) vs `research_notes/ux-audit/report-muse.md` + `screens/01–25`, `docs/ui-rules.md`, `docs/ux-plan-2026-11-15.md` (Стан + Доповнення). Read-only; no code touched.

## 1. What improved since the morning audit

- Deck row 2 now text buttons `Playlist / Rack / Roll / Mixer / Browser` with LED left; TEMPO knob gone in new shots (`plugin-wrapper.png`, `mixer-docked.png`); LCD has ▲▼ steppers; hint bar shows hovered control.
- Browser: search moved to top (`Search assets`); sections `Plugins / Channel / Mixer / Effect presets / Packs / Libraries / Recorded / Rendered / Folders` — matches FL categories; width resizable 160–360.
- Rack: lit steps warm-white + red 2–3px velocity bar; step numbers `01–16` printed; `--` for Master insert (not `M`); solo blue dot + status `Only Kick plays…`.
- Piano roll: 8 icon tools in header (`roll-after-midi-drop.png`), `Alt+Q` dialog, `Ctrl+L/U/G` work; MIDI XDND drop confirmed (`9 note(s)… into Bass at bar 2`).
- Mixer docks as tab next to Rack (`mixer-docked.png`), no longer 94%-wide float covering work.
- Plugin wrapper single (`Plugin / Parameters`, `◀ preset ▶`, `☰`); Parameters page full-width sliders + `On/Off` switch + filter + 94 params.
- Presets `.kfxchannel/.kfxmixer/.kfxfx` + stems `Split mixer tracks`; `UI 100%` in status; `Ctrl+=/-/0` 80–150% + Settings → General → Interface size 50–200%; F1 has filter; `Ctrl+Shift+1…0` solo.
- Sampler copy changed toward `Load sound… / Replace sound…` (plan claims; shot still says `Load preset` — see §2.8).

## 2. TOP 12 remaining (ranked, daily use first)

### 1. Rack strip still 5 micro-controls in 230px — `rack-solo-f1.png` bottom, `roll-after-midi-drop.png` mid
Wrong: LED ~16px, Pan/Vol knobs ~18px side-by-side, mixer box `24×18`, no numeric values (tooltip only). Below own 24px hit rule. Wheel over box still risks silent reroute.
Breaks: FL `channelrack.htm` chunky Mute/Pan/Vol/Mixer; TE “no 9px ghost as only label”; `ui-rules.md §1.4/§1.8`.
Fix: `src/ui/rack.rs` row header → `[LED 24][Name flex][M-box 34×22]`; Pan/Vol 24px + value on hover + R-click `Type value`; wheel reroutes only with `Ctrl`. Effort: M.

### 2. Piano-roll header icon-only, no legend — `roll-after-midi-drop.png` lower third
Wrong: 7–8 glyphs ~28×26 (`pencil/brush/X/mute/slice/grid/zoom`) with no text; `VELOCITY 100` knob confuses new-note vs lane; ghost (yellow contour), slide (amber line `to C4`), porta (white slash), 6 note colours unexplained.
Breaks: FL `pianoroll.htm` 8 labelled tools; TE “combos documented on-device, never hidden”.
Fix: `src/ui/roll.rs` header → labelled `tool_button`s + `NEW VEL` caption; first-open dismissible legend strip; add zoom `−/+` buttons (trackpads). Effort: M.

### 3. Playlist header still gesture-duplicates, no tools — `mixer-docked.png` centre, `rack-solo-f1.png` behind F1
Wrong: `Add clip / Make unique / Delete` (3×26px) burn header; no Select/Paint/Delete/Mute/Slice/Zoom tools, no markers/groups, snap readout only in deck row 2, zoom Ctrl+wheel only.
Breaks: FL `playlist.htm` tools `B/P/C/E/D/T/Z`, `Shift+4/5`, `Alt+T` markers.
Fix: `src/ui/playlist.rs` header → tool chips sharing `Tool` with roll + snap mirror + Follow toggle; keep single empty-state `Add clip`. Effort: M.

### 4. Mixer strips cramped, meters not to be trusted — `mixer-docked.png` bottom
Wrong: strips 64+4px, pan+sep 20px knobs illegible, fader travel ~120px, meter 6px bar no peak-hold/clip/number; route `→M` ~10px; `Insert 11` clipped at right edge (no visible scrollbar); inspector shows only 5/10 slots.
Breaks: FL `mixer.htm` (select/multi, wheel over names, `Alt+R` render armed, peak+clip); TE “vital keys get LEDs”.
Fix: `src/ui/mixer.rs` rows 28px, LED 20px, mix knob on hover/selected only, peak-hold + clip tick (click clears), fader ≥140px, dock min-height 300px, always-visible h-scrollbar. Effort: M.

### 5. Browser has the right shelves, empty shelves — left column all 7 shots
Wrong: `No VST3 plugins found`, `Музика / mallar2d` (= home root) as Folders, no `Favorites ★`, no Packs/Recorded content, preview click-only (no vol/pos/loop, no `Shift+↑↓` audition-step), no Plugin Picker `F8`.
Breaks: FL `browser.htm` (Plugin database/favorites/Channel/Mixer presets/Packs/Recorded/Rendered, `F8` picker, R-click Send-to).
Fix: `src/ui/browser.rs` + `files.rs` hide home-root default, add `★ Favorites`, picker `Window` grid generators|effects with filter, `Shift+↑↓` stepping. Effort: M.

### 6. Parameters page raw numbers + clipped toggle — `plugin-parameters.png` centre
Wrong: sliders full-width good, but values `-57.36 / 0 / 1` raw units, no Hz/ms/% (rule: units mono next to value); toggles `Off/On` vs `On` mix; right column `Offf` (4 chars, cell ~40px too narrow); header `VST3 ready` faint 12px grey.
Breaks: FL `plugins/wrapper.htm` params with units; `ui-rules.md §2.1` units mono; BMS “interface never lies”.
Fix: `src/ui/plugins_ui.rs` param rows → formatted value + unit, fixed value column ≥64px mono tabular, single `kit::switch`. Effort: S.

### 7. Wrapper arrows + hamburger too small, double chrome — `plugin-wrapper.png` top, `plugin-parameters.png`
Wrong: `◀ ▶` ~16px hit, preset name elided, `☰` 24px at far right 12px from `×`; Parameters dialog opens as second `Window` inside Playlist window (two title bars + two `×`, dim behind missing).
Breaks: FL wrapper preset arrows + `Save preset as (.fst, draggable)` + Detached; TE “one hero accent, honest material”.
Fix: `src/ui/plugins_ui.rs` header arrows 28×26 + `ComboBox` name; dialogs modal (`Order::Foreground` + 40% dim, `Esc`=close, `Enter`=primary). Effort: S.

### 8. Sampler empty state is a full-screen apology — `sampler-window.png`
Wrong: window ~86%×92% for one sentence + `Load preset` button duplicated in header (`Piano roll / Load preset`) and body; shot still says `Load preset`, not plan's `Load sound… / Replace sound…`; drum voices same dead-end pattern.
Breaks: `ui-rules.md §1.7` empty = one sentence + next-step button; FL `chansettings.htm` Save/Replace/Route/Rename.
Fix: `src/ui/sampler.rs` + `instrument.rs` compact dialog 480px, single primary `Load sound…`, secondary `Replace…` when loaded, wrapper verbs (Save/Clone/Route/Ctrl+L). Effort: S.

### 9. Hint bar + Record still whisper — deck row 2 all shots, row 1 left
Wrong: hint elided `…Hover a zone f…` (available ~400px, text ~700px); Record 40×40 with 6.5px dot, no `REC` text/time/take; Pattern `−/[01 Pattern 1 ▾]/+` minus/plus ~22px.
Breaks: FL hint bar = hovered control verbatim (`basics_interface.htm`); red = record+locks only (OP-XY).
Fix: `src/ui/deck.rs` hint = hovered `tip()` else focused-view one-liner, never tutorial; Rec armed = `BRAND` + ring + take timer; steppers 26px. Effort: S.

### 10. F1 sheet filter without groups, missing new keys — `rack-solo-f1.png` centre
Wrong: flat 13-row list (Space…Ctrl+R only), no sections Transport/Windows/Rack/Roll/Playlist/Mixer/Browser, no `Ctrl+Shift+1…0` solo, no `Alt+Q/V`, `Ctrl+G/U/L`, `Ctrl+L`, `S/F2`, short `Save new version (name_2…)` label truncated.
Breaks: plan Wave 1.7 (grouped + filter); FL `basics_shortcuts.htm` full map.
Fix: `src/ui/menu.rs` shortcuts window → sectioned + filter (already `TextEdit`), keymap table from old audit §13. Effort: S.

### 11. Caps still 10–11px everywhere — `mixer-docked.png` (`TRACK/EMPTY/No clips yet`), `roll-after-midi-drop.png` (`BEAT/PATTERN 1`), `settings-general-70pct.png`
Wrong: `TRACK / BEAT 1 / 16 STEPS / SWING 0% / INSERTS / FROM:` ~10px mono-uppercase 0.12em — below own 12px floor (`kit::caps`), ~7px cap-height at 1366. Prose in caps too.
Breaks: `ui-rules.md §1.3` min 12px + plan Wave 1.1; FL vector UI at 125–200%.
Fix: `src/ui/kit.rs` `caps()` floor 12px + `caps_fit` tracking 0.08em at 12; `theme.rs` prose 13–14 sentence-case. Effort: S.

### 12. Scaling + docking leak — `settings-general-70pct.png` whole, `mixer-docked.png` right edge
Wrong: `INTERFACE SIZE 70%` shrinks text but `MENU_H 26 / ROW1_H 48 / BROWSER_W 208 / NAME_W 230` stay px → at 150% overflow, at 70% dead field; `<1100px` browser collapse never triggers in shots; mixer h-scroll clips `Insert 11` with no affordance; floating sampler/plugin cover the rack they edit.
Breaks: FL `envsettings_general.htm` Main/popups/toolbars/cursor separation; plan “float 720×480, never cover edited”.
Fix: `src/ui/workspace.rs` + `settings_ui.rs` breakpoints (<1100 collapse browser to icons, <800 inspector → bottom sheet), floats capped + offset to free quadrant, `%` readout kept in status. Effort: M.

## 3. Looks broken (clip/overlap/readability)

- `plugin-parameters.png`: `Delay Enable … Offf` clipped; two stacked title bars (`Playlist` behind `Plugin · Grand Piano`);Gain `-57.36` raw; window ~62%×66% covers rack it edits.
- `rack-solo-f1.png`: F1 dialog 420px covers rack; behind it playlist `Add clip/Make unique/Delete` + empty `…at bar 1.` sentence cut; rack tooltip `slide to C4` overlaps pads.
- `roll-after-midi-drop.png`: roll toolbar icons 20px/1.5-stroke at 1366 unreadable without hover; `to C4` label sits on grid line; velocity lane hidden (window <260px rule?) with no affordance.
- `mixer-docked.png`: `Insert 11` half-painted at dock right edge; `16 INSERTS · SELECTED MASTER` caps ~10px; `→M` route 10px; inspector `1 (empty)…5 (empty)` — slots 6–10 below fold, no scrollbar.
- `sampler-window.png`: modal covers Playlist + Rack simultaneously; header `Piano roll / Load preset` + body `Load preset` duplicate; `Bass` 20px semibold but body 13px grey on `sunken` low contrast (~4.0).
- `settings-general-70pct.png` at 70%: deck LCD `01:09 / 120.0` ~9px, menu 26px bar text ~10px — below AA; `Run as an X11 window… (restart to apply)` wraps to 2 lines inside 480px dialog; `Close` lone button, no `Enter`=Close hint.
- `plugin-wrapper.png`: vendor piano keyboard at bottom clipped by status bar (`audiolatry… .0` fragment); `REVERB/CHORUS/DELAY` caps ~10px on sky background contrast <3:1 (decorative VST art, not ours — but wrapper should dim it 10%).
- Global: `Untitled - unsaved` vs `Untitled` inconsistent (menu bar right); `48.0 KHZ · PPQ 480 · KFXL 0.1.0` faint 11px right block; green `● Channel preset… loaded` vs white solo hint vs green MIDI-drop hint — three status tones for three successes (rule: green = just-done action only — MIDI + preset OK, solo hint should be `Quiet` white).

*Lines: ~150. Files that own almost all of §2: `rack.rs`, `roll.rs`, `playlist.rs`, `mixer.rs`, `browser.rs`+`files.rs`, `plugins_ui.rs`, `sampler.rs`+`instrument.rs`, `deck.rs`, `menu.rs`, `kit.rs`+`theme.rs`, `workspace.rs`+`settings_ui.rs`.*
