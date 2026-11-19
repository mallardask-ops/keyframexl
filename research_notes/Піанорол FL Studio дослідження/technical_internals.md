# FL Studio Piano Roll — Technical Internals

## Data model (notes storage, fields, PPQ resolution)

### Takeaway
FL Studio stores piano-roll note data inside **Patterns**, not channels: each Pattern owns a list of notes (plus controllers and time markers), and a Pattern can be linked to a channel-rack instrument via a channel IID. Note timing is integer ticks against a project-wide PPQ, and each note carries ~15 fields (pitch, time, length, velocity, pan, release, mod X/Y, fine pitch, group, slide/portamento, mute, color/MIDI channel, rack channel).

### Cited Findings
- Notes, controllers and time markers are contained in a `Pattern`; `Pattern.notes` yields MIDI notes — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- A Pattern exposes `iid` (internal index from 1), `name`, `color`, `length` (steps × PPQ, or None in auto/looped mode), `looped` (live loop mode), `controllers`, `timemarkers` — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- `PatternID` event IDs include `Notes=224`, `Controllers=223`, `Name=193`, `Color=150`, `ChannelIID=160`, `Length=164`, `Looped=26` — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- Pattern → channel link is stored as `ChannelIID` (160); a Note stores `rack_channel` = containing channel's IID — [PyFLP Pattern/Note reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- Note fields (PyFLP): `position` (ticks), `length` (ticks; 0 for step-sequencer punches), `key`/pitch (0–131), `velocity` (0–128, default 100), `pan` (0–128, default 64), `release` (0–128, default 64), `mod_x` (0–255, default 128), `mod_y` (0–255, default 128), `fine_pitch` (0–240, ±1200 cents, default 120), `group` (0 = ungrouped), `slide` (bool), `midi_channel` (0–15 colors, +128 for MIDI-dragged notes), `rack_channel` — [PyFLP Note reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- FL Studio's scripting API exposes additional per-note fields: `number` (MIDI note, 48 = C4/C5 depending on version), `time`, `length` (ticks), `group`, `pan` (0.0–1.0), `velocity` (0.0–1.0), `release` (0.0–1.0), `color` (0–15), `fcut`/`fres` (0.0–1.0), `pitchofs` (−120..120, tenths of semitone), `slide`, `porta`, `muted`, `selected`, `repeats` — [FL Studio Piano Roll Scripting API](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_scripting_api.htm).
- Project-wide PPQ exposed read-only as `score.PPQ`; note times/lengths are in ticks — [FL Studio Piano Roll Scripting API](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_scripting_api.htm).
- Time signature numerator/denominator exposed as `score.tsnum`/`score.tsden` (project-level, not marker-level) — [FL Studio Score API](https://maddyguthridge.github.io/FL-Studio-API-Stubs/piano_roll_scripting/flpianoroll/score).
- PPQ is a header field of the FLP file (`uint16_t ppq`) — [PyFLP FLP format](https://pyflp.readthedocs.io/en/latest/architecture/flp-format.html).

### Inferences
- Because note positions are integer ticks and PPQ is project-global, changing PPQ mid-project can shift notes that fall between the new grid (matches manual warning).
- `rack_channel` + `midi_channel` (color) let a single Pattern drive up to 16 routed voices/patches, effectively making the Pattern multi-timbral within one piano-roll view.

### Gaps
- Exact binary layout of the `Notes` (224) sub-event/struct (field order, bit-packing) is not officially documented; PyFLP/FLPKit reverse-engineer it — no primary spec found.
- Whether note data is duplicated per Pattern-clip instance or referenced by IID is not documented (inferred as reference by IID).

## Timing (PPQ, ticks, swing, tempo, time signatures)

### Takeaway
FL Studio uses a single project-wide **Timebase (PPQ)** (default 96, adjustable up to 960 per community reports; manual documents adjustability but not the exact max). Timing is expressed in ticks; tempo is BPM (quarter notes/min); time signatures can be set globally or via markers in the Playlist/Piano roll; swing is primarily a Channel Rack feature (global knob + per-channel Swing Mix) plus Piano Roll groove/quantize templates, not a pure piano-roll property.

### Cited Findings
- Timebase (PPQ) sets event resolution; default 96; higher PPQ = finer control and more CPU; Playlist/pattern zoom limited by PPQ — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- PPQ affects: internal controllers, automation clip nodes, edit events, piano roll note placement, pattern clip placement — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- Lowering PPQ mid-project can move events that fall between the new grid; raising PPQ does not — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- Time signatures: global "Set as time division" (bar/beat) vs "Set as time signature" (numerator/denominator); multiple time-signature markers possible via Piano roll/Playlist (Shift+Alt+T) — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- BPM counts quarter notes/min regardless of denominator; denominator changes beat note value and grid (e.g., 6/8 → 8th-note beats) — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- Global Swing knob in Channel Rack delays even steps; per-channel Swing Mix multiplier; "Truncate swing notes" option — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Swing in newer FL versions also affects piano-roll notes played by a channel; older versions did not — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/isz8un/is_there_a_way_to_add_swing_to_a_piano_roll_drum/).
- Piano Roll swing via Quantize tool (Alt+Q) groove templates (e.g., shuffle presets) and manual Alt-drag off-grid — [Quantize/forums](https://www.theflipsideforum.com/index.php?topic=29402.0).
- Community-reported PPQ range 24–960; 96 default — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/dwcpin/timebase_ppq).

### Inferences
- Because swing is applied at playback/quantize time rather than written into note ticks (for step sequencer), piano-roll notes must be manually shifted or quantized to swing; the Channel Rack knob only delays even 16ths of the pattern's grid.

### Gaps
- Exact PPQ maximum and whether it's 960 or higher is not stated in the official manual page retrieved; community reports say 960.
- Internal swing delay math (e.g., 25% swing = 1/128 note) is community-derived, not official.

## Pattern vs. clip architecture

### Takeaway
FL's piano roll edits a **Pattern** (a reusable note container); the Playlist arranges **Pattern Clips** (instances referencing a Pattern by IID). Editing a clip's pattern edits all instances; "Make unique" clones the Pattern so one instance can diverge. Patterns can be auto-length (looped) or fixed length (steps × PPQ).

### Cited Findings
- Pattern contains notes/controllers/timemarkers and has `looped` (live loop) and `length` (steps × PPQ, or None when auto) — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- "Make unique" creates a new version of an existing clip/pattern in the arrangement via the clip's piano-icon menu — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/n5vvfv/help).
- Playlist places pattern clips; clip length/zoom tied to PPQ — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- "Play truncated notes in clips" restores notes spanning slice points in Pattern Clips (off by default causes tail drop) — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- Patterns have an associated color and channel IID — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).

### Inferences
- Pattern clips are references (by IID), which is why "make unique" must clone the underlying Pattern rather than just the clip.

### Gaps
- Exact Playlist clip struct (clip start tick, pattern IID, track index) layout not retrieved; PyFLP has `Playlist`/`Track` refs not consulted here.

## Channel rack integration

### Takeaway
FL follows a one-channel-per-instrument model: each Channel Rack slot is an instrument with its own settings (routing, polyphony, mono/portamento, gate, swing mix, cut groups). The piano roll can show all channels' notes together, distinguished by **note color** (0–15, mapped to MIDI channel), enabling multi-timbral editing in a single Pattern; per-note color can route through Patcher/VFX Color Mapper to separate instruments.

### Cited Findings
- Notes carry `midi_channel` 0–15 (colors 1–16) for routing; +128 flag for MIDI-dragged notes — [PyFLP Note reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- Color selector in piano roll upper-left assigns note color/MIDI channel; hover shows channel number — [FL Studio colors guide](https://flstudiopro.com/fl-studio-colors).
- Up to 16 note channels/colors in one instrument track via Patcher + MIDI Out/VFX Color Mapper — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/j0tqo8/is_there_any_method_to_make_fl_show_whole_notes).
- Channel settings: target mixer track, polyphony (MAX), Mono, Porta, Portamento time (SLIDE), Gate, Shift, Swing Mix, Truncate swing notes, Cut groups — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Max polyphony: oldest voices are force-released over the limit; Mono mode triggers release state (can overlap); Max Poly overridden by Mono — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Channel Rack "Sort by Color" and per-channel color changes — [Channel Rack manual](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/channelrack.htm).

### Inferences
- Because note color doubles as MIDI channel, FL's multi-timbral piano-roll editing is essentially MIDI-channel multiplexing inside one Pattern, not true multi-instrument tracks.

### Gaps
- Whether the piano roll's channel selector filters by channel IID vs. note color is not explicitly documented.

## MIDI (import/export, learn, controllers, MPE)

### Takeaway
FL Studio supports MIDI file/clipboard import into the piano roll (with track/channel selection, blend, realign, time-sig import) and MIDI export from the piano roll. External controllers are supported via the MIDI controller scripting API. **Native MPE is not supported** in the piano roll as of 2026; Image-Line is reportedly prioritizing MIDI 2.0 over MPE 1.0, with community workarounds (MPE Emulator, hosting synths in plugins like MIDISketch).

### Cited Findings
- MIDI import dialog: choose tracks/channels, Blend with Existing Data, Realign events, Import time signatures, Import zero-velocity notes — [Piano Roll MIDI import](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_midi.htm).
- Export as MIDI file from Piano roll (File menu) — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/adljwc/how_i_copy_the_notes_from_one_midi_to_another).
- MIDI controller scripting API exposes channels, mixer, patterns, playlist, transport, etc. — [FL Studio Python API tree](https://maddyguthridge.github.io/FL-Studio-API-Stubs/piano_roll_scripting/flpianoroll/score).
- No native MPE in piano roll: pitch bend is a channel message affecting all notes on the channel; no per-note pressure/slide editing — [MPE in FL Studio guide](https://eliotonic.com/guides/mpe-fl-studio/).
- Image-Line confirmed MPE will not be added; development aimed at full MIDI 2.0 instead — [MPESYNTH blog (Jul 2026)](https://www.mpesynth.com/blog/fl-studio-skips-mpe-for-midi-2).
- Workarounds: host synth inside MPE-aware plugin (MIDISketch), MPE Emulator plugin, pitch-wheel-per-instance — [MPE guide](https://eliotonic.com/guides/mpe-fl-studio/).
- MIDI learn / controller mapping details not found in retrieved sources — gap.

### Inferences
- FL's existing note-level pitch controls (slide/portamento, pitchofs, mod X/Y) are its substitute for per-note expression, but they are not MPE and don't interoperate with MPE controllers without translation.

### Gaps
- Official documentation on MIDI learn workflow and MIDI 2.0 implementation status/timeline (only third-party reports found).
- Whether FL Studio 2026 added any MIDI 2.0 features (not in release notes retrieved).

## Automation

### Takeaway
FL separates note data from automation: Patterns store **Controller** events (channel/position/value) for per-note/per-pattern parameter automation, and the Playlist hosts **Automation Clips** that can be triggered by piano-roll notes, step sequencer, or live controllers. Piano-roll "Edit events" can be converted to Automation Clips. Channel target mixer track links instruments to the mixer.

### Cited Findings
- Pattern has `controllers` property: `Controller` with `channel` (channel IID), `position`, `value` — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- Automation Clips can be triggered with the Piano roll, Step Sequencer, or live with a controller keyboard; clip automates while note is held — [Automation Clips manual](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/playlist_automationclip.htm).
- Right-click a mixer control → "Edit events" opens event editor; create automation clip from it — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/y08wj6/how_to_go_from_controls_in_piano_roll_to).
- Internal controllers (Fruity Envelope Controller) play back user-specified envelopes from Piano roll/Step Sequencer — [Internal Controllers manual](http://flstudio.com/help/html/automation_internal.htm).
- Channel → mixer routing via "Target mixer track" in channel INS settings — [Audeobox 2026 channel rack guide](https://www.audeobox.com/learn/fl-studio/how-to-use-the-channel-rack-in-fl-studio/).

### Inferences
- Pattern-level Controller events and Playlist Automation Clips are two parallel automation systems; converting between them requires "convert to automation clip" with decimation.

### Gaps
- Exact decimation/quantization behavior and whether Controller events support all automatable params is not documented in retrieved sources.

## Audio rendering (voice triggering, polyphony, voice stealing)

### Takeaway
When a Pattern plays, its notes trigger the assigned channel's instrument; voice count is governed per-channel by **Max Polyphony** (oldest voices force-released over limit) and **Mono** mode (release-state overlap + portamento). Cut groups choke instruments, Gate truncates note length, Shift/Swing delay triggering. Render-time "Disable Maximum Polyphony" ignores per-channel limits so all notes complete.

### Cited Findings
- Max polyphony: forced release of oldest notes over limit; notes stop immediately — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Mono mode: max one note; overlapping notes slide (including cutoff/resonance/pan); overrides Max Poly — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Portamento not supported by VST/AU instruments — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Gate truncates notes to set length; Shift delays triggering up to one step; Swing Mix per channel; Truncate swing notes — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Cut groups silence other channels (e.g., closed hat chokes open hat) — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Render dialog "Disable Maximum Polyphony" disables per-channel limits during export — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/ufpjho/should_i_disable_maximum_polyphony/).
- "Play truncated notes in clips" restores notes spanning clip slice points — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).

### Inferences
- Voice stealing is simple oldest-first release, not priority-based; this can cause audible cutoffs on dense polyphonic material unless polyphony is raised or disabled at render.

### Gaps
- Internal voice allocation/scheduling (sample-accurate timing, render buffer size) not documented.
- Plugin-side polyphony (VST internal) is separate and not controllable by FL's Max Poly.

## Rendering/UI (grid, notes, performance, HiDPI)

### Takeaway
The piano roll draws a grid whose subdivisions depend on snap setting and zoom (resolution can change with zoom when snap is "Line"); scroll speed slows with non-Line snaps. FL Studio handles its own HiDPI scaling; Windows per-app scaling can distort the grid. Performance is tied to PPQ and note count; higher PPQ costs more CPU.

### Cited Findings
- Resolution/grid set by Snap setting; scroll speed slows when snaps other than Line are used; resolution can change with zoom when Snap is Line — [Piano Roll manual](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm).
- PPQ affects CPU; default 96, higher = finer + more CPU — [Project General Settings](https://image-line.com/support/flstudio_online_betamanual/html/songsettings_settings.htm).
- FL Studio does its own scaling; Windows High DPI scaling can cause grid issues; ensure not scaled by Windows — [Reddit r/FL_Studio](https://www.reddit.com/r/FL_Studio/comments/1kp0p79/weird_piano_roll_grid_issue_is_my_monitor_too/).
- HiDPI plugin hosting concerns (host tells plugin DPI scaling) discussed for FL — [JUCE forum](https://forum.juce.com/t/hidpi-and-flstudio/17873).

### Inferences
- Grid rendering is likely vector/line-based tied to the project tick grid, not sample-accurate; visual smoothness limited by PPQ.

### Gaps
- No official documentation of rendering backend (GDI/Direct2D/GPU), frame rates, or HiDPI implementation details.
- Note drawing performance with very large scores not documented.

## Plugin hosting (VST instruments, note expression)

### Takeaway
VST/VST3/AU instruments are hosted as Channel Rack channels and receive piano-roll notes; note color (0–15) maps to MIDI channel, allowing per-note routing to plugin inputs via Patcher. FL's per-note expression is limited to slide/portamento, pitch offset, and mod X/Y; true MPE/per-note expression is not native. VST wrappers expose Input ports; FL native plugins do not, limiting routing into them.

### Cited Findings
- VST/AU plugins hosted as channels; target mixer track routes audio — [Channel Rack/Channel settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Portamento/slide not supported by VST/AU instruments — [Channel Settings Misc](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/chansettings_misc.htm).
- Only VST/AU/CLAP plugins have Input ports in the wrapper; FL native plugins (Slicex, Sytrus, Harmor, FLEX, 3x Osc, Sampler) have none — [MPE guide](https://eliotonic.com/guides/mpe-fl-studio/).
- Per-note fields available to plugins: velocity, pan, release, mod X (fcut), mod Y (fres), pitch offset, slide, porta, color/MIDI channel — [FL Piano Roll Scripting API](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_scripting_api.htm).
- VST note expression / MPE: not native; workarounds required — [MPESYNTH blog](https://www.mpesynth.com/blog/fl-studio-skips-mpe-for-midi-2).

### Inferences
- FL's "note expression" is effectively MIDI-channel multiplexing plus a few per-note modulation lanes, not VST3 Note Expression or MPE.

### Gaps
- Whether FL supports VST3 Note Expression or CLAP per-note params directly is not confirmed in retrieved sources.

## File format (FLP structure relevant to piano roll)

### Takeaway
FLP is a binary TLV format: a header (`FLhd`, size 6, format, num_channels, ppq) followed by a data chunk (`FLdt`) of typed events. Piano-roll data lives in Pattern events (Notes=224, Controllers=223, etc.), with Patterns linked to channels via ChannelIID. The format is proprietary and reverse-engineered (PyFLP/flpkit).

### Cited Findings
- Header struct: magic `FLhd`, size 6, format (int16), num_channels (uint16), ppq (uint16) — [PyFLP FLP format](https://pyflp.readthedocs.io/en/latest/architecture/flp-format.html).
- Data chunk: magic `FLdt`, size, events — [PyFLP FLP format](https://pyflp.readthedocs.io/en/latest/architecture/flp-format.html).
- Events: types 0–63 (1 byte), 64–127 (2 bytes), 128–191 (4 bytes), 192–255 (length-prefixed varint) — [PyFLP FLP format](https://pyflp.readthedocs.io/en/latest/architecture/flp-format.html).
- Pattern event IDs: Notes 224, Controllers 223, Name 193, Color 150, ChannelIID 160, Length 164, Looped 26 — [PyFLP Pattern reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- Note fields as stored (position, length, key, velocity, pan, release, mod_x, mod_y, fine_pitch, group, slide, midi_channel, rack_channel) — [PyFLP Note reference](https://pyflp.readthedocs.io/en/latest/reference/patterns/pattern.html).
- FLP is proprietary/undocumented; reverse-engineered by PyFLP, flpkit, dawtool — [PyFLP docs](https://pyflp.readthedocs.io/en/latest/architecture/flp-format.html); [flpkit](https://github.com/clarkipeng/flpkit).

### Inferences
- Because PPQ is in the header and note positions are ticks, an FLP is self-describing for timing resolution.

### Gaps
- Official format spec does not exist; field-level binary encoding of Notes event is community-reverse-engineered and may lag behind FL Studio versions.
- Project format version number semantics (header `format`) not documented in retrieved sources.

## Undo system scope

### Takeaway
FL Studio maintains a project-wide edit history (Browser → Project tab) with configurable max undo levels; piano-roll edits are recorded as discrete steps (e.g., "Piano roll shift note"). Ctrl+Alt+Z steps back, Ctrl+Z undoes/redoes last edit. Undo of knob tweaks is optional.

### Cited Findings
- "Maximum undo levels" and "Undo knob tweaks" in General settings — [System Settings General](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm).
- Ctrl+Alt+Z step-by-step undo; Ctrl+Z undo/redo last edit — [Keyboard shortcuts](http://flstudio.com/help/html/basics_shortcuts.htm); [Edit menu](https://image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_edit.htm).
- Undo history visible in Browser Project tab; each edit recorded — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/1kvfnb8/how_do_i_undo_this_i_guess_splice/).
- Piano roll Ctrl+Z quirk: often requires Ctrl+Alt+Z (AutoHotkey workaround commonly used) — [Reddit r/FL_Studio](https://reddit.com/r/FL_Studio/comments/1pxsiwd/fix_for_fl_studio_piano_roll_ctrlz_undoredo).
- Changes can be undone via History or Ctrl+Z after MIDI import — [Piano Roll MIDI import](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll_midi.htm).

### Inferences
- Undo scope is project-wide but granularity is per edit action; rapid nudging creates many history entries (users report "Piano roll shift note" spam).

### Gaps
- Whether undo history persists across project save/reopen is not stated.
- Max undo levels default and memory behavior not documented in retrieved sources.
