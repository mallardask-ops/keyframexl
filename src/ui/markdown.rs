//! Small Markdown renderer for GitHub release notes: headings, paragraphs,
//! nested bullets, numbered items, quotes, rules, fenced code, and inline
//! bold / italic / `code` / links / bare URLs. Not a full CommonMark parser.

use crate::theme as t;
use eframe::egui::{self, RichText, Stroke, vec2};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Inline {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        code: bool,
    },
    Link {
        text: String,
        url: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>),
    /// `marker` is "•" or the item's number ("3.").
    Item {
        depth: usize,
        marker: String,
        body: Vec<Inline>,
    },
    Quote(Vec<Inline>),
    Rule,
    Code(String),
}

/// Split Markdown source into blocks.
pub(crate) fn parse(src: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para = String::new();
    let mut code: Option<String> = None;

    let flush = |para: &mut String, blocks: &mut Vec<Block>| {
        if !para.trim().is_empty() {
            blocks.push(Block::Para(inline(para.trim())));
        }
        para.clear();
    };

    for raw in src.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim_start();

        if let Some(buf) = code.as_mut() {
            if trimmed.starts_with("```") {
                blocks.push(Block::Code(std::mem::take(buf)));
                code = None;
            } else {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(line);
            }
            continue;
        }
        if trimmed.starts_with("```") {
            flush(&mut para, &mut blocks);
            code = Some(String::new());
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
            continue;
        }
        if is_rule(trimmed) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Rule);
            continue;
        }
        let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut para, &mut blocks);
            let text = trimmed[hashes..].trim().trim_end_matches('#').trim();
            blocks.push(Block::Heading(hashes as u8, inline(text)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Quote(inline(rest.trim())));
            continue;
        }
        if let Some((marker, body)) = list_item(trimmed) {
            flush(&mut para, &mut blocks);
            let indent = line.len() - trimmed.len();
            blocks.push(Block::Item {
                depth: (indent / 2).min(4),
                marker,
                body: inline(body),
            });
            continue;
        }
        // Continuation of a list item's text (indented, no marker).
        if para.is_empty()
            && line.starts_with(' ')
            && let Some(Block::Item { body, .. }) = blocks.last_mut()
        {
            body.push(plain(" "));
            body.extend(inline(trimmed));
            continue;
        }
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(trimmed);
    }
    if let Some(buf) = code {
        blocks.push(Block::Code(buf));
    }
    flush(&mut para, &mut blocks);
    blocks
}

fn is_rule(s: &str) -> bool {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    s.len() >= 3
        && (s.chars().all(|c| c == '-')
            || s.chars().all(|c| c == '*')
            || s.chars().all(|c| c == '_'))
}

fn list_item(s: &str) -> Option<(String, &str)> {
    for m in ["- ", "* ", "+ "] {
        if let Some(rest) = s.strip_prefix(m) {
            return Some(("•".into(), rest.trim_start()));
        }
    }
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if (1..=3).contains(&digits) {
        let after = &s[digits..];
        if let Some(rest) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            return Some((format!("{}.", &s[..digits]), rest.trim_start()));
        }
    }
    None
}

fn plain(s: &str) -> Inline {
    Inline::Text {
        text: s.into(),
        bold: false,
        italic: false,
        code: false,
    }
}

/// Parse inline spans: `**bold**`, `__bold__`, `*italic*`, `` `code` ``,
/// `[text](url)`, and bare `http(s)://` URLs.
pub(crate) fn inline(s: &str) -> Vec<Inline> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<Inline> = Vec::new();
    let mut buf = String::new();
    let (mut bold, mut italic) = (false, false);
    let mut i = 0;

    let push = |out: &mut Vec<Inline>, buf: &mut String, bold: bool, italic: bool| {
        if buf.is_empty() {
            return;
        }
        let text = std::mem::take(buf);
        if let Some(Inline::Text {
            text: prev,
            bold: b,
            italic: it,
            code: false,
        }) = out.last_mut()
            && *b == bold
            && *it == italic
        {
            prev.push_str(&text);
            return;
        }
        out.push(Inline::Text {
            text,
            bold,
            italic,
            code: false,
        });
    };
    let find = |from: usize, pat: &[char]| -> Option<usize> {
        (from..chars.len().saturating_sub(pat.len() - 1)).find(|&j| chars[j..].starts_with(pat))
    };

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        // Backslash escape.
        if c == '\\' && next.is_some_and(|n| n.is_ascii_punctuation()) {
            buf.push(next.unwrap_or_default());
            i += 2;
            continue;
        }
        if c == '`'
            && let Some(end) = find(i + 1, &['`'])
        {
            push(&mut out, &mut buf, bold, italic);
            out.push(Inline::Text {
                text: chars[i + 1..end].iter().collect(),
                bold: false,
                italic: false,
                code: true,
            });
            i = end + 1;
            continue;
        }
        if (c == '*' || c == '_') && next == Some(c) {
            // Only toggle on when something closes it later.
            if bold || find(i + 2, &[c, c]).is_some() {
                push(&mut out, &mut buf, bold, italic);
                bold = !bold;
                i += 2;
                continue;
            }
        }
        if c == '*' {
            let opens = !italic
                && next.is_some_and(|n| !n.is_whitespace())
                && find(i + 1, &['*']).is_some();
            let closes = italic && i > 0 && !chars[i - 1].is_whitespace();
            if opens || closes {
                push(&mut out, &mut buf, bold, italic);
                italic = !italic;
                i += 1;
                continue;
            }
        }
        if c == '['
            && let Some(close) = find(i + 1, &[']'])
            && chars.get(close + 1) == Some(&'(')
            && let Some(paren) = find(close + 2, &[')'])
        {
            push(&mut out, &mut buf, bold, italic);
            let text: String = chars[i + 1..close].iter().collect();
            let url: String = chars[close + 2..paren].iter().collect();
            out.push(Inline::Link {
                text: text.replace(['*', '`'], ""),
                url: url.trim().to_string(),
            });
            i = paren + 1;
            continue;
        }
        if c == 'h' && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let rest: String = chars[i..chars.len().min(i + 8)].iter().collect();
            if rest.starts_with("https://") || rest.starts_with("http://") {
                let mut end = i;
                while end < chars.len() && !chars[end].is_whitespace() {
                    end += 1;
                }
                while end > i && matches!(chars[end - 1], '.' | ',' | ')' | ';' | ':' | '!' | '?') {
                    end -= 1;
                }
                push(&mut out, &mut buf, bold, italic);
                let url: String = chars[i..end].iter().collect();
                out.push(Inline::Link {
                    text: short_url(&url),
                    url,
                });
                i = end;
                continue;
            }
        }
        buf.push(c);
        i += 1;
    }
    push(&mut out, &mut buf, bold, italic);
    out
}

/// GitHub PR/compare URLs read better short: `#12`, `v0.1.0...v0.1.1`.
fn short_url(url: &str) -> String {
    let path = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let parts: Vec<&str> = path.split('/').collect();
    if parts.first() == Some(&"github.com") && parts.len() >= 5 {
        match parts[3] {
            "pull" | "issues" => return format!("#{}", parts[4]),
            "compare" => return parts[4..].join("/"),
            _ => {}
        }
    }
    url.to_string()
}

/// Draw `src`; returns the URL of a clicked link.
pub(crate) fn show(ui: &mut egui::Ui, src: &str, size: f32) -> Option<String> {
    let mut clicked = None;
    let blocks = parse(src);
    let width = ui.available_width();
    for (n, block) in blocks.iter().enumerate() {
        let first = n == 0;
        match block {
            Block::Heading(level, spans) => {
                if !first {
                    ui.add_space(if *level <= 2 { 8.0 } else { 4.0 });
                }
                let hsize = match level {
                    1 => size + 4.0,
                    2 => size + 2.5,
                    3 => size + 1.0,
                    _ => size,
                };
                wrapped(ui, spans, hsize, t::text(), true, &mut clicked);
            }
            Block::Para(spans) => wrapped(ui, spans, size, t::text(), false, &mut clicked),
            Block::Item {
                depth,
                marker,
                body,
            } => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(4.0 + *depth as f32 * 16.0);
                    let marker_w = if marker == "•" { 14.0 } else { 22.0 };
                    let (r, _) =
                        ui.allocate_exact_size(vec2(marker_w, size + 3.0), egui::Sense::hover());
                    ui.painter().text(
                        r.left_center(),
                        egui::Align2::LEFT_CENTER,
                        marker,
                        t::sans(size),
                        if *depth == 0 { t::wait() } else { t::muted() },
                    );
                    ui.vertical(|ui| {
                        wrapped(ui, body, size, t::text(), false, &mut clicked);
                    });
                });
            }
            Block::Quote(spans) => {
                egui::Frame::new()
                    .fill(t::wait_soft())
                    .stroke(Stroke::new(1.0_f32, t::line()))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        wrapped(ui, spans, size, t::muted(), false, &mut clicked);
                    });
            }
            Block::Rule => {
                ui.add_space(2.0);
                let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, t::line());
                ui.add_space(2.0);
            }
            Block::Code(text) => {
                egui::Frame::new()
                    .fill(t::bg())
                    .stroke(Stroke::new(1.0_f32, t::line()))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(text)
                                .font(t::mono(size - 1.0))
                                .color(t::text()),
                        );
                    });
            }
        }
    }
    clicked
}

fn wrapped(
    ui: &mut egui::Ui,
    spans: &[Inline],
    size: f32,
    color: egui::Color32,
    heading: bool,
    clicked: &mut Option<String>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for span in spans {
            match span {
                Inline::Text {
                    text,
                    bold,
                    italic,
                    code,
                } => {
                    let mut rt = RichText::new(text);
                    rt = if *code {
                        rt.font(t::mono(size - 1.0))
                            .color(t::text())
                            .background_color(t::raised())
                    } else if *bold || heading {
                        rt.font(t::semibold(size)).color(t::text())
                    } else {
                        rt.font(t::sans(size)).color(color)
                    };
                    if *italic {
                        rt = rt.italics();
                    }
                    ui.label(rt);
                }
                Inline::Link { text, url } => {
                    let font = if heading {
                        t::semibold(size)
                    } else {
                        t::sans(size)
                    };
                    let resp = ui
                        .link(RichText::new(text).font(font).color(t::enc_blue()))
                        .on_hover_text(url);
                    if resp.clicked() {
                        *clicked = Some(url.clone());
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(s: &str, bold: bool, italic: bool, code: bool) -> Inline {
        Inline::Text {
            text: s.into(),
            bold,
            italic,
            code,
        }
    }

    #[test]
    fn inline_bold_italic_code_links() {
        assert_eq!(
            inline("a **b** *c* `d` [e](http://x)"),
            vec![
                txt("a ", false, false, false),
                txt("b", true, false, false),
                txt(" ", false, false, false),
                txt("c", false, true, false),
                txt(" ", false, false, false),
                txt("d", false, false, true),
                txt(" ", false, false, false),
                Inline::Link {
                    text: "e".into(),
                    url: "http://x".into()
                },
            ]
        );
        // Lone stars and snake_case stay text.
        assert_eq!(
            inline("2 * 3 snake_case"),
            vec![txt("2 * 3 snake_case", false, false, false)]
        );
    }

    #[test]
    fn bare_github_urls_shorten() {
        let spans = inline("by @me in https://github.com/o/r/pull/12.");
        assert_eq!(
            spans[1],
            Inline::Link {
                text: "#12".into(),
                url: "https://github.com/o/r/pull/12".into()
            }
        );
        assert_eq!(spans[2], txt(".", false, false, false));
        let cmp = inline("https://github.com/o/r/compare/v0.1.0...v0.1.1");
        assert!(matches!(&cmp[0], Inline::Link { text, .. } if text == "v0.1.0...v0.1.1"));
    }

    #[test]
    fn blocks_from_release_notes() {
        let src = "## Title\n\nSome **bold**\ntext.\n\n> note\n\n---\n\n- one\n  - nested\n1. first\n\n```sh\ntar -xzf a\n./b\n```\n";
        let b = parse(src);
        assert!(matches!(&b[0], Block::Heading(2, _)));
        assert!(matches!(&b[1], Block::Para(s) if s.len() == 3));
        assert!(matches!(&b[2], Block::Quote(_)));
        assert_eq!(b[3], Block::Rule);
        assert!(matches!(&b[4], Block::Item { depth: 0, marker, .. } if marker == "•"));
        assert!(matches!(&b[5], Block::Item { depth: 1, .. }));
        assert!(matches!(&b[6], Block::Item { marker, .. } if marker == "1."));
        assert_eq!(b[7], Block::Code("tar -xzf a\n./b".into()));
    }
}
