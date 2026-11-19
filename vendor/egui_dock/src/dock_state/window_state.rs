use egui::{Pos2, Rect, Vec2};

/// The state of a [`Surface::Window`](crate::Surface::Window).
///
/// Doubles as a handle for the surface, allowing the user to set its size and position.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct WindowState {
    /// The [`Rect`] that this window was last taking up.
    screen_rect: Option<Rect>,

    /// Was this window dragged in the last frame?
    dragged: bool,

    /// The next position this window should be set to next frame.
    next_position: Option<Pos2>,

    /// The next size this window should be set to next frame.
    next_size: Option<Vec2>,

    /// The height of the window before it was fully collapsed
    expanded_height: Option<f32>,

    /// True the first frame this window is drawn.
    /// handles expanding after being fully collapsed, etc.
    new: bool,

    /// True if the window is minimized
    minimized: bool,

    /// KeyFrameXL: the sector the window is snapped to; its rect follows the
    /// work area (resizing the program or the UI scale keeps it there).
    #[cfg_attr(feature = "serde", serde(default))]
    snap: Option<Snap>,

    /// KeyFrameXL: the size it had before it was snapped, for unsnapping.
    #[cfg_attr(feature = "serde", serde(default))]
    restore: Option<Vec2>,
}

/// A sector of the work area a floating window can snap to (KeyFrameXL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum Snap {
    /// The whole work area.
    Max,
    /// Left half.
    Left,
    /// Right half.
    Right,
    /// Top half.
    Top,
    /// Bottom half.
    Bottom,
    /// Top-left quarter.
    TopLeft,
    /// Top-right quarter.
    TopRight,
    /// Bottom-left quarter.
    BottomLeft,
    /// Bottom-right quarter.
    BottomRight,
}

impl Snap {
    /// The rect of this sector in `area`.
    /// The rect of this sector in `area`.
    pub fn rect(self, area: Rect) -> Rect {
        let (l, r, t, b, cx, cy) = (area.left(), area.right(), area.top(), area.bottom(), area.center().x, area.center().y);
        let r4 = |a: f32, b: f32, c: f32, d: f32| Rect::from_min_max(Pos2::new(a, b), Pos2::new(c, d));
        match self {
            Snap::Max => area,
            Snap::Left => r4(l, t, cx, b),
            Snap::Right => r4(cx, t, r, b),
            Snap::Top => r4(l, t, r, cy),
            Snap::Bottom => r4(l, cy, r, b),
            Snap::TopLeft => r4(l, t, cx, cy),
            Snap::TopRight => r4(cx, t, r, cy),
            Snap::BottomLeft => r4(l, cy, cx, b),
            Snap::BottomRight => r4(cx, cy, r, b),
        }
    }

    /// What the sector is called, for the snap preview.
    pub fn label(self) -> &'static str {
        match self {
            Snap::Max => "Fill the work area",
            Snap::Left => "Left half",
            Snap::Right => "Right half",
            Snap::Top => "Top half",
            Snap::Bottom => "Bottom half",
            Snap::TopLeft => "Top-left quarter",
            Snap::TopRight => "Top-right quarter",
            Snap::BottomLeft => "Bottom-left quarter",
            Snap::BottomRight => "Bottom-right quarter",
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            screen_rect: None,
            dragged: false,
            next_position: None,
            next_size: None,
            expanded_height: None,
            new: true,
            minimized: false,
            snap: None,
            restore: None,
        }
    }
}

impl WindowState {
    /// Create a default window state.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Set the position for this window in screen coordinates.
    pub fn set_position(&mut self, position: Pos2) -> &mut Self {
        self.next_position = Some(position);
        self.snap = None;
        self
    }

    /// Set the size of this window in egui points.
    pub fn set_size(&mut self, size: Vec2) -> &mut Self {
        self.next_size = Some(size);
        self.snap = None;
        self
    }

    /// The sector it is snapped to, if any.
    pub fn snap(&self) -> Option<Snap> {
        self.snap
    }

    /// Snap to `sector` (None: back to the size it had before, at `at`).
    pub fn set_snap(&mut self, sector: Option<Snap>) -> &mut Self {
        match sector {
            Some(s) => {
                if self.snap.is_none() {
                    self.restore = self.screen_rect.map(|r| r.size());
                }
                self.snap = Some(s);
            }
            None => self.snap = None,
        }
        self
    }

    /// The size to go back to when a snapped window is dragged away.
    pub(crate) fn restore_size(&self) -> Option<Vec2> {
        self.restore
    }

    /// This frame's rect: pending position and size applied, snapped to its
    /// sector, else kept inside `bounds` and at least `min` in size.
    pub(crate) fn resolve(&mut self, bounds: Rect, min: Vec2) -> Rect {
        let min = min.min(bounds.size());
        if let Some(s) = self.snap {
            let r = s.rect(bounds);
            self.screen_rect = Some(r);
            self.next_position = None;
            self.next_size = None;
            return r;
        }
        let mut r = self
            .screen_rect
            .filter(|r| r.is_finite() && r.is_positive())
            .unwrap_or_else(|| Rect::from_center_size(bounds.center(), (bounds.size() * 0.6).max(min)));
        if let Some(size) = self.next_size.take() {
            r.set_width(size.x);
            r.set_height(size.y);
        }
        if let Some(pos) = self.next_position.take() {
            r = Rect::from_min_size(pos, r.size());
        }
        let size = r.size().max(min).min(bounds.size());
        let x = r.left().min(bounds.right() - size.x).max(bounds.left());
        let y = r.top().min(bounds.bottom() - size.y).max(bounds.top());
        let r = Rect::from_min_size(Pos2::new(x, y), size);
        self.screen_rect = Some(r);
        r
    }

    /// Put the window at `rect` now (dragging, resizing).
    pub(crate) fn set_rect(&mut self, rect: Rect) {
        self.screen_rect = Some(rect);
        self.snap = None;
    }

    /// Get the [`Rect`] which this window occupies.
    /// If this window hasn't been shown before, this will be [`Rect::NOTHING`].
    pub fn rect(&self) -> Rect {
        // The reason why we're unwrapping an Option with a default value instead of
        // just storing Rect::NOTHING for the None variant is that deserializing Rect::NOTHING
        // with serde_json causes a panic, because f32::INFINITY serializes into null in JSON.
        self.screen_rect.unwrap_or(Rect::NOTHING)
    }

    /// Returns if this window is currently being dragged or not.
    pub fn dragged(&self) -> bool {
        self.dragged
    }

    /// Set the height of this window when it is expanded.
    #[inline(always)]
    pub(crate) fn set_expanded_height(&mut self, height: f32) -> &mut Self {
        self.expanded_height = Some(height);
        self
    }

    #[inline(always)]
    pub(crate) fn set_new(&mut self, new: bool) -> &mut Self {
        self.new = new;
        self
    }

    #[inline(always)]
    pub(crate) fn toggle_minimized(&mut self) {
        self.minimized = !self.minimized;
    }

    #[inline(always)]
    pub(crate) fn is_minimized(&self) -> bool {
        self.minimized
    }
}
