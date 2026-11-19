# DirectWave — інвентар параметрів для KeyFrameXL Sampler

Джерела: офіційний мануал Image-Line (одна сторінка з якорями) та код ConvertWithMoss (реверс формату .dwp).
Базова сторінка мануалу: https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/DirectWave.htm
Код формату: https://github.com/git-moss/ConvertWithMoss (каталог `src/main/java/de/mossgrabers/convertwithmoss/format/directwave/`).

Примітка про нотацію: DirectWave вживає Image-Line-октави, де middle C (MIDI 60) — це **C5**, негативних октав немає
(див. `DirectWaveFileNameParser.java`). Назви параметрів нижче — дослівно як у DirectWave.

Мануал майже ніде не дає одиниць, діапазонів і дефолтів ручок (крім випадків, зазначених явно) — це позначено «манул мовчить».

## 1. Program: рівень програми, поліфонія, portamento, pitch bend

Джерело: `DirectWave.htm#DirectWave_ProgramTab`, `DirectWave.htm#DirectWave_MultiBankWindow`.

- **Play Mode**: `Poly` (поліфонія, більше однієї ноти) / `Mono` (моно, одна нота) / `Legato` (моно з portamento). Рівень: program.
- **Volume**: загальна гучність програми. Рівень: program. Діапазон/дефолт: мануал мовчить («It's a volume knob»).
- **Glide (Portamento)**: слайд між нотами. Параметри: **Time** (час слайду 0.001–10 с), **Fixed** (portamento завжди),
  **Auto** (portamento лише на legato-нотах, тобто перекритих). Рівень: program.
- **PB Rng**: pitch bend range у центах, задається подвійним кліком у Multi-Bank Window на MIDI-канал. Рівень: multi/bank.
  Дефолт: мануал мовчить.
- Multi-Bank Window (видимий лише коли увімкнено **Multitimbral Mode**; до 16 програм на 16 MIDI-каналах, вихід на окремі
  мікшерні канали через **Output**): також **Pitch Bend**, **Mod Wheel**, **Volume**, **Panning**, **Expression**
  (множник гучності), **Mute**/**Solo**, **Ch.**, **Index**, **Name**. Мануал мовчить про діапазони.
- **FX Drive A & B — Amount (%)**: овердрайв/дисторшн програми. Рівень: program. Крива/дефолт: мануал мовчить.

## 2. Zone mapping: клавіші, velocity, root, tune

Джерело: `DirectWave.htm#DirectWave_ProgramWindow` (таблиця зон + Zone Mapper + контекстне меню).

- **Zone** — числовий номер семпла; **Zone Name** — ім'я (клік для перейменування); **Sample Size (S.SIZE)** — довжина в семплах;
  **Sample Rate (S.RATE)** — частота семпла (будь-яка, прямий playback); **Voice Count (V)** — лічильник голосів (read-only).
- **Low Key (LKEY)** / **High Key (HKEY)** — нижня/верхня межа key range (MIDI-ноти). Правка також drag-маркерами Zone Mapper.
- **Low velocity (LVEL)** / **High velocity (HVEL)** — межі velocity range 0–127. Правка drag-ом по вертикалі в Zone Mapper.
- **Root** — MIDI-номер root key (нота, де семпл грає з записаною висотою). Зміна: правий клік по клавіатурі прев'ю
  (помаранчевий маркер). Drag з **Ctrl** зсуває тільки зону, з **Ctrl+Alt** — зону разом із Root.
- **Transpose in Semitones (SEMI)** — транспозиція ±36 семітонів. Рівень: zone.
- **Fine tune (FINE)** — підстройка ±50 центів. Рівень: zone.
- **Keyboard Track (KTRK)** — трекінг клавіатури, +100…−100 (100 = 100 центів/семітон, 0 = без трекінгу). Рівень: zone.
- **Mute** — exclusive mute group; тригер семпла з group > 0 глушить граючі голоси тієї ж групи (приклад: закритий/відкритий хет).
- **Ticks** — довжина семпла в тіках (4 тіки = 1 біт); впливає на роботу **Sync**.
- **Beat Sync (SY)** — чекбокс: гра на root key підганяє пітч під темп хоста.
- **Tick Slice (SL)** — грає слайс семпла залежно від ноти відносно root key. Деталі: мануал мовчить.
- **Lock (LK)** — блокує параметри зони від редагування.
- Контекстне меню зон: **New/Delete/Duplicate Zone**, **Add/Replace Sample/s**, **Automap Zone/s** (мапить за embedded
  key-range/loop у .wav), **Remap Zones/s as Drumkit** (розкласти по сусідніх семітонах), **Lock/Unlock all**,
  **Find pitch root** (детект висоти семпла).
- Імена файлів як мапінг (`Create program from samples` + опція **Extract root key from filename**):
  `samplename_C4+KA3-F#4+V64-95+TG1+TY3+TF100+TO15.wav` задає root **C4**, key range **A3–F#4**, velocity **64–95**,
  trigger group **#1**, type **avoid previous**, frequency **100%**, overlap **15%**. Токени: нота A–G = root;
  `K` = key range; `V` = velocity range; `TG` = group 0–99; `TY` = type 0–3 (normal, cycle, random, avoid previous);
  `TF` = frequency 0–100; `TO` = overlap 0–100.

## 3. Sample: старт, лупи, reverse

Джерело: `DirectWave.htm#DirectWave_SampleTab`.

- **Start** — позиція старту семпла (червоний маркер), ручка + точне підведення зумом.
- **Loop Start** / **Loop End** — дві точки лупу (чорні маркери); Start можна потягнути за End (це «точки A і B», не порядок).
- **Loop Type**: **Disabled** (без лупу, маркери приховані); **Forward** (повтор стрибком на Loop Start);
  **One Shot** (грає від старту до кінця незалежно від довжини ноти, для перкусії);
  **Sustained** (луп тримається, поки натиснута клавіша, після release грає хвіст за Loop End — потрібен ненульовий
  Release огинаючої); **Bounce** (луп уперед-назад). Кросфейду лупу в ручному режимі немає (тільки через Channel Sampler).
- **Input / Sampling** (лише VST-версія в FX-слоті мікшера): **Monitor** (слухати трек), **440 Hz** (тест-тон A4 для стрóю),
  **Record** (запис починається з появою сигналу). Автосемплінг VST — див. розділ 9.
- Wave-редактор: повний набір cut/copy/paste (insert/replace/mix/envelope/spectral/stretch/drum), **Reverse**,
  **Normalize**/**Lossy normalize**, **Fade in/out**, **Declick in/out**, **Center** (DC offset), **Tune Loop** (Ctrl+L,
  підгін точок лупу до zero-crossing), **Trim**, **Delete part after loop**, **Reverse polarity**, генерація шуму,
  скрипти, гейт шуму, конволюційний реверб/blur/EQ. Для семплера важливий лише **Reverse** + **Tune Loop**.

## 4. Amp: гучність, velocity, огинаюча

Джерело: `DirectWave.htm#DirectWave_ZoneTab` (Main; AMP, ENV1 & ENV2).

- **Gain** — мапить семпли в просторі pitch/velocity (формулювання мануалу; фактично гучність зони). Рівень: zone.
- **Panning** — панорама зони: ліворуч −50 … праворуч +50. Рівень: zone.
- **Post Gain** — індивідуальна гучність семпла; разом із **Send** задає dry/wet глобального FX. Рівень: zone.
- **AMP** — виділена огинаюча гучності ADSR: **Attack (A)**, **Decay (D)**, **Sustain (S)**, **Release (R)**.
  A/D/R — швидкість (ручка вниз = швидко), S — рівень (вниз = тихо). Секунди/дефолти: мануал мовчить.
- **ENV1**, **ENV2** — дві вільно призначувані огинаючі з тими ж стадіями A/D/S/R. Цілі — лише через Modulation Matrix.
- Velocity→gain: явної ручки «velocity sensitivity» у Zone Tab немає; velocity працює через шари (LVEL/HVEL) і опцію
  семплера **Link velocity to gain** (див. розділ 9). Глобальні перемикачі поведінки velocity — **Compatible velocity**,
  **Clip volume** (розділ 8).

## 5. Pitch: tune, key tracking, pitch envelope

Джерело: `DirectWave.htm#DirectWave_ZoneTab` (Main; Modulation Matrix).

- **Pitch Tune** — підстройка висоти зони. Діапазон: мануал мовчить (на відміну від SEMI ±36 / FINE ±50 у таблиці зон).
- **Keyboard Track (KTRK)** — див. розділ 2 (це і є key tracking).
- Виділеної pitch-огинаючої немає: пітч модулюється через **ENV1/ENV2 → Modulation Matrix → pitch** або через LFO.
  Список усіх modulation targets мануал не наводить (показані лише приклади; перший банк матриці «за дефолтом контролює
  базові нотні параметри» без переліку). Повний список цілей/джерел: мануал мовчить.

## 6. Filter: типи, cutoff, резонанс, трекінг, огинаюча фільтра

Джерело: `DirectWave.htm#DirectWave_ZoneTab` (Filter 1 & Filter 2).

- Два фільтри зі змінним станом, конфігурація **паралельно або послідовно** (перемикач-іконка; деталі маршрутизації: мовчить).
- **Type** (на фільтр): **Off** (bypass), **Lowpass**, **Highpass**, **Bandpass**, **Notch**, **Allpass**,
  **MiniSynth**, **Vox** (вокальний формантний). Параметри MiniSynth — у мануалі MiniSynth, не DirectWave.
- **Frequency** — cutoff для lowpass/highpass/bandpass. Гц/дефолт: мануал мовчить.
- **Emphasis** — підйом смуги біля cutoff («sharpening»). Шкала: мануал мовчить.
- **Shape** — hardclip-лімітування (дисторшн) на виході фільтра. Крива: мануал мовчить.
- Key tracking фільтра окремою ручкою не показаний; огинаюча фільтра — через **ENV1/ENV2 → Modulation Matrix → cutoff**.
  Мануал мовчить про список цілей (див. розділ 5) і про те, чи трекає cutoff клавіатуру за дефолтом.

## 7. LFO: кількість, форми, rate, sync, delay, призначення

Джерело: `DirectWave.htm#DirectWave_ZoneTab` (LFO1 & LFO2; Modulation Matrix), `DirectWave.htm#DirectWave_ProgramTab` (LFO 1 & LFO 2).

- **Zone LFO1 / LFO2** (per-zone, вкладка Zone) і **Program LFO 1 / LFO 2** (глобальні, вкладка Program) — разом 4 LFO.
  Параметри однакові: **Rate** (швидкість; одиниці: мануал мовчить), **Sync** (старт LFO з note-on),
  **Attack** (час появи модуляції, від миттєво до 10 с затримки — єдиний параметр із одиницями в мануалі),
  **Phase** (стартова фаза; крайнє праве = **Free Run**, LFO незалежний від note-on), **Waveform**.
- Форми (8): **Sine**, **Abs Sine** (позитивна), **Triangle**, **Square**, **Saw**, **Inv Saw**, **Random** (шум),
  **LP Random** (фільтрований шум).
- Призначення — лише через **Modulation Matrix 1–4**: 4 банки по 4 слоти (16 слотів), кожен: джерело (верхнє поле) →
  ціль (нижнє поле) → ручка кількості −100%…+100%. Спецджерела: **Mod Amt** (зв'язок матриці з собою — мультиплікатор
  кількості іншого слота), **Mod Val** (зовнішні джерела, автоматизуються через Plugin Menu → Browse Parameters).
  Повний список джерел/цілей: мануал мовчить.

## 8. Modulation sources / matrix, FX, Browser/Settings

Джерело: `DirectWave.htm#DirectWave_ZoneTab` (Modulation Matrix, Effects, Edit/Automation),
`DirectWave.htm#DirectWave_Options`, `DirectWave.htm#DirectWave_LibraryWindow`.

- **Edit/Automation: Current / Global** — чи правки Zone Tab стосуються вибраної зони чи всіх зон (Global перезаписує!).
- Per-voice FX (унікальні на зону): **Ring Modulation (Rate, Mix)**, **Decimator (Step, Mix)**, **Quantizer (Bits, Mix)**,
  **Phaser (Frequency, Mix)**; у кожного тумблер активації. Шкали: мануал мовчить, крім крайніх позицій (Full Left = 0/stop).
- **Send: Delay, Chorus, Reverb** — посили на глобальні FX (вкладка Program), дефолт 50% (−12 dB), за дефолтом вимкнені.
- Глобальні FX (вкладка Program): **Delay** (Delay, Feedback, Low Cut, High Cut, **Bounce** — стерео-скачки / **Normal** —
  моно); **Reverb** (Room/size, Damp/high-freq damping, Diffusion, Decay до −60 dB); **Chorus** (Delay 0–20 мс,
  Depth 0–20 мс, Rate, Feedback). Дефолти пресетів: мануал мовчить.
- Library/Browser: **Library location (Home)**, **Parent**, **Refresh**, завантаження подвійним кліком; імпорт сторонніх
  форматів (AKAI AKP, Battery 1–3, EXS24, Giga, Kontakt/2, Kurzweil, MPC, NN-XT, Recycle, SoundFont2, SFZ) правим кліком
  по **Open**; у SoundFont імпортуються лише базові параметри (key/velocity range, loop points — без cutoff/chorus/reverb).
  Важливо для нас: **.dwp не зберігає wave-дані** — семпли лежать у підпапці з ім'ям .dwp (немонолітний режим);
  колекція до 16 програм = **Bank (.dwb)**; збереження: **Save / Save as / Save as variation / Save for FL Studio Mobile**
  (.dwp у Monolithic mode, сумісний з FL Studio Mobile), **Flush / Flush all**.
- Options: **Content Library** (папка бібліотеки/дефолт збереження); **Multitimbral Mode**; **Use tablet PC or digitizing
  stylus**; **Switch program channel** (MIDI-канал program change, дефолт '---' = off); **16-Point sinc interpolation**
  (дешевша, більше aliasing); **Monolithic mode** (семпли всередині .dwb / вшивання пресету у .flp);
  **Show note names in host**; **High Quality Rendering** (автоматизація майже на audio-rate, потрібно для швидких
  модуляцій фільтра); **Pitch analysis after sample load**; **Auto normalize samples**; **Extract root key from filename**
  (root з кінця імені: `Strings_C4.wav` або `Strings - C4.wav`); **Show load progress**; **Compatible velocity**;
  **Clip volume** (обидва — лише для старих банків, новим лишати off).

## 9. Recording / Editing / Auto-map / Slicing / Channel Sampler

Джерело: `DirectWave.htm#DirectWave_SampleTab`, `DirectWave.htm#DirectWave_ChannelSampler`.

- Редактор семплів — див. розділ 3 (набір рівня Edison, але без piano-roll).
- **Automap Zone/s** — автомапінг за embedded key-range/loop-даними .wav; зв'язка з Edison
  (**File → Export regions → For sampler use**) для нарізки довгих записів.
- **Channel Sampler** («Create DirectWave instrument»): робот автосемплінгу плагінів. Параметри семплінгу:
  **Low note**, **High note**, **Key zones** (ширина зони), **Max velocity**, **Velocity layers** (кожен шар = множення
  розміру патчу), **Root offset**, **Link velocity to gain**, **Cycle layers** (round-robin варіації),
  **Stop on: Max length / Silence** (−96 dB), **Max length** (с/такты/біти), **Release after**; якість: **Resampling**,
  **Bit depth** (16 / 32-bit float), **Stereo mode** (Stereo/Mono L/Mono R/Mono Merged), **HQ for all plugins**,
  **Disable maximum polyphony**, **Dithering** (shaped, >10 кГц), **Trim PDC delay**, **Declick in/out** (фейд 20 мс),
  **Normalize**; луп: **Enable**, **Start point** (с/біти/такти), **Crossfade** з **Length/Tension/Snap** (pitch-detection
  проти фазових гасінь); інше: **Enable master/insert effects**, **Monolithic file**, **Open in DirectWave**.

## 10. Що зберігає .dwp і що читає ConvertWithMoss

Джерела: реліз ConvertWithMoss 20.1 («read + write FL Studio DirectWave (DWP): key/velocity ranges, gain, panning,
loops, amplitude envelope, filter, pitch and volume LFOs, monolithic programs»,
https://github.com/git-moss/ConvertWithMoss/releases),
`DirectWaveChunk.java`, `DirectWaveTag.java`, `DirectWaveDetector.java`, `DirectWaveCreator.java`,
`DirectWaveFileNameParser.java` (усі — `format/directwave/` у репозиторії вище).

- Формат: магік `DwPr`, фіксована преамбула (0x5A байт для версій **0x25** — FL Mobile, **0x26** — FL Desktop 20.x;
  байт версії за offset 4; розмір файлу−48 за offset 0x28; кількість зон за offset 0x4A), далі потік блоків
  «tag(u32) + length(u32) + reserved(u32) + payload». Ім'я інструмента (0x0066), шлях (0x0067), метадані (0x006A–0x006D,
  однакові в усіх 117 заводських програмах), **99–100 parameter slots** (0x006E, програмні параметри — вміст
  не документовано), далі по контейнеру зони (0x0003) на семпл + термінатор (0x0002).
- Контейнер зони: **zone mapping** (0x01F4: root/low/high key — байти; low/high velocity — байти; **gain — float,
  лінійна амплітуда, 1.0 = 0 dB**; **panning — float, 0.5 = центр**); ім'я (0x01F5) і шлях (0x01F6) семпла;
  **audio format** (0x01F7: frame count, channels, sample rate float, **loop mode 0=off / 1=one-shot / 2=forward /
  3=sustained (грає до release) / 4=bounce**, loop start/end у фреймах, бітність 16/24/32); **amp envelope** (0x01FD:
  4 float-ручки attack/decay/sustain/release); **два filter-блоки** (0x01FB: type u32 0=off/1=LP/2=HP/3=BP/4=notch,
  cutoff і resonance — float-ручки 0–1); **два LFO-блоки** (0x0203: waveform u32 0–7 + rate float-ручка + 3 невідомі
  float); **16 modulation slots** (0x0204: source u16, target u16, amount float; відомі source 11=LFO1, 12=LFO2, 3=wheel;
  target 1=pitch, 2=gain, 7=cutoff фільтра 1, 32+i=amount i-го слота); **embedded audio**: моноліт (0x0206 FLAC —
  довжина + 8 байт заголовка; або 0x0205 нестиснутий PCM, канали planar з 512 guard-фреймами, половина до і половина
  після семпла; float32 конвертується у 24-bit). Немонолітний .dwp аудіо не містить — семпли поруч у підпапці.
- ConvertWithMoss **читає**: key/low/high, velocity low/high, root, gain, panning, петлі (включно з one-shot/sustained/
  bounce) або з .dwp, або з loop-чанків семпла; amp-ADSR; **перший** фільтр (тип LP/HP/BP/notch; cutoff експоненційним
  мапінгом ручки 0–1 від 45 Гц до максимуму; resonance); LFO→pitch, LFO→gain, LFO→cutoff (rate за законом
  Гц = 20·pos², макс 20 Гц; глибина через кубічний закон amount навколо нейтралі 0.5; wheel→amount через
  MODULATION_AMOUNT_RANGE 0.5); envelope-часи за законом t = 9.07·pos^3.96. DWB-банки і семпльовані плагіни
  реконструюються з імен файлів (дві конвенції: `Name_C5_127[_cycle]` з Image-Line-октавами C5=60, і токени
  `_C4+KA3-F#4+V64-95+TG.+TY.+TF.+TO.` з мануалу).
- **Втрачається / не читається**: другий фільтр; типи фільтрів Allpass/MiniSynth/Vox; ENV1/ENV2; усі модуляції крім
  трьох перелічених (включно з Mod Amt/Mod Val); програмний рівень (Play Mode, Glide, Volume, Drive, програмні LFO,
  Delay/Chorus/Reverb і посили — крім ігнору); per-voice FX (Ringmod/Decimator/Quantizer/Phaser); Time Stretch
  (Time/Sync/Amt/Grain/Smooth) і Beat Sync/Tick Slice/Ticks; trigger-групи з бінарника (місце в структурі невідоме —
  round-robin при записі взагалі відкидається, лишається перший цикл); SEMI/FINE/KTRK як окремі поля (тюнінг при записі
  запікається в root key з точністю до семітона, центи губляться); Mute-групи; невідомі блоки
  (0x01F8/01F9/01FA/01FC/01FE–0202) і 99 parameter slots. Пише завжди монолітний .dwp (`Name_Note_Velocity.wav` всередині).
  Відкрите питання upstream: запис немонолітного .dwp (issue #503).

## 11. Мінімальний повний набір для KeyFrameXL Sampler

- **P1 (без цього типові .dwp не грають правильно)**: зони з key/low/high + velocity low/high + root; gain (лінійний,
  1.0 = 0 dB); panning (0.5 = центр); усі 5 loop-режимів включно з one-shot і sustained (хвіст після release) + loop
  start/end у семплах; amp-ADSR (атака/дікей/реліз 0–~9 с, сустейн-рівень); базова поліфонія + Play Mode Poly/Mono/Legato;
  перший фільтр LP/HP/BP/notch з cutoff/resonance; pitch→LFO і gain→LFO через 16-слотну матрицю (amount 0–1, нейтраль 0.5,
  кубічний закон); читання і монолітного (FLAC/PCM planar + guard-фрейми), і немонолітного .dwp.
- **P2 (звучить як задумано)**: Glide Time/Fixed/Auto; program Volume; PB Rng (центи); KTRK; SEMI ±36 / FINE ±50;
  Mute-групи; trigger Group/Type (Normal/Cycle/Random/Avoid previous)/Frequency/Overlap; cutoff→LFO і wheel→amount;
  другий фільтр + series/parallel; ENV1/ENV2 з повною матрицею джерел/цілей; програмні LFO 1/2; per-voice FX
  (Ringmod/Decimator/Quantizer/Phaser) і глобальні Delay/Chorus/Reverb із посилами; One-Shot незалежно від note-off;
  коректне поводження дефолтних огинаючих v0x25/v0x26 (порожній слот = дефолт конверсії, не нулі).
- **P3 (повна вірність / авторінг)**: Time Stretch + Beat Sync + Tick Slice; MiniSynth/Vox/Allpass; Shape/Emphasis;
  редактор семплів (Reverse, Tune Loop, normalize/fade/declick); Automap + токени імен файлів + Extract root key;
  Channel Sampler (автосемплінг з Crossfade Length/Tension/Snap); Multi-Bank/Multitimbral + Expression/Output/Mute/Solo;
  High Quality Rendering (audio-rate автоматизація); імпорт сторонніх форматів — поза скоупом .dwp.
