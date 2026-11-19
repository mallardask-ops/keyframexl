# KeyFrameXL — план перепису (ядро + аудіо + UI)

Дата: 2026-11-14. Три розвідки зійшлися в одному: egui лишаємо, аудіо і модель переписуємо.

## Вердикти

- **UI: лишити eframe/egui, підняти до 0.32+, розпиляти app.rs.** Перепис на iced/Dioxus/GPUI/Slint — 3–6 тижнів без виграшу fps (бенч Zed 09.2026: egui+wgpu 7.03ms ≈ GPUI 6.95ms). Додати `egui_tiles` (докінг як FL), `egui_extras/plot`, `egui_kittest` замість probe-хаків.
- **Аудіо: переписати.** cpal 0.15 застарів, ArcSwap(Song) в колбеку дає кліки, пофреймовий рендер O(семпли×ноти×зони) в 10–50× повільніше блокового, час frame/rate — джитер 3–20мс. Стек: `cpal 0.18 + rtrb 0.4 + basedrop + fundsp 0.23 + midly 0.5 + midir 0.10 + rubato (офлайн)`.
- **Ядро: нове kfxl-core.** Час `start:u8` → тики `i64` PPQ 480, drums+notes злити в Part per Instrument, ID на все, command stack undo, міграція v1→v2 біт-в-біт.

## Фази

### 1. Ядро (зараз, без зупинки UI)
- `src/core/time.rs`: TICKS_PER_BEAT=480, step=120, bar=1920, quantize.
- `src/core/model.rs`: Project(format=2)/Instrument/Pattern(len_ticks)/Part/Note(id,start,len,pitch,velocity,muted,slide_group,color,channel)/Slide/Automation/Clip(id,pattern,start_ticks).
- `src/core/select.rs`: Selection + Clipboard.
- `src/core/cmd.rs`: Command enum + власний Record (без зовнішнього undo-крейта — у registry `undo 0.52` не той API, щоб не тягнути плутанину) з merge по gesture-id. Preview не пише в історію.
- `src/core/migrate.rs`: untagged enum V2/V1, drums→ноти pitch 36/38/42, tick=u8*120, clip=bar*1920, tempo 120.
- DoD-тести: міграція біт-в-біт, 120 тік/крок, тріоль 160 + offgrid 7, MIDI roundtrip ±0, one gesture=one undo, per-voice velocity, erase однієї ноти, slide-ланцюг C3→C4→G3, старі файли без format відкриваються, invalid відхиляється.

### 2. Аудіо (після ядра)
1. cpal 0.15→0.18 (SampleRate(u32), device.id, host PipeWire/JACK/ALSA).
2. `engine::render_block` чиста функція, без Song в колбеку.
3. rtrb UI→RT (4096) + basedrop, VoicePool 32–64, sample_clock монотонний, позиція в UI раз на блок.
4. Блоковий DSP: драмс на fundsp-ноди, семплер — кубічна інтерполяція + rubato-ресемпл на завантаженні, кросфейд 2–5мс на ретригер/луп.
5. Musical time: tick→sample через TempoMap, lookahead 2 блоки, свінг як перетворення часу.
6. MIDI import/export (midly) → live вхід (midir) → плагіни (nih-plug, за флагом).

### 3. UI (після ядра, паралельно з аудіо)
- Розпиляти app.rs (~1350 рядків) → app.rs + ui/{state,commands,transport,sidebar,rack,roll,playlist,inspector,paint} + audio_bridge.
- Viewport зі своїм id_salt, culling видимих нот, зум Ctrl+wheel X/Y окремо, плейхед окремим шаром, repaint тільки в playing.
- egui_tiles докінг, egui_plot velocity lane, widget_info для скрінрідера + kittest.

## Порядок
1→2.1→2.3→3→2.4→2.5→2.6. Перші три дають безкліковий транспорт і прибирають CPU-костиль, решта — якість і фічі.
