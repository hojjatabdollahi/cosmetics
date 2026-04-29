// SPDX-License-Identifier: MPL-2.0

//! Fisheye row widget. See [`super`] for usage.
//!
//! Algorithm: Bederson-style fisheye + macOS Dock cosine bell.
//! Each item has an anchor at uniform spacing `s_anchor = total_w / N`. A bell of
//! width `effect_w = K * s_anchor` is centered on the cursor; each anchor that
//! falls inside the bell receives a cosine weight in [0, 1]. Weights drive raw
//! widths in [s_min, full_w]. Raw widths are normalized to sum to total_w, then
//! laid out left-to-right. The focal item is whichever zone contains the cursor
//! after layout — *not* the item the bell magnifies most. This is what lets the
//! cursor reach every item by sweeping the row width: in compressed regions the
//! item zones are narrow, so a small cursor move switches focal; in the wide
//! region (where the bell magnifies) the focal item's zone is wide and stable.

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

const DEFAULT_COMFORTABLE_COUNT: usize = 4;
const DEFAULT_HEIGHT: f32 = 96.0;
const DEFAULT_SNAP_DURATION: Duration = Duration::from_millis(220);

#[derive(Debug, Default)]
struct State {
    focal_x: f32,
    is_inside: bool,
    snap: Option<SnapAnim>,
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

pub struct FisheyeRow<'a, Message> {
    children: Vec<Element<'a, Message>>,
    active: usize,
    comfortable_count: usize,
    height: f32,
    snap_duration: Duration,
    max_item_width: Option<f32>,
    on_select: Option<Box<dyn Fn(usize) -> Message + 'a>>,
}

impl<'a, Message> Default for FisheyeRow<'a, Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message> FisheyeRow<'a, Message> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            active: 0,
            comfortable_count: DEFAULT_COMFORTABLE_COUNT,
            height: DEFAULT_HEIGHT,
            snap_duration: DEFAULT_SNAP_DURATION,
            max_item_width: None,
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

    pub fn snap_duration(mut self, duration: Duration) -> Self {
        self.snap_duration = duration;
        self
    }

    /// Cap the focal item's width. If unset, the focal item fills
    /// `total_w / comfortable_count` (the natural "comfortable" slot size).
    /// Set this to something closer to the content's intrinsic width if you
    /// don't want the focal item to balloon out with extra padding.
    pub fn max_item_width(mut self, width: f32) -> Self {
        self.max_item_width = Some(width.max(0.0));
        self
    }

    pub fn on_select(mut self, callback: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }

    fn full_w(&self, total_w: f32) -> f32 {
        let comfortable = total_w / self.comfortable_count.max(1) as f32;
        let n = self.children.len().max(1) as f32;
        // Clamp into [total_w/N, comfortable]: below that the math degenerates
        // (s_min would exceed full_w and there'd be no fisheye), above that
        // we'd just be making the focal bigger than its natural slot.
        match self.max_item_width {
            Some(w) => w.clamp(total_w / n, comfortable),
            None => comfortable,
        }
    }

    fn active_clamped(&self) -> usize {
        self.active.min(self.children.len().saturating_sub(1))
    }

    /// Cursor x at which the active item is the focal in the resulting layout.
    /// For middle items the active item's anchor naturally lands in its zone,
    /// but for items near the ends the cumulative layout drifts the active
    /// item's zone away from the anchor — so we binary-search for a focal_x
    /// whose hit-test returns the active item.
    fn rest_focal_x(&self, total_w: f32) -> f32 {
        let n = self.children.len();
        if n == 0 {
            return 0.0;
        }
        let active = self.active_clamped();
        let s_anchor = total_w / n as f32;

        if n <= self.comfortable_count {
            return (active as f32 + 0.5) * s_anchor;
        }

        let anchor = (active as f32 + 0.5) * s_anchor;
        let (_, _, focal_at_anchor) = self.compute(anchor, total_w);
        if focal_at_anchor == active {
            return anchor;
        }

        let mut lo = 0.0_f32;
        let mut hi = total_w;
        let mut best = anchor;
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            let (_, _, focal) = self.compute(mid, total_w);
            if focal == active {
                return mid;
            }
            if focal < active {
                lo = mid;
            } else {
                hi = mid;
            }
            best = mid;
            if hi - lo < 0.25 {
                break;
            }
        }
        best
    }

    fn current_focal_x(&self, state: &State, total_w: f32) -> f32 {
        if let Some(snap) = &state.snap {
            snap.current()
        } else if state.is_inside {
            state.focal_x
        } else {
            self.rest_focal_x(total_w)
        }
    }

    /// Returns `(lefts, widths, focal_idx)`. Widths sum to `total_w`.
    fn compute(&self, focal_x: f32, total_w: f32) -> (Vec<f32>, Vec<f32>, usize) {
        let n = self.children.len();
        if n == 0 || total_w <= 0.0 {
            return (Vec::new(), Vec::new(), 0);
        }

        if n <= self.comfortable_count {
            let w = total_w / n as f32;
            let lefts: Vec<f32> = (0..n).map(|i| i as f32 * w).collect();
            let widths = vec![w; n];
            let focal = ((focal_x / w).floor() as i32).clamp(0, n as i32 - 1) as usize;
            return (lefts, widths, focal);
        }

        let n_f = n as f32;
        let k_f = self.comfortable_count.max(1) as f32;
        let s_anchor = total_w / n_f;
        let full_w = self.full_w(total_w);
        let effect_w = k_f * s_anchor;
        // s_min picked so raw widths sum to total_w when the bell is fully
        // on-screen (K items in bell with weight sum = K/2):
        //   K * s_min/2 + K * full_w/2 + (N-K) * s_min = total_w
        //   ⇒ s_min = (2*total_w − K*full_w) / (2N − K)
        let s_min = ((2.0 * total_w - k_f * full_w) / (2.0 * n_f - k_f)).max(0.0);

        // Center the bell on the cursor — no clamping. When the cursor is near
        // an edge, the bell extends off-screen, but that's fine: items beyond
        // the bell collapse to s_min and width normalization redistributes the
        // freed budget so the visible end items grow to fill the row.
        let bell_left = focal_x - effect_w / 2.0;
        let two_pi = 2.0 * std::f32::consts::PI;

        let raw: Vec<f32> = (0..n)
            .map(|i| {
                let a = (i as f32 + 0.5) * s_anchor;
                let pos = ((a - bell_left) / effect_w).clamp(0.0, 1.0);
                let theta = pos * two_pi;
                let weight = (1.0 - theta.cos()) * 0.5;
                s_min + weight * (full_w - s_min)
            })
            .collect();

        let raw_sum: f32 = raw.iter().sum();
        let scale = if raw_sum > 0.0 { total_w / raw_sum } else { 1.0 };
        let widths: Vec<f32> = raw.iter().map(|w| w * scale).collect();

        let mut lefts = Vec::with_capacity(n);
        let mut x = 0.0;
        for &w in &widths {
            lefts.push(x);
            x += w;
        }

        // Focal = which item zone the cursor falls in.
        let cursor = focal_x.clamp(0.0, total_w);
        let focal = (0..n)
            .rev()
            .find(|&i| cursor >= lefts[i])
            .unwrap_or(0);

        (lefts, widths, focal)
    }
}

impl<'a, Message: 'a> Widget<Message, cosmic::Theme, cosmic::Renderer> for FisheyeRow<'a, Message> {
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
        let focal_x = self.current_focal_x(state, total_w);
        let (lefts, widths, focal) = self.compute(focal_x, total_w);
        let child_layouts: Vec<_> = layout.children().collect();
        let active = self.active_clamped();
        let cosmic_theme = theme.cosmic();
        let accent: Color = cosmic_theme.accent_color().into();
        let r_s = cosmic_theme.corner_radii.radius_s[0];
        let highlight_color = Color { a: 0.18, ..accent };
        let shade: Color = cosmic_theme.shade.into();
        let shadow_color = Color { a: 0.32, ..shade };

        // Per-item corner radii: only "exposed" sides are rounded. The side
        // tucked under the next-closer-to-focal item stays flat.
        let radius_for = |i: usize| -> [f32; 4] {
            // Order: top_left, top_right, bottom_right, bottom_left.
            if i < focal {
                [r_s, 0.0, 0.0, r_s]
            } else if i > focal {
                [0.0, r_s, r_s, 0.0]
            } else {
                [r_s; 4]
            }
        };

        let Some(widget_clip) = bounds.intersection(viewport) else {
            return;
        };

        // Draw items in z-order: items farther from the focal go first, the
        // focal item last. Adjacent items' shadows leak under the next item
        // so the row reads as a stack.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| -(i as i64 - focal as i64).abs());

        renderer.with_layer(widget_clip, |renderer| {
            for i in order.iter().copied() {
                let zone_left = lefts[i];
                let zone_right = zone_left + widths[i];
                let zone_w = widths[i];
                if zone_w <= 0.0 {
                    continue;
                }

                // Child renders at full natural size — no scaling. We clip
                // its drawing to the item's zone, so width shrinks but height
                // never does. Items end up overlapping in their full natural
                // span; the clip is what makes only `zone_w` of each visible.
                let child_size = child_layouts[i].bounds().size();
                let half_w = child_size.width * 0.5;

                // Content center follows the cursor, clamped to the zone.
                // When the zone is wide enough to fit the whole icon, clamp so
                // the icon stays fully inside (no clipping). When the zone is
                // narrower than the icon, the icon spills past the zone and the
                // clip shows only the cursor-facing edge.
                let target_local = if zone_w >= child_size.width {
                    focal_x.clamp(zone_left + half_w, zone_right - half_w)
                } else {
                    focal_x.clamp(zone_left, zone_right)
                };

                let dx = target_local - half_w;
                let dy = (bounds.height - child_size.height) * 0.5;

                let item_clip = Rectangle {
                    x: bounds.x + zone_left,
                    y: bounds.y,
                    width: zone_w,
                    height: bounds.height,
                };
                let Some(item_clip) = item_clip.intersection(viewport) else {
                    continue;
                };

                let zone_rect = Rectangle {
                    x: bounds.x + zone_left,
                    y: bounds.y,
                    width: zone_w,
                    height: bounds.height,
                };

                let item_radius = radius_for(i);

                // Drop shadow under each item — drawn outside the per-zone
                // clip so it can fall into the neighbor's zone, which is what
                // makes the row read as overlapping cards rather than tiles.
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

                    if i == focal {
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
                    let new_x = (cursor.position().unwrap().x - bounds.x).clamp(0.0, total_w);
                    state.focal_x = new_x;
                    state.is_inside = true;
                    state.snap = None;
                    shell.request_redraw();
                } else if state.is_inside {
                    state.is_inside = false;
                    state.snap = Some(SnapAnim {
                        from: state.focal_x,
                        to: self.rest_focal_x(total_w),
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
                    let focal_x = self.current_focal_x(state, total_w);
                    let (_, _, focal) = self.compute(focal_x, total_w);
                    shell.publish(on_select(focal));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(_)) => {
                if let Some(snap) = state.snap {
                    if snap.is_done() {
                        state.focal_x = snap.to;
                        state.snap = None;
                    } else {
                        shell.request_redraw();
                    }
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

impl<'a, Message: 'a> From<FisheyeRow<'a, Message>> for Element<'a, Message> {
    fn from(row: FisheyeRow<'a, Message>) -> Self {
        Element::new(row)
    }
}

pub fn fisheye_row<'a, Message>() -> FisheyeRow<'a, Message> {
    FisheyeRow::new()
}
