//! Linux/X11 embedded plugin editor containers (two-window clip hierarchy,
//! XShape bounding regions, ICCCM ConfigureNotify + WM_STATE for yabridge/Wine).

use super::NativeWindow;
use super::host::{Handle, Loaded, Reply};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// An editor living inside our window as a child X11 window.
pub(super) struct Embedded {
    /// Shows only `clip` of the editor; `win` sits inside it, offset.
    frame: xcb::x::Window,
    /// The plugin's parent window, the editor's full size.
    win: xcb::x::Window,
    rect: [i32; 4],
    clip: [i32; 4],
    visible: bool,
    /// Screen position last told to the editor (`announce`).
    told: Option<(i16, i16)>,
    /// Plugin windows already marked as shown (`mark_shown`).
    marked: Vec<u32>,
}

/// Owns the host thread's dedicated X11 connection for child editor windows.
pub(super) struct Editors {
    conn: Option<xcb::Connection>,
    last_told: Instant,
}

impl Editors {
    pub(super) fn new() -> Self {
        Self {
            conn: None,
            last_told: Instant::now(),
        }
    }

    fn ensure_conn(&mut self) -> Result<&xcb::Connection, String> {
        if self.conn.is_none() {
            self.conn = xcb::Connection::connect(None).ok().map(|(c, _)| c);
        }
        self.conn
            .as_ref()
            .ok_or_else(|| "No X11 display for the editor".to_string())
    }

    pub(super) fn embed(
        &mut self,
        plugin: &Handle,
        parent: NativeWindow,
        rect: [i32; 4],
        clip: [i32; 4],
        regions: &[[i32; 4]],
    ) -> Result<(Embedded, i32, i32), String> {
        let NativeWindow::X11(parent_id) = parent else {
            return Err("Expected an X11 parent window on Linux".into());
        };
        let conn = self.ensure_conn()?;
        let (frame, win, w, h) = embed_handle(plugin, conn, parent_id, rect, clip)?;
        shape_editor(conn, frame, clip, regions);
        Ok((
            Embedded {
                frame,
                win,
                rect: [rect[0], rect[1], w, h],
                clip,
                visible: true,
                told: None,
                marked: Vec::new(),
            },
            w,
            h,
        ))
    }

    pub(super) fn update(
        &mut self,
        e: &mut Embedded,
        rect: [i32; 4],
        clip: [i32; 4],
        regions: &[[i32; 4]],
    ) {
        let Ok(conn) = self.ensure_conn() else {
            return;
        };
        let visible = !regions.is_empty();
        if visible {
            shape_editor(conn, e.frame, clip, regions);
        }
        place(conn, e, rect, clip, visible);
    }

    pub(super) fn unembed(&mut self, plugin: &Handle, e: Embedded) {
        plugin.close_editor();
        if let Some(c) = &self.conn {
            c.send_request(&xcb::x::DestroyWindow { window: e.frame });
            let _ = c.flush();
        }
    }

    pub(super) fn poll(
        &mut self,
        loaded: &mut HashMap<u64, Loaded>,
        tx: &mpsc::Sender<Reply>,
        poke: &dyn Fn(),
    ) {
        if let Some(conn) = &self.conn {
            while let Ok(Some(_)) = conn.poll_for_event() {}
        }
        let tell = self.last_told.elapsed() >= Duration::from_millis(200);
        if tell {
            self.last_told = Instant::now();
        }
        for (id, l) in loaded.iter_mut() {
            let Some(e) = &mut l.embed else { continue };
            if tell
                && e.visible
                && let Some(conn) = &self.conn
            {
                mark_shown(conn, e);
                announce(conn, e);
            }
            if let Some((w, h)) = l.plugin.service()
                && let Some(conn) = &self.conn
            {
                let rect = [e.rect[0], e.rect[1], w, h];
                let (clip, vis) = (e.clip, e.visible);
                place(conn, e, rect, clip, vis);
                let _ = tx.send(Reply::EditorSize { id: *id, w, h });
                poke();
            }
        }
    }
}

fn embed_handle(
    handle: &Handle,
    conn: &xcb::Connection,
    parent: u32,
    rect: [i32; 4],
    clip: [i32; 4],
) -> Result<(xcb::x::Window, xcb::x::Window, i32, i32), String> {
    match handle {
        Handle::V3(p) => {
            let mut p = p.lock().map_err(|_| "busy".to_string())?;
            let size = p.get_editor_size().ok();
            create_and_attach(conn, size, parent, rect, clip, |win| {
                p.open_editor(vst3_host::plugin::WindowHandle::from_x11(win))
                    .map_err(|e| e.to_string())
            })
        }
        Handle::V2(p) => {
            let mut p = p.lock().map_err(|_| "busy".to_string())?;
            // Size after opening: some plugins (Odyssey under yabridge) never
            // answer effEditGetRect before their editor exists.
            let (frame, win, w, h) = create_and_attach(conn, None, parent, rect, clip, |win| {
                if p.editor_open(win as usize) {
                    Ok(())
                } else {
                    Err("the plugin did not open its editor".into())
                }
            })?;
            let size = p
                .editor_rect()
                .map(|r| ((r.right - r.left) as i32, (r.bottom - r.top) as i32))
                .filter(|s| s.0 > 0 && s.1 > 0);
            match size {
                Some((nw, nh)) if (nw, nh) != (w, h) => {
                    conn.send_request(&xcb::x::ConfigureWindow {
                        window: win,
                        value_list: &[
                            xcb::x::ConfigWindow::Width(nw as u32),
                            xcb::x::ConfigWindow::Height(nh as u32),
                        ],
                    });
                    let _ = conn.flush();
                    Ok((frame, win, nw, nh))
                }
                _ => Ok((frame, win, w, h)),
            }
        }
        Handle::Clap(p) => {
            let mut p = p.lock().map_err(|_| "busy".to_string())?;
            let mut opened_size: Option<(i32, i32)> = None;
            let (frame, win, w, h) = create_and_attach(conn, None, parent, rect, clip, |win| {
                let sz = p.editor_open(win as u64)?;
                opened_size = Some(sz);
                Ok(())
            })?;
            match opened_size.filter(|s| s.0 > 0 && s.1 > 0) {
                Some((nw, nh)) if (nw, nh) != (w, h) => {
                    conn.send_request(&xcb::x::ConfigureWindow {
                        window: win,
                        value_list: &[
                            xcb::x::ConfigWindow::Width(nw as u32),
                            xcb::x::ConfigWindow::Height(nh as u32),
                        ],
                    });
                    let _ = conn.flush();
                    Ok((frame, win, nw, nh))
                }
                _ => Ok((frame, win, w, h)),
            }
        }
        Handle::Fl => {
            Err("The FL Studio link has no editor here: FL Studio shows its own window".into())
        }
    }
}

/// Create the clip frame under `parent`, the editor area inside it, and attach
/// the editor. Returns `(frame, area, editor width, height)`.
fn create_and_attach(
    conn: &xcb::Connection,
    size: Option<(i32, i32)>,
    parent: u32,
    rect: [i32; 4],
    clip: [i32; 4],
    open: impl FnOnce(u32) -> Result<(), String>,
) -> Result<(xcb::x::Window, xcb::x::Window, i32, i32), String> {
    use xcb::{Xid, XidNew};
    let (w, h) = size.unwrap_or((rect[2], rect[3]));
    let (w, h) = (w.max(1), h.max(1));
    let child = |parent: xcb::x::Window, r: [i32; 4]| -> Result<xcb::x::Window, String> {
        let win: xcb::x::Window = conn.generate_id();
        conn.send_and_check_request(&xcb::x::CreateWindow {
            depth: xcb::x::COPY_FROM_PARENT as u8,
            wid: win,
            parent,
            x: r[0] as i16,
            y: r[1] as i16,
            width: r[2].max(1) as u16,
            height: r[3].max(1) as u16,
            border_width: 0,
            class: xcb::x::WindowClass::InputOutput,
            visual: xcb::x::COPY_FROM_PARENT,
            value_list: &[xcb::x::Cw::BackPixel(0)],
        })
        .map_err(|e| format!("Cannot create the editor area: {e}"))?;
        conn.send_request(&xcb::x::MapWindow { window: win });
        Ok(win)
    };
    let frame = child(xcb::x::Window::new(parent), clip)?;
    let win = match child(frame, [rect[0] - clip[0], rect[1] - clip[1], w, h]) {
        Ok(w) => w,
        Err(e) => {
            conn.send_request(&xcb::x::DestroyWindow { window: frame });
            let _ = conn.flush();
            return Err(e);
        }
    };
    let _ = conn.flush();
    open(win.resource_id()).map_err(|e| {
        conn.send_request(&xcb::x::DestroyWindow { window: frame });
        let _ = conn.flush();
        format!("Editor: {e}")
    })?;
    Ok((frame, win, w, h))
}

fn place(conn: &xcb::Connection, e: &mut Embedded, rect: [i32; 4], clip: [i32; 4], visible: bool) {
    let geo = |r: [i32; 4]| {
        [
            xcb::x::ConfigWindow::X(r[0]),
            xcb::x::ConfigWindow::Y(r[1]),
            xcb::x::ConfigWindow::Width(r[2].max(1) as u32),
            xcb::x::ConfigWindow::Height(r[3].max(1) as u32),
        ]
    };
    if clip != e.clip {
        conn.send_request(&xcb::x::ConfigureWindow {
            window: e.frame,
            value_list: &geo(clip),
        });
    }
    if rect != e.rect || clip != e.clip {
        conn.send_request(&xcb::x::ConfigureWindow {
            window: e.win,
            value_list: &geo([rect[0] - clip[0], rect[1] - clip[1], rect[2], rect[3]]),
        });
    }
    e.rect = rect;
    e.clip = clip;
    if visible != e.visible {
        if visible {
            conn.send_request(&xcb::x::MapWindow { window: e.frame });
        } else {
            conn.send_request(&xcb::x::UnmapWindow { window: e.frame });
        }
        e.visible = visible;
    }
    let _ = conn.flush();
}

fn shape_editor(
    conn: &xcb::Connection,
    frame: xcb::x::Window,
    clip: [i32; 4],
    regions: &[[i32; 4]],
) {
    use xcb::{shape, x};
    let visible: Vec<_> = regions
        .iter()
        .map(|r| x::Rectangle {
            x: (r[0] - clip[0]) as i16,
            y: (r[1] - clip[1]) as i16,
            width: r[2] as u16,
            height: r[3] as u16,
        })
        .collect();
    // The bounding shape limits both painting and input for the child windows.
    conn.send_request(&shape::Rectangles {
        operation: shape::So::Set,
        destination_kind: shape::Sk::Bounding,
        ordering: x::ClipOrdering::Unsorted,
        destination_window: frame,
        x_offset: 0,
        y_offset: 0,
        rectangles: &visible,
    });
    let _ = conn.flush();
}

/// Tell the editor where its area is on screen, with a synthetic
/// ConfigureNotify on the area window (ICCCM: root coordinates).
fn announce(conn: &xcb::Connection, e: &mut Embedded) {
    let Ok(tree) = conn.wait_for_reply(conn.send_request(&xcb::x::QueryTree { window: e.win }))
    else {
        return;
    };
    let Ok(at) = conn.wait_for_reply(conn.send_request(&xcb::x::TranslateCoordinates {
        src_window: e.win,
        dst_window: tree.root(),
        src_x: 0,
        src_y: 0,
    })) else {
        return;
    };
    let pos = (at.dst_x(), at.dst_y());
    if e.told == Some(pos) {
        return;
    }
    e.told = Some(pos);
    let ev = xcb::x::ConfigureNotifyEvent::new(
        e.win,
        e.win,
        xcb::x::WINDOW_NONE,
        pos.0,
        pos.1,
        e.rect[2].max(1) as u16,
        e.rect[3].max(1) as u16,
        0,
        false,
    );
    conn.send_request(&xcb::x::SendEvent {
        propagate: false,
        destination: xcb::x::SendEventDest::Window(e.win),
        event_mask: xcb::x::EventMask::STRUCTURE_NOTIFY,
        event: &ev,
    });
    let _ = conn.flush();
}

/// Mark the plugin's own windows as shown (`WM_STATE` Normal) for Wine 10+.
fn mark_shown(conn: &xcb::Connection, e: &mut Embedded) {
    use xcb::Xid;
    static WM_STATE: std::sync::OnceLock<Option<xcb::x::Atom>> = std::sync::OnceLock::new();
    let Some(atom) = *WM_STATE.get_or_init(|| {
        conn.wait_for_reply(conn.send_request(&xcb::x::InternAtom {
            only_if_exists: false,
            name: b"WM_STATE",
        }))
        .ok()
        .map(|r| r.atom())
    }) else {
        return;
    };
    let children = |w: xcb::x::Window| {
        conn.wait_for_reply(conn.send_request(&xcb::x::QueryTree { window: w }))
            .map(|t| t.children().to_vec())
            .unwrap_or_default()
    };
    for top in children(e.win) {
        for w in std::iter::once(top).chain(children(top)) {
            if e.marked.contains(&w.resource_id()) {
                continue;
            }
            e.marked.push(w.resource_id());
            e.told = None;
            conn.send_request(&xcb::x::ChangeProperty {
                mode: xcb::x::PropMode::Replace,
                window: w,
                property: atom,
                r#type: atom,
                data: &[1u32, 0],
            });
        }
    }
    let _ = conn.flush();
}

/// The pointer over X11 window `win`, in its pixels (None off X11).
pub(super) fn pointer_in(win: NativeWindow) -> Option<(i32, i32)> {
    let NativeWindow::X11(id) = win else {
        return None;
    };
    let (conn, _) = xcb::Connection::connect(None).ok()?;
    let window = <xcb::x::Window as xcb::XidNew>::new(id);
    let r = conn
        .wait_for_reply(conn.send_request(&xcb::x::QueryPointer { window }))
        .ok()?;
    r.same_screen()
        .then_some((r.win_x() as i32, r.win_y() as i32))
}
