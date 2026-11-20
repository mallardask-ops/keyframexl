//! Every keyboard shortcut and mouse gesture, in one place: the F1 window
//! draws it and `docs/manual/12-shortcuts.md` is generated from it (a test
//! keeps the two equal).

use crate::i18n::Language;

pub(crate) struct Group {
    pub title: &'static str,
    pub rows: &'static [(&'static str, &'static str)],
}

pub(crate) const GROUPS: &[Group] = &[
    Group {
        title: "Everywhere",
        rows: &[
            ("Space", "Play / stop and rewind"),
            ("Ctrl+Space", "Pause in place"),
            ("L", "Pattern / song mode"),
            ("R", "Record audio, MIDI and automation"),
            (
                "F5 · F6 · F7 · F9",
                "Playlist · Channel rack · Piano roll · Mixer (again closes)",
            ),
            ("Alt+F8", "Show / hide the browser"),
            ("F8", "Quick file picker"),
            ("F12", "Close all windows"),
            ("Alt+1 · 2 · 3", "Workspaces: Arrange · Compose · Edit"),
            ("Ctrl+Z", "Undo"),
            ("Ctrl+Y · Ctrl+Shift+Z", "Redo"),
            ("Ctrl+Shift+N", "New project"),
            ("Ctrl+O", "Open"),
            ("Ctrl+S", "Save"),
            ("Ctrl+Shift+S", "Save as"),
            ("Ctrl+N", "Save new version (name_2, name_3 …)"),
            ("Ctrl+R", "Export audio"),
            ("Ctrl+Shift+M", "Export MIDI"),
            ("Ctrl+Shift+Ins", "New pattern"),
            ("F2", "Rename the pattern"),
            ("F4", "Next empty pattern"),
            ("F11", "Project info"),
            ("F10", "MIDI settings"),
            ("Ctrl+Alt+← · →", "Previous · next song marker"),
            ("Ctrl+1 … 0", "Mute channel 1 … 10"),
            ("Ctrl+Shift+1 … 0", "Solo channel 1 … 10"),
            ("Ctrl+M", "Metronome"),
            ("Ctrl+T", "Typing keyboard plays the active channel"),
            ("Ctrl+J", "Multilink to controllers"),
            ("Ctrl+H", "Stop all sound (MIDI panic)"),
            ("Alt+E", "Riff Machine"),
            (
                "Ctrl+= · Ctrl+- · Ctrl+0",
                "Interface larger · smaller · 100%",
            ),
            ("F1", "This list"),
        ],
    },
    Group {
        title: "Piano roll",
        rows: &[
            ("P · B · D · T", "Draw · paint · delete · mute tool"),
            ("C · E · Z · S", "Slice · select · zoom · slide tool"),
            ("Ctrl+A · Ctrl+D", "Select all · deselect"),
            ("Ctrl+C · Ctrl+X · Ctrl+V", "Copy · cut · paste notes"),
            ("Ctrl+B", "Duplicate the selection"),
            ("Ctrl+Q · Alt+Q", "Quick quantize · quantize dialog"),
            ("Ctrl+L · Ctrl+U · Ctrl+G", "Legato · chop · glue"),
            ("Alt+V", "Ghost notes of other channels"),
            ("↑ · ↓", "Move a semitone"),
            ("Ctrl+↑ · Ctrl+↓", "Move an octave"),
            ("← · →", "Move a grid step"),
            ("Alt+← · Alt+→", "Move one tick"),
            ("Shift+← · Shift+→", "Shorter · longer by a grid step"),
            ("Delete", "Delete the selected notes"),
            ("F2", "Vocal channel: edit the note's lyric"),
        ],
    },
    Group {
        title: "Playlist",
        rows: &[
            ("P · B · D · T · C · E · Z", "Tools, as in the piano roll"),
            ("Right-drag", "Select clips with a box"),
            ("Right-click", "Delete the clip under the mouse"),
            ("Ctrl+A · Esc", "Select all · deselect"),
            ("Delete", "Delete the selected clips"),
            ("Drag on the ruler", "Loop a time selection"),
            ("Right-click the ruler", "Add a marker"),
        ],
    },
    Group {
        title: "Channel rack",
        rows: &[
            ("Shift+click a step", "Cycle its velocity"),
            ("Right-click a step", "Clear it"),
            ("Alt+↑ · Alt+↓", "Move the selected channels"),
            ("Alt+C", "Clone the selected channels with their notes"),
            ("Alt+Delete", "Delete the selected channels"),
            ("Ctrl+K", "Velocity graph"),
            ("Ctrl+L", "Route the selected channels to free inserts"),
            (
                "Ctrl+wheel on the insert",
                "Step the channel's mixer insert",
            ),
        ],
    },
    Group {
        title: "Knobs and the mixer",
        rows: &[
            ("Alt (drag)", "Skip snap"),
            ("Shift (drag a knob)", "Fine adjust"),
            ("Double-click a knob", "Reset"),
            ("Right-click a knob", "Type, copy or paste the value"),
            ("Right-click · Ctrl+click a lamp", "Solo (again: unsolo)"),
        ],
    },
];

pub(crate) const GROUPS_UK: &[Group] = &[
    Group {
        title: "Скрізь",
        rows: &[
            ("Space", "Відтворити / зупинити й повернутися на початок"),
            ("Ctrl+Space", "Пауза на місці"),
            ("L", "Режим патерна / пісні"),
            ("R", "Запис аудіо, MIDI та автоматизації"),
            (
                "F5 · F6 · F7 · F9",
                "Плейліст · Стелаж каналів · Піано-рол · Мікшер (повторно — закрити)",
            ),
            ("Alt+F8", "Показати / сховати браузер"),
            ("F8", "Швидкий вибір файлу"),
            ("F12", "Закрити всі вікна"),
            (
                "Alt+1 · 2 · 3",
                "Робочі простори: Аранжування · Композиція · Редагування",
            ),
            ("Ctrl+Z", "Скасувати"),
            ("Ctrl+Y · Ctrl+Shift+Z", "Повторити"),
            ("Ctrl+Shift+N", "Новий проєкт"),
            ("Ctrl+O", "Відкрити"),
            ("Ctrl+S", "Зберегти"),
            ("Ctrl+Shift+S", "Зберегти як"),
            ("Ctrl+N", "Зберегти нову версію (name_2, name_3 …)"),
            ("Ctrl+R", "Експортувати аудіо"),
            ("Ctrl+Shift+M", "Експортувати MIDI"),
            ("Ctrl+Shift+Ins", "Новий патерн"),
            ("F2", "Перейменувати патерн"),
            ("F4", "Наступний порожній патерн"),
            ("F11", "Інформація про проєкт"),
            ("F10", "Налаштування MIDI"),
            ("Ctrl+Alt+← · →", "Попередній · наступний маркер пісні"),
            ("Ctrl+1 … 0", "Заглушити канал 1 … 10"),
            ("Ctrl+Shift+1 … 0", "Соло каналу 1 … 10"),
            ("Ctrl+M", "Метроном"),
            ("Ctrl+T", "Клавіатура грає на активному каналі"),
            ("Ctrl+J", "Мультиприв’язка до контролерів"),
            ("Ctrl+H", "Зупинити весь звук (MIDI-паніка)"),
            ("Alt+E", "Генератор рифів"),
            (
                "Ctrl+= · Ctrl+- · Ctrl+0",
                "Масштаб інтерфейсу: більший · менший · 100%",
            ),
            ("F1", "Цей список"),
        ],
    },
    Group {
        title: "Піано-рол",
        rows: &[
            (
                "P · B · D · T",
                "Інструменти: олівець · пензель · видалення · заглушення",
            ),
            (
                "C · E · Z · S",
                "Інструменти: розрізання · виділення · масштаб · слайд",
            ),
            ("Ctrl+A · Ctrl+D", "Виділити все · зняти виділення"),
            (
                "Ctrl+C · Ctrl+X · Ctrl+V",
                "Копіювати · вирізати · вставити ноти",
            ),
            ("Ctrl+B", "Дублювати виділене"),
            ("Ctrl+Q · Alt+Q", "Швидке квантування · діалог квантування"),
            ("Ctrl+L · Ctrl+U · Ctrl+G", "Легато · нарізати · склеїти"),
            ("Alt+V", "Тіньові ноти інших каналів"),
            ("↑ · ↓", "Зсунути на півтон"),
            ("Ctrl+↑ · Ctrl+↓", "Зсунути на октаву"),
            ("← · →", "Зсунути на крок сітки"),
            ("Alt+← · Alt+→", "Зсунути на один тік"),
            ("Shift+← · Shift+→", "Коротше · довше на крок сітки"),
            ("Delete", "Видалити виділені ноти"),
            ("F2", "Вокальний канал: редагувати склад ноти"),
        ],
    },
    Group {
        title: "Плейліст",
        rows: &[
            ("P · B · D · T · C · E · Z", "Інструменти, як у піано-ролі"),
            ("Перетягування ПКМ", "Виділити кліпи рамкою"),
            ("ПКМ", "Видалити кліп під курсором"),
            ("Ctrl+A · Esc", "Виділити все · зняти виділення"),
            ("Delete", "Видалити виділені кліпи"),
            ("Перетягування по лінійці", "Зациклити часову ділянку"),
            ("ПКМ по лінійці", "Додати маркер"),
        ],
    },
    Group {
        title: "Стелаж каналів",
        rows: &[
            ("Shift+клік по кроку", "Перемкнути його швидкість"),
            ("ПКМ по кроку", "Очистити крок"),
            ("Alt+↑ · Alt+↓", "Перемістити вибрані канали"),
            ("Alt+C", "Клонувати вибрані канали разом із нотами"),
            ("Alt+Delete", "Видалити вибрані канали"),
            ("Ctrl+K", "Графік швидкості"),
            ("Ctrl+L", "Призначити вибрані канали на вільні інсерти"),
            ("Ctrl+колесо на інсерті", "Змінити інсерт мікшера каналу"),
        ],
    },
    Group {
        title: "Ручки та мікшер",
        rows: &[
            ("Alt (перетягування)", "Обійти прив’язку"),
            ("Shift (перетягування ручки)", "Точне налаштування"),
            ("Подвійний клік по ручці", "Скинути"),
            ("ПКМ по ручці", "Увести, скопіювати або вставити значення"),
            ("ПКМ · Ctrl+клік по лампі", "Соло (повторно — зняти)"),
        ],
    },
];

pub(crate) const fn groups(lang: Language) -> &'static [Group] {
    match lang {
        Language::English => GROUPS,
        Language::Ukrainian => GROUPS_UK,
    }
}

/// The shortcuts chapter of the manual (written by the test below).
#[cfg(test)]
pub(crate) fn markdown() -> String {
    let mut s = String::from(
        "# Keyboard shortcuts\n\n\
         The same list opens with `F1`. This page is generated from the program\n\
         (`src/ui/shortcuts.rs`); edit that file, not this one.\n",
    );
    for g in GROUPS {
        s.push_str(&format!("\n## {}\n\n", g.title));
        for (k, what) in g.rows {
            let keys: Vec<String> = k.split(" · ").map(|k| format!("`{k}`")).collect();
            s.push_str(&format!("- {}: {what}\n", keys.join(" · ")));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn manual_page_matches_the_list() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/manual/12-shortcuts.md");
        let want = super::markdown();
        if std::env::var_os("KFXL_WRITE_DOCS").is_some() {
            std::fs::write(path, &want).unwrap();
        }
        let have = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            have == want,
            "docs/manual/12-shortcuts.md is out of date: run \
             KFXL_WRITE_DOCS=1 cargo test --bin keyframexl manual_page"
        );
    }

    #[test]
    fn uk_groups_match_en_structure() {
        assert_eq!(super::GROUPS.len(), super::GROUPS_UK.len());
        for (en, uk) in super::GROUPS.iter().zip(super::GROUPS_UK.iter()) {
            assert_eq!(en.rows.len(), uk.rows.len(), "group {}", en.title);
        }
    }
}
