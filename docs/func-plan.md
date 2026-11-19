# KeyFrameXL — майстер-план функціоналу (раунд 2, зведення 8 розвідок)

Дата: 2026-11-14. Джерела: 8 звітів раунду 1 (аудіо, MIDI/AKAI, плагіни, DWP, Piano Roll, DSP, проєкт/експорт, мікшер). Бриф: етапи 0–5.

## Стек (зафіксовано розвідками)

| Домен | Вердикт |
|---|---|
| Аудіо-вихiд | `cpal 0.18.2` + фіча `pipewire` (нативно, `ErrorKind::Xrun`, `DeviceChanged`); `pipewire-rs` — запасний; `cubeb` — ні |
| RT-міст | `rtrb` (команди UI→аудіо), `basedrop::Shared` (семпли/пресети), пул голосів, алок-гард у тестах |
| DSP | Ядро — чистий Rust; `fundsp 0.23` тільки ноди ефектів (етап 4); компресор/choke — свій код |
| MIDI live | `midir 0.11.1` (ALSA seq, polling hotplug) + `midly 0.5` (парсинг LiveEvent); SMF теж `midly` |
| Плагіни | Свій `PluginInstance`-трейт; CLAP — `clack-host 0.2.0`; VST3 — `vst3-host 0.11.1`; `maolan` — ні; yabridge 5.1.1 + Wine Staging 9.21 (пін) |
| Семпли/експорт | `hound` (є), `flacenc 0.5.1`, `midly 0.5.3`, `uuid 1` (v4/v7+serde), `sha2`, `zip` опційно |
| Фабричний контент | Тільки свій синтез або CC0 з піном ревізії + SOURCES.txt; CC-BY/SA, NC/ND, пропрієтарні DWP — заборонено |

## Етап 0 — План, інвентар, базові заміри (поточний)

- [x] Інвентар матеріалів (кіт, код, фікстури) — є
- [ ] `pw-cli info`, `pw-top` (RATE/QUANT/ERR), кванти, rtkit — **потрібна машина користувача**
- [ ] DWP-еталон: 1 bass/keys/pad + dry-рендер з DirectWave + фікстури (legacy PCM, external, missing, one-shot) — **потрібен пресет користувача**
- [ ] AKAI: модель + `aconnect -l` + `aseqdump` натискань — **потрібні дані користувача**
- [ ] Еталонні плагіни: 1 безкоштовний CLAP + 1 Linux VST3 (кандидати: Surge XT, Odin2, sfizz) — вирішити
- [ ] `ParamAddr`/`StableId` заготовка + версійований конверт документа + міграція V1→V2 (Uuid)

## Етап 1 — Звук і DWP

- cpal 0.15→0.18.2 (UPGRADING: StreamConfig by value, явний play(), `device.id()`); лог HostId; `buffer_size()` = квант; лічильник Xrun. Перевірка: `PipeWire` в лозі, звук одразу, QUANT збігається з `pw-top`.
- Семпл-точний `render_block(view, start, out)` — один для live і WAV; темп/свінг/мют усередині. Перевірка: live-такт == WAV побайтово; свінг 0.3 без фламу; луп без клацання.
- RT-гарантії: пул голосів, преалок, rtrb/basedrop, 1× load/колбек, алок-гард. Перевірка: 10 хв ERR=0.
- DWP: контейнери + Relink (частковий імпорт, не падіння), root/краї/velocity (±1 дБ по шарах), петлі рамковими тестами, one-shot гейт. Перевірка: кожен варіант грає; missing → діалог → звук повертається.
- MIDI-каркас: порт + індикатор входу (vmpk-тест), ноти/velocity/sustain/CC/bend.

## Етап 2 — Перший трек

- Патерн: 2+ партії, Make unique, поліфонічні слайди в різні боки (групова семантика з каталогу).
- Drum Rack: пади 36/38/42, choke open/closed 5–10мс fade, gain/pan/trim, поліфонія+steal. Перевірка: старі проєкти біт-в-біт.
- Субтрактивний синт: 2 OSC + Sub + Noise → Moog LP → Amp, ADSR/LFO/glide/unison, пресети bass/lead/pluck.
- MIDI Learn + пресети (`~/.config/keyframexl/midi_maps.json`), reconnect + Panic + all-notes-off.
- CLAP-скан (дескриптори, кеш) + 1 безкоштовний CLAP: звук/MIDI/стан-біт-ідентично.
- Strip/gain/pan/mute/solo + 1 Bus + 1 Send + Master limiter (без PDC).

## Етап 3 — Контролер і плагіни

- AKAI-тест (реальний або віртуальний loopback), LED/SysEx по моделі за capability-флагом, clock 24ppqn ±0.5 BPM.
- VST3 (`vst3-host`): scan→discover у probe-процесі, in-process + ізольовані інстанси, `service_run_loop()` для GUI, bypass/latency, kill-helper→recover().
- Пісочниця: падаючий плагін валить тільки probe; blocklist.
- yabridge-шлях (опційно): детект обгорток, 1 Windows VST3 наскрізно.
- Метроном + count-in (клік повз Master FX), MIDI overdub з компенсацією, автоматизація read/write/latch + PDC (тест 512 семплів — клік синхронний).

## Етап 4 — Повна пісня

- Аудіокліпи (запис через PipeWire-вхід, моніторинг, тейки, fades 5мс), автоматизація в плейлисті.
- Мікшер: inserts/sends/buses, sidechain-вхід без циклів, PDC всюди, Master FX + лімітер останнім.
- FX: EQ, sync-delay, reverb-сенд, bus-компресор (свій).
- Рендер з хвостами; stems (pre-send/pre-master/post на вибір); упакований проєкт з іншої теки грає так само.
- Freeze/bounce; LV2 або переоцінка maolan.

## Етап 5 — Повсякденне використання

- Решта Piano Roll за каталогом (slice/glue, quantize+preview, chop/strum/arp/stamps/humanize/flip, ghost редаговані, маркери, PDF).
- Профілі контролерів, ширша DWP-матриця (фільтр/LFO/FX — reproduced/approx/omitted), LV2.
- Crash recovery, інсталяційний пакет, квант 128 приймальні виміри (xruns/10хв=0, TTFB).

## Відкриті питання до користувача (блокери раунду 3)

1. Машина: `pw-cli info`, `pw-top` під грою, кванти, `rtkit` статус, suspend-поведінка.
2. DWP: улюблений пресет + dry-рендер з DirectWave (tempo/SR канон).
3. AKAI: модель, `aconnect -l`, `aseqdump` 5–10 подій, пріоритет контролів, clock/LED зараз чи ні.
4. Еталони: 1 CLAP + 1 VST3 назви; чи потрібен PDC вже на етапі 2; чи шипати helper-бінарі.
5. CLAP-обсяг: версія, автоматизація, офлайн-рендер/стан уже зараз?

## Зроблено поза планом (логічна модель інструментів)
- Song.instruments: Kick/Snare/Hat (Drum) + Bass (Sampler); семплер — вид інструмента, не глобальний слот.
- Міграція: legacy global sample -> Bass preset; save тримає legacy-дзеркало для старих читачів.
- DWP-імпорт кладе пресет в Bass-інструмент (ім'я з пресета), аудіо/рендер беруть preset з реєстру.
- Наступне: per-part прив'язка нот до інструментів (pattern parts), потім CLAP-упаковка спільного ядра.
