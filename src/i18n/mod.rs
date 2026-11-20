//! UI localization: English and Ukrainian string tables, CLDR plurals,
//! placeholder formatting, and system locale detection.

#![allow(dead_code)]

mod en;
mod plural;
mod uk;

pub use en::EN;
#[allow(unused_imports)]
pub use plural::{Plural, PluralStr, fill};
pub use uk::UK;

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

    /// Parse a POSIX locale tag (`uk_UA.UTF-8`, `uk-UA`, `uk`, `uk:en`, …).
    /// Returns Ukrainian for `uk*`, falling back to English for anything else.
    pub fn from_locale(locale: &str) -> Self {
        let first = locale.split(':').next().unwrap_or("").trim();
        let lang = first
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if lang == "uk" {
            Self::Ukrainian
        } else {
            Self::English
        }
    }

    /// Detect the user's preferred language from standard POSIX locale env vars,
    /// falling back to English.
    pub fn from_system() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Ok(val) = std::env::var(var) {
                let trimmed = val.trim();
                if !trimmed.is_empty() && trimmed != "C" && trimmed != "POSIX" {
                    return Self::from_locale(trimmed);
                }
            }
        }
        Self::English
    }

    /// Optional explicit override via `KFXL_LANG=en|uk` (useful for captures and testing).
    pub fn from_env_override() -> Option<Self> {
        let raw = std::env::var("KFXL_LANG").ok()?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "en" | "eng" | "english" => Some(Self::English),
            "uk" | "ua" | "ukr" | "ukrainian" => Some(Self::Ukrainian),
            _ => None,
        }
    }
}

const CTX_LANG_ID: &str = "kfxl-ui-language";

/// Store the active UI language on the egui context so shared widget helpers
/// (such as `kit::auto_menu_item` or knob context menus) can read it without
/// global mutable state races across parallel tests.
pub fn set_ctx_lang(ctx: &eframe::egui::Context, lang: Language) {
    ctx.data_mut(|d| d.insert_temp(eframe::egui::Id::new(CTX_LANG_ID), lang));
}

/// Active UI language stored on the egui context (defaults to English).
pub fn ctx_lang(ctx: &eframe::egui::Context) -> Language {
    ctx.data(|d| d.get_temp::<Language>(eframe::egui::Id::new(CTX_LANG_ID)))
        .unwrap_or(Language::English)
}

/// String table for the active UI language on the egui context.
pub fn ctx_tr(ctx: &eframe::egui::Context) -> &'static Strings {
    strings(ctx_lang(ctx))
}

macro_rules! define_strings {
    (
        strings: { $($field:ident),* $(,)? },
        plurals: { $($pfield:ident),* $(,)? },
    ) => {
        /// One complete set of UI strings for a language.
        pub struct Strings {
            pub lang: Language,
            $(pub $field: &'static str,)*
            $(pub $pfield: PluralStr,)*
        }

        #[cfg(test)]
        impl Strings {
            pub fn entries(&self) -> Vec<(&'static str, &'static str)> {
                vec![$((stringify!($field), self.$field),)*]
            }

            pub fn plural_entries(&self) -> Vec<(&'static str, &PluralStr)> {
                vec![$((stringify!($pfield), &self.$pfield),)*]
            }
        }
    };
}

define_strings! {
    strings: {
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
    },
    plurals: {
        clips_deleted,
    },
}

impl Strings {
    pub fn clips_deleted_msg(&self, n: usize) -> String {
        self.clips_deleted.fmt(self.lang, n)
    }
}

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
            for (name, p) in table.plural_entries() {
                for (form, value) in [
                    ("one", p.one),
                    ("few", p.few),
                    ("many", p.many),
                    ("other", p.other),
                ] {
                    assert!(!value.trim().is_empty(), "{lang}.{name}.{form} is empty");
                }
            }
        }
    }

    #[test]
    fn both_tables_have_the_same_fields() {
        assert_eq!(EN.entries().len(), UK.entries().len());
        let en: Vec<_> = EN.entries().iter().map(|(n, _)| *n).collect();
        let uk: Vec<_> = UK.entries().iter().map(|(n, _)| *n).collect();
        assert_eq!(en, uk);
        let en_p: Vec<_> = EN.plural_entries().iter().map(|(n, _)| *n).collect();
        let uk_p: Vec<_> = UK.plural_entries().iter().map(|(n, _)| *n).collect();
        assert_eq!(en_p, uk_p);
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
        assert_eq!(strings(Language::English).lang, Language::English);
        assert_eq!(strings(Language::Ukrainian).lang, Language::Ukrainian);
        assert_eq!(strings(Language::English).menu_file, EN.menu_file);
        assert_eq!(strings(Language::Ukrainian).menu_file, UK.menu_file);
        assert_eq!(Language::default(), Language::English);
    }

    #[test]
    fn locale_detection_handles_ukrainian_and_fallbacks() {
        assert_eq!(Language::from_locale("uk_UA.UTF-8"), Language::Ukrainian);
        assert_eq!(Language::from_locale("uk-UA"), Language::Ukrainian);
        assert_eq!(Language::from_locale("uk"), Language::Ukrainian);
        assert_eq!(Language::from_locale("UK_UA:en_US"), Language::Ukrainian);
        assert_eq!(Language::from_locale("en_US.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("en_US:uk_UA"), Language::English);
        assert_eq!(Language::from_locale("de_DE.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("C"), Language::English);
        assert_eq!(Language::from_locale(""), Language::English);
    }

    #[test]
    fn ukrainian_plural_categories() {
        use Plural::*;
        let cases = [
            (0, Many),
            (1, One),
            (2, Few),
            (3, Few),
            (4, Few),
            (5, Many),
            (10, Many),
            (11, Many),
            (12, Many),
            (14, Many),
            (20, Many),
            (21, One),
            (22, Few),
            (24, Few),
            (25, Many),
            (100, Many),
            (101, One),
            (111, Many),
            (112, Many),
            (121, One),
            (122, Few),
        ];
        for (n, want) in cases {
            assert_eq!(Language::Ukrainian.plural(n), want, "n = {n}");
        }
        assert_eq!(Language::Ukrainian.plural_f64(1.5), Other);
        assert_eq!(Language::Ukrainian.plural_f64(2.0), Few);
        assert_eq!(Language::English.plural(1), One);
        assert_eq!(Language::English.plural(0), Other);
        assert_eq!(Language::English.plural(2), Other);
        assert_eq!(Language::English.plural_f64(1.5), Other);
    }

    #[test]
    fn ukrainian_counts_read_naturally() {
        assert_eq!(
            UK.clips_deleted_msg(0),
            "0 кліпів видалено. Ctrl+Z скасовує."
        );
        assert_eq!(UK.clips_deleted_msg(1), "1 кліп видалено. Ctrl+Z скасовує.");
        assert_eq!(
            UK.clips_deleted_msg(2),
            "2 кліпи видалено. Ctrl+Z скасовує."
        );
        assert_eq!(
            UK.clips_deleted_msg(5),
            "5 кліпів видалено. Ctrl+Z скасовує."
        );
        assert_eq!(
            UK.clips_deleted_msg(11),
            "11 кліпів видалено. Ctrl+Z скасовує."
        );
        assert_eq!(
            UK.clips_deleted_msg(21),
            "21 кліп видалено. Ctrl+Z скасовує."
        );
        assert_eq!(EN.clips_deleted_msg(1), "1 clip deleted. Ctrl+Z undoes.");
        assert_eq!(EN.clips_deleted_msg(2), "2 clips deleted. Ctrl+Z undoes.");
    }

    #[test]
    fn fill_substitutes_named_placeholders_without_rescanning() {
        assert_eq!(
            fill("Hello {name}, {n} items", &[("name", "Tide"), ("n", "3")]),
            "Hello Tide, 3 items"
        );
        assert_eq!(
            fill("{a} {b}", &[("a", "{b}"), ("b", "ok")]),
            "{b} ok",
            "substituted values are not re-scanned"
        );
        assert_eq!(fill("keep {unknown}", &[("x", "1")]), "keep {unknown}");
        assert_eq!(
            fill("unclosed {brace", &[("brace", "x")]),
            "unclosed {brace"
        );
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
