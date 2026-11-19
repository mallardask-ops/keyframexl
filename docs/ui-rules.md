# KeyFrameXL — UI Rules & Component Reference

This document defines the visual design system, interaction standards, and widget contracts for KeyFrameXL.

It combines three deliberate influences:
1. **BMS Design Kit** — dark/light surface hierarchy, restrained color semantics, IBM Plex typography, two-depth physical surfaces, and accessibility rules.
2. **Teenage Engineering (late-era hardware)** — dense functional modules, color-as-address coding, crisp 1 px seams, tabular numbering, and zero skeuomorphic clutter (no decorative screws, straps, tiny 9 px OLED fonts, or secret shift combos).
3. **FL Studio 21/24 Workflow Canon** — muscle-memory shortcuts, pattern-based Channel Rack / Piano Roll / Playlist / Mixer conventions, and right-click gesture contracts.

---

## 1. Core Principles

Every screen and control should pass four checks:

1. **Healthy state is silent.** Visual indicators only appear when something requires attention or is actively engaged.
2. **The interface never lies.** Never disguise an empty step pattern as musical notes; state unknowns explicitly.
3. **One obvious next action.** At most one red hero button (`brand`) per viewport (Play / Record).
4. **Only changing data moves.** Playheads and audio meters animate smoothly; no decorative bounces, parallax, or idle glows.

### Explicit Anti-Patterns (Do Not Use)
- Hardcoded `Color32::from_*` values outside `src/theme.rs`.
- Explanatory hint paragraphs pasted into the canvas as a crutch for missing affordances.
- Permanent green "everything is OK" status dots (green is reserved strictly for a just-completed action).
- Neon glows, glassmorphism/blur, decorative background grids, pill-shaped buttons, nested panels-inside-panels, or icon-library chrome on every button.
- Fake/placeholder stats or native browser/OS `confirm()` dialogs.

---

## 2. Color Tokens (`src/theme.rs`)

All colors live in `src/theme.rs`. Dark is the primary studio theme (Light is the planned secondary theme).

| Role | Tokens |
| --- | --- |
| **Surfaces (darkest → lightest)** | `edge #040505` · `sunken #0C0D0E` · `side #0F1011` · `bg #141516` · `surface #1A1B1D` · `raised #222326` · `selection #2A2C2F` |
| **Panels & Controls** | `panel_top #222427` / `panel_bottom #202225` · `ctl_top #323438` / `ctl_bottom #2B2D30` / `ctl_hover #43474C` |
| **Lines & Shading** | `line #2D2F32` · `input_line #34373B` · `strong #6A6D72` · `hi` (white 12%) · `low` (black 60%) · `shadow` (black 80%) |
| **Text & Focus** | `text #E9EAEB` · `muted #A3A6AB` · `faint #A0A3A8` · `focus = text` |
| **Brand (Hero Accent)** | `brand #F0584C` · `brand_top #E5392B` · `brand_fill #D63426` · `brand_glow #F04E3F` · `brand_edge #6E140D` · `brand_soft #2B1A18` · `on_brand #FFFFFF` |
| **Semantic Status** | `ok #5FCF8F` · `wait #E5B64E` (`wait_soft #2A2316`) · `bad #F2766B` (`bad_soft #2E1B19`) |
| **Encoder Address Colors** | `enc_blue #5C9AFF` · `enc_green #5FCF8F` · `enc_white #D6D8DB` · `enc_orange #FF8A3D` |

### Color Rules
- **Brand red** is used exclusively for the hero transport action, active steps, and owned notes. Playback position uses a crisp white frame/caret, not red.
- **Color is never the sole indicator:** mute/solo/arm/active states always pair color with shape or lettering (`M`/`S`/`R`, filled vs. outlined).
- **Color = address, not status:** Encoder colors (`blue`, `green`, `white`, `orange`) and pattern codes identify *which* control or pattern you are looking at, not whether it is healthy or failing.
- **Contrast:** Maintain WCAG AA (`≥ 4.5:1` for normal text). On `brand_top`, use `on_brand` text at medium weight (`500`) and `≥ 14 px`.

---

## 3. Typography (`IBM Plex` only, bundled locally)

Bundled in `assets/fonts/` (`IBMPlexSans` + `IBMPlexMono`). Allowed weights: **400, 500, 600**. No `700` bold and no italics. System CJK fonts are loaded dynamically via `fc-match` only for kana/kanji lyrics and voicebank names.

| Role | Face & Size |
| --- | --- |
| Page / Window title | IBM Plex Sans `20 px` / `600` (line-height `1.3`) |
| Section heading | IBM Plex Sans `15 px` / `600` |
| Body & regular buttons | IBM Plex Sans `14 px` / `400` |
| Primary action button | IBM Plex Sans `14 px` / `500` |
| Secondary row / menu | IBM Plex Sans `13 px` / `400` |
| Captions & headers | IBM Plex Sans `12 px` / `400` |
| Engraved module labels | IBM Plex Mono `10–12 px` uppercase, tracking `0.12–0.18 em` (never for prose) |
| Machine values (`Bar:Step:Tick`, BPM, IDs, dB, ms, Hz) | IBM Plex Mono `13 px` / `400` (`tabular-nums`); logs `12 px` |
| Large numeric readout | IBM Plex Mono `20 px` / `500` |

---

## 4. Layout, Depth & Motion

### Surface Depth (Two Depths Only, Overhead Light)
- **Raised** (buttons, active tabs, outer panels): `1.5 px` `edge` outline + `2 px` bottom lip + `ctl_top → ctl_bottom` gradient + `1 px` `hi` top highlight.
- **Sunken** (LCD wells, input fields, slider tracks, disabled buttons): `sunken` fill + `1.5 px` `edge` (`2 px` `input_line` for inputs) + `1 px` `low` inner top shadow.
- **Pressed state:** Instantaneous (no spring/bounce) — bottom lip collapses, face shifts down `2 px`, gradient inverts.
- **Performance rule:** Drop shadows are only drawn on outer panels, dialogs, and toasts — never on hundreds of individual step pads or piano-roll notes (which use flat fills + `1 px` borders).

### Sizing & Radii
- **Radii:** buttons/inputs `7 px`, thumbs `6 px`, tracks `8 px`, panels/dialogs/toasts `10 px`, progress bars `5 px`, step pads/checkboxes `4–6 px`. No pill radii.
- **Heights:** standard buttons `34 px`, transport controls `34×34 px` (minimum hit target `≥ 24 px` in dense grids), deck toolbar buttons `26 px` (`TOOL_H`), rack rows `30 px` (pads `22 px` tall, `22–34 px` wide), playlist rows `32 px`, browser rows `24 px`.
- **Minimum window size:** `1180×680` (default `1366×768`). UI scale adjustable via `Ctrl+=` / `Ctrl+-` / `Ctrl+0` (`80%–150%`).

### Motion & States
- **Transitions:** Hover/focus `120 ms`, tab thumb slide `200 ms` ease-out, toast/dialog entrance `150 ms`. Playheads and meters draw directly per frame with zero easing.
- **Five required screen states:** *Data*, *Loading* (brief inline text, no skeletons), *Empty* (one clear sentence + next-action button), *Error* (what happened + how to fix it), and *Partial* (`wait` tone stating what succeeded and what was skipped).
- **Accessibility:** Full keyboard navigation matching visual order, `1.5 px` `focus` ring (`2 px` gap) on keyboard focus, tooltips on all icon-only buttons, and `Escape` closes popups/dialogs.

---

## 5. Application Shell (`src/ui/`)

```text
┌ menu (26px) ── [K] FILE EDIT ADD PATTERNS VIEW OPTIONS TOOLS HELP ─── Untitled · unsaved ┐
├ deck row 1 (48px) ───────────────────────────────────────────────────────────────────────┤
│ [▶ red][■][●] [Pattern|Song] [LCD BAR:STEP │ BPM] [Tap] [Metronome][Count-in][Blend] CPU │
├ deck row 2 (34px) ───────────────────────────────────────────────────────────────────────┤
│ [Playlist][Rack][Roll][Mixer][Browser] [Snap 1/16 ▾] [−][01 Pattern 1 ▾][+]   hint bar…  │
├ browser (208px) ┬ dock (egui_dock) ──────────────────────────────────────────────────────┤
│ Project | Files │ ┌ Playlist ────────────────────────────────────────────────────── × ┐  │
│  Instruments    │ │ ▾ header context                                 [tool buttons]   │  │
│  Effects (32)   │ │ tracks & clips                                                    │  │
│  Tide / Presets │ ├ Channel rack ────────────────────────────────────────────────── × ┤  │
│  Folders        │ │ ▾ 01 PATTERN 1                                       SWING (knob) │  │
│                 │ │ M/S · Pan/Vol · Mix · Name │ BEAT 1 │ BEAT 2 │ BEAT 3 │ BEAT 4    │  │
├ status (28px) ──┴────────────────────────────────────────────────────────────────────────┤
```

### Window & Workspace Contract
- Every editor is a dockable tab (`Playlist`, `Channel rack`, per-instrument `Piano roll`, `Instrument`, `Mixer`). Dragging a tab into a pane groups it, to an edge splits the pane, or outside floats it (`Right-click → Float / Dock`).
- `F5` / `F6` / `F7` / `F9` follow FL Studio toggle rules: closed → open in default position; open but unfocused → focus; focused → close.
- Three workspace presets (`Arrange`, `Compose`, `Edit` via `Alt+1..3`) each remember user customizations and save to `~/.config/keyframexl/workspace.ron`.
- **Unsaved changes guard:** New, Open, drag-opening a `.kfxl`, and closing the app prompt *Save / Don't save / Cancel* whenever the project is dirty.

### Editor Contracts

- **Header bar (`App::panel_header`):** Every editor starts with a `▾` window menu (ending in *Detach/Dock* and *Close*), uppercase context label, right-aligned tools, and a `1 px` seam.
- **Deck & Knobs (`kit::knob`):**
  - Vertical drag = full travel in `200 px` (`Shift` for 10× fine precision).
  - Mouse wheel = `1%` per detent (captured so the enclosing scroll area does not scroll).
  - `Up`/`Down` arrows when focused; `Double-click` or `Ctrl+click` resets to default.
  - `Right-click` menu: *Reset*, *Type in value* (parses `%`, `Hz`/`kHz`, `ms`/`s`), *Copy / Paste value*, *Create automation clip*, and *Link to controller* (MIDI learn).
  - Each complete gesture (drag, wheel burst within `0.6 s`, or typed entry) records a single Undo step. Time and frequency knobs use logarithmic scaling (`log: true`).
  - LCD BPM readout also responds to vertical drag (`0.2 BPM/px`, `Shift` `0.02`), scroll wheel (`±1`, `Shift` `±0.1`), double-click to type, and right-click to reset to `120`.
- **Channel Rack (`src/ui/rack.rs`):** 4-step beat trays with a `10 px` gap, white playhead LED row above the pads, red active pads with `2 px` bottom velocity bar, and `230 px` channel header (green/blue mute/solo LED, `18 px` Pan/Vol knobs, target mixer track selector, and channel button). Channels with off-grid/melodic notes display an inline note preview strip that opens the Piano Roll on click.
- **Piano Roll (`src/ui/roll.rs`):** `48 px` piano keyboard (`key_white` / `key_black` at 62% width), `24 px` beat ruler, `88 px` bottom control lane (*Velocity / Pan / Release / Fine / Mod X / Mod Y*, auto-hidden below `260 px` pane height). `Ctrl+wheel` zooms X around cursor (`4–160 px/cell`), `Alt+wheel` zooms Y (`8–30 px/row`), `Shift+wheel` scrolls horizontally. Note edges have a `7 px` resize handle on both ends; `Alt` temporarily bypasses snap.
- **Playlist (`src/ui/playlist.rs`):** `120 px` track headers (`32 px` rows) with mute lamps. Clips span their exact tick duration (`2 px` corner radius, no gaps or pill margins) with a `13 px` colored title header (`▾` menu, name, and `×N` shared-instance badge) above a real-time note preview. Extending a clip past its pattern length loops the pattern with faint repeat ticks.
- **Mixer (`src/ui/mixer.rs`):** Floating window (`F9`) with horizontally scrollable `64 px` strips (Master first, arm-for-record `●`, mute/solo lamp, pan/stereo-sep knobs, dB fader `−60..+6 dB` with peak meter, Master route toggle) and a pinned `320 px` right-hand inspector (source channels, 10 FX slots with enable LED + mix + editor, 3-band strip EQ `±18 dB`, and loop-free sends to higher-numbered inserts).
- **Browser (`src/ui/browser.rs`, `src/ui/files.rs`):** `Project | Files` tabs. Files tab filters to supported formats (`wav`, `flac`, `ogg`, `dwp`, `kfxl`), sorts by *Name / Type / Newest / Largest*, runs background recursive search, auditions audio on click (`Esc` stops), and supports drag-and-drop onto channels or empty rack space.
- **Status Bar:** Uses explicit semantic tones (`Tone::Quiet`, `Done`, `Wait`, `Bad`). Long messages truncate with `…` before the right-hand technical legend (`48 KHZ · PPQ 480 · KFXL`) and show full text on hover.

---

## 6. Component Library (`src/ui/kit.rs`)

All custom controls must be built with `src/ui/kit.rs` primitives:

| Helper | Purpose | Behavior / Rule |
| --- | --- | --- |
| `raised` / `control` | Beveled interactive surface | `1.5 px` edge, `2 px` bottom lip, vertical gradient, `hi` top line; flattens & inverts on press |
| `sunken` | Recessed well | Darkest surface (`sunken`), inner top shadow (`low`) |
| `panel` | Outer container block | 6-step soft shadow, `10 px` radius, `1 px` lip |
| `thumb` | Active selection / tab indicator | `6 px` radius, `1 px` lip |
| `button` / `button_sized` | `Primary` / `Normal` / `Ghost` button | `34 px` height; disabled state is sunken with `faint` text |
| `tool_button` / `dropdown` | Compact toolbar controls | `26 px` (`TOOL_H`) for deck row 2 and panel headers |
| `chips` | Segmented toggle control | Animated thumb slide (`200 ms`) inside sunken track |
| `knob` / `fader` | TE rotary encoder & mixer fader | `270°` arc, address color, full keyboard/wheel/right-click contract |
| `seg_text` | 7-segment LCD digits | Unlit ghost segments remain faintly visible (`lcd_off`) |
| `led` | Status / mute / playhead lamp | Clean flat dot, no glow halo |
| `caps` | Engraved module label | Uppercase IBM Plex Mono, `0.12 em` tracking, `10–11 px` |
| `icon_brand` | Step Matrix K app mark | Vector brand mark scaled optically for `16 px` menu bar and `48 px` About dialog |
