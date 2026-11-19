//! UI localization: English and Ukrainian string tables.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    Ukrainian,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Ukrainian];

    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Ukrainian => "Українська",
        }
    }
}

macro_rules! define_strings {
    ($($field:ident),* $(,)?) => {
        /// One complete set of UI strings for a language.
        pub struct Strings {
            $(pub $field: &'static str,)*
        }

        #[cfg(test)]
        impl Strings {
            fn entries(&self) -> Vec<(&'static str, &'static str)> {
                vec![$((stringify!($field), self.$field),)*]
            }
        }
    };
}

define_strings! {
    // Top menu
    menu_file, menu_edit, menu_add, menu_patterns, menu_view, menu_options, menu_tools, menu_help,
    // File menu
    file_new, file_open, file_open_recent, file_save, file_save_as, file_save_new_version,
    file_import_bundle, file_export_bundle, file_import_flp, file_import_midi, file_export_audio,
    file_export_midi, file_quit,
    // Edit / Help menu
    edit_undo, edit_redo, help_welcome, help_check_updates, help_about,
    // Deck / shell
    deck_pattern, deck_song, deck_playlist, deck_rack, deck_roll, deck_mixer, deck_browser,
    untitled, unsaved,
    // Settings
    settings_title, tab_audio, tab_midi, tab_plugins, tab_general, setting_language,
    setting_language_hint, setting_show_welcome, setting_show_welcome_hint,
    setting_check_updates, setting_check_updates_hint,
    // Welcome screen
    welcome_window_title, welcome_subtitle, welcome_recent_heading, welcome_recent_empty,
    welcome_recent_missing, welcome_templates_heading, welcome_presets_heading,
    welcome_quick_actions, welcome_new_project, welcome_open_project, welcome_import_bundle,
    welcome_import_flp, welcome_settings, welcome_show_on_startup, welcome_continue,
    welcome_update_badge, welcome_tab_templates, welcome_tab_tide, welcome_tab_forge,
    welcome_search_presets,
    // Templates
    tpl_beat_title, tpl_beat_desc, tpl_demo_title, tpl_demo_desc, tpl_tide_title, tpl_tide_desc,
    tpl_forge_title, tpl_forge_desc, tpl_vocal_title, tpl_vocal_desc, tpl_blank_title,
    tpl_blank_desc,
    // Update checker
    update_title, update_checking, update_up_to_date, update_available, update_install_now,
    update_downloading, update_verifying, update_installing, update_ready_restart,
    update_restart_now, update_restart_save_hint, update_download_linux, update_open_release,
    update_skip_version, update_check_again, update_close, update_later, update_source_build,
    update_copy_command, update_failed, update_nightly, update_nightly_hint,
}

pub const EN: Strings = Strings {
    menu_file: "FILE",
    menu_edit: "EDIT",
    menu_add: "ADD",
    menu_patterns: "PATTERNS",
    menu_view: "VIEW",
    menu_options: "OPTIONS",
    menu_tools: "TOOLS",
    menu_help: "HELP",

    file_new: "New project",
    file_open: "Open…",
    file_open_recent: "Open recent",
    file_save: "Save",
    file_save_as: "Save as…",
    file_save_new_version: "Save new version",
    file_import_bundle: "Import bundle…",
    file_export_bundle: "Export bundle…",
    file_import_flp: "Import FL Studio project (.flp)…",
    file_import_midi: "Import MIDI file…",
    file_export_audio: "Export audio…",
    file_export_midi: "Export MIDI…",
    file_quit: "Quit",

    edit_undo: "Undo",
    edit_redo: "Redo",
    help_welcome: "Welcome screen…",
    help_check_updates: "Check for updates…",
    help_about: "About KeyFrameXL",

    deck_pattern: "Pattern",
    deck_song: "Song",
    deck_playlist: "Playlist",
    deck_rack: "Channels",
    deck_roll: "Piano roll",
    deck_mixer: "Mixer",
    deck_browser: "Browser",
    untitled: "Untitled",
    unsaved: "Unsaved",

    settings_title: "Settings",
    tab_audio: "Audio",
    tab_midi: "MIDI",
    tab_plugins: "Plugins",
    tab_general: "General",
    setting_language: "Language",
    setting_language_hint: "Interface language. Applies immediately.",
    setting_show_welcome: "Show welcome screen on startup",
    setting_show_welcome_hint: "Open the welcome screen with recent projects and templates when KeyFrameXL starts.",
    setting_check_updates: "Check for updates on startup",
    setting_check_updates_hint: "Look for a newer release on GitHub when KeyFrameXL starts.",

    welcome_window_title: "Welcome to KeyFrameXL",
    welcome_subtitle: "Start a new project, pick up where you left off, or begin from a template.",
    welcome_recent_heading: "Recent projects",
    welcome_recent_empty: "No recent projects yet. Saved projects will show up here.",
    welcome_recent_missing: "File not found",
    welcome_templates_heading: "Templates",
    welcome_presets_heading: "Presets",
    welcome_quick_actions: "Quick actions",
    welcome_new_project: "New project",
    welcome_open_project: "Open .kfxl…",
    welcome_import_bundle: "Import bundle…",
    welcome_import_flp: "Import .flp…",
    welcome_settings: "Settings",
    welcome_show_on_startup: "Show on startup",
    welcome_continue: "Continue",
    welcome_update_badge: "Update available",
    welcome_tab_templates: "Templates",
    welcome_tab_tide: "Tide",
    welcome_tab_forge: "Forge",
    welcome_search_presets: "Search presets…",

    tpl_beat_title: "Beat starter",
    tpl_beat_desc: "Drums, bass and a simple chord loop to build a beat on.",
    tpl_demo_title: "Demo song",
    tpl_demo_desc: "A short arranged song to explore the playlist, mixer and piano roll.",
    tpl_tide_title: "Tide synth",
    tpl_tide_desc: "A project built around the Tide synth, ready to play and tweak.",
    tpl_forge_title: "Forge SFX",
    tpl_forge_desc: "Design game sound effects with the Forge channel.",
    tpl_vocal_title: "Vocal",
    tpl_vocal_desc: "A singing channel with a voicebank, ready for lyrics and notes.",
    tpl_blank_title: "Blank project",
    tpl_blank_desc: "An empty project to start from scratch.",

    update_title: "Software update",
    update_checking: "Checking for updates…",
    update_up_to_date: "You are on the latest version.",
    update_available: "A new version is available",
    update_install_now: "Install update",
    update_downloading: "Downloading update…",
    update_verifying: "Verifying SHA-256 checksum…",
    update_installing: "Extracting and installing…",
    update_ready_restart: "Update installed! Restart KeyFrameXL to use the new version.",
    update_restart_now: "Restart now",
    update_restart_save_hint: "You have unsaved changes — save your project before restarting.",
    update_download_linux: "Download manually",
    update_open_release: "Open release page",
    update_skip_version: "Skip this version",
    update_check_again: "Check again",
    update_close: "Close",
    update_later: "Later",
    update_source_build: "This copy is built from source, so it updates with git:",
    update_copy_command: "Copy update command",
    update_failed: "Update failed",
    update_nightly: "Nightly builds",
    update_nightly_hint: "Get every new build from main (GitHub prereleases), not only stable releases.",
};

pub const UK: Strings = Strings {
    menu_file: "ФАЙЛ",
    menu_edit: "РЕДАГУВАННЯ",
    menu_add: "ДОДАТИ",
    menu_patterns: "ПАТЕРНИ",
    menu_view: "ВИГЛЯД",
    menu_options: "ПАРАМЕТРИ",
    menu_tools: "ІНСТРУМЕНТИ",
    menu_help: "ДОВІДКА",

    file_new: "Новий проєкт",
    file_open: "Відкрити…",
    file_open_recent: "Останні проєкти",
    file_save: "Зберегти",
    file_save_as: "Зберегти як…",
    file_save_new_version: "Зберегти нову версію",
    file_import_bundle: "Імпортувати пакет…",
    file_export_bundle: "Експортувати пакет…",
    file_import_flp: "Імпортувати проєкт FL Studio (.flp)…",
    file_import_midi: "Імпортувати MIDI-файл…",
    file_export_audio: "Експортувати аудіо…",
    file_export_midi: "Експортувати MIDI…",
    file_quit: "Вийти",

    edit_undo: "Скасувати",
    edit_redo: "Повторити",
    help_welcome: "Екран вітання…",
    help_check_updates: "Перевірити оновлення…",
    help_about: "Про KeyFrameXL",

    deck_pattern: "Патерн",
    deck_song: "Пісня",
    deck_playlist: "Плейліст",
    deck_rack: "Канали",
    deck_roll: "Піано-рол",
    deck_mixer: "Мікшер",
    deck_browser: "Браузер",
    untitled: "Без назви",
    unsaved: "Не збережено",

    settings_title: "Налаштування",
    tab_audio: "Аудіо",
    tab_midi: "MIDI",
    tab_plugins: "Плагіни",
    tab_general: "Загальні",
    setting_language: "Мова",
    setting_language_hint: "Мова інтерфейсу. Застосовується одразу.",
    setting_show_welcome: "Показувати екран вітання під час запуску",
    setting_show_welcome_hint: "Відкривати екран вітання з останніми проєктами та шаблонами під час запуску KeyFrameXL.",
    setting_check_updates: "Перевіряти оновлення під час запуску",
    setting_check_updates_hint: "Шукати новішу версію на GitHub під час запуску KeyFrameXL.",

    welcome_window_title: "Ласкаво просимо до KeyFrameXL",
    welcome_subtitle: "Створіть новий проєкт, продовжте роботу або почніть із шаблону.",
    welcome_recent_heading: "Останні проєкти",
    welcome_recent_empty: "Останніх проєктів ще немає. Збережені проєкти з’являться тут.",
    welcome_recent_missing: "Файл не знайдено",
    welcome_templates_heading: "Шаблони",
    welcome_presets_heading: "Пресети",
    welcome_quick_actions: "Швидкі дії",
    welcome_new_project: "Новий проєкт",
    welcome_open_project: "Відкрити .kfxl…",
    welcome_import_bundle: "Імпортувати пакет…",
    welcome_import_flp: "Імпортувати .flp…",
    welcome_settings: "Налаштування",
    welcome_show_on_startup: "Показувати під час запуску",
    welcome_continue: "Продовжити",
    welcome_update_badge: "Доступне оновлення",
    welcome_tab_templates: "Шаблони",
    welcome_tab_tide: "Tide",
    welcome_tab_forge: "Forge",
    welcome_search_presets: "Пошук пресетів…",

    tpl_beat_title: "Заготовка біту",
    tpl_beat_desc: "Ударні, бас і проста акордова петля, щоб побудувати біт.",
    tpl_demo_title: "Демо-пісня",
    tpl_demo_desc: "Коротка аранжована пісня, щоб освоїти плейліст, мікшер і піано-рол.",
    tpl_tide_title: "Синтезатор Tide",
    tpl_tide_desc: "Проєкт навколо синтезатора Tide: грайте й налаштовуйте одразу.",
    tpl_forge_title: "Звукові ефекти Forge",
    tpl_forge_desc: "Створюйте звукові ефекти для ігор у каналі Forge.",
    tpl_vocal_title: "Вокал",
    tpl_vocal_desc: "Співочий канал із голосовою бібліотекою, готовий до тексту й нот.",
    tpl_blank_title: "Порожній проєкт",
    tpl_blank_desc: "Порожній проєкт, щоб почати з нуля.",

    update_title: "Оновлення програми",
    update_checking: "Перевірка оновлень…",
    update_up_to_date: "У вас остання версія.",
    update_available: "Доступна нова версія",
    update_install_now: "Оновити зараз",
    update_downloading: "Завантаження оновлення…",
    update_verifying: "Перевірка контрольної суми SHA-256…",
    update_installing: "Розпакування та встановлення…",
    update_ready_restart: "Оновлення встановлено! Перезапустіть KeyFrameXL для запуску нової версії.",
    update_restart_now: "Перезапустити зараз",
    update_restart_save_hint: "Є незбережені зміни — збережіть проєкт перед перезапуском.",
    update_download_linux: "Завантажити вручну",
    update_open_release: "Відкрити сторінку релізу",
    update_skip_version: "Пропустити цю версію",
    update_check_again: "Перевірити ще раз",
    update_close: "Закрити",
    update_later: "Пізніше",
    update_source_build: "Ця копія зібрана з вихідного коду, тому оновлюється через git:",
    update_copy_command: "Скопіювати команду оновлення",
    update_failed: "Не вдалося оновити",
    update_nightly: "Нічні збірки",
    update_nightly_hint: "Отримувати кожну нову збірку з main (пре-релізи GitHub), а не лише стабільні релізи.",
};

pub const fn strings(lang: Language) -> &'static Strings {
    match lang {
        Language::English => &EN,
        Language::Ukrainian => &UK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_is_non_empty() {
        for (lang, table) in [("EN", &EN), ("UK", &UK)] {
            for (name, value) in table.entries() {
                assert!(!value.trim().is_empty(), "{lang}.{name} is empty");
            }
        }
    }

    #[test]
    fn both_tables_have_the_same_fields() {
        assert_eq!(EN.entries().len(), UK.entries().len());
        let en: Vec<_> = EN.entries().iter().map(|(n, _)| *n).collect();
        let uk: Vec<_> = UK.entries().iter().map(|(n, _)| *n).collect();
        assert_eq!(en, uk);
    }

    #[test]
    fn language_labels_are_distinct() {
        let labels: Vec<_> = Language::ALL.iter().map(|l| l.label()).collect();
        assert!(labels.iter().all(|l| !l.is_empty()));
        for (i, a) in labels.iter().enumerate() {
            for b in &labels[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn strings_selects_the_matching_table() {
        assert_eq!(strings(Language::English).menu_file, EN.menu_file);
        assert_eq!(strings(Language::Ukrainian).menu_file, UK.menu_file);
        assert_eq!(Language::default(), Language::English);
    }

    #[test]
    fn ukrainian_translates_key_labels() {
        let pairs = [
            ("menu_file", EN.menu_file, UK.menu_file),
            ("file_new", EN.file_new, UK.file_new),
            ("help_welcome", EN.help_welcome, UK.help_welcome),
            ("deck_pattern", EN.deck_pattern, UK.deck_pattern),
            ("deck_mixer", EN.deck_mixer, UK.deck_mixer),
            ("settings_title", EN.settings_title, UK.settings_title),
            (
                "welcome_new_project",
                EN.welcome_new_project,
                UK.welcome_new_project,
            ),
            (
                "welcome_templates_heading",
                EN.welcome_templates_heading,
                UK.welcome_templates_heading,
            ),
            ("update_title", EN.update_title, UK.update_title),
            ("tpl_blank_title", EN.tpl_blank_title, UK.tpl_blank_title),
        ];
        for (name, en, uk) in pairs {
            assert_ne!(en, uk, "{name} is not translated");
        }
    }
}
