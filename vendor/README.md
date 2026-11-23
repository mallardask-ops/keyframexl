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
