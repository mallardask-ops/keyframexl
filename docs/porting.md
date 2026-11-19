# Порт на Windows і macOS

Аналіз станом на v0.1.2 (листопад 2026). Мета — повноцінний порт: та сама
функціональність, ті самі проєкти `.kfxl`, без «урізаних» режимів і без
платформних багів. Оцінки — для однієї людини з агентами, з тестуванням на
реальному залізі.

## Коротко

Ядро (секвенсор, рушій, вбудовані інструменти й ефекти, Forge, вокал, імпорт
FLP, UI на egui) майже повністю переносиме: ~99 тис. рядків Rust, а
Linux-специфіка зосереджена приблизно в 12 файлах. Складність порту не в UI і
не в DSP, а в чотирьох шарах:

| Шар | Linux зараз | Windows | macOS | Обсяг |
|---|---|---|---|---|
| Хостинг плагінів (VST2/VST3/CLAP: завантаження + GUI) | `dlopen`, X11-вбудовування через `xcb` | `LoadLibrary`, HWND | `CFBundle`/`dlopen`, NSView | **великий** |
| Міст плагінів в окремому процесі (`src/bridge`) | `UnixStream` + `mmap` + `futex` | named pipe + file mapping + Event | `UnixStream` + `mmap` + семафор/`os_sync_wait_on_address` | середній |
| Wine/yabridge/FL-інтеграція (`fl_link`, `fl_native`, `flp_freeze`) | Wine, PipeWire-утиліти | **не потрібна**: Windows-плагіни і FL Studio працюють нативно | FL Studio має нативну macOS-версію; Windows-плагіни недоступні | переписати або вимкнути |
| Аудіо/MIDI-бекенд і тюнінг затримки | ALSA/PipeWire, власний патч cpal | WASAPI (+ ASIO) | CoreAudio | малий–середній |

Решта — дрібниці: `xdg-open`, `fc-match`, `tar` в оновлювачі,
`libmp3lame.so`, шляхи до папок плагінів.

**Рекомендований порядок:** спершу Windows, потім macOS. Windows дає найбільший
виграш (рідні Windows-плагіни і FL Studio без Wine — саме те, що на Linux
найкрихкіше: FLEX під Wine падає), а CI-ранер `windows-latest` безкоштовний.

## Інвентар Linux-специфічного коду

| Файл | Що там | Що робити |
|---|---|---|
| `src/plugins.rs` (3.3 тис. рядків, 57 викликів `xcb::`, 177 згадок Wine/yabridge) | сканування, вбудовування редакторів через X11 (`WM_STATE`, reparent), фікси під yabridge | винести X11 у `editor/x11.rs`, додати `editor/win32.rs` і `editor/cocoa.rs` за спільним трейтом; Wine-частину сховати за `cfg(target_os = "linux")` |
| `src/vst2.rs` | `libc::dlopen` `.so`, X11-вікно редактора | `libloading` для `.dll` / `.vst`-бандла (`Contents/MacOS/…`), HWND/NSView для редактора |
| `src/clap.rs` | `dlopen` `.clap`, GUI API `"x11"`, `libc::poll` для timer/fd-підтримки | `libloading`; GUI API `"win32"` / `"cocoa"`; fd-підтримка (`posix-fd-support`) — тільки Unix, на Windows таймер на UI-потоці |
| `vendor/vst3-host` | вже має `embed.rs` з HWND і NSView та пошук бінарника в `Contents/<arch>` і `Contents/MacOS` | перевірити й увімкнути; ймовірно найлегша з трьох частин |
| `src/bridge/{mod,host,shm,supervisor}.rs` | `UnixStream`, `mmap` файлу, `futex` | Windows: `CreateFileMapping` + named Event (або `WaitOnAddress` — тільки в межах процесу, тож не підходить) + named pipe; macOS: `mmap` лишається, `futex` → `os_sync_wait_on_address` з прапором shared (macOS 14.4+) або POSIX-семафор |
| `src/fl_link.rs`, `src/fl_native.rs`, `src/flp_freeze.rs` | запуск FL під Wine, захоплення звуку через `pw-cat`/`parec`/`pactl`/`pw-link`, віртуальні MIDI-порти `midir::os::unix` | Windows/macOS: завантажувати FL Studio VSTi як звичайний плагін (на Linux це вже працює), а міст `fl_link` вимкнути — він потрібен лише для обходу Wine |
| `src/audio.rs` | детект PipeWire, `PIPEWIRE_LATENCY`, гаряче перепідключення | гілки за `cfg`; на Windows — вибір WASAPI shared/exclusive та ASIO; на macOS — розмір буфера CoreAudio |
| `vendor/cpal`, `vendor/winit` | патчі для ALSA-underrun і X11 XDND | на інших ОС не заважають, але XDND-фікс для перетягування з плагінів треба повторити для OLE drag&drop на Windows і NSPasteboard на macOS |
| `src/mp3.rs` | `dlopen("libmp3lame.so.0")` | статично зібраний LAME (`mp3lame-encoder`) на всіх ОС, щоб експорт MP3 працював без системних бібліотек |
| `src/theme.rs` | `fc-match` для шрифту з каною (вокал) | Windows: `C:\Windows\Fonts\YuGothM.ttc`/`msgothic.ttc`; macOS: `Hiragino Sans`; або вбудувати Noto Sans JP subset |
| `src/updater.rs` | заміна бінарника через `rename`, `tar`, `chmod` | див. розділ «Оновлювач» |
| `xdg-open` у 5 місцях | відкрити URL/папку | одна функція `open_path()`: `explorer`/`cmd /c start` на Windows, `open` на macOS (або крейт `opener`) |
| `Cargo.toml` | `xcb`, `eframe` з фічами `wayland`/`x11` для всіх ОС | перенести в `[target.'cfg(target_os = "linux")'.dependencies]` |
| `dirs::state_dir` (`main.rs`, лог) | на Windows і macOS повертає `None` | запасний варіант через `data_local_dir()` |

## Шари докладніше

### 1. Хостинг плагінів — головна робота

Найбільший ризик багів саме тут: на Linux пів року налагоджувалися
особливості X11 і Wine (`WM_STATE`, фокус, XDND, ресайз). На Windows і macOS
будуть **свої** особливості:

- **Windows:** редактор — дочірнє HWND у вікні winit. Потрібно: DPI-awareness
  (per-monitor v2; плагіни часто не готові до масштабу 150 %), пересилання
  клавіатури (плагін «краде» фокус, і хоткеї DAW перестають працювати),
  `WM_SIZE` при ресайзі редактора, а також окремий потік повідомлень для
  мосту плагінів у дочірньому процесі.
- **macOS:** редактор — NSView всередині вікна, де egui малює через OpenGL
  (glow). OpenGL на macOS застарілий, але працює; на Apple Silicon
  стабільніше перейти на `wgpu` (Metal) — це зміна бекенду eframe, не UI.
  Редактори плагінів мають жити на головному потоці (AppKit-обмеження) —
  сьогоднішня архітектура «редактор у дочірньому процесі» на macOS потребує
  окремого вікна дочірнього процесу (як у Bitwig) замість вбудовування.
- **Формати:** на macOS також **Audio Units (AUv2/AUv3)** — без них частина
  плагінів (Logic-екосистема, Apple-ефекти) недоступна. Окремий хостинг через
  `AudioToolbox`, це ~2–3 тис. рядків.
- **VST2** офіційно знятий Steinberg з SDK; власні заголовки в репо — ок, але
  нові плагіни під macOS arm64 часто вже без VST2.

### 2. Міст плагінів (`src/bridge`)

Протокол (`protocol.rs`) і логіка супервізора переносимі. Міняється транспорт:

- Windows: `CreateFileMappingW` + `MapViewOfFile` замість `mmap`,
  `CreateEventW` (named, auto-reset) замість `futex_wait/wake`, named pipe
  замість `UnixStream`. Дочірній процес — `CreateProcess` з
  `CREATE_NO_WINDOW`.
- macOS: `mmap` і `UnixStream` лишаються; `futex` замінити.
- Тест: існуючі тести мосту запускати в CI на всіх трьох ОС.

### 3. Wine / FL Studio

На Windows цілий шар обходів Wine (`fl_link`, `fl_native`, `flp_freeze`,
`library.rs`, налаштування префіксу) просто не потрібен: FL Studio VSTi і
FLEX — звичайні Windows-плагіни. Це, ймовірно, **виправить FLEX**, який під
Wine падає. На macOS FL Studio теж має нативну версію з VST3/AU.
`flp_freeze` (рендер FL-каналів через Wine) треба переробити на
«завантажити FL як плагін і зрендерити».

### 4. Аудіо і MIDI

- cpal підтримує WASAPI і CoreAudio з коробки. Для низької затримки на
  Windows потрібен **ASIO** (`cpal` feature `asio`, збірка з ASIO SDK від
  Steinberg — перевірити ліцензію на розповсюдження).
- Вибір пристрою/буфера в Settings → Audio — показувати ASIO-драйвери, а
  WASAPI-exclusive як опцію.
- `midir` працює на Windows (WinMM) і macOS (CoreMIDI). Віртуальні MIDI-порти
  є на macOS, але **немає на Windows** — це зачіпає лише `fl_link`, який там
  і так не потрібен.
- Запис входу на macOS вимагає `NSMicrophoneUsageDescription` в `Info.plist`,
  інакше система мовчки віддає тишу.

## Збірка, підпис, розповсюдження

| | Windows | macOS |
|---|---|---|
| Ціль | `x86_64-pc-windows-msvc` (+ пізніше `aarch64`) | universal `x86_64` + `aarch64-apple-darwin` (`lipo`) |
| CI | `windows-latest` | `macos-14` (arm64) |
| Пакет | `.zip` з `keyframexl.exe` + інсталятор (Inno Setup або MSI через `cargo-wix`) | `KeyFrameXL.app` у `.dmg` |
| Підпис | без нього SmartScreen лякає попередженням; сертифікат OV/EV або Azure Trusted Signing (~$10/міс) | **обов'язковий**: Apple Developer ($99/рік), `codesign` + notarization, інакше Gatekeeper не дасть запустити |
| C-залежності | `vorbis_rs`, `rsworld-sys` (WORLD, C++) — збираються MSVC | збираються clang; перевірити arm64 |

## Оновлювач

- **Windows:** працюючий `.exe` не можна перезаписати, але **можна
  перейменувати**: `keyframexl.exe` → `keyframexl.old.exe`, новий файл на його
  місце, при наступному запуску видалити `.old`. Архів — `.zip` (крейт `zip`
  уже в залежностях) замість `tar`. Установка в `Program Files` потребує прав
  адміністратора — тому краще ставити в `%LOCALAPPDATA%\Programs\KeyFrameXL`.
- **macOS:** замінювати весь `.app`-бандл атомарно, зберігши підпис; знімати
  атрибут quarantine не можна — новий бандл має бути вже нотаризований.
  Найпростіше — Sparkle-подібна схема: завантажити `.dmg`/`.zip`, перевірити
  підпис (`codesign --verify`), замінити бандл, перезапустити.
- Активи в релізі: `keyframexl-windows-x86_64.zip`,
  `keyframexl-macos-universal.zip` + `.sha256`; `pick_linux_asset` стає
  `pick_asset_for(target)`.

## Що треба для порту «без багів»

1. **Шар абстракції платформи** (`src/platform/{linux,windows,macos}.rs`):
   open_path, системні шрифти, лог-папка, запуск дочірнього процесу, IPC.
   Інакше `cfg` розповзуться по 40 файлах.
2. **CI-матриця на трьох ОС** з першого дня: `cargo test --lib` і
   `--bin keyframexl` на кожній. Зараз 560 тестів — вони і є сітка безпеки.
3. **Тестові плагіни на кожній ОС:** Surge XT (VST3+CLAP, вільний), Vital,
   Dexed, один JUCE-плагін, один iPlug2-плагін, FL Studio VSTi. Сценарії:
   завантаження, GUI, ресайз, збереження/відновлення стану, краш у мосту.
4. **Реальне залізо:** Windows 10/11 з ASIO-картою, Mac на Apple Silicon.
   Віртуалки не покажуть проблем із затримкою і GUI плагінів.
5. **Сумісність проєктів:** `.kfxl` зберігає шляхи до плагінів і семплів —
   потрібна нормалізація (`/` vs `\`, регістр, пошук плагіна за ID/назвою, а
   не за абсолютним шляхом), щоб проєкт з Linux відкривався на Windows.

## Етапи й оцінка

| Етап | Зміст | Оцінка |
|---|---|---|
| 0 | `cfg`-гігієна: target-залежності в `Cargo.toml`, `platform/`-модуль, проєкт **компілюється** під Windows/macOS з вимкненим хостингом плагінів; CI-матриця | 1 тиждень |
| 1 | Windows: аудіо (WASAPI/ASIO), вбудовані інструменти, експорт, оновлювач, `.zip`-реліз у nightly | 1–2 тижні |
| 2 | Windows: VST3 (через `vst3-host`) + CLAP + VST2 з HWND-редакторами, міст плагінів на named pipes | 3–4 тижні |
| 3 | Windows: FL Studio VSTi/FLEX нативно, `flp_freeze` без Wine, підпис | 1–2 тижні |
| 4 | macOS: збірка universal, CoreAudio, `.app` + notarization, оновлювач | 2 тижні |
| 5 | macOS: VST3/CLAP з NSView, редактори в окремих вікнах, AU-хостинг | 4–6 тижнів |

Разом: Windows — приблизно 1.5–2 місяці, macOS — ще 1.5–2 місяці.

## Що потрібно від тебе

- Доступ до Windows-машини (або хоча б VM з GPU passthrough) і до Mac на
  Apple Silicon для живих перевірок GUI плагінів і затримки.
- Рішення щодо підпису: Apple Developer ($99/рік) обов'язковий для macOS;
  для Windows — сертифікат або Azure Trusted Signing, або жити з
  попередженням SmartScreen на ранніх nightly.
- Ліцензійне питання ASIO SDK для розповсюдження.
