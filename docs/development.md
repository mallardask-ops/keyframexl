# Процес розробки KeyFrameXL

Правила погоджені між Claude Opus 5.5, GPT 6.2 Astra і Gemini 4 Neon (листопад 2026)
і затверджуються власником. **MUST** — обов'язково. **SHOULD** — за замовчуванням;
відступ пояснюється в описі PR. Короткий конспект для агентів — у [`AGENTS.md`](../AGENTS.md).

## 1. Стек

- **MUST** Основа: Rust 2024, одна crate `keyframexl`, eframe/egui 0.31 (glow), cpal 0.18
  (PipeWire + ALSA fallback на Linux, WASAPI + ASIO на Windows). Заміну GUI-, аудіо- чи
  плагінного стеку власник погоджує до початку роботи.
- **MUST** Тулчейн: останній stable. На dev-машині системний Rust з Arch (без rustup), тому
  `rust-toolchain.toml` не додаємо: локально його ніхто не читатиме. Мінімальна версія
  записується в `rust-version` у `Cargo.toml`. CI збирає на stable. Якщо новий Clippy
  ламає CI, виправлення йде окремим PR `build:`.
- **MUST** `Cargo.lock` у git, CI працює з `--locked`.
- **MUST** Нова залежність: в описі PR указати, навіщо вона, які features вмикаються,
  які нативні бібліотеки тягне і як це вплине на інсталятор. Ліцензії лише
  MIT/Apache-2.0/BSD/Zlib/ISC (без GPL/LGPL). Пояснювати критичні feature-прапорці
  коментарем у `Cargo.toml` (як зроблено для заборони `cpal/realtime-dbus`).
- **MUST** Код у `vendor/` описаний у `vendor/README.md`: upstream і ревізія, що змінено й
  чому, як перевірити, коли можна повернутися на upstream. Новий vendoring лише тоді, коли
  виправити upstream не вдається.
- **MUST** Стандартний rustfmt без `rustfmt.toml`. Винятки Clippy живуть у `[lints.clippy]`
  в `Cargo.toml`. Локальний `#[allow]` ставиться на найменшу ділянку коду й має коментар
  із причиною.
- **MUST** Код під конкретну ОС живе в `src/platform/`, `src/bridge/ipc/`,
  `src/plugins/editor_{x11,win32}.rs` і `src/audio.rs`. `#[cfg(target_os)]` не з'являється
  всередині `src/ui/` і DSP (`engine.rs`, `fx/`, синтів, семплера).
- **SHOULD** Великі файли (`engine.rs` 6.6k, `ui/roll.rs` 4.6k, `ui/mod.rs` 3.9k, `flp.rs`
  3.8k рядків) не збільшувати. Нова логіка йде в підмодуль. Розбиття робиться попутно, коли
  в цьому місці з'являється нова функція, і окремим механічним комітом
  `refactor(scope): split …` перед змістовною зміною.

## 2. Гілки

- **MUST** Модель trunk-based: `main` завжди зелена й готова до релізу.
- **MUST** Зміни потрапляють у `main` лише через PR, і перед злиттям мають пройти обидва
  CI jobs (`linux-test`, `windows-test`). Агенти ніколи не пушать у `main` напряму.
- **MUST** Один агент працює в одній гілці й одному worktree. Основний чекаут
  `~/.t3/projects/keyframexl` належить власнику.
- Назви гілок: людина `<type>/<slug>` (`fix/ssd5-export`, `feat/sampler-slice`); гілки,
  які створює T3, лишаються `t3/<slug>`; інші агенти — `agent/<model>/<slug>`; гілка
  релізу — `release/X.Y.Z`. Пакети робіт із плану: `vX.Y.Z/w<N>-<slug>`.
- **MUST** На GitHub увімкнено `delete_branch_on_merge`. Після злиття worktree видаляється
  (`git worktree remove`, `git branch -D`), але тільки коли агент закінчив, у worktree
  немає незакомічених змін і HEAD збігається зі злитим PR. Після squash змінюються SHA,
  тому `git branch --merged` для перевірки не годиться: звіряйте з PR.

## 3. Коміти й PR

- **MUST** Заголовок PR і squash-коміт: Conventional Commits англійською, наказовий спосіб,
  не довше 72 символів: `type(scope): Summary`.
  - Типи: `feat fix perf refactor test docs build ci chore revert`.
  - Області: `engine audio fx synth sampler mixer roll playlist rack ui i18n plugins bridge
    vst3 clap wine flp midi export project vocal forge updater platform release vendor`.
  - Злам формату `.kfxl` чи налаштувань позначається `!` (`feat(project)!: …`) і описується
    в тілі коміту разом зі способом міграції.
- **MUST** Один PR — одна причина для відкату. Коміт на кшталт «Fix click-through, offline
  export, Piano Roll UX, lead channel і playlist clip memory» розбивається на п'ять PR. Код,
  тести й документація однієї функції лишаються в одному PR.
- **MUST** Злиття через squash. Merge-коміти вимкнені. Rebase-merge дозволений лише для
  серії комітів, кожен з яких окремо проходить CI.
- **MUST** Коміти агента закінчуються трейлером `Co-Authored-By: <Model> <noreply@…>`.
  В описі PR указується агент.
- **MUST** Опис PR (шаблон `.github/pull_request_template.md`): що змінено, чому, які
  команди пройшли, що **не** перевірено (зокрема Windows runtime, живі плагіни, звук).

## 4. Версії

- **MUST** SemVer для 0.x: MINOR — нова велика можливість, нова ОС, зміна формату `.kfxl`.
  PATCH — виправлення й невеликі сумісні доповнення. Версія формату файлу ведеться окремо
  від версії програми.
- **MUST** Єдине джерело версії — `package.version` у `Cargo.toml`. Між релізами там стоїть
  наступна цільова версія з `-dev` (`0.2.1-dev`). Workflow-и беруть core (`0.2.1`) з
  маніфесту й додають суфікс каналу. Обчислення `patch + 1` прибирається.
- Канали й теги:
  - nightly — `v0.2.1-nightly.YYYYMMDDHHMM`, prerelease, зберігаються останні 10;
  - кандидат — `v0.2.1-rc.N`, prerelease, випускається вручну;
  - stable — `v0.2.1`, тег мусить збігатися з `Cargo.toml` (перевіряє `release.yml`).
  Порядок для updater: `nightly < rc < stable`. Старі теги `nightly.rc.N.DATE`
  підтримуються для зворотної сумісності.
- **MUST** Nightly збирається за розкладом раз на добу (03:00 UTC) і вручну через
  `workflow_dispatch`, лише коли з попередньої збірки змінився код і CI для цього SHA
  зелений. Публікації після кожного push у `main` більше немає: updater розсилав
  користувачам кожен коміт, тільки за 21–22.11 вийшло 25 nightly.

## 5. Реліз

1. **MUST** Кожен PR із видимою для користувача зміною додає один-два короткі рядки в
   `## Unreleased` у `CHANGELOG.md` (розділи Added / Changed / Fixed / Removed), англійською
   і з погляду музиканта. Внутрішні подробиці лишаються в PR. Якщо змінився UI, у тому ж PR
   оновлюються рядки EN і UK та сторінки посібника.
2. **SHOULD** Для MINOR і для змін платформи спершу виходить `vX.Y.Z-rc.1`. З моменту
   прийнятого RC у реліз потрапляють лише метадані й документація. Будь-яка зміна коду
   означає наступний RC.
3. **MUST** Release PR `chore(release): Prepare vX.Y.Z`: прибрати `-dev`, датувати секцію
   CHANGELOG, залишити порожній `Unreleased`; у `docs/vX.Y.Z.md` поставити
   `Статус: випущено YYYY-MM-DD`, а незроблене перенести в `docs/roadmap.md`. Після релізу
   план не редагується.
4. **MUST** Поставити тег `vX.Y.Z` на злитий SHA, дочекатися `release.yml` і перевірити
   артефакти: Linux `.tar.gz` і `.sha256`; Windows `.zip`, `-setup.exe` і `.sha256`, лише
   якщо в репозиторії задано змінну `RELEASE_WINDOWS=true`; а також версію всередині
   бінарника. Нотатки релізу беруться з секції CHANGELOG.
5. **MUST** Одразу після релізу: `chore(release): Bump version to X.Y.(Z+1)-dev`.
6. Опублікований тег або артефакт не перезаписується. Якщо знайшовся дефект, виходить
   наступний patch або RC.

## 6. Перевірки якості

- **MUST** локально, перш ніж віддавати PR на злиття:
  ```sh
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
  KFXL_NO_UPDATE_CHECK=1 cargo test --locked
  ```
  (`cargo test` без фільтрів запускає й `tests/mixer_effects.rs` та `tests/vocal_synthetic.rs`.)
- **MUST** CI на Linux і Windows: `cargo test --locked` замість окремих `--lib` / `--bin`.
  Тести, яким потрібен реальний voicebank, мережа, Wine чи аудіопристрій, вмикаються лише
  явною змінною `KFXL_*` і пишуть у лог, що їх пропущено.
- **SHOULD** `cargo-deny` (ліцензії й advisories) у PR, де змінюються `Cargo.toml` /
  `Cargo.lock`, і раз на тиждень за розкладом.
- **MUST** ручна перевірка для змін в аудіо, рушії, хостингу плагінів чи форматі проєкту:
  відкрити старий `.kfxl`, відтворити його наживо (`scripts/live/listen.sh`), зберегти,
  відкрити знову, експортувати офлайн. Для мосту плагінів додатково перевірити реальний
  VST через yabridge: відкрити й закрити редактор, переконатися, що звук є і після
  повторного відкриття проєкту. Записати в PR ОС, backend, плагін, sample rate, буфер і
  результат.
- **MUST** для змін UI: перевірка за [`docs/ui-rules.md`](ui-rules.md) і скріншот через
  `scripts/ux/shot.sh` у темній і світлій темі.
- Windows CI перевіряє збірку й автотести, але не реальні WASAPI/ASIO-пристрої. Якщо в
  релізі є зміни аудіо для Windows, які на справжньому пристрої не перевірялися, це прямо
  пишеться в нотатках релізу.

## 7. Кілька агентів одночасно

- **MUST** Одна задача — один автор, одна гілка, один worktree. Перед початком у задачі чи
  чернетці PR записати мету, базовий SHA і файли з ризиком конфлікту (`engine.rs`, формат
  `.kfxl`, `ui/mod.rs`). Якщо дві задачі зачіпають одне місце, порядок визначає власник.
- **MUST** Звичайний `cargo test` не звертається до аудіопристрою, `~/.wine` чи мережі.
- **MUST** Фізичний аудіопристрій, `~/.wine`/yabridge, FL Link і `scripts/live/*`
  одночасно використовує лише один агент. Доступ іде через лок:
  `flock /tmp/kfxl-hw.lock <команда>`. Експерименти зі встановленням плагінів робляться в
  окремому `WINEPREFIX`. Чужі процеси Wine чи PipeWire не зупиняти.
- **MUST** Рендери, логи й скріншоти зберігаються в `/tmp/kfxl-*` або `artifacts/<branch>/`,
  а не в корені репозиторію.
- **MUST** Під час передачі роботи вказати HEAD, що зроблено, що перевірено, що не
  перевірено і що лишилося.
- Інструкції для агентів зібрані в `AGENTS.md`. `CLAUDE.md` і `GEMINI.md` лише посилаються
  на нього: це звичайні файли, а не symlink, щоб чекаут працював і на Windows.

## 8. Порядок впровадження

1. `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, цей документ, `.github/pull_request_template.md`.
2. `nightly.yml`: замість push — розклад і `workflow_dispatch`; версія береться з
   `Cargo.toml`. `release.yml`: тег має точно збігатися з версією в маніфесті, бо зараз
   шаблон `v*.*.*` пропускає `-rc.1` як stable; нотатки беруться з CHANGELOG.
   `ci.yml`: `cargo test --locked`, `--locked` у clippy.
3. GitHub: захист `main` з обов'язковими `linux-test` і `windows-test`, лише squash,
   `delete_branch_on_merge`. Прибрати старі гілки (`e1-pipewire`, `t3/*`, remote
   `v0.2.1-w*`), спершу звіривши їх із PR.
4. `Cargo.toml` → `0.2.1-dev` і `rust-version`; `src/updater.rs` / `src/version.rs`
   навчити нових суфіксів (`-dev`, `-rc.N`) з тестами на старі теги; `vendor/README.md`.
5. Репетиція релізу `v0.2.1-rc.1` за чеклістом із §5.
