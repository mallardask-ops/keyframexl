# KeyFrameXL — дорожня карта функціоналу

Дата: 2026-11-14. Основа: інтерфейс і вікна з `docs/ui-rules.md`, інвентар верхньої частини FL у `research_notes/fl-toolbar-inventory.md` (з посиланнями на мануал Image-Line), стек і етапи з `docs/func-plan.md`, ядро format 2 у `src/core/`.

Правило порядку: спочатку модель, яка не бреше, потім UI для неї. Кожен пункт закривається тестом і нативним скріншотом.

## 0. Зроблено в цьому кроці: канали та власний Sampler

- Окремого «Import DWP» більше немає. Є **канал** (FL: Channel) і його **інструмент**. Наш вбудований інструмент — **Sampler** (аналог DirectWave): `.dwp` — лише один із форматів пресетів, які він завантажує.
- **Add channel** — внизу Channel rack і «+» біля CHANNELS у Browser. Новий канал отримує ім'я `Sampler N` і одразу відкриває своє вікно.
- **Вікно інструмента** (FL: channel settings / plugin window) — плаваюча вкладка `Tab::Instrument`. Ім'я редагується на місці (один крок undo). Load preset, Piano roll, карта зон клавіші × velocity з підказкою по зоні, таблиця зон, звіт конвертора. Порожній стан чесно каже, що ноти грають тестовий тон.
- **Drag & drop**: `.dwp` на вікно → у семплер у фокусі (вікно інструмента чи його Piano roll), інакше новий канал; `.kfxl` → відкрити проєкт. Поки файл над вікном, оверлей каже, що станеться.
- Клік по імені каналу в Rack/Browser відкриває вікно інструмента, правий клік: Piano roll, Load preset, Delete channel (з нотами, один undo). З 2026-11-15 видаляється будь-який канал, включно з барабанами.
- Формати пресетів збираються в одному місці: `PRESET_EXTS` у `src/ui/instrument.rs` + завантажувач у `import.rs`.

## 0.1 Зроблено: рушій на голосах і Sampler v2 (DirectWave)

- **Рушій** (`src/engine.rs`): голоси зі станом, блоковий рендер, події на точному семплі. Той самий код грає наживо і рендерить WAV (тест: live-блоки по 300 = офлайн побайтово). Реліз звучить через стик циклу без подвійного обчислення. Пул 128 голосів, крадіжка найстарішого, без алокацій у колбеку на note-on.
- **Sampler v2** (`src/sampler.rs`, імена як у DirectWave, див. `research_notes/directwave-params.md`):
  Program — Volume, Pan, Transpose, Play Mode Poly/Mono/Legato, Poly (голоси), Glide Time + Fixed/Auto, PB Rng;
  Zone — SEMI ±36, FINE ±50, KTRK ±100, Gain, Panning, Velocity, AMP DAHDSR, Loop Type Disabled/Forward/One Shot/Sustained/Bounce, Reverse, Start, Mute group;
  Filter 1 & 2 — Off/Lowpass/Highpass/Bandpass/Notch/Allpass, Freq, Emph, Serial/Parallel (TPT SVF);
  ENV 1/2; LFO 1/2 — 8 форм DW, Rate, Attack, Phase, Sync/Free Run;
  Modulation Matrix — 16 слотів: LFO 1/2, ENV 1/2, Velocity, Key, Random → Pitch, Gain, Pan, Filter 1/2 freq і emph, LFO 1/2 rate, amount ±100%.
- **Редактор** у вікні інструмента: карта зон (клік — редагувати одну зону), Current zone / All zones як Edit Current/Global у DW, вкладки Program / Zone / Filter & Env / Modulation / Zones, енкодери з логарифмічною шкалою для часу й частоти, графік огинаючої. Кожен жест — один крок undo.
- **Імпорт .dwp** читає з SFZ ConvertWithMoss: фільтр (тип, крутизна → два фільтри послідовно, cutoff, резонанс), огинаючі фільтра й пітча, pitch/amp/filter LFO через матрицю, key/vel tracking, DAHDSR, one-shot, reverse, групи, PB range. Перевірено на справжньому `.dwp` (`tests/fixtures/dwp/probe.dwp`).
- **Не робимо поки** (P2/P3 з дослідження): другий набір програмних LFO, per-voice FX (Ringmod, Decimator, Quantizer, Phaser), посили Delay/Chorus/Reverb, Drive, Time Stretch / Beat Sync / Tick Slice, MiniSynth/Vox, Shape, round robin. ConvertWithMoss однаково губить ENV 1/2, другий фільтр і більшість матриці, тож для повної вірності потрібен свій читач `.dwp` (формат описаний у дослідженні §10).

## 0.2 Зроблено: співочі канали (Vocal)

Канал **Vocal** співає ноти з лірикою голосом UTAU-банку (Kasane Teto, далі будь-який). Рендер: WORLD + правила UTAU, фрази рендеряться у фоні й кешуються за вмістом, рушій грає їх на точному семплі з випередженням на preutterance. Лірика — `Pattern.lyrics`, редагується в Piano roll (подвійний клік, F2, Tools → Lyrics…). Деталі й обмеження: `docs/vocal.md`.

## 1. Канали й інструменти

1. ✅ 2026-11-15 (етап A, `docs/plan-2026-11-15.md`). **Кожен канал має степи.** Зараз три барабани — це фіксовані ряди `drums[3][16]`, а семплер — тільки ноти. У FL будь-який канал має і step sequencer, і Piano roll (степи — це ноти фіксованої довжини). Перейти на `core::model` (format 2, тіки PPQ 480, `Part` на інструмент, id на кожну ноту) з міграцією v1 → v2 біт-в-біт. Тоді барабан = Sampler з one-shot WAV, Add channel → будь-який тип, видаляти можна все.
2. **Типи інструментів** за спільним інтерфейсом `Instrument` (note on/off, render_block, параметри, стан):
   - вбудовані: Sampler (DWP зараз; WAV one-shot, SFZ далі), Drum synth (поточний синтез як пресет), простий субтрактивний синт (func-plan етап 2);
   - плагіни: CLAP (`clack-host`), VST3 (`vst3-host`), опційно yabridge. Вікно плагіна = та сама вкладка `Tab::Instrument`, тільки вміст — GUI плагіна або загальний редактор параметрів.
3. **Channel routing**: канал → mixer track (FL «Target mixer track»). Поле в моделі одразу, UI з мікшером.
4. **Plugin picker / browser**: список встановлених інструментів (вбудовані + скановані плагіни), drag у Rack. Сканування — у окремому процесі (падіння плагіна не валить програму).

## 2. Параметри та автоматизація

Одна система для всього: параметри інструментів, плагінів, мікшера, транспорту (темп).

- **Адреса**: `ParamAddr { owner: Channel(id) | Mixer(track) | Plugin(slot) | Transport, param: StableId }`. `StableId` не змінюється між версіями (для плагінів — їхній param id).
- **Опис**: `ParamInfo { name, unit, min, max, default, curve (lin/log/stepped), group }`. У моделі значення зберігається нормалізованим 0..1, показується в юнітах.
- **UI**: один компонент `kit::knob` / слайдер для будь-якого `ParamAddr` (вже є енкодер TE з кольором-адресою). Правий клік по будь-якому параметру — канон FL: *Edit events*, *Create automation clip*, *Link to controller* (MIDI learn), *Copy / Paste value*, *Reset*.
- **Last tweaked** (FL Tools → Last tweaked): програма пам'ятає останній змінений параметр; з нього — створити кліп автоматизації чи зв'язати з контролером.
- **Автоматизація**: кліпи автоматизації в Playlist (точки + криві, tension), прив'язані до `ParamAddr`. Запис рухів ручок під час Record (фільтр Automation).
- **RT**: зміни йдуть через `rtrb` з точним семплом, згладжування 5–20 мс, жест = один крок undo.

## 3. MIDI

- **Вхід** (`midir`, ALSA seq, hotplug-опитування): гра в канал у фокусі (FL: selected channel), індикатор активності в деці, velocity-крива, sustain, pitch bend, CC.
- **Налаштування пристроїв** (FL Options → MIDI settings): увімкнути порт, прив'язати до каналу, контролер-тип (AKAI — з func-plan).
- **MIDI learn** = *Link to controller* із §2; карти зберігаються в `~/.config/keyframexl/midi_maps.ron`.
- **Запис**: Record + фільтр (ноти / автоматизація), Count-in (Ctrl+P), Metronome (Ctrl+M), Blend/Overdub (Ctrl+B), Wait for input (Ctrl+I), Step editing (Ctrl+E), квантизація на запис.
- **Typing keyboard to piano** (Ctrl+T): клавіатура комп'ютера як MIDI.
- **Файли**: імпорт `.mid` (`midly`) → патерни/канали за треками; експорт патерна чи пісні в `.mid`; Piano roll: імпорт/експорт MIDI для одного каналу.
- Пізніше: MIDI out, MIDI clock master/slave (func-plan етап 3).

## 4. Імпорт і експорт

| Що | Зараз | Далі |
|---|---|---|
| Проєкт `.kfxl` | JSON v1, Open/Save, drag & drop | Save as (Ctrl+Shift+S), Save new version, Recent files, автозбереження + Revert to backup, міграція в format 2 |
| Рендер | WAV пісні або патерна | діалог Render (Ctrl+R): WAV/FLAC, частота, біти, дизер, хвіст, діапазон, стеми по каналах/мікшеру |
| Пресети Sampler | DWP через ConvertWithMoss | WAV one-shot, SFZ, перетягування семплів з файлового браузера |
| MIDI | — | імпорт/експорт (див. §3) |
| Аудіо | — | аудіокліпи в Playlist (func-plan етап 4) |
| Упакований проєкт | DWP-семпли вже лежать у проєкті | «зібрати все в теку» з плагін-станами |

## 5. Верх вікна: меню й тулбар (за інвентарем FL)

Деку не перевантажувати: часті дії — на деці, решта — у **головному меню** зліва у деці (FILE, EDIT, ADD, PATTERNS, VIEW, OPTIONS, TOOLS, HELP), кожен пункт із гарячою клавішею.

**P1 (разом із §1–§3):**
- Меню: New / Open / Save / Save as / Recent / Export; Undo / Redo / History; Add → Channel (типи інструментів); Patterns → New, Clone, Rename (F2), Find next empty (F4), Delete; View → вікна й робочі простори; Options → Audio, MIDI, Project (темп, розмір, PPQ).
- Дека: Pattern selector з +/– (FL), Record + Metronome + Count-in, глобальний Snap (замість окремого на кожен Piano roll), позиція у форматі Bar:Beat:Step з перемиканням на Min:Sec, Monitor volume + пікметр з кліпом, CPU % + поліфонія, MIDI-активність.
- Hint bar уже є (статус-рядок); додати показ гарячої клавіші під курсором.

**P2:** Render діалог (Ctrl+R), Import MIDI/аудіо, Last tweaked, Link to controller / Multilink (Ctrl+J), Master pitch, Plugin manager, автоматизація темпу, Transpose / Split by channel / Group для патернів, Quick tweak.

**P3:** кастомізація деки (пресети панелей), осцилограф/спектр у деці, Chord panel, макроси, пакування проєкту.

**P4 — не робимо:** хмарні сервіси, магазин, онлайн-новини, ліцензійні діалоги FL, специфіку Edison/Fruity.

## 6. Порядок робіт

> Оновлено 2026-11-15: актуальний порядок — етапи A–E у `docs/plan-2026-11-15.md`
> (після дослідження міксера, інтерфейсу, ролу/плейлиста, браузера й контролів). Список нижче — історія.

0. Свій читач `.dwp` (без втрат через SFZ) — коли знадобиться повна вірність.
1. Format 2: ядро + міграція, степи для кожного каналу, видалення будь-якого каналу, `Add channel` з вибором типу.
2. Рушій: зроблено на голосах (§0.1). Лишилось: `cpal 0.18` з нативним PipeWire, `rtrb` для команд замість клонування всієї пісні, пікметр і CPU в деку.
3. Параметри (§2) з UI-компонентом і правим кліком; Last tweaked.
4. Головне меню + Pattern selector + глобальний Snap + Record/Metronome/Count-in у деці.
5. MIDI вхід, learn, запис; імпорт/експорт `.mid`.
6. Render діалог, Save as / Recent / автозбереження.
7. Мікшер (routing каналів), потім плагіни CLAP → VST3.
