//! Scripted input for live checks of the real app (`KFXL_CAPTURE_SCRIPT`):
//! pointer, wheel, key and text events are fed through eframe's raw input
//! hook at given times, so the whole app reacts as to a person.
//!
//! One command per line, `#` starts a comment, times in ms from start:
//! `500 move X Y` · `500 click X Y` · `500 dbl X Y` · `500 drag X1 Y1 X2 Y2`
//! · `500 wheel X Y NOTCHES` · `500 key Enter [ctrl] [shift] [alt]` · `500 text 97`
//! · `500 rclick X Y` (right button) · `500 close` (the window manager asks the window to close)
//! · `500 drop /path/file.mid` (a file dropped on the window)
//! · `500 down X Y` / `900 up X Y` (hold the left button; `move` while held drags)

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, pos2, vec2};

#[derive(Default)]
pub struct Script {
    /// (time in seconds, modifiers, event), in time order.
    events: Vec<(f64, Modifiers, Event)>,
    next: usize,
    /// Times the window manager asks the window to close (`close`).
    closes: Vec<f64>,
    /// Files dropped on the window (`drop PATH`), as from a file manager or a plugin.
    drops: Vec<(f64, std::path::PathBuf)>,
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}

impl Script {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut events = Vec::new();
        let mut closes = Vec::new();
        let mut drops = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let bad = || format!("script line {}: {line}", n + 1);
            let w: Vec<&str> = line.split_whitespace().collect();
            let t = w
                .first()
                .and_then(|v| v.parse::<f64>().ok())
                .ok_or_else(bad)?
                / 1000.0;
            let num = |i: usize| w.get(i).and_then(|v| v.parse::<f32>().ok()).ok_or_else(bad);
            let none = Modifiers::default();
            match w.get(1).copied() {
                Some("move") => events.push((t, none, Event::PointerMoved(pos2(num(2)?, num(3)?)))),
                Some("click") | Some("dbl") => {
                    let p = pos2(num(2)?, num(3)?);
                    let times = if w[1] == "dbl" { 2 } else { 1 };
                    events.push((t, none, Event::PointerMoved(p)));
                    for k in 0..times {
                        let at = t + 0.02 + k as f64 * 0.12;
                        events.push((at, none, button(p, true)));
                        events.push((at + 0.05, none, button(p, false)));
                    }
                }
                Some("rclick") => {
                    let p = pos2(num(2)?, num(3)?);
                    events.push((t, none, Event::PointerMoved(p)));
                    let rb = |pressed| Event::PointerButton {
                        pos: p,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::default(),
                    };
                    events.push((t + 0.02, none, rb(true)));
                    events.push((t + 0.07, none, rb(false)));
                }
                Some("down") | Some("up") => {
                    let p = pos2(num(2)?, num(3)?);
                    events.push((t, none, Event::PointerMoved(p)));
                    events.push((t + 0.01, none, button(p, w[1] == "down")));
                }
                Some("drag") => {
                    let (a, b) = (pos2(num(2)?, num(3)?), pos2(num(4)?, num(5)?));
                    events.push((t, none, Event::PointerMoved(a)));
                    events.push((t + 0.02, none, button(a, true)));
                    for k in 1..=12 {
                        events.push((
                            t + 0.02 + k as f64 * 0.025,
                            none,
                            Event::PointerMoved(a + (b - a) * (k as f32 / 12.0)),
                        ));
                    }
                    events.push((t + 0.4, none, button(b, false)));
                }
                Some("wheel") => {
                    let p = pos2(num(2)?, num(3)?);
                    events.push((t, none, Event::PointerMoved(p)));
                    let notches = num(4)? as i32;
                    for k in 0..notches.abs() {
                        let delta = vec2(0.0, 50.0 * notches.signum() as f32);
                        events.push((
                            t + 0.02 + k as f64 * 0.03,
                            none,
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta,
                                modifiers: none,
                            },
                        ));
                    }
                }
                Some("key") => {
                    let key = w
                        .get(2)
                        .and_then(|k| egui::Key::from_name(k))
                        .ok_or_else(bad)?;
                    let has = |m: &str| w[3..].contains(&m);
                    let modifiers = Modifiers {
                        ctrl: has("ctrl"),
                        command: has("ctrl"),
                        shift: has("shift"),
                        alt: has("alt"),
                        ..Default::default()
                    };
                    let ev = |pressed| Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers,
                    };
                    events.push((t, modifiers, ev(true)));
                    events.push((t + 0.05, modifiers, ev(false)));
                }
                Some("close") => closes.push(t),
                Some("drop") if w.len() > 2 => {
                    drops.push((t, std::path::PathBuf::from(w[2..].join(" "))))
                }
                Some("text") => events.push((t, none, Event::Text(w[2..].join(" ")))),
                _ => return Err(bad()),
            }
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Self {
            events,
            next: 0,
            closes,
            drops,
        })
    }

    /// Feed the events that are due at `now` (seconds since start).
    pub fn feed(&mut self, now: f64, raw: &mut egui::RawInput) {
        if let Some(i) = self.closes.iter().position(|t| *t <= now) {
            self.closes.remove(i);
            raw.viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .events
                .push(egui::ViewportEvent::Close);
        }
        while let Some(i) = self.drops.iter().position(|(t, _)| *t <= now) {
            let (_, path) = self.drops.remove(i);
            raw.dropped_files.push(egui::DroppedFile {
                path: Some(path),
                ..Default::default()
            });
        }
        while let Some((t, mods, ev)) = self.events.get(self.next) {
            if *t > now {
                break;
            }
            raw.modifiers = *mods;
            raw.events.push(ev.clone());
            if std::env::var_os("KFXL_SCRIPT_TRACE").is_some() {
                eprintln!("script {now:.3}s: {ev:?}");
            }
            self.next += 1;
        }
    }

    pub fn pending(&self) -> bool {
        self.next < self.events.len() || !self.closes.is_empty() || !self.drops.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_lines_become_timed_events() {
        let mut s =
            Script::parse("# demo\n100 dbl 10 20\n300 key N ctrl\n400 text 97\n500 close\n")
                .unwrap();
        let mut raw = egui::RawInput::default();
        s.feed(0.2, &mut raw);
        let presses = raw
            .events
            .iter()
            .filter(|e| matches!(e, Event::PointerButton { pressed: true, .. }))
            .count();
        assert_eq!(
            presses, 1,
            "the second click of the double-click is not due yet"
        );
        let mut raw = egui::RawInput::default();
        s.feed(0.6, &mut raw);
        assert!(raw.events.iter().any(|e| matches!(e, Event::Key { key: egui::Key::N, pressed: true, modifiers, .. } if modifiers.command)));
        assert!(raw.events.contains(&Event::Text("97".into())));
        assert!(
            raw.viewports[&egui::ViewportId::ROOT]
                .events
                .contains(&egui::ViewportEvent::Close)
        );
        assert!(!s.pending());
        assert!(Script::parse("100 jump 1 2").is_err());
    }
}
