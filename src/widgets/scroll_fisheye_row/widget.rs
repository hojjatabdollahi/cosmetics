// SPDX-License-Identifier: MPL-2.0

//! Scrolling fisheye row. See [`super`] for usage.
//!
//! Layout: at every `scroll` (∈ [0, 1]), the row is split into
//!   left compressed | full region (`full_fraction`) | right compressed
//! whose widths shift with `scroll`. Items inside the full region have
//! uniform `full_w` width. Items in compressed regions have widths that
//! taper linearly from `full_w` at the edge of the full region down to
//! a minimum at the widget edge.
//!
//! Cursor in compressed region → auto-scroll toward that side, with
//! quadratic speed in distance from the full region. Cursor in full
//! region → no scroll. Cursor leaves widget → snap `scroll` to the
//! position that puts the active item in its natural place.

use cosmic::Element;
use cosmic::iced::core::Renderer as _;
use cosmic::iced::core::event::Event;
use cosmic::iced::core::layout::{self, Layout};
use cosmic::iced::core::mouse::{self, Cursor};
use cosmic::iced::core::renderer;
use cosmic::iced::core::widget::tree::{self, Tree};
use cosmic::iced::core::{
    Background, Border, Clipboard, Color, Length, Rectangle, Shadow, Shell, Size, Vector, Widget,
    window,
};
use std::time::{Duration, Instant};

const DEFAULT_VISIBLE_COUNT: usize = 4;
const DEFAULT_FULL_FRACTION: f32 = 0.6;
const DEFAULT_HEIGHT: f32 = 96.0;
const DEFAULT_SNAP_DURATION: Duration = Duration::from_millis(220);
const DEFAULT_MAX_SCROLL_SPEED: f32 = 1500.0; // pixels (of widget width) per second at edge

#[derive(Debug, Default)]
struct State {
    /// scroll position in [0, 1]: 0 = active is the first item, 1 = active is the last.
    scroll: f32,
    is_inside: bool,
    cursor_x: f32,
    last_tick: Option<Instant>,
    snap: Option<SnapAnim>,
    initialized: bool,
}

#[derive(Debug, Clone, Copy)]
struct SnapAnim {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
}

impl SnapAnim {
    fn current(&self) -> f32 {
        let total = self.duration.as_secs_f32().max(f32::EPSILON);
        let t = (self.started.elapsed().as_secs_f32() / total).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * eased
    }

    fn is_done(&self) -> bool {
        self.started.elapsed() >= self.duration
    }
}

pub struct ScrollFisheyeRow<'a, Message> {
    children: Vec<Element<'a, Message>>,
    active: usize,
    visible_count: usize,
    full_fraction: f32,
    height: f32,
    max_item_width: Option<f32>,
    snap_duration: Duration,
    max_scroll_speed: f32,
    on_select: Option<Box<dyn Fn(usize) -> Message + 'a>>,
}

impl<'a, Message> Default for ScrollFisheyeRow<'a, Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message> ScrollFisheyeRow<'a, Message> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            active: 0,
            visible_count: DEFAULT_VISIBLE_COUNT,
            full_fraction: DEFAULT_FULL_FRACTION,
            height: DEFAULT_HEIGHT,
            max_item_width: None,
            snap_duration: DEFAULT_SNAP_DURATION,
            max_scroll_speed: DEFAULT_MAX_SCROLL_SPEED,
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

    pub fn visible_count(mut self, count: usize) -> Self {
        self.visible_count = count.max(1);
        self
    }

    pub fn full_fraction(mut self, fraction: f32) -> Self {
        self.full_fraction = fraction.clamp(0.1, 1.0);
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

    pub fn snap_duration(mut self, duration: Duration) -> Self {
        self.snap_duration = duration;
        self
    }

    /// Maximum auto-scroll speed at the very edge of the widget, in
    /// (widget-width pixels per second). Speed scales quadratically
    /// from 0 at the full-region boundary up to this at the widget edge.
    pub fn max_scroll_speed(mut self, px_per_sec: f32) -> Self {
        self.max_scroll_speed = px_per_sec.max(0.0);
        self
    }

    pub fn on_select(mut self, callback: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }

    fn full_w(&self, total_w: f32) -> f32 {
        let full_region = self.full_fraction * total_w;
        let k = self.visible_count.max(1) as f32;
        let comfortable = full_region / k;
        match self.max_item_width {
            Some(w) => w.clamp(0.0, comfortable),
            None => comfortable,
        }
    }

    fn active_clamped(&self) -> usize {
        self.active.min(self.children.len().saturating_sub(1))
    }

    /// Scroll position [0, 1] that puts the active item in its rest spot.
    fn rest_scroll(&self) -> f32 {
        let n = self.children.len();
        if n <= 1 {
            return 0.0;
        }
        self.active_clamped() as f32 / (n - 1) as f32
    }

    /// Build (lefts, widths). Widths sum to total_w.
    fn compute(&self, scroll: f32, total_w: f32) -> (Vec<f32>, Vec<f32>) {
        let n = self.children.len();
        if n == 0 || total_w <= 0.0 {
            return (Vec::new(), Vec::new());
        }
        let scroll = scroll.clamp(0.0, 1.0);

        if n <= self.visible_count {
            let w = total_w / n as f32;
            let lefts: Vec<f32> = (0..n).map(|i| i as f32 * w).collect();
            return (lefts, vec![w; n]);
        }

        let k = self.visible_count;
        let full_w = self.full_w(total_w);
        let full_region_w = full_w * k as f32;
        // The remaining width (total - full_region) is split between left and
        // right compressed regions in proportion to scroll.
        let comp_total = (total_w - full_region_w).max(0.0);
        let comp_left_w = scroll * comp_total;
        let comp_right_w = comp_total - comp_left_w;

        // Pick which K items live in the full region. With scroll = active/(N-1),
        // the active item naturally falls inside the full window.
        let first_full = (scroll * (n - k) as f32).round() as usize;
        let first_full = first_full.min(n - k);
        let last_full = first_full + k - 1;

        let mut widths = vec![0.0_f32; n];

        // Full region: K items at full_w each.
        for i in first_full..=last_full {
            widths[i] = full_w;
        }

        // Left compressed: items 0..first_full. The item closest to the full
        // region gets the most width, the item at the widget edge the least.
        // Use a triangular weighting normalized to comp_left_w.
        if first_full > 0 && comp_left_w > 0.0 {
            let m = first_full;
            let sum: f32 = (1..=m).map(|w| w as f32).sum(); // 1+2+...+m = m(m+1)/2
            for j in 0..m {
                // j=0 is the leftmost (most compressed); j=m-1 is right next
                // to the full region (least compressed).
                let weight = (j + 1) as f32;
                widths[j] = comp_left_w * weight / sum;
            }
        }

        // Right compressed: items last_full+1..n. Item next to full = widest.
        if last_full + 1 < n && comp_right_w > 0.0 {
            let m = n - last_full - 1;
            let sum: f32 = (1..=m).map(|w| w as f32).sum();
            for j in 0..m {
                // j=0 is right next to the full region (least compressed);
                // j=m-1 is the rightmost (most compressed).
                let weight = (m - j) as f32;
                widths[last_full + 1 + j] = comp_right_w * weight / sum;
            }
        }

        let mut lefts = Vec::with_capacity(n);
        let mut x = 0.0;
        for &w in &widths {
            lefts.push(x);
            x += w;
        }
        (lefts, widths)
    }

    fn full_region_bounds(&self, scroll: f32, total_w: f32) -> (f32, f32) {
        let n = self.children.len();
        if n == 0 || total_w <= 0.0 {
            return (0.0, total_w);
        }
        if n <= self.visible_count {
            return (0.0, total_w);
        }
        let full_w = self.full_w(total_w);
        let full_region_w = full_w * self.visible_count as f32;
        let comp_total = (total_w - full_region_w).max(0.0);
        let comp_left_w = scroll.clamp(0.0, 1.0) * comp_total;
        (comp_left_w, comp_left_w + full_region_w)
    }

    fn current_scroll(&self, state: &State) -> f32 {
        if let Some(snap) = &state.snap {
            snap.current()
        } else if state.is_inside || state.initialized {
            state.scroll
        } else {
            self.rest_scroll()
        }
    }

    fn hit(lefts: &[f32], x: f32) -> Option<usize> {
        if lefts.is_empty() {
            return None;
        }
        for i in (0..lefts.len()).rev() {
            if x >= lefts[i] {
                return Some(i);
            }
        }
        Some(0)
    }
}

impl<'a, Message: 'a> Widget<Message, cosmic::Theme, cosmic::Renderer>
    for ScrollFisheyeRow<'a, Message>
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
        let scroll = self.current_scroll(state);
        let (lefts, widths) = self.compute(scroll, total_w);
        let child_layouts: Vec<_> = layout.children().collect();
        let active = self.active_clamped();

        // Focal = item under cursor (only when inside).
        let focal = if state.is_inside {
            Self::hit(&lefts, state.cursor_x)
        } else {
            None
        };

        let cosmic_theme = theme.cosmic();
        let accent: Color = cosmic_theme.accent_color().into();
        let r_s = cosmic_theme.corner_radii.radius_s[0];
        let highlight_color = Color { a: 0.18, ..accent };
        let shade: Color = cosmic_theme.shade.into();
        let shadow_color = Color { a: 0.28, ..shade };

        let Some(widget_clip) = bounds.intersection(viewport) else {
            return;
        };

        // Draw order: from edges inward, with active and focal on top.
        // The "top" is the active item; the focal (cursor) item is one below.
        // Items farther from active drawn first.
        let mut order: Vec<usize> = (0..n).collect();
        let pivot = focal.unwrap_or(active) as i64;
        order.sort_by_key(|&i| -(i as i64 - pivot).abs());

        let radius_for = |i: usize| -> [f32; 4] {
            let pivot_idx = focal.unwrap_or(active);
            // Order: top_left, top_right, bottom_right, bottom_left.
            if i < pivot_idx {
                [r_s, 0.0, 0.0, r_s]
            } else if i > pivot_idx {
                [0.0, r_s, r_s, 0.0]
            } else {
                [r_s; 4]
            }
        };

        renderer.with_layer(widget_clip, |renderer| {
            for i in order.iter().copied() {
                let zone_left = lefts[i];
                let zone_w = widths[i];
                if zone_w <= 0.0 {
                    continue;
                }
                let zone_right = zone_left + zone_w;

                let child_size = child_layouts[i].bounds().size();
                let half_w = child_size.width * 0.5;

                // Always center content in its zone (the row is the scroll
                // mechanism — items don't lean).
                let target_local = (zone_left + zone_right) * 0.5;

                let dx = target_local - half_w;
                let dy = (bounds.height - child_size.height) * 0.5;

                let zone_rect = Rectangle {
                    x: bounds.x + zone_left,
                    y: bounds.y,
                    width: zone_w,
                    height: bounds.height,
                };
                let item_clip = match zone_rect.intersection(viewport) {
                    Some(c) => c,
                    None => continue,
                };

                let item_radius = radius_for(i);

                // Drop shadow under each item.
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

                    if Some(i) == focal {
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

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let inside = cursor.position().is_some_and(|p| bounds.contains(p));
                if inside {
                    let cx = (cursor.position().unwrap().x - bounds.x).clamp(0.0, total_w);
                    if !state.is_inside {
                        // First step inside: start tick clock from now and
                        // capture the current scroll as our working position.
                        state.last_tick = Some(Instant::now());
                        if !state.initialized {
                            state.scroll = self.rest_scroll();
                        } else if let Some(snap) = state.snap {
                            state.scroll = snap.current();
                        }
                        state.snap = None;
                    }
                    state.cursor_x = cx;
                    state.is_inside = true;
                    state.initialized = true;
                    shell.request_redraw();
                } else if state.is_inside {
                    state.is_inside = false;
                    state.last_tick = None;
                    let target = self.rest_scroll();
                    state.snap = Some(SnapAnim {
                        from: state.scroll,
                        to: target,
                        started: Instant::now(),
                        duration: self.snap_duration,
                    });
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(ref on_select) = self.on_select
                    && let Some(p) = cursor.position()
                    && bounds.contains(p)
                {
                    let cx = (p.x - bounds.x).clamp(0.0, total_w);
                    let scroll = self.current_scroll(state);
                    let (lefts, _widths) = self.compute(scroll, total_w);
                    if let Some(idx) = Self::hit(&lefts, cx) {
                        shell.publish(on_select(idx));
                        shell.capture_event();
                    }
                }
            }
            Event::Window(window::Event::RedrawRequested(_)) => {
                let mut keep_redrawing = false;

                if state.is_inside {
                    let now = Instant::now();
                    let dt = match state.last_tick {
                        Some(t) => (now - t).as_secs_f32().min(0.05),
                        None => 0.0,
                    };
                    state.last_tick = Some(now);

                    let (full_left, full_right) =
                        self.full_region_bounds(state.scroll, total_w);
                    let cx = state.cursor_x;
                    let mut delta = 0.0_f32;

                    // Speed ramp: quadratic on top of a non-zero floor. Without
                    // the floor the speed asymptotes to 0 as the cursor nears
                    // the full-region edge, and the scroll stalls before the
                    // highlighted item is actually pulled into the full region.
                    let speed_factor =
                        |d: f32| 0.12 + 0.88 * d * d;

                    if cx < full_left {
                        // Auto-scroll left.
                        let denom = full_left.max(1.0);
                        let dist_norm = ((full_left - cx) / denom).clamp(0.0, 1.0);
                        let speed = -self.max_scroll_speed * speed_factor(dist_norm);
                        delta = speed * dt;
                    } else if cx > full_right {
                        // Auto-scroll right.
                        let denom = (total_w - full_right).max(1.0);
                        let dist_norm = ((cx - full_right) / denom).clamp(0.0, 1.0);
                        let speed = self.max_scroll_speed * speed_factor(dist_norm);
                        delta = speed * dt;
                    }

                    if delta != 0.0 {
                        // Convert pixel delta into scroll-units (scroll is [0, 1]).
                        let comp_total = ((1.0 - self.full_fraction) * total_w).max(1.0);
                        state.scroll = (state.scroll + delta / comp_total).clamp(0.0, 1.0);
                        keep_redrawing = true;
                    }
                }

                if let Some(snap) = state.snap {
                    if snap.is_done() {
                        state.scroll = snap.to;
                        state.snap = None;
                    } else {
                        state.scroll = snap.current();
                        keep_redrawing = true;
                    }
                }

                if keep_redrawing {
                    shell.request_redraw();
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
        if self.on_select.is_some()
            && cursor.position().is_some_and(|p| layout.bounds().contains(p))
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, Message: 'a> From<ScrollFisheyeRow<'a, Message>> for Element<'a, Message> {
    fn from(row: ScrollFisheyeRow<'a, Message>) -> Self {
        Element::new(row)
    }
}

pub fn scroll_fisheye_row<'a, Message>() -> ScrollFisheyeRow<'a, Message> {
    ScrollFisheyeRow::new()
}
