//! Windows/Win32 embedded plugin editor containers:
//! - Two-window `WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS` hierarchy
//!   (`frame` clips to viewport, `win` holds the full plugin editor at offset).
//! - Multi-rect `SetWindowRgn` clipping so egui menus, popups, and overlapping
//!   dock windows paint and receive clicks cleanly above embedded editors.
//! - Per-Monitor v2 DPI forwarding (`GetDpiForWindow`) for VST3 and CLAP.
//! - Keyboard hotkey forwarding (`WM_KEYDOWN`/`WM_KEYUP` → host root window).
//! - Per-frame Win32 message pumping (`PeekMessageW` / `DispatchMessageW`) on
//!   the plugin host thread.

use super::NativeWindow;
use super::host::{Handle, Loaded, Reply};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::{OnceLock, mpsc};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, ScreenToClient, SetWindowRgn,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
            DispatchMessageW, GA_ROOT, GetAncestor, GetCursorPos, MSG, PM_REMOVE, PeekMessageW,
            PostMessageW, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOZORDER, SetWindowPos, ShowWindow, TranslateMessage, WM_KEYDOWN, WM_KEYUP,
            WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
            WS_VISIBLE,
        },
    },
};

const CONTAINER_CLASS: &str = "KeyFrameXLPluginHost\0";

fn class_name_utf16() -> &'static [u16] {
    static NAME: OnceLock<Vec<u16>> = OnceLock::new();
    NAME.get_or_init(|| CONTAINER_CLASS.encode_utf16().collect())
}

fn ensure_window_class() -> Result<(), String> {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let ok = *REGISTERED.get_or_init(|| unsafe {
        let hinstance = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(container_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name_utf16().as_ptr(),
            hIconSm: ptr::null_mut(),
        };
        RegisterClassExW(&wc) != 0
    });
    if ok {
        Ok(())
    } else {
        Err("Failed to register Win32 plugin container window class".into())
    }
}

/// Window procedure for the host's child container HWNDs: forwards unhandled
/// keyboard messages to the top-level eframe window so DAW hotkeys (Space,
/// shortcuts) keep working when the plugin container holds focus.
unsafe extern "system" fn container_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
            let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
            if !root.is_null() && root != hwnd {
                unsafe {
                    PostMessageW(root, msg, wparam, lparam);
                }
                return 0;
            }
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// An editor living inside our Win32 host window as a pair of child HWNDs.
pub(super) struct Embedded {
    /// Clipped to `clip` (and shaped with `SetWindowRgn` for `regions`).
    frame: HWND,
    /// Child of `frame`, positioned at `[rect[0]-clip[0], rect[1]-clip[1]]` with full editor size.
    win: HWND,
    rect: [i32; 4],
    clip: [i32; 4],
    visible: bool,
    dpi: u32,
}

pub(super) struct Editors;

impl Editors {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn embed(
        &mut self,
        plugin: &Handle,
        parent: NativeWindow,
        rect: [i32; 4],
        clip: [i32; 4],
        regions: &[[i32; 4]],
    ) -> Result<(Embedded, i32, i32), String> {
        let NativeWindow::Win32(parent_raw) = parent else {
            return Err("Expected a Win32 HWND parent window on Windows".into());
        };
        let parent_hwnd = parent_raw as usize as HWND;
        if parent_hwnd.is_null() {
            return Err("Null parent HWND".into());
        }
        let (frame, win, w, h, dpi) = embed_handle(plugin, parent_hwnd, rect, clip)?;
        apply_regions(frame, clip, regions);
        let visible = !regions.is_empty();
        if !visible {
            unsafe {
                ShowWindow(frame, SW_HIDE);
            }
        }
        Ok((
            Embedded {
                frame,
                win,
                rect: [rect[0], rect[1], w, h],
                clip,
                visible,
                dpi,
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
        let visible = !regions.is_empty();
        if visible {
            apply_regions(e.frame, clip, regions);
        }
        place(e, rect, clip, visible);
    }

    pub(super) fn unembed(&mut self, plugin: &Handle, e: Embedded) {
        // Always detach the plugin editor before destroying its parent HWND.
        plugin.close_editor();
        unsafe {
            DestroyWindow(e.frame);
        }
    }

    pub(super) fn poll(
        &mut self,
        loaded: &mut HashMap<u64, Loaded>,
        tx: &mpsc::Sender<Reply>,
        poke: &dyn Fn(),
    ) {
        pump_messages();
        for (id, l) in loaded.iter_mut() {
            let Some(e) = &mut l.embed else { continue };
            // Per-Monitor v2 DPI tracking: notify VST3/CLAP if the window moved
            // onto a monitor with a different scaling factor.
            let dpi = window_dpi(e.win);
            if dpi != e.dpi {
                e.dpi = dpi;
                let scale = dpi as f32 / 96.0;
                forward_dpi_scale(&l.plugin, scale);
            }
            if let Some((w, h)) = l.plugin.service() {
                let rect = [e.rect[0], e.rect[1], w, h];
                let (clip, vis) = (e.clip, e.visible);
                place(e, rect, clip, vis);
                let _ = tx.send(Reply::EditorSize { id: *id, w, h });
                poke();
            }
        }
    }
}

fn window_dpi(hwnd: HWND) -> u32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 0 { dpi } else { 96 }
}

fn forward_dpi_scale(handle: &Handle, scale: f32) {
    match handle {
        Handle::V3(p) => {
            if let Ok(mut p) = p.try_lock() {
                let _ = p.set_editor_scale_factor(scale);
            }
        }
        Handle::Clap(p) => {
            if let Ok(mut p) = p.try_lock() {
                p.editor_set_scale(scale as f64);
            }
        }
        Handle::V2(_) | Handle::Fl => {}
    }
}

fn pump_messages() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn create_child_pair(
    parent: HWND,
    rect: [i32; 4],
    clip: [i32; 4],
    w: i32,
    h: i32,
) -> Result<(HWND, HWND), String> {
    ensure_window_class()?;
    let hinstance = unsafe { GetModuleHandleW(ptr::null()) };
    let style = WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_VISIBLE;
    let frame = unsafe {
        CreateWindowExW(
            0,
            class_name_utf16().as_ptr(),
            ptr::null(),
            style,
            clip[0],
            clip[1],
            clip[2].max(1),
            clip[3].max(1),
            parent,
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        )
    };
    if frame.is_null() {
        return Err("Failed to create Win32 plugin clip frame window".into());
    }
    let win = unsafe {
        CreateWindowExW(
            0,
            class_name_utf16().as_ptr(),
            ptr::null(),
            style,
            rect[0] - clip[0],
            rect[1] - clip[1],
            w.max(1),
            h.max(1),
            frame,
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        )
    };
    if win.is_null() {
        unsafe {
            DestroyWindow(frame);
        }
        return Err("Failed to create Win32 plugin editor container window".into());
    }
    Ok((frame, win))
}

fn resize_child(win: HWND, w: i32, h: i32) {
    unsafe {
        SetWindowPos(
            win,
            ptr::null_mut(),
            0,
            0,
            w.max(1),
            h.max(1),
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn embed_handle(
    handle: &Handle,
    parent: HWND,
    rect: [i32; 4],
    clip: [i32; 4],
) -> Result<(HWND, HWND, i32, i32, u32), String> {
    match handle {
        Handle::V3(p) => {
            let mut p = p.lock().map_err(|_| "busy".to_string())?;
            let (w, h) = p
                .get_editor_size()
                .ok()
                .filter(|s| s.0 > 0 && s.1 > 0)
                .unwrap_or((rect[2].max(1), rect[3].max(1)));
            let (frame, win) = create_child_pair(parent, rect, clip, w, h)?;
            let dpi = window_dpi(win);
            let _ = p.set_editor_scale_factor(dpi as f32 / 96.0);
            // SAFETY: `win` is a live child HWND destroyed only after `close_editor`.
            let wh = unsafe { vst3_host::plugin::WindowHandle::from_hwnd(win as *mut c_void) };
            if let Err(e) = p.open_editor(wh) {
                unsafe {
                    DestroyWindow(frame);
                }
                return Err(format!("Editor: {e}"));
            }
            let (nw, nh) = p
                .get_editor_size()
                .ok()
                .filter(|s| s.0 > 0 && s.1 > 0)
                .unwrap_or((w, h));
            if (nw, nh) != (w, h) {
                resize_child(win, nw, nh);
            }
            Ok((frame, win, nw, nh, dpi))
        }
        Handle::V2(p) => {
            let mut p = p.lock().map_err(|_| "busy".to_string())?;
            let (w, h) = (rect[2].max(1), rect[3].max(1));
            let (frame, win) = create_child_pair(parent, rect, clip, w, h)?;
            let dpi = window_dpi(win);
            if !p.editor_open(win as usize) {
                unsafe {
                    DestroyWindow(frame);
                }
                return Err("Editor: the plugin did not open its editor".into());
            }
            let (nw, nh) = p
                .editor_rect()
                .map(|r| ((r.right - r.left) as i32, (r.bottom - r.top) as i32))
                .filter(|s| s.0 > 0 && s.1 > 0)
                .unwrap_or((w, h));
            if (nw, nh) != (w, h) {
                resize_child(win, nw, nh);
            }
            Ok((frame, win, nw, nh, dpi))
        }
        Handle::Clap(p) => {
            let mut p = p.lock().map_err(|_| "busy".to_string())?;
            let (w, h) = (rect[2].max(1), rect[3].max(1));
            let (frame, win) = create_child_pair(parent, rect, clip, w, h)?;
            let dpi = window_dpi(win);
            let opened = p.editor_open_with_scale(win as usize as u64, dpi as f64 / 96.0);
            let (nw, nh) = match opened {
                Ok(sz) if sz.0 > 0 && sz.1 > 0 => sz,
                Ok(_) => (w, h),
                Err(e) => {
                    unsafe {
                        DestroyWindow(frame);
                    }
                    return Err(format!("Editor: {e}"));
                }
            };
            if (nw, nh) != (w, h) {
                resize_child(win, nw, nh);
            }
            Ok((frame, win, nw, nh, dpi))
        }
        Handle::Fl => {
            Err("The FL Studio link has no editor here: FL Studio shows its own window".into())
        }
    }
}

fn place(e: &mut Embedded, rect: [i32; 4], clip: [i32; 4], visible: bool) {
    if clip != e.clip {
        unsafe {
            SetWindowPos(
                e.frame,
                ptr::null_mut(),
                clip[0],
                clip[1],
                clip[2].max(1),
                clip[3].max(1),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    if rect != e.rect || clip != e.clip {
        unsafe {
            SetWindowPos(
                e.win,
                ptr::null_mut(),
                rect[0] - clip[0],
                rect[1] - clip[1],
                rect[2].max(1),
                rect[3].max(1),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    e.rect = rect;
    e.clip = clip;
    if visible != e.visible {
        unsafe {
            ShowWindow(e.frame, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
        e.visible = visible;
    }
}

/// Apply visible region clipping (`SetWindowRgn`) to `frame` in frame-local pixels.
fn apply_regions(frame: HWND, clip: [i32; 4], regions: &[[i32; 4]]) {
    if regions.is_empty() {
        return;
    }
    // Single full-viewport rect: clear custom region for zero GDI overhead.
    if regions.len() == 1 && regions[0] == clip {
        unsafe {
            SetWindowRgn(frame, ptr::null_mut(), 1);
        }
        return;
    }
    unsafe {
        let mut combined = ptr::null_mut();
        for r in regions {
            let x1 = r[0] - clip[0];
            let y1 = r[1] - clip[1];
            let x2 = x1 + r[2].max(0);
            let y2 = y1 + r[3].max(0);
            if x2 <= x1 || y2 <= y1 {
                continue;
            }
            let part = CreateRectRgn(x1, y1, x2, y2);
            if part.is_null() {
                continue;
            }
            if combined.is_null() {
                combined = part;
            } else {
                CombineRgn(combined, combined, part, RGN_OR);
                DeleteObject(part);
            }
        }
        if !combined.is_null() && SetWindowRgn(frame, combined, 1) == 0 {
            // `SetWindowRgn` only takes ownership of the region when it succeeds.
            DeleteObject(combined);
        }
    }
}

/// The pointer over Win32 window `win`, in its physical client pixels.
pub(super) fn pointer_in(win: NativeWindow) -> Option<(i32, i32)> {
    let NativeWindow::Win32(raw) = win else {
        return None;
    };
    let hwnd = raw as usize as HWND;
    if hwnd.is_null() {
        return None;
    }
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) == 0 {
            return None;
        }
        if ScreenToClient(hwnd, &mut pt) == 0 {
            return None;
        }
        Some((pt.x, pt.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{GetWindowRgnBox, NULLREGION, SIMPLEREGION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsWindowVisible, WS_OVERLAPPEDWINDOW,
    };

    #[test]
    fn win32_editor_container_clips_resizes_and_hides() {
        ensure_window_class().unwrap();
        let hinstance = unsafe { GetModuleHandleW(ptr::null()) };
        let parent = unsafe {
            CreateWindowExW(
                0,
                class_name_utf16().as_ptr(),
                ptr::null(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                800,
                600,
                ptr::null_mut(),
                ptr::null_mut(),
                hinstance,
                ptr::null_mut(),
            )
        };
        assert!(!parent.is_null());

        let clip = [10, 20, 400, 300];
        let rect = [10, 20, 500, 400];
        let (frame, win) = create_child_pair(parent, rect, clip, 500, 400).unwrap();
        assert!(window_dpi(win) >= 96);

        let mut emb = Embedded {
            frame,
            win,
            rect,
            clip,
            visible: true,
            dpi: window_dpi(win),
        };

        // Multi-rect clipping (e.g., popup menu covering the middle) sets a real GDI region.
        let regions = [[10, 20, 150, 300], [260, 20, 150, 300]];
        apply_regions(emb.frame, clip, &regions);
        let mut box_rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let rgn_kind = unsafe { GetWindowRgnBox(emb.frame, &mut box_rect) };
        assert_ne!(rgn_kind, NULLREGION);
        assert_eq!(box_rect.right - box_rect.left, 400);

        // Full-clip single rect clears custom region without error.
        apply_regions(emb.frame, clip, &[clip]);
        let _ = SIMPLEREGION;

        // Resizing & moving updates child geometry.
        let new_clip = [20, 30, 320, 240];
        let new_rect = [20, 30, 640, 480];
        place(&mut emb, new_rect, new_clip, true);
        let mut wr = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        assert_ne!(unsafe { GetWindowRect(emb.win, &mut wr) }, 0);
        assert_eq!(wr.right - wr.left, 640);
        assert_eq!(wr.bottom - wr.top, 480);

        // Hiding toggles visibility.
        place(&mut emb, new_rect, new_clip, false);
        assert_eq!(unsafe { IsWindowVisible(emb.frame) }, 0);

        // Pointer coordinate query succeeds on a valid HWND.
        assert!(pointer_in(NativeWindow::Win32(parent as usize as u64)).is_some());

        pump_messages();
        unsafe {
            DestroyWindow(emb.frame);
            DestroyWindow(parent);
        }
    }
}
