# Vendored crates

Local copies of upstream crates that KeyFrameXL patches. Each entry says what
was changed and why. See docs/development.md §1 for the rules.

This list is being filled in; changes made before 2026-11-23 are not all
recorded yet.

## egui_dock (0.16)

- `TabViewer::min_window_size(tab)`: the smallest floating window that shows a
  tab whole. A window keeps the largest minimum of its tabs, on top of the dock's
  own 320 × (tab bar + 140). Used for Settings, which was cut off when shrunk.
  Check: shrink the Settings window by its corner; it stops at 920 × 520.
- `WindowState::raise()` and `DockState::top_window()`: code can bring a
  floating window to the front, and the dock keeps the windows' stacking order
  (`z`, not saved). The window toolbar uses them so a button raises a window
  hidden under another instead of only focusing (or closing) it.
- `DockState::windows_covering(rect)`, `WindowState::set_minimized` /
  `is_minimized` public: the toolbar folds floating windows that hide a docked
  panel it is asked to show.

## vst3-host (0.11.1)

- Linux `IRunLoop`: an event handler registered for a connection to the X
  server (`is_x11_connection`: Unix socket under `/tmp/.X11-unix/`, or TCP
  6000-6099) runs on every `service_run_loop`, not only when its fd is
  readable. VSTGUI reads X events into xcb's queue during round trips (the
  `xcb_aux_sync` at the end of its handler, cairo drawing); the fd then stays
  quiet and a queued Expose was never dispatched, so the editor stayed black
  after it was hidden and shown again. Check: open MT-PowerDrumKit (native
  VST3), switch to Parameters and back to Plugin; the kit is drawn again.
  Tests in the crate run with `cargo test --lib` from a copy of the folder (it
  is not a workspace member).
