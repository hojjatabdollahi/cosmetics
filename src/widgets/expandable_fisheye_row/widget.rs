// SPDX-License-Identifier: MPL-2.0

//! Expandable fisheye row. See [`super`] for usage.

use cosmic::Element;
use cosmic::iced::core::Renderer as _;
use cosmic::iced::core::event::Event;
use cosmic::iced::core::keyboard::{self, key::Named};
use cosmic::iced::core::layout::{self, Layout};
use cosmic::iced::core::mouse::{self, Cursor};
use cosmic::iced::core::renderer;
use cosmic::iced::core::text::Renderer as TextRenderer;
use cosmic::iced::core::widget::operation::{Focusable, Operation};
use cosmic::iced::core::widget::tree::{self, Tree};
use cosmic::iced::core::{
    Background, Border, Clipboard, Color, Length, Pixels, Rectangle, Shadow, Shell, Size, Vector,
    Widget, alignment, window,
};
use std::time::{Duration, Instant};

const DEFAULT_COMFORTABLE_COUNT: usize = 4;
const DEFAULT_HEIGHT: f32 = 96.0;
const DEFAULT_CAP_W: f32 = 30.0;
/// Fraction of the full item width below which items collapse into the caps.
const DEFAULT_MIN_ITEM_FRACTION: f32 = 0.3;
/// While the cursor rests over a cap, the window pans one item every interval.
const CAP_AUTOPAGE: Duration = Duration::from_millis(110);
/// Each wheel notch pans the window by this many items.
const WHEEL_ITEMS: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

#[derive(Debug, Default)]
struct State {
    is_inside: bool,
    cursor_x: f32,
    /// Index of the left-most item currently inside the visible window.
    window_start: usize,
    /// Throttles cap auto-paging while the cursor dwells over a cap.
    last_page: Option<Instant>,
    /// Keyboard focus state.
    is_focused: bool,
    last_focused: bool,
    keyboard_focal: usize,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }
    fn focus(&mut self) {
        self.is_focused = true;
    }
    fn unfocus(&mut self) {
        self.is_focused = false;
    }
}

/// A resolved window layout: which items are visible, their positions, and the
/// hidden counts collapsed into the left/right caps.
struct Window {
    /// Per-item left edge (local x); hidden items are off-screen.
    lefts: Vec<f32>,
    /// Per-item width; hidden items are 0.
    widths: Vec<f32>,
    start: usize,
    count: usize,
    left_cap_w: f32,
    right_cap_w: f32,
    left_hidden: usize,
    right_hidden: usize,
}

pub struct ExpandableFisheyeRow<'a, Message> {
    children: Vec<Element<'a, Message>>,
    active: usize,
    comfortable_count: usize,
    height: f32,
    max_item_width: Option<f32>,
    cap_w: f32,
    min_item_fraction: f32,
    on_select: Option<Box<dyn Fn(usize) -> Message + 'a>>,
}

impl<'a, Message> Default for ExpandableFisheyeRow<'a, Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message> ExpandableFisheyeRow<'a, Message> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            active: 0,
            comfortable_count: DEFAULT_COMFORTABLE_COUNT,
            height: DEFAULT_HEIGHT,
            max_item_width: None,
            cap_w: DEFAULT_CAP_W,
            min_item_fraction: DEFAULT_MIN_ITEM_FRACTION,
            on_select: None,
        }
    }

    pub fn push(mut self, child: impl Into<Element<'a, Message>>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn extend(mut self, items: impl IntoIterator<Item = Element<'a, Message>>) -> Self {
        self.children.extend(items);
        self
    }

    pub fn active(mut self, index: usize) -> Self {
        self.active = index;
        self
    }

    pub fn comfortable_count(mut self, count: usize) -> Self {
        self.comfortable_count = count.max(1);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(0.0);
        self
    }

    pub fn max_item_width(mut self, width: f32) -> Self {
        self.max_item_width = Some(width.max(0.0));
        self
    }

    /// Width of the overflow caps on each side.
    pub fn cap_width(mut self, width: f32) -> Self {
        self.cap_w = width.max(0.0);
        self
    }

    /// Smallest an item may compress to, as a fraction of the full item width,
    /// before it overflows into the caps. Lower packs in more compressed items
    /// before any are hidden; higher makes the caps appear sooner. Default 0.3.
    pub fn min_item_fraction(mut self, fraction: f32) -> Self {
        self.min_item_fraction = fraction.clamp(0.05, 1.0);
        self
    }

    pub fn on_select(mut self, callback: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }

    fn full_w(&self, total_w: f32) -> f32 {
        let comfortable = total_w / self.comfortable_count.max(1) as f32;
        let n = self.children.len().max(1) as f32;
        match self.max_item_width {
            Some(w) => w.clamp(total_w / n, comfortable),
            None => comfortable,
        }
    }

    fn active_clamped(&self) -> usize {
        self.active.min(self.children.len().saturating_sub(1))
    }

    /// Cap width, clamped so the overflow tabs never dominate a narrow row
    /// (otherwise two 30px caps swallow half a compact panel applet).
    fn cap_w_eff(&self, total_w: f32) -> f32 {
        self.cap_w.min(total_w * 0.13)
    }

    /// Largest number of items the fisheye can show while keeping the smallest
    /// (edge) item at least a legible floor wide. Anything past this overflows
    /// into the caps. Derived from the same `s_min` the bell uses, so the cut
    /// happens exactly where edge items would become unreadable slivers.
    fn window_count(&self, total_w: f32) -> usize {
        let n = self.children.len();
        if n == 0 {
            return 0;
        }
        let full = self.full_w(total_w);
        let k = self.comfortable_count.max(1) as f32;
        // Edge items below this width overflow into the caps. Lower = more
        // items packed in (compressed) before any are collapsed; higher = caps
        // kick in sooner.
        let floor = (full * self.min_item_fraction).clamp(4.0, full.max(1.0));
        let cap_w = self.cap_w_eff(total_w);
        for c in (1..=n).rev() {
            let caps = if c < n { 2.0 * cap_w } else { 0.0 };
            let avail = (total_w - caps).max(1.0);
            let cf = c as f32;
            let s_min = if 2.0 * cf > k {
                (2.0 * avail - k * full) / (2.0 * cf - k)
            } else {
                full
            };
            if s_min >= floor {
                return c;
            }
        }
        1
    }

    /// Window start that centres the given item.
    fn centre_start(&self, total_w: f32, focal: usize) -> usize {
        let n = self.children.len();
        let count = self.window_count(total_w);
        focal
            .saturating_sub(count / 2)
            .min(n.saturating_sub(count))
    }

    /// The window start to actually draw with. While the user is driving (hover
    /// or keyboard focus) the paged `window_start` is honoured; otherwise the
    /// window follows the active item, so external selection changes (e.g. a
    /// keyboard workspace switch) scroll the active item into view.
    fn effective_start(&self, state: &State, total_w: f32) -> usize {
        if state.is_inside || state.is_focused {
            let count = self.window_count(total_w);
            state
                .window_start
                .min(self.children.len().saturating_sub(count))
        } else {
            self.centre_start(total_w, self.active_clamped())
        }
    }

    /// Resolve the visible window. `focal_x` (local x) is where the fisheye bell
    /// peaks — the cursor when hovering, otherwise the active item's slot.
    fn window(&self, total_w: f32, window_start: usize, focal_x: Option<f32>) -> Window {
        let n = self.children.len();
        let mut lefts = vec![-1.0e6; n];
        let mut widths = vec![0.0; n];
        if n == 0 || total_w <= 0.0 {
            return Window {
                lefts,
                widths,
                start: 0,
                count: 0,
                left_cap_w: 0.0,
                right_cap_w: 0.0,
                left_hidden: 0,
                right_hidden: 0,
            };
        }

        let count = self.window_count(total_w);
        let start = window_start.min(n - count);
        let left_hidden = start;
        let right_hidden = n - start - count;
        let cap_w = self.cap_w_eff(total_w);
        let left_cap_w = if left_hidden > 0 { cap_w } else { 0.0 };
        let right_cap_w = if right_hidden > 0 { cap_w } else { 0.0 };
        let avail = (total_w - left_cap_w - right_cap_w).max(1.0);

        // Original fisheye bell over the visible items: a narrow bell of
        // `comfortable_count` anchors enlarges the items around the focal point
        // up to `full`, while the rest compress toward `s_min`. This is what
        // packs many items in; the floor on `s_min` is enforced by the count.
        let full = self.full_w(total_w);
        let k = self.comfortable_count.max(1) as f32;
        let cf = count as f32;
        let s_anchor = avail / cf;
        let effect_w = k * s_anchor;
        let s_min = ((2.0 * avail - k * full) / (2.0 * cf - k)).max(0.0);

        // Where the bell peaks: cursor while hovering, else the active item.
        let active = self.active_clamped();
        let focal = match focal_x {
            Some(fx) => fx.clamp(left_cap_w, left_cap_w + avail),
            None if active >= start && active < start + count => {
                left_cap_w + (active - start) as f32 * s_anchor + s_anchor * 0.5
            }
            None => left_cap_w + avail * 0.5,
        };
        let bell_left = focal - effect_w / 2.0;
        let two_pi = 2.0 * std::f32::consts::PI;
        let raw: Vec<f32> = (0..count)
            .map(|j| {
                let a = left_cap_w + (j as f32 + 0.5) * s_anchor;
                let pos = ((a - bell_left) / effect_w).clamp(0.0, 1.0);
                let weight = (1.0 - (pos * two_pi).cos()) * 0.5;
                s_min + weight * (full - s_min)
            })
            .collect();
        let rsum: f32 = raw.iter().sum();

        let mut x = left_cap_w;
        for j in 0..count {
            let w = raw[j] / rsum * avail;
            lefts[start + j] = x;
            widths[start + j] = w;
            x += w;
        }

        Window {
            lefts,
            widths,
            start,
            count,
            left_cap_w,
            right_cap_w,
            left_hidden,
            right_hidden,
        }
    }

    fn hit_item(win: &Window, x: f32) -> Option<usize> {
        for i in win.start..win.start + win.count {
            if win.widths[i] > 0.0 && x >= win.lefts[i] && x < win.lefts[i] + win.widths[i] {
                return Some(i);
            }
        }
        None
    }

    fn cap_under(win: &Window, total_w: f32, x: f32) -> Option<Side> {
        if win.left_cap_w > 0.0 && x < win.left_cap_w {
            Some(Side::Left)
        } else if win.right_cap_w > 0.0 && x > total_w - win.right_cap_w {
            Some(Side::Right)
        } else {
            None
        }
    }

    /// Pan the window by `step` items toward `side`, clamped to range.
    fn page(&self, start: usize, total_w: f32, side: Side, step: usize) -> usize {
        let n = self.children.len();
        let count = self.window_count(total_w);
        match side {
            Side::Left => start.saturating_sub(step),
            Side::Right => (start + step).min(n.saturating_sub(count)),
        }
    }

    /// Adjust window start so `idx` is inside the visible window.
    fn reveal(&self, start: usize, total_w: f32, idx: usize) -> usize {
        let n = self.children.len();
        let count = self.window_count(total_w);
        let max_start = n.saturating_sub(count);
        if idx < start {
            idx.min(max_start)
        } else if idx >= start + count {
            (idx + 1).saturating_sub(count).min(max_start)
        } else {
            start.min(max_start)
        }
    }
}

impl<'a, Message: 'a> Widget<Message, cosmic::Theme, cosmic::Renderer>
    for ExpandableFisheyeRow<'a, Message>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &cosmic::Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(None, layout.bounds(), state);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = limits.max();
        let width = if max.width.is_finite() { max.width } else { 800.0 };
        let height = self.height;
        let n = self.children.len();
        if n == 0 {
            return layout::Node::new(Size::new(width, height));
        }
        let slot = Size::new(self.full_w(width), height);
        let limits = layout::Limits::new(Size::ZERO, slot);
        let nodes: Vec<_> = self
            .children
            .iter_mut()
            .zip(tree.children.iter_mut())
            .map(|(child, child_tree)| child.as_widget_mut().layout(child_tree, renderer, &limits))
            .collect();
        layout::Node::with_children(Size::new(width, height), nodes)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let n = self.children.len();
        if n == 0 || bounds.width <= 0.0 {
            return;
        }
        let total_w = bounds.width;
        let active = self.active_clamped();

        // Bell peaks at the cursor while hovering. Over a cap the cursor x clamps
        // to that edge, so the bell peaks there and items panning in arrive big.
        let focal_x = if state.is_inside {
            Some(state.cursor_x)
        } else {
            None
        };
        let start = self.effective_start(state, total_w);
        let win = self.window(total_w, start, focal_x);
        let over_cap = state.is_inside && Self::cap_under(&win, total_w, state.cursor_x).is_some();

        let child_layouts: Vec<_> = layout.children().collect();

        let hovered = if state.is_inside && !over_cap {
            Self::hit_item(&win, state.cursor_x)
        } else if state.is_focused {
            Some(state.keyboard_focal.min(n - 1))
        } else {
            None
        };

        let cosmic_theme = theme.cosmic();
        let accent: Color = cosmic_theme.accent_color().into();
        let r_s = cosmic_theme.corner_radii.radius_s[0];
        let highlight_color = Color { a: 0.18, ..accent };
        let shade: Color = cosmic_theme.shade.into();
        let shadow_color = Color { a: 0.28, ..shade };
        let cap_bg: Color = cosmic_theme.bg_component_color().into();

        let Some(widget_clip) = bounds.intersection(viewport) else {
            return;
        };

        // Z-order: draw items further from the pivot first so the focal item
        // overlaps its neighbours.
        let pivot = hovered.unwrap_or(active);
        let mut order: Vec<usize> = (win.start..win.start + win.count).collect();
        order.sort_by_key(|&i| -((i as i64 - pivot as i64).abs()));

        let radius_for = |i: usize| -> [f32; 4] {
            if i < pivot {
                [r_s, 0.0, 0.0, r_s]
            } else if i > pivot {
                [0.0, r_s, r_s, 0.0]
            } else {
                [r_s; 4]
            }
        };

        renderer.with_layer(widget_clip, |renderer| {
            for i in order.iter().copied() {
                let zone_left = win.lefts[i];
                let zone_w = win.widths[i];
                if zone_w <= 0.0 {
                    continue;
                }
                let zone_right = zone_left + zone_w;
                let child_size = child_layouts[i].bounds().size();
                let target_local = (zone_left + zone_right) * 0.5;
                let dx = target_local - child_size.width * 0.5;
                let dy = (bounds.height - child_size.height) * 0.5;

                let zone_rect = Rectangle {
                    x: bounds.x + zone_left,
                    y: bounds.y,
                    width: zone_w,
                    height: bounds.height,
                };
                let Some(item_clip) = zone_rect.intersection(&widget_clip) else {
                    continue;
                };
                let item_radius = radius_for(i);

                renderer.fill_quad(
                    renderer::Quad {
                        bounds: zone_rect,
                        border: Border {
                            radius: item_radius.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        shadow: Shadow {
                            color: shadow_color,
                            offset: Vector::new(0.0, 3.0),
                            blur_radius: 8.0,
                        },
                        snap: false,
                    },
                    Background::Color(Color::TRANSPARENT),
                );

                renderer.with_layer(item_clip, |renderer| {
                    if i == active {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: zone_rect,
                                border: Border {
                                    radius: item_radius.into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                shadow: Shadow::default(),
                                snap: false,
                            },
                            Background::Color(highlight_color),
                        );
                    }

                    renderer.with_translation(Vector::new(dx, dy), |renderer| {
                        self.children[i].as_widget().draw(
                            &tree.children[i],
                            renderer,
                            theme,
                            style,
                            child_layouts[i],
                            cursor,
                            viewport,
                        );
                    });

                    if Some(i) == hovered {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: zone_rect,
                                border: Border {
                                    radius: item_radius.into(),
                                    width: 1.5,
                                    color: accent,
                                },
                                shadow: Shadow::default(),
                                snap: false,
                            },
                            Background::Color(Color::TRANSPARENT),
                        );
                    }
                });
            }

            // Overflow caps: collapse the hidden items on each side into a tab
            // that shows how many are there and points the way.
            if win.left_cap_w > 0.0 {
                let rect = Rectangle {
                    x: bounds.x,
                    y: bounds.y,
                    width: win.left_cap_w,
                    height: bounds.height,
                };
                let hovered = state.is_inside && state.cursor_x < win.left_cap_w;
                draw_cap(
                    renderer,
                    rect,
                    Side::Left,
                    win.left_hidden,
                    hovered,
                    accent,
                    cap_bg,
                    r_s,
                    renderer.default_font(),
                );
            }
            if win.right_cap_w > 0.0 {
                let rect = Rectangle {
                    x: bounds.x + bounds.width - win.right_cap_w,
                    y: bounds.y,
                    width: win.right_cap_w,
                    height: bounds.height,
                };
                let hovered = state.is_inside && state.cursor_x > total_w - win.right_cap_w;
                draw_cap(
                    renderer,
                    rect,
                    Side::Right,
                    win.right_hidden,
                    hovered,
                    accent,
                    cap_bg,
                    r_s,
                    renderer.default_font(),
                );
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        _renderer: &cosmic::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width <= 0.0 {
            return;
        }
        let total_w = bounds.width;
        let state = tree.state.downcast_mut::<State>();

        // Keyboard focus enters/leaves: centre the window on the focal item.
        if state.is_focused != state.last_focused {
            state.last_focused = state.is_focused;
            if state.is_focused {
                state.keyboard_focal = self.active_clamped();
                state.window_start = self.centre_start(total_w, state.keyboard_focal);
            } else if !state.is_inside {
                state.window_start = self.centre_start(total_w, self.active_clamped());
            }
            shell.request_redraw();
        }

        // Cursor leaves: snap the window back to the active item.
        let mut leave = |state: &mut State| {
            if !state.is_inside {
                return;
            }
            state.is_inside = false;
            state.last_page = None;
            if !state.is_focused {
                state.window_start = self.centre_start(total_w, self.active_clamped());
            }
            shell.request_redraw();
        };

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let inside = cursor.position().is_some_and(|p| bounds.contains(p));
                if inside {
                    let was_inside = state.is_inside;
                    state.cursor_x = (cursor.position().unwrap().x - bounds.x).clamp(0.0, total_w);
                    state.is_inside = true;
                    state.last_page = None;
                    // Hover begins from wherever the resting window sits (centred
                    // on the active item), so paging continues without a jump.
                    if !was_inside {
                        state.window_start = self.centre_start(total_w, self.active_clamped());
                    }
                    shell.request_redraw();
                } else {
                    leave(state);
                }
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                leave(state);
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position() else { return };
                if !bounds.contains(p) {
                    return;
                }
                let cx = (p.x - bounds.x).clamp(0.0, total_w);
                state.cursor_x = cx;
                let win = self.window(total_w, state.window_start, Some(cx));

                // Clicking a cap jumps a whole window toward that side.
                if let Some(side) = Self::cap_under(&win, total_w, cx) {
                    state.window_start = self.page(state.window_start, total_w, side, win.count);
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }

                // Otherwise select the item under the cursor.
                if let Some(ref on_select) = self.on_select
                    && let Some(idx) = Self::hit_item(&win, cx)
                {
                    shell.publish(on_select(idx));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !state.is_inside {
                    return;
                }
                let d = match delta {
                    mouse::ScrollDelta::Lines { x, y } => {
                        if x.abs() > 0.0 { *x } else { *y }
                    }
                    mouse::ScrollDelta::Pixels { x, y } => {
                        if x.abs() > 0.0 { *x } else { *y }
                    }
                };
                if d != 0.0 {
                    let side = if d > 0.0 { Side::Left } else { Side::Right };
                    state.window_start = self.page(state.window_start, total_w, side, WHEEL_ITEMS);
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if !state.is_focused {
                    return;
                }
                let n = self.children.len();
                if n == 0 {
                    return;
                }
                let mut moved = true;
                match key.as_ref() {
                    keyboard::Key::Named(Named::ArrowLeft) => {
                        state.keyboard_focal = state.keyboard_focal.saturating_sub(1);
                    }
                    keyboard::Key::Named(Named::ArrowRight) => {
                        state.keyboard_focal = (state.keyboard_focal + 1).min(n - 1);
                    }
                    keyboard::Key::Named(Named::Home) => {
                        state.keyboard_focal = 0;
                    }
                    keyboard::Key::Named(Named::End) => {
                        state.keyboard_focal = n - 1;
                    }
                    keyboard::Key::Named(Named::Enter) => {
                        moved = false;
                        if let Some(ref on_select) = self.on_select {
                            shell.publish(on_select(state.keyboard_focal.min(n - 1)));
                            shell.capture_event();
                        }
                    }
                    keyboard::Key::Character(s) if AsRef::<str>::as_ref(s) == " " => {
                        moved = false;
                        if let Some(ref on_select) = self.on_select {
                            shell.publish(on_select(state.keyboard_focal.min(n - 1)));
                            shell.capture_event();
                        }
                    }
                    _ => moved = false,
                }
                if moved {
                    state.window_start =
                        self.reveal(state.window_start, total_w, state.keyboard_focal);
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(_)) => {
                if !state.is_inside {
                    return;
                }
                // Dwelling over a cap pans the window one item at a time.
                let win = self.window(total_w, state.window_start, Some(state.cursor_x));
                if let Some(side) = Self::cap_under(&win, total_w, state.cursor_x) {
                    let now = Instant::now();
                    match state.last_page {
                        None => state.last_page = Some(now),
                        Some(t) if now - t >= CAP_AUTOPAGE => {
                            state.window_start = self.page(state.window_start, total_w, side, 1);
                            state.last_page = Some(now);
                        }
                        _ => {}
                    }
                    shell.request_redraw();
                } else {
                    state.last_page = None;
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        if let Some(p) = cursor.position()
            && layout.bounds().contains(p)
            && self.on_select.is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cap(
    renderer: &mut cosmic::Renderer,
    rect: Rectangle,
    side: Side,
    count: usize,
    hovered: bool,
    accent: Color,
    bg: Color,
    radius: f32,
    font: cosmic::iced::Font,
) {
    if rect.width <= 0.0 {
        return;
    }
    let bg_alpha = if hovered { 0.95 } else { 0.72 };
    let bg = Color { a: bg_alpha, ..bg };
    let border_color = if hovered { accent } else { Color::TRANSPARENT };
    let radii: [f32; 4] = match side {
        Side::Left => [radius, 0.0, 0.0, radius],
        Side::Right => [0.0, radius, radius, 0.0],
    };

    renderer.fill_quad(
        renderer::Quad {
            bounds: rect,
            border: Border {
                radius: radii.into(),
                width: if hovered { 1.0 } else { 0.0 },
                color: border_color,
            },
            shadow: Shadow::default(),
            snap: false,
        },
        Background::Color(bg),
    );

    let label = match side {
        Side::Left => format!("‹{count}"),
        Side::Right => format!("{count}›"),
    };
    let icon_color = if hovered {
        accent
    } else {
        Color { a: 0.9, ..accent }
    };
    let cx = rect.x + rect.width * 0.5;
    let cy = rect.y + rect.height * 0.5;

    renderer.fill_text(
        cosmic::iced::core::Text {
            content: label,
            bounds: Size::new(rect.width, rect.height),
            size: Pixels((rect.height * 0.2).clamp(11.0, 18.0)),
            line_height: cosmic::iced::core::text::LineHeight::default(),
            font,
            align_x: alignment::Horizontal::Center.into(),
            align_y: alignment::Vertical::Center,
            shaping: cosmic::iced::core::text::Shaping::Basic,
            wrapping: cosmic::iced::core::text::Wrapping::None,
            ellipsize: cosmic::iced::core::text::Ellipsize::None,
        },
        cosmic::iced::Point::new(cx, cy),
        icon_color,
        rect,
    );
}

impl<'a, Message: 'a> From<ExpandableFisheyeRow<'a, Message>> for Element<'a, Message> {
    fn from(row: ExpandableFisheyeRow<'a, Message>) -> Self {
        Element::new(row)
    }
}

pub fn expandable_fisheye_row<'a, Message>() -> ExpandableFisheyeRow<'a, Message> {
    ExpandableFisheyeRow::new()
}
