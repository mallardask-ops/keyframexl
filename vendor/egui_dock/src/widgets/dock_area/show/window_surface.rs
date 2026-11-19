use egui::{
    pos2, vec2, Align, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, LayerId,
    Layout, Order, PointerButton, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind,
    Ui, UiBuilder, Vec2, WidgetText,
};

use crate::{
    dock_area::state::State,
    utils::{fade_visuals, rect_set_size_centered},
    DockArea, Node, NodeIndex, Snap, Style, SurfaceIndex, TabViewer,
};

/// How far a pointer may be from the work area's edge to pick a sector.
const SNAP_REACH: f32 = 18.0;
/// How close a window edge comes before it sticks to another edge.
const MAGNET: f32 = 10.0;

/// The sector under the pointer while a window is dragged (FL / Windows:
/// edges give halves, their ends quarters, the top edge the whole area).
fn sector_at(p: Pos2, area: Rect) -> Option<Snap> {
    let near_l = p.x <= area.left() + SNAP_REACH;
    let near_r = p.x >= area.right() - SNAP_REACH;
    let near_t = p.y <= area.top() + SNAP_REACH;
    let near_b = p.y >= area.bottom() - SNAP_REACH;
    let upper = p.y < area.top() + area.height() * 0.25;
    let lower = p.y > area.bottom() - area.height() * 0.25;
    match (near_l, near_r) {
        (true, _) if upper => Some(Snap::TopLeft),
        (true, _) if lower => Some(Snap::BottomLeft),
        (true, _) => Some(Snap::Left),
        (_, true) if upper => Some(Snap::TopRight),
        (_, true) if lower => Some(Snap::BottomRight),
        (_, true) => Some(Snap::Right),
        _ if near_t => Some(Snap::Max),
        _ if near_b => Some(Snap::Bottom),
        _ => None,
    }
}

/// `v` moved onto the nearest of `targets` within `MAGNET`, else unchanged.
fn stick(v: f32, targets: &[f32]) -> Option<f32> {
    targets.iter().map(|t| (t, (t - v).abs())).filter(|(_, d)| *d <= MAGNET).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(t, _)| *t)
}

/// A moved window whose edges stick to the work area's and other windows'.
fn magnet_move(r: Rect, area: Rect, others: &[Rect]) -> Rect {
    let xs: Vec<f32> = [area.left(), area.right()].into_iter().chain(others.iter().flat_map(|o| [o.left(), o.right()])).collect();
    let ys: Vec<f32> = [area.top(), area.bottom()].into_iter().chain(others.iter().flat_map(|o| [o.top(), o.bottom()])).collect();
    let dx = match (stick(r.left(), &xs), stick(r.right(), &xs)) {
        (Some(t), _) => t - r.left(),
        (None, Some(t)) => t - r.right(),
        _ => 0.0,
    };
    let dy = match (stick(r.top(), &ys), stick(r.bottom(), &ys)) {
        (Some(t), _) => t - r.top(),
        (None, Some(t)) => t - r.bottom(),
        _ => 0.0,
    };
    r.translate(vec2(dx, dy))
}

/// `r` moved inside `area` (shrunk first if it is bigger).
fn keep_inside(r: Rect, area: Rect) -> Rect {
    let size = r.size().min(area.size());
    let x = r.left().min(area.right() - size.x).max(area.left());
    let y = r.top().min(area.bottom() - size.y).max(area.top());
    Rect::from_min_size(pos2(x, y), size)
}

/// What the pointer started on a window: moving it or one of its edges.
#[derive(Clone, Copy)]
struct Grab {
    start: Pos2,
    rect: Rect,
    /// Edges a resize moves: left, right, top, bottom (none: a move).
    edges: [bool; 4],
    /// The sector the pointer is over now (moves only).
    sector: Option<Snap>,
}

impl<Tab> DockArea<'_, Tab> {
    /// KeyFrameXL: a floating window is a plain rect we own (not an
    /// `egui::Window`, which grows to its content and loses its size when the
    /// UI scale changes). Its tab bar moves it, its edges and corners resize
    /// it, edges stick to the work area and other windows, and dragging the
    /// pointer to an edge snaps it to a half, a quarter or the whole area.
    pub(super) fn show_window_surface(
        &mut self,
        ui: &Ui,
        surf_index: SurfaceIndex,
        tab_viewer: &mut impl TabViewer<Tab = Tab>,
        state: &mut State,
        fade_style: Option<(&Style, f32, SurfaceIndex)>,
    ) {
        let id = Id::new(("kfxl-window", surf_index));
        let bounds = self.window_bounds.unwrap();
        let tab_bar_height = self.style.as_ref().unwrap().tab_bar.height;
        let radius = self.style.as_ref().unwrap().tab_bar.corner_radius.nw;
        let min = vec2(320.0, tab_bar_height + 140.0);

        // Calculate fading of the window (if any)
        let (fade_factor, fade_style) = match fade_style {
            Some((style, factor, surface_index)) => {
                if surface_index == surf_index {
                    (1.0, None)
                } else {
                    (factor, Some((style, factor)))
                }
            }
            None => (1.0, None),
        };

        let others: Vec<Rect> = self
            .dock_state
            .valid_surface_indices()
            .iter()
            .filter(|s| **s != surf_index && !s.is_main())
            .filter_map(|s| self.dock_state.get_window_state(*s).map(|w| w.rect()))
            .filter(|r| r.is_finite() && r.is_positive())
            .collect();

        let ws = self.dock_state.get_window_state_mut(surf_index).unwrap();
        let mut rect = ws.resolve(bounds, min);
        let minimized = ws.is_minimized();
        if minimized {
            rect.set_height(tab_bar_height);
        }
        let snapped = ws.snap();
        let restore = ws.restore_size();

        // Title of the focused tab (shown when minimized) and the tab count.
        let title = {
            let node_id = self.dock_state[surf_index].focused_leaf().unwrap_or_else(|| {
                for node_index in self.dock_state[surf_index].breadth_first_index_iter() {
                    if self.dock_state[surf_index][node_index].is_leaf() {
                        return node_index;
                    }
                }
                unreachable!("a window surface should never be empty")
            });
            let Node::Leaf { tabs, active, .. } = &mut self.dock_state[surf_index][node_id] else {
                unreachable!()
            };
            tab_viewer.title(&mut tabs[active.0]).color(ui.visuals().widgets.noninteractive.fg_stroke.color)
        };
        let mut tab_count = 0;
        for node_index in self.dock_state[surf_index].breadth_first_index_iter() {
            if self.dock_state[surf_index][node_index].is_leaf() {
                tab_count += self.dock_state[surf_index][node_index].tabs_count();
            }
        }

        let mut frame = Frame::window(ui.style());
        if fade_factor != 1.0 {
            frame.fill = frame.fill.linear_multiply(fade_factor);
            frame.stroke.color = frame.stroke.color.linear_multiply(fade_factor);
            frame.shadow.color = frame.shadow.color.linear_multiply(fade_factor);
        }

        let ctx = ui.ctx().clone();
        let grab_id = id.with("grab");
        let mut moved: Option<Rect> = None;
        let mut snap_to: Option<Option<Snap>> = None;
        // Room around the window for the resize handles and the shadow.
        let pad = 8.0;
        let outer = rect.expand(pad);
        egui::Area::new(id)
            .order(Order::Middle)
            .fixed_pos(outer.min)
            .constrain(false)
            .interactable(true)
            .show(&ctx, |ui| {
                ui.set_clip_rect(outer.expand(frame.shadow.blur as f32 + frame.shadow.spread as f32));
                ui.expand_to_include_rect(outer);
                let layer = ui.layer_id();
                let painter = ui.painter().clone();
                let corner = CornerRadius::same(radius);
                painter.add(frame.shadow.as_shape(rect, corner));
                painter.rect_filled(rect, corner, frame.fill);

                // Under everything: the tab bar's empty space and its single
                // title tab (which takes clicks only) move the window.
                let title_rect = Rect::from_min_size(rect.min, vec2(rect.width(), tab_bar_height));
                let mover = ui.interact(title_rect, id.with("move"), Sense::drag());

                let mut content = ui.new_child(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::top_down(Align::Min))
                        .id_salt("content"),
                );
                content.set_clip_rect(rect);
                if fade_factor != 1.0 {
                    fade_visuals(content.visuals_mut(), fade_factor);
                }
                if minimized {
                    self.minimized_body(&mut content, surf_index, fade_style.map(|(style, _)| style), title, tab_count);
                } else {
                    self.render_nodes(&mut content, tab_viewer, state, surf_index, fade_style);
                }
                painter.rect_stroke(rect, corner, frame.stroke, StrokeKind::Inside);

                let pointer = ui.input(|i| i.pointer.interact_pos());
                let free = ui.input(|i| i.modifiers.alt);

                // Edges and corners, on top of the content.
                let e = 5.0;
                let c = 16.0;
                let handles: [([bool; 4], Rect, CursorIcon); 8] = [
                    ([true, false, false, false], Rect::from_min_max(pos2(rect.left() - e, rect.top() + c), pos2(rect.left() + e, rect.bottom() - c)), CursorIcon::ResizeHorizontal),
                    ([false, true, false, false], Rect::from_min_max(pos2(rect.right() - e, rect.top() + c), pos2(rect.right() + e, rect.bottom() - c)), CursorIcon::ResizeHorizontal),
                    // The top edge is thin: the tab bar right below it moves the window.
                    ([false, false, true, false], Rect::from_min_max(pos2(rect.left() + c, rect.top() - e), pos2(rect.right() - c, rect.top() + 2.0)), CursorIcon::ResizeVertical),
                    ([false, false, false, true], Rect::from_min_max(pos2(rect.left() + c, rect.bottom() - e), pos2(rect.right() - c, rect.bottom() + e)), CursorIcon::ResizeVertical),
                    ([true, false, true, false], Rect::from_min_max(rect.left_top() - vec2(e, e), rect.left_top() + vec2(c - e, 3.0)), CursorIcon::ResizeNwSe),
                    ([false, true, true, false], Rect::from_min_max(pos2(rect.right() - c + e, rect.top() - e), pos2(rect.right() + e, rect.top() + 3.0)), CursorIcon::ResizeNeSw),
                    ([true, false, false, true], Rect::from_min_max(pos2(rect.left() - e, rect.bottom() - c + e), pos2(rect.left() + c - e, rect.bottom() + e)), CursorIcon::ResizeNeSw),
                    ([false, true, false, true], Rect::from_min_max(rect.right_bottom() - vec2(c - e, c - e), rect.right_bottom() + vec2(e, e)), CursorIcon::ResizeNwSe),
                ];
                let mut grab: Option<Grab> = ctx.data(|d| d.get_temp(grab_id));
                if !minimized {
                    for (k, (edges, hit, icon)) in handles.iter().enumerate() {
                        let r = ui.interact(*hit, id.with(("resize", k)), Sense::drag()).on_hover_cursor(*icon);
                        if let (true, Some(p)) = (r.drag_started(), pointer) {
                            grab = Some(Grab { start: p, rect, edges: *edges, sector: None });
                        }
                        if r.dragged() {
                            ctx.set_cursor_icon(*icon);
                        }
                    }
                }
                if let (true, Some(p)) = (mover.drag_started(), pointer) {
                    // Leaving a sector: back to the size it had, under the
                    // pointer at the same place along the title bar.
                    let mut start = rect;
                    if snapped.is_some() {
                        let size = restore.unwrap_or(rect.size() * 0.7).max(min);
                        let fx = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                        start = Rect::from_min_size(pos2(p.x - fx * size.x, rect.top()), size);
                    }
                    grab = Some(Grab { start: p, rect: start, edges: [false; 4], sector: None });
                }

                let active = ctx.dragged_id().is_some_and(|d| d == mover.id || (0..8).any(|k| d == id.with(("resize", k))));
                if let (Some(g), true, Some(p)) = (grab.as_mut(), active, pointer) {
                    let d = p - g.start;
                    let xs: Vec<f32> = [bounds.left(), bounds.right()].into_iter().chain(others.iter().flat_map(|o| [o.left(), o.right()])).collect();
                    let ys: Vec<f32> = [bounds.top(), bounds.bottom()].into_iter().chain(others.iter().flat_map(|o| [o.top(), o.bottom()])).collect();
                    let r = if g.edges == [false; 4] {
                        g.sector = if free { None } else { sector_at(p, bounds) };
                        let r = g.rect.translate(d);
                        keep_inside(if free { r } else { magnet_move(r, bounds, &others) }, bounds)
                    } else {
                        let [l, rt, t, b] = g.edges;
                        let mut r = g.rect;
                        let edge = |v: f32, set: &[f32]| if free { v } else { stick(v, set).unwrap_or(v) };
                        if l {
                            r.min.x = edge(r.min.x + d.x, &xs).max(bounds.left()).min(r.max.x - min.x);
                        }
                        if rt {
                            r.max.x = edge(r.max.x + d.x, &xs).min(bounds.right()).max(r.min.x + min.x);
                        }
                        if t {
                            r.min.y = edge(r.min.y + d.y, &ys).max(bounds.top()).min(r.max.y - min.y);
                        }
                        if b {
                            r.max.y = edge(r.max.y + d.y, &ys).min(bounds.bottom()).max(r.min.y + min.y);
                        }
                        r
                    };
                    moved = Some(r);
                    if let Some(sector) = g.sector {
                        let shown = sector.rect(bounds).shrink(6.0);
                        let p = ctx.layer_painter(LayerId::new(Order::Foreground, id.with("sector")));
                        let accent = ui.visuals().widgets.active.fg_stroke.color;
                        p.rect_filled(shown, corner, accent.gamma_multiply(0.10));
                        p.rect_stroke(shown, corner, Stroke::new(1.5_f32, accent.gamma_multiply(0.7)), StrokeKind::Inside);
                        // The name in a plate at the sector's centre, above whatever is under it.
                        let galley = p.layout_no_wrap(format!("{}  ·  release to snap  ·  hold Alt to place freely", sector.label()), FontId::proportional(14.0), accent);
                        let plate = Rect::from_center_size(shown.center(), galley.size() + vec2(28.0, 16.0));
                        p.rect_filled(plate, CornerRadius::same(8), frame.fill);
                        p.rect_stroke(plate, CornerRadius::same(8), Stroke::new(1.0_f32, accent.gamma_multiply(0.7)), StrokeKind::Inside);
                        p.galley(plate.center() - galley.size() / 2.0, galley, accent);
                    }
                    ctx.data_mut(|d| d.insert_temp(grab_id, *g));
                } else if let Some(g) = grab.take() {
                    // Released: into the sector it was over, else where it is.
                    if let Some(sector) = g.sector {
                        snap_to = Some(Some(sector));
                    }
                    ctx.data_mut(|d| d.remove::<Grab>(grab_id));
                }
                if grab.is_some() && !active {
                    // Started this frame: keep it for the next.
                    if let Some(g) = grab {
                        ctx.data_mut(|d| d.insert_temp(grab_id, g));
                    }
                }

                // Double-click on the title bar: fill the work area, or back.
                let double = ui.input(|i| i.pointer.button_double_clicked(PointerButton::Primary));
                if pointer.is_some_and(|p| double && title_rect.contains(p) && p.x < title_rect.right() - 40.0 && ctx.layer_id_at(p) == Some(layer)) {
                    snap_to = Some(if snapped == Some(Snap::Max) { None } else { Some(Snap::Max) });
                }
                // A press anywhere on it brings it to the front.
                let pressed = ui.input(|i| i.pointer.any_pressed());
                if pointer.is_some_and(|p| pressed && outer.contains(p) && ctx.layer_id_at(p) == Some(layer)) {
                    ctx.move_to_top(layer);
                }
            });

        let ws = self.dock_state.get_window_state_mut(surf_index).unwrap();
        if let Some(r) = moved {
            ws.set_rect(r);
        }
        match snap_to {
            Some(Some(sector)) => {
                ws.set_snap(Some(sector));
            }
            Some(None) => {
                let size = restore.unwrap_or(rect.size() * 0.7).max(min);
                ws.set_snap(None);
                ws.set_rect(keep_inside(Rect::from_center_size(rect.center(), size), bounds));
            }
            None => {}
        }
    }

    fn minimized_body(
        &mut self,
        ui: &mut Ui,
        surface_index: SurfaceIndex,
        fade_style: Option<&Style>,
        title: WidgetText,
        tab_count: usize,
    ) {
        ui.horizontal(|ui| {
            let style = fade_style.unwrap_or_else(|| self.style.as_ref().unwrap());
            let (tabbar_outer_rect, _) = ui.allocate_exact_size(
                vec2(Style::TAB_EXPAND_BUTTON_SIZE, style.tab_bar.height),
                Sense::hover(),
            );
            ui.painter().rect_filled(
                tabbar_outer_rect,
                style.tab_bar.corner_radius,
                style.tab_bar.bg_fill,
            );
            self.window_expand(ui, surface_index, tabbar_outer_rect, fade_style);
            ui.label(title);
            if tab_count > 1 {
                ui.label(
                    RichText::new(format!("+{}", tab_count - 1))
                        .color(ui.visuals().weak_text_color()),
                );
            }
            ui.allocate_space(ui.available_size());
        });
    }

    /// Draws the expand window button.
    fn window_expand(
        &mut self,
        ui: &mut Ui,
        surface_index: SurfaceIndex,
        tabbar_outer_rect: Rect,
        fade_style: Option<&Style>,
    ) {
        let rect = tabbar_outer_rect;

        let ui = &mut ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center))
                .id_salt((surface_index, "window_expand")),
        );

        let (rect, mut response) = ui.allocate_exact_size(ui.available_size(), Sense::click());

        response = response.on_hover_cursor(CursorIcon::PointingHand);

        let style = fade_style.unwrap_or_else(|| self.style.as_ref().unwrap());
        let color = if response.hovered() || response.has_focus() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::ZERO,
                style.buttons.minimize_window_bg_fill,
            );
            style.buttons.minimize_window_active_color
        } else {
            style.buttons.minimize_window_color
        };

        let mut arrow_rect = rect;

        rect_set_size_centered(&mut arrow_rect, Vec2::splat(Style::TAB_EXPAND_ARROW_SIZE));

        Self::draw_chevron_right(ui, &mut response, style, color, arrow_rect);

        // Draw button right border.
        ui.painter().vline(
            rect.right(),
            rect.y_range(),
            Stroke::new(
                ui.ctx().pixels_per_point().recip(),
                style.buttons.minimize_window_border_color,
            ),
        );

        if response.clicked() {
            self.window_toggle_minimized(surface_index);
        }
    }

    fn draw_chevron_right(
        ui: &mut Ui,
        response: &mut Response,
        style: &Style,
        color: Color32,
        arrow_rect: Rect,
    ) {
        ui.painter().add(Shape::convex_polygon(
            // Arrow pointing rightwards.
            vec![
                arrow_rect.left_top(),
                arrow_rect.center(),
                arrow_rect.left_bottom(),
            ],
            color,
            Stroke::NONE,
        ));

        // Chevron pointing rightwards.
        ui.painter().add(Shape::convex_polygon(
            vec![
                arrow_rect.center_top(),
                arrow_rect.right_center(),
                arrow_rect.center_bottom(),
            ],
            color,
            Stroke::NONE,
        ));
        let color = if response.hovered() || response.has_focus() {
            style.buttons.minimize_window_bg_fill
        } else {
            style.tab_bar.bg_fill
        };
        ui.painter().add(Shape::convex_polygon(
            vec![
                arrow_rect
                    .center_top()
                    .lerp(arrow_rect.center_bottom(), 0.25),
                arrow_rect.center().lerp(arrow_rect.right_center(), 0.5),
                arrow_rect
                    .center_top()
                    .lerp(arrow_rect.center_bottom(), 0.75),
            ],
            color,
            Stroke::NONE,
        ));
    }

    pub(super) fn window_toggle_minimized(&mut self, surf_index: SurfaceIndex) {
        let minimized = self
            .dock_state
            .get_window_state(surf_index)
            .unwrap()
            .is_minimized();
        let surface = &mut self.dock_state[surf_index];

        if surface.root_node().is_some_and(|node| node.is_collapsed()) {
            // The window is already fully collapsed,
            // so `expanded_height` has already been set.
            // We don't need to set `new` either.
            if let Some(window_state) = self.dock_state.get_window_state_mut(surf_index) {
                window_state.toggle_minimized();
            }
        } else if minimized {
            if let Some(window_state) = self.dock_state.get_window_state_mut(surf_index) {
                window_state.set_new(true);
                window_state.toggle_minimized();
            }
        } else {
            let root_index = NodeIndex::root();
            let surface_height = if surface.root_node().is_some() {
                surface[root_index].rect().unwrap().height()
            } else {
                0.0
            };
            if let Some(window_state) = self.dock_state.get_window_state_mut(surf_index) {
                window_state.set_expanded_height(surface_height);
                window_state.toggle_minimized();
            }
        }
    }
}
