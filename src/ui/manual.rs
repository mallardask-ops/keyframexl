//! HELP → Manual: the user manual (`docs/manual/*.md`) built into the
//! program, so it works offline and matches the version that is running.

use super::{App, markdown};
use crate::theme as t;
use eframe::egui::{self, RichText, vec2};

/// File name and text of each chapter, in reading order.
pub(crate) const CHAPTERS: &[(&str, &str)] = &[
    ("01-start.md", include_str!("../../docs/manual/01-start.md")),
    (
        "02-first-beat.md",
        include_str!("../../docs/manual/02-first-beat.md"),
    ),
    (
        "03-sequencing.md",
        include_str!("../../docs/manual/03-sequencing.md"),
    ),
    (
        "04-automation.md",
        include_str!("../../docs/manual/04-automation.md"),
    ),
    (
        "05-instruments.md",
        include_str!("../../docs/manual/05-instruments.md"),
    ),
    ("06-forge.md", include_str!("../../docs/manual/06-forge.md")),
    ("07-vocal.md", include_str!("../../docs/manual/07-vocal.md")),
    ("08-mixer.md", include_str!("../../docs/manual/08-mixer.md")),
    (
        "09-audio-midi.md",
        include_str!("../../docs/manual/09-audio-midi.md"),
    ),
    (
        "10-plugins-fl.md",
        include_str!("../../docs/manual/10-plugins-fl.md"),
    ),
    ("11-files.md", include_str!("../../docs/manual/11-files.md")),
    (
        "12-shortcuts.md",
        include_str!("../../docs/manual/12-shortcuts.md"),
    ),
    ("13-help.md", include_str!("../../docs/manual/13-help.md")),
];

/// The Ukrainian manual (`docs/manual/uk/`), the same files in the same order.
pub(crate) const CHAPTERS_UK: &[(&str, &str)] = &[
    (
        "01-start.md",
        include_str!("../../docs/manual/uk/01-start.md"),
    ),
    (
        "02-first-beat.md",
        include_str!("../../docs/manual/uk/02-first-beat.md"),
    ),
    (
        "03-sequencing.md",
        include_str!("../../docs/manual/uk/03-sequencing.md"),
    ),
    (
        "04-automation.md",
        include_str!("../../docs/manual/uk/04-automation.md"),
    ),
    (
        "05-instruments.md",
        include_str!("../../docs/manual/uk/05-instruments.md"),
    ),
    (
        "06-forge.md",
        include_str!("../../docs/manual/uk/06-forge.md"),
    ),
    (
        "07-vocal.md",
        include_str!("../../docs/manual/uk/07-vocal.md"),
    ),
    (
        "08-mixer.md",
        include_str!("../../docs/manual/uk/08-mixer.md"),
    ),
    (
        "09-audio-midi.md",
        include_str!("../../docs/manual/uk/09-audio-midi.md"),
    ),
    (
        "10-plugins-fl.md",
        include_str!("../../docs/manual/uk/10-plugins-fl.md"),
    ),
    (
        "11-files.md",
        include_str!("../../docs/manual/uk/11-files.md"),
    ),
    (
        "12-shortcuts.md",
        include_str!("../../docs/manual/uk/12-shortcuts.md"),
    ),
    (
        "13-help.md",
        include_str!("../../docs/manual/uk/13-help.md"),
    ),
];

/// The chapters in the interface language.
pub(crate) const fn chapters(
    lang: crate::i18n::Language,
) -> &'static [(&'static str, &'static str)] {
    match lang {
        crate::i18n::Language::English => CHAPTERS,
        crate::i18n::Language::Ukrainian => CHAPTERS_UK,
    }
}

const ONLINE: &str = "https://github.com/mallardask-ops/keyframexl/blob/main/docs/manual/";
const ONLINE_UK: &str = "https://github.com/mallardask-ops/keyframexl/blob/main/docs/manual/uk/";

/// A chapter's title: its first `# ` heading.
pub(crate) fn title(src: &str) -> &str {
    src.lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or("Untitled")
        .trim()
}

/// Chapters that contain every word of `query` (case-insensitive), with
/// how many times the first word occurs.
pub(crate) fn search(chapters: &[(&str, &str)], query: &str) -> Vec<(usize, usize)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return (0..chapters.len()).map(|i| (i, 0)).collect();
    }
    chapters
        .iter()
        .enumerate()
        .filter_map(|(i, (_, src))| {
            let low = src.to_lowercase();
            words
                .iter()
                .all(|w| low.contains(w.as_str()))
                .then(|| (i, low.matches(words[0].as_str()).count()))
        })
        .collect()
}

/// The chapter a link points at (`03-sequencing.md`, `./03-sequencing.md#x`).
fn chapter_of(chapters: &[(&str, &str)], url: &str) -> Option<usize> {
    let file = url.trim_start_matches("./").split('#').next()?;
    chapters.iter().position(|(f, _)| *f == file)
}

impl App {
    pub(super) fn manual_window(&mut self, ctx: &egui::Context) {
        if !self.show_manual {
            return;
        }
        let tr = self.tr();
        let chapters = chapters(tr.lang);
        let online = match tr.lang {
            crate::i18n::Language::English => ONLINE,
            crate::i18n::Language::Ukrainian => ONLINE_UK,
        };
        let mut open = true;
        let page_id = egui::Id::new("manual-page");
        let query_id = egui::Id::new("manual-query");
        let mut page: usize = ctx
            .data(|d| d.get_temp(page_id))
            .unwrap_or(0usize)
            .min(chapters.len() - 1);
        let mut query: String = ctx.data(|d| d.get_temp(query_id)).unwrap_or_default();
        let mut follow = None;
        let screen = ctx.screen_rect();
        egui::Window::new(tr.help_manual)
            .id(egui::Id::new("kfxl-manual-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(
                (screen.width() - 120.0).clamp(420.0, 860.0),
                (screen.height() - 160.0).clamp(300.0, 640.0),
            ))
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 60.0))
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut query)
                                .hint_text(tr.manual_search_hint)
                                .desired_width(190.0),
                        );
                        ui.add_space(4.0);
                        let hits = search(chapters, &query);
                        if hits.is_empty() {
                            ui.label(
                                RichText::new(tr.manual_no_match)
                                    .font(t::sans(12.0))
                                    .color(t::faint()),
                            );
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("manual-toc")
                            .show(ui, |ui| {
                                for (i, n) in hits {
                                    let name = title(chapters[i].1);
                                    let label = if n > 0 {
                                        format!("{name}  ·  {n}")
                                    } else {
                                        name.to_string()
                                    };
                                    let r = ui.add(
                                        egui::Button::new(RichText::new(label).font(t::sans(13.0)))
                                            .selected(i == page)
                                            .min_size(vec2(190.0, 0.0)),
                                    );
                                    if r.clicked() {
                                        page = i;
                                    }
                                }
                                ui.add_space(8.0);
                                if ui
                                    .link(RichText::new(tr.manual_open_online).font(t::sans(12.0)))
                                    .on_hover_text(tr.manual_open_online_tip)
                                    .clicked()
                                {
                                    follow = Some(format!("{online}{}", chapters[page].0));
                                }
                            });
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt(("manual-text", page))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(ui.available_width().min(640.0));
                                if let Some(url) = markdown::show(ui, chapters[page].1, 13.0) {
                                    follow = Some(url);
                                }
                                ui.add_space(12.0);
                                ui.horizontal(|ui| {
                                    if page > 0 && ui.button(tr.manual_prev).clicked() {
                                        page -= 1;
                                    }
                                    if page + 1 < chapters.len()
                                        && ui.button(tr.manual_next).clicked()
                                    {
                                        page += 1;
                                    }
                                });
                            });
                        });
                });
            });
        if let Some(url) = follow {
            match chapter_of(chapters, &url) {
                Some(i) => page = i,
                None => {
                    if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
                        let err = e.to_string();
                        self.say(
                            super::Tone::Bad,
                            crate::i18n::fill(tr.manual_open_browser_err, &[("err", &err)]),
                        );
                    }
                }
            }
        }
        ctx.data_mut(|d| {
            d.insert_temp(page_id, page);
            d.insert_temp(query_id, query);
        });
        self.show_manual = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_manual_page_is_built_in_and_titled() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/manual");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|f| f.ends_with(".md") && f.as_bytes()[0].is_ascii_digit())
            .collect();
        files.sort();
        let built: Vec<&str> = CHAPTERS.iter().map(|(f, _)| *f).collect();
        assert_eq!(files, built, "add new chapters to CHAPTERS");
        let uk: Vec<&str> = CHAPTERS_UK.iter().map(|(f, _)| *f).collect();
        assert_eq!(
            built, uk,
            "every chapter needs a Ukrainian translation in docs/manual/uk"
        );
        for (f, src) in CHAPTERS.iter().chain(CHAPTERS_UK) {
            assert_ne!(title(src), "Untitled", "{f} has no # title");
        }
    }

    #[test]
    fn search_and_links_find_chapters() {
        let forge = CHAPTERS
            .iter()
            .position(|(f, _)| *f == "06-forge.md")
            .unwrap();
        assert!(
            search(CHAPTERS, "mutate KFXforge")
                .iter()
                .any(|(i, _)| *i == forge)
        );
        assert!(search(CHAPTERS, "zzqqxx").is_empty());
        assert!(!search(CHAPTERS_UK, "мутація").is_empty());
        assert_eq!(search(CHAPTERS, "").len(), CHAPTERS.len());
        assert_eq!(chapter_of(CHAPTERS, "./06-forge.md#layers"), Some(forge));
        assert_eq!(chapter_of(CHAPTERS, "https://example.com"), None);
    }
}
