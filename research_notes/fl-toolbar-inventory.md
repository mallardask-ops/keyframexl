# Інвентар верхньої зони FL Studio (Menu + Toolbar) для KeyFrameXL

> Джерела: офіційний онлайн-мануал Image-Line (актуальний для FL Studio 21 / 2024; для 2025 окремої сторінки зі змінами топ-панелі не знайдено — якщо не вказано інше, вважати спільним).
> Український опис, англійські назви як у FL. Категорії: [transport] [recording] [project-io] [import-export] [midi] [parameters/automation] [editing] [windows/view] [patterns] [tools] [settings] [monitoring].
> Якщо не впевнено — позначено «(невідомо / перевірити)».

## 0. Базові сторінки

- Огляд Toolbar: https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm
- Редагування Toolbar і пресети: https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_edit.htm
- Menu Bar: https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu.htm

## 1. FILE — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_file.htm

- **New** — новий проєкт із шаблону за замовчуванням | — | [project-io]
- **New from template / Genre, Minimal, Other, Utility** — новий проєкт із вибраного шаблону | — | [project-io]
- **Open** (Ctrl+O) — відкрити .flp | Ctrl+O | [project-io]
- **Save** (Ctrl+S) — перезаписати поточний файл | Ctrl+S | [project-io]
- **Save as** (Shift+Ctrl+S) — зберегти під новим ім'ям/форматом | Shift+Ctrl+S | [project-io]
- **Save new version** (Ctrl+N) — зберегти як MyProj_2, _3… | Ctrl+N | [project-io]
- **Save and back up to FL Cloud** — разовий бекап .zip у FL Cloud | — | [project-io]
- **Save to new project folder** — зберегти в нову папку проєкту | Ctrl+N (контекстно) | [project-io]
- **Save as template** — зберегти проєкт як шаблон | — | [project-io]
- **Import** — імпорт не-FL файлів (див. Open/Import File Formats) | — | [import-export]
- **Export > Wave file** (Ctrl+R) — рендер у .wav | Ctrl+R | [import-export]
- **Export > mp3 / ogg** (Shift+Ctrl+R) — рендер у стиснений формат | Shift+Ctrl+R | [import-export]
- **Export > MIDI file** (Shift+Ctrl+M) — експорт нот/контролерів без аудіо | Shift+Ctrl+M | [import-export]
- **Export > Master (FL Cloud)** — AI-мастеринг під LUFS платформ | — | [import-export]
- **Export > All playlist tracks / track groups** — стеми по треках Playlist | — | [import-export]
- **Export > Video (ZGameEditor Visualizer)** — рендер візуалайзера | — | [import-export]
- **Export > Zipped loop package (.zip)** — .flp + семпли в архіві | — | [project-io]
- **Export > Project bones** — пресети .fst + партитури .fsc + .nfo | — | [project-io]
- **Export > Project data files** — копії всіх семплів/даних в папку | — | [project-io]
- **Recent projects** (Alt+1…0, More до 50) — останні проєкти | Alt+цифра | [project-io]
- **Revert to last backup** — відкат до автобекапу | — | [project-io]
- **Exit** — закрити FL Studio | — | [project-io]

## 2. EDIT — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_edit.htm

> У мануалі лише 4 команди; ціль залежить від активного вікна (Channel Rack / Piano roll / Playlist).
- **Undo** — крок назад/вперед по історії (історія в Project Browser) | Ctrl+Alt+Z / Ctrl+Z | [editing]
- **Cut** (Ctrl+X) — вирізати ноти вибраних каналів | Ctrl+X | [editing]
- **Copy** (Ctrl+C) — копіювати ноти вибраних каналів | Ctrl+C | [editing]
- **Paste** (Ctrl+V) — вставити з верхнього вибраного каналу | Ctrl+V | [editing]

## 3. ADD — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_add.htm

- **Channel** — додати інструмент/генератор у Channel Rack | — | [windows/view]
- **Effect** — додати ефект у Mixer | — | [windows/view]
- **View plugin picker** — відкрити Plugin Picker | — | [windows/view]
- **Browse plugin database / all installed / presets** — відкрити папки Browser | — | [windows/view]
- **Refresh plugins list (fast scan)** — швидке сканування VST | — | [settings]
- **Manage plugins** — Plugin Manager, повне сканування і категорії | — | [settings]
- **Automation for last tweaked parameter** — Automation Clip для останнього руху | — | [parameters/automation]
- **Pattern** — створити новий Pattern | — | [patterns]

## 4. PATTERNS — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_patterns.htm

- **Find first empty** (Shift+F4) — перший порожній патерн | Shift+F4 | [patterns]
- **Find next empty** (F4) — наступний порожній патерн | F4 | [patterns]
- **Find next empty (no naming)** (Ctrl+4) — наступний порожній без діалогу | Ctrl+4 | [patterns]
- **Select in playlist** — виділити всі кліпи патерна в Playlist | — | [patterns]
- **Rename and color** (F2, повторний F2 — колір) — ім'я/колір патерна | F2 | [patterns]
- **Random color** — випадковий колір | — | [patterns]
- **Open in project browser** (Shift+Ctrl+P) — показати патерн у Browser | Shift+Ctrl+P | [patterns]
- **Group / Ungroup selected** — групи фільтра Picker Panel | — | [patterns]
- **Set time signature** — розмір такту для патерна | — | [patterns]
- **Transpose** — транспозиція вибраних каналів у півтонах | — | [editing]
- **Insert one** (Shift+Ctrl+Ins) — вставити порожній перед поточним | Shift+Ctrl+Ins | [patterns]
- **Clone** (Alt+C) — копія після поточного | Alt+C | [patterns]
- **Delete** (Del) — видалити патерн | Del | [patterns]
- **Move up / down** (Shift+Ctrl+Up/Down) — порядок у селекторі | Shift+Ctrl+Up/Down | [patterns]
- **Split by channel** — по патерну на кожен активний канал | — | [patterns]
- **Quick render as audio clip** — bounce патерна у WAV поточними налаштуваннями | — | [import-export]
- **Render as audio clip** — bounce з діалогом Render Settings | — | [import-export]
- **Render and replace** — bounce + заміна кліпів в Playlist | — | [import-export]

## 5. VIEW — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_view.htm

- **Playlist** (F5, Right-click — maximize) — показати/сховати Playlist | F5 | [windows/view]
- **Piano roll** (F7) — показати/сховати Piano roll | F7 | [windows/view]
- **Channel rack** (F6) — показати/сховати Channel Rack | F6 | [windows/view]
- **Mixer** (F9) — показати/сховати Mixer | F9 | [windows/view]
- **Browser** (Alt+F8) — показати/сховати Browser | Alt+F8 | [windows/view]
- **Project picker** (Ctrl+F8) — сітка кліпів/каналів і зв'язків | Ctrl+F8 | [windows/view]
- **Plugin picker** (F8, drag-and-drop) — сітка інструментів/ефектів | F8 | [windows/view]
- **Tempo tapper** — відкрити Tap tempo | — | [transport]
- **Touch controller** (Alt+F7) — сенсорна клавіатура/пади | Alt+F7 | [midi]
- **Script output** — консоль Python MIDI Scripting | — | [tools]
- **Toolbars > Edit / Separators / Hint bar / Flat buttons / Hide (Ctrl+F11)** — налаштування тулбару | Ctrl+F11 | [windows/view]
- **Plugin performance monitor** (також double-click по CPU-панелі) — CPU/час по плагінах | — | [monitoring]
- **Welcome to FL Studio** — стартове вікно | — | [windows/view]
- **Close all windows** (F12) / **all plugin windows** (Alt+F12) / **unfocused** (Ctrl+F12) / **Align editors** (Shift+F12) — керування вікнами | F12… | [windows/view]
- **Arrange windows > Realign (Shift+Ctrl+H), Into workspace, Desktop/Alternate/Tablet/Custom, Save custom layout** — лейаути | Shift+Ctrl+H | [windows/view]
- **Background > image / HTML / color** — шпалери робочого столу | — | [windows/view]
- **Browser > Undo history / Patterns / Generators / Effects / Remote control / Plugin database** — розкрити гілки Browser | — | [windows/view]
- **Downloads / News / Notifications** — вкладки Communications Panel | — | [windows/view]

## 6. OPTIONS — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_options.htm

- **MIDI settings** (F10) — вікно MIDI | F10 | [midi]
- **Audio settings** — вікно Audio | — | [settings]
- **General settings** — вікно General | — | [settings]
- **File settings** — вікно File | — | [settings]
- **Theme settings** — теми | — | [settings]
- **Manage plugins** — менеджер плагінів | — | [settings]
- **Project info** (F11) — назва/автор/метадані | F11 | [project-io]
- **Project general settings** — PPQ, такт, темп-проєкт | — | [settings]
- **Debugging log** — вікно Debug | — | [tools]
- **Enable MIDI remote control / output / master sync** — глобальні MIDI-вимикачі | — | [midi]
- **Typing to piano keyboard** (Ctrl+T) — друк як MIDI-клавіатура | Ctrl+T | [midi]
- **Metronome** (Ctrl+M) — клік на кожну долю | Ctrl+M | [recording]
- **Recording precount** (Ctrl+P) — відлік перед записом | Ctrl+P | [recording]
- **Start on input** (Ctrl+I) — Play чекає першого MIDI-вводу | Ctrl+I | [recording]
- **Blend recorded notes** (Ctrl+B) — змішувати замість перезапису | Ctrl+B | [recording]
- **Step edit** (Ctrl+E) — покроковий ввід у Piano roll | Ctrl+E | [recording]
- **Loop record** — запис по колу у виділення Playlist | — | [recording]
- **Auto scrolling** — автопрокрутка редакторів | Scroll Lock | [windows/view]
- **Enable groups** — групування нот/кліпів | — | [editing]
- **Multi-link to controllers** (Ctrl+J) — пакетна прив'язка контролерів | Ctrl+J | [midi]
- **Manage cloud accounts** — зв'язок із SoundCloud/FL Cloud | — | [settings]

## 7. TOOLS — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_tools.htm

- **Browser find** (Ctrl+F) — пошук по Browser | Ctrl+F | [tools]
- **One-click audio recording > Into Edison / Into Playlist** — майстер запису аудіо | — | [recording]
- **Remix a song** — візард стем-розділення для реміксу | — | [tools]
- **Riff machine** — генератор партитур (з Tools — ще й створює канал) | — | [tools]
- **Chord progression tool** — конструктор акордових послідовностей | — | [tools]
- **Control creator** — кастом-контроли для Control Surface/Patcher | — | [tools]
- **Macros > Panic > Stop sound** (Ctrl+H) — заглушити всі ноти | Ctrl+H | [tools]
- **Macros > Panic > Cancel recording** — скасувати запис і стерти .wav | — | [recording]
- **Macros > Prepare for performance / MIDI export** — підготовка проєкту | — | [tools]
- **Macros > Select unused / Purge unused / Smart disable / audio-clip stretch / lock-unlock tracks / Reset tracks / Remove Edison / Delete unused files / Attach-Detach editors / Declicking** — пакетні чистки/перемикання | — | [tools]
- **FL Cloud > Sounds / Mastering / Distribution / Manage plugins / My account** — онлайн-контент і сервіси | — | [tools]
- **Clipboard > Save audio / MIDI clipboard to browser** — зберегти буфер у .wav/.mid | — | [project-io]
- **Score logger > Dump last 2/5/10/20/30 min / Clear** — вивантажити фоновий лог MIDI | — | [recording]
- **Audio logger > Enable / 1/2/5/10 min / Pre-FX / Post-FX pre-fader** — кільцевий буфер Master-треку в RAM | — | [recording]
- **Last / Before-last tweaked > Edit events / Init song-pattern / Delete initial value / Create automation clip / Link to controller / Copy-Paste value / Override volatile link** — дії над останнім параметром | — | [parameters/automation]
- **External tools** — зовнішні редактори/браузери як інструменти | — | [tools]

## 8. HELP — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_help.htm

- **Help Index** (F1) — відкрити мануал | F1 | [tools]
- **Gopher** (Alt+F1) — LLM-помічник по мануалу/KB | Alt+F1 | [tools]
- **Tutorials (Loop Starter, Stepsequencer…)** — інтерактивні уроки | — | [tools]
- **Product information / Buy / Unlock / Register code / My account** — ліцензії й магазин | — | [settings]
- **Diagnostics / Fix problems / Reset settings-DPI / rendering tests / Fix song project** — діагностика і ремонт | — | [tools]
- **Fix graphics performance (macOS: paint rects, VSync, PBO)** — фікси малювання | — | [settings]
- **What's new** — список змін релізу | — | [windows/view]
- **Internet Links: homepage / Mobile / tutorials / Forum / Knowledgebase** — зовнішні посилання | — | [windows/view]
- **About** — версія, кредити, ліцензія | — | [windows/view]

## 9. Toolbar: Title / Hint / Sync — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_title

- **Title Bar** — ліцензія (User/Licence Plate) + ім'я проєкту | — | [windows/view]
- **Hint Bar** — підказка під курсором + значення при крученні; Right-click — копія значення | — | [windows/view]
- **Hint icons: Recording / MIDI plug / Right-mouse / Happy-Sad / Warning / Clock / FF-Rewind / Group** — що вміє контрол під курсором | — | [windows/view]
- **Sync indicator / MIDI Activity (Orange=volatile link, Blue=global link, Green=unhandled, Grey=none; blink=beat, blue=bar)** — вхід MIDI і MIDI Sync | — | [monitoring]
- **Extended Hint Panel** (View > Toolbars > Hint bar; прозора, draggable) — великий хінт у кутку | — | [windows/view]

## 10. Toolbar: Transport — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_transport

- **Pat / Song** (L; Right-click PAT=Channel Rack, SONG=Playlist) — режим патерн vs аранжування | L | [transport]
- **Play/Pause** — старт; повтор — стоп+назад; при armed Rec — запис | Space / Ctrl+Space=pause | [transport]
- **Stop** — стоп; double-click — panic; Right-click — Remember seek time | Space | [transport]
- **Record** (R; Alt+Click — приховане вікно «що писати») — arm запису | R | [recording]
- **Recording Filter (Right-click Rec): Automation / Notes / Audio / Clips** — що саме писати | — | [recording]
- **Recording Filter: starts playback / Disarm on stop / Enable markers / Quantize (start/end/leave duration) / Record to stepsequencer / Automations** — поведінка запису | — | [recording]
- **Tempo (10–522 BPM) + fine tune 1/1000** — темп проєкту | — | [transport]
- **Tempo Right-click: Edit events / Create automation clip / Copy-Paste-Type value** — автоматизація темпу | — | [parameters/automation]
- **Tap** — натапати темп (можна на ходу) | — | [transport]
- **Tap +/- nudge (hold) / MIDI pitch-bend = smooth nudge** — тимчасово тягнути темп під зовнішнє джерело | — | [transport]
- **Tap Sync / Mute / Now / 80–160 presets / Half speed** — режими тапу і половинна швидкість для запису складного | — | [transport]
- **Song position** — позиція/перетягування плейхеда Playlist/Piano roll | — | [transport]
- **Window controls: Minimize / Restore (Right-click/Alt+Enter — Taskbar/Dock) / Multi-monitor drag / Close (Alt+F4)** — вікно FL | Alt+F4 | [windows/view]

## 11. Toolbar: Volume / Pitch / Snap / Add / Multilink / Time / Pattern / Output / CPU / Chord / Online

- **Monitor Volume** — гучність прослуховування; не в рендері, не автоматизується, зберігається в проєкті | — | [monitoring] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_vol
- **Master Pitch** — стрій (default A=440 Hz, крок 100 cents, Shift — без снапу; потрібен Enable main pitch на каналі) | — | [parameters/automation] — там само
- **Global Snap: Line / Cell / (none) / Steps 1/6–1 / Beats 1/6–1 / Bar; Right-click — довжина ноти; Alt — тимчасово без снапу** — снап Piano roll/Playlist/Event Editor + квантайз MIDI-вводу | — | [editing] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_snap
- **Add Panel** — дублікат ADD-меню на тулбарі | — | [windows/view] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_add
- **Multilink to controllers** (Ctrl+J; Right-click: Edit events / Init song-pattern / Create automation clips / Link / Override global links / Randomize / Humanize / Cancel) — пакетний лінк/автоматизація, Browser-linking | Ctrl+J | [midi] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_multilinks
- **Time Panel: Bar:Beat:Step:Tick vs Min:Sec:Cs; S/B switch** — формат часу пісні | — | [monitoring] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_time
- **Pattern Selector (+/-; Right-click — список до 999)** — поточний патерн | +/- | [patterns] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_pattern
- **Pattern options: ті ж що PATTERNS-меню (F4/Shift+F4/F2/Shift+Ctrl+P/Ins/Alt+C/Del/стрілки/Split/Render)** — див. розд. 4 | як у розд. 4 | [patterns] — там само
- **Output monitor: Oscilloscope (Stereo/Long) / Spectrograph / Peak Meter (Right-click опції)** — вихідний сигнал; кліп=червоне | — | [monitoring] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_output
- **CPU Meter % (час буфера, не OS-CPU) / Graph / Polyphony voices / Memory (64-bit адресний простір) / Right-click: Refresh-Voice-FPS-Misc / Double-click=Performance Monitor** — навантаження і пам'ять | — | [monitoring] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_cpu
- **Chord Panel (Right-click: час показу None–Forever)** — розпізнавання акорду з MIDI/друку/виділення | — | [tools] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_chords
- **Online / Communications Panel: News / Downloads / Notifications / Help / Gopher** — RSS від Image-Line, тільки по кліку, потрібен інтернет | — | [windows/view] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_online

## 12. Toolbar: Shortcut Icons (окремо винесені кнопки, додаються/знімаються в Edit) — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm#panel_shortcuticons

- **Metronome** (Ctrl+M; Right-click — звук/трек) — клік | Ctrl+M | [recording]
- **Recording Count-in** (Ctrl+P; Right-click — такти) — передзапис | Ctrl+P | [recording]
- **Recording Blend / Overdub** (Ctrl+B) — ноти змішувати; аудіо — sound-on-sound | Ctrl+B | [recording]
- **Recording Loop** — запис по колу | — | [recording]
- **Recording One-click (мікрофон)** — візард «куди писати аудіо» | — | [recording]
- **Wait for Input** (Ctrl+I; sub: Only when recording / Respect filter) — чекати вводу | Ctrl+I | [recording]
- **Step Editing** (Ctrl+E) — покроковий ввід | Ctrl+E | [recording]
- **Typing keyboard to Piano** (Ctrl+T; Root/Octave/Scale/Key) — друк як піаніно | Ctrl+T | [midi]
- **Render as Audio File** (Ctrl+R; Right-click — MP3) — швидкий рендер | Ctrl+R | [import-export]
- **Save As** (Ctrl+Shift+S; блимає як нагадування) — швидке «зберегти як» | Ctrl+Shift+S | [project-io]
- **Undo** (Ctrl+Z) / **History (Right-click)** — undo + історія | Ctrl+Z | [editing]
- **Cut** (Ctrl+X) / **Paste-Duplicate** (Ctrl+V / Ctrl+B) — буфер активного редактора | Ctrl+X/V/B | [editing]
- **Auto Scroll** (Scroll Lock; Right-click — Continuous) — слідкування за плейхедом | Scroll Lock | [windows/view]
- **Grouping Notes & Clips** — увімк/вимк групування | — | [editing]
- **View Playlist / Piano roll / Rack / Mixer / Browser / Picker / Info / Tap / Touch / Gopher / Help (F1)** — дублікати View/Help | F5/F6/F7/F8/F9/F11… | [windows/view]
- **Open Audio Editor (Edison; Right-click — новий)** — аудіоредактор | Shift+E (з Mixer) | [tools]
- **Quick Tweak** — великий ручка останнього параметра | — | [parameters/automation]
- **Arrange / Alt / Ctrl / Shift keys / Language / Shop** — службові кнопки | — | [windows/view]
- **Toolbar Edit: Right-click фон > Edit; Separators / Flat buttons / Hint bar / Hide (Ctrl+F11) / Presets (Factory, Auto saved, User); 2 racks + Space-панелі; Shift — без снапу при ресайзі** — кастомізація | Ctrl+F11 | [windows/view] — https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_edit.htm

## 13. Рекомендований порядок для KeyFrameXL

**P1 — must-have для працездатного секвенсора:**
Pat/Song (L), Play/Pause/Stop (Space, Ctrl+Space, panic), Tempo+BPM + Tap, Song position, Time Bar:Beat:Tick, Pattern selector + Find next empty (F4) + Rename (F2) + Clone/Delete, Global Snap (Line/Cell/none/steps/beats/bar), Record + Filter (Notes/Automation/Audio), Metronome (Ctrl+M), Precount (Ctrl+P), Blend (Ctrl+B), Step edit (Ctrl+E), Loop record, New/Open/Save/Save as (Ctrl+O/S/Shift+Ctrl+S), Undo/Cut/Copy/Paste (Ctrl+Z/X/C/V), Add Channel/Effect, View toggles Playlist/Piano roll/Rack/Mixer/Browser (F5/F6/F7/F9/Alt+F8), Auto Scroll, Monitor Volume + Peak Meter з кліпом, CPU % + Polyphony, MIDI Activity індикатор, Hint Bar, Typing-to-piano (Ctrl+T), Project Info/Settings (F11).

**P2 — продакшн (другий етап):**
Export WAV/MP3/OGG/MIDI (Ctrl+R…), Render pattern to audio + Replace, Import аудіо/MIDI, Save new version/template, Recent + Revert backup, Transpose, Time signature на патерн, Select in playlist, Split by channel, Group/Ungroup + Enable groups, Quantize on record (start/end/leave), Count-in такти, Recording markers + Disarm on stop, One-click recording, Score logger Dump/Clear + Audio logger, Last tweaked (Edit events/Init/Create clip/Link/Copy-Paste), Multilink (Ctrl+J) + Override global links, Master Pitch (A=440), Plugin Manager + fast scan, Automation Clip для темпу, Plugin Performance Monitor, Project Picker (Ctrl+F8), Mixer input/arm для аудіо (мінімум).

**P3 — later / optional:**
Zipped loop package, Project bones/data files, All playlist tracks stems, Time Min:Sec:Cs + S/B, Song Start Remember seek time, Half speed + Nudge/Sync/Mute tap-режими, Extended Hint Panel, Flat/Separators/Spaces + Toolbar presets/user layouts, Multi-monitor + Arrange/Align/Background, Touch controller (Alt+F7), Script output (MIDI Python), Riff machine + Chord progression + Control creator, Macros (smart disable/purge/lock tracks/reset/remove Edison…), Clipboard to browser, External tools, Audio Editor інтеграція, Chord Panel, Spectrograph/Oscilloscope опції, Refresh/FPS/Handles діагностика, Tutorials, Language switch, Quick Tweak.

**P4 — skip (FL-специфічне / онлайн-маркетинг):**
FL Cloud (Backup to cloud, Sounds-підписка, AI Mastering, Distribution, Manage plugins-app, My account), Shop + Trial-lock, News/Downloads/Online RSS + Welcome-маркетинг, Homepage/Mobile/Forum/KB зовнішні лінки, Product info/Buy/Unlock/Register/About-ліцензії (замінити своєю схемою), Diagnostics/Reset/DPI/macos-PBO-VSync (свої), «What's new» формат, Gopher LLM (пізніше своє), Fix song project через видалення VST (FL-специфічне), Edison-специфічні макроси (Remove all Edison), Fruity LSD/GM шлях для MIDI-експорту, Elastique/Realtime/Legacy stretch-пресети як у FL, ZGameEditor Video (свій відеошар пізніше).
