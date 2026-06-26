// SPDX-License-Identifier: MPL-2.0

//! Cover Flow widget: a 3D, perspective window switcher rendered through a
//! custom wgpu primitive (see [`super::primitive`]). The widget owns layout,
//! the eased scroll animation, and pointer hit-testing; the GPU primitive owns
//! the actual textured-card rendering.

use std::time::Instant;

use cosmic::Element;
use cosmic::iced::core::event::Event;
use cosmic::iced::core::layout::{self, Layout};
use cosmic::iced::core::mouse::{self, Cursor};
use cosmic::iced::core::widget::tree::{self, Tree};
use cosmic::iced::core::{
    Clipboard, Color, Length, Rectangle, Shell, Size, Widget, image, renderer, window,
};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;

use super::camera::{self, VISIBLE_SPAN};
use super::primitive::{CoverCard, CoverFlowPrimitive};

/// How quickly the scroll position eases toward the active card (1/seconds).
const EASE_SPEED: f32 = 14.0;
const DEFAULT_HEIGHT: f32 = 360.0;

/// One window in the flow: a thumbnail (or `None` for a flat fallback) plus the
/// tint used for the fallback / dim.
#[derive(Clone)]
pub struct CoverFlowItem {
    handle: Option<image::Handle>,
    badge: Option<image::Handle>,
    tint: [f32; 4],
}

impl CoverFlowItem {
    pub fn image(handle: image::Handle) -> Self {
        Self {
            handle: Some(handle),
            badge: None,
            // White composite background: brightens frosted/translucent windows
            // (premultiplied) while leaving opaque ones unchanged.
            tint: [1.0, 1.0, 1.0, 1.0],
        }
    }

    /// A window with no thumbnail yet: render a flat card in `color`.
    pub fn fallback(color: Color) -> Self {
        Self {
            handle: None,
            badge: None,
            tint: [color.r, color.g, color.b, color.a],
        }
    }

    /// Attach an app-icon badge (premultiplied RGBA) drawn on the card.
    pub fn badge(mut self, handle: image::Handle) -> Self {
        self.badge = Some(handle);
        self
    }
}

pub struct CoverFlow<'a, Message> {
    items: Vec<CoverFlowItem>,
    active: usize,
    height: f32,
    reflection: bool,
    table: bool,
    on_select: Option<Box<dyn Fn(usize) -> Message + 'a>>,
}

impl<'a, Message> Default for CoverFlow<'a, Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Message> CoverFlow<'a, Message> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            active: 0,
            height: DEFAULT_HEIGHT,
            reflection: true,
            table: true,
            on_select: None,
        }
    }

    pub fn push(mut self, item: CoverFlowItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn extend(mut self, items: impl IntoIterator<Item = CoverFlowItem>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn active(mut self, index: usize) -> Self {
        self.active = index;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(0.0);
        self
    }

    pub fn reflection(mut self, on: bool) -> Self {
        self.reflection = on;
        self
    }

    /// Draw a coloured floor/table band beneath the cards.
    pub fn table(mut self, on: bool) -> Self {
        self.table = on;
        self
    }

    pub fn on_select(mut self, callback: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }

    fn active_clamped(&self) -> f32 {
        self.active.min(self.items.len().saturating_sub(1)) as f32
    }
}

#[derive(Default)]
struct State {
    scroll: f32,
    initialized: bool,
    last: Option<Instant>,
}

impl<'a, Message: 'a> Widget<Message, cosmic::Theme, cosmic::Renderer> for CoverFlow<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = limits.max();
        let width = if max.width.is_finite() {
            max.width
        } else {
            960.0
        };
        layout::Node::new(Size::new(width, self.height))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if self.items.is_empty() || bounds.width <= 0.0 {
            return;
        }
        let state = tree.state.downcast_ref::<State>();
        let scroll = if state.initialized {
            state.scroll
        } else {
            self.active_clamped()
        };

        // Card background: the theme window background, used to fill any
        // fully-transparent margins of a thumbnail so they blend with the panel.
        let card_bg = {
            let c: Color = theme.cosmic().bg_color().into();
            [c.r, c.g, c.b, 1.0]
        };

        let cards = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| CoverCard {
                handle: item.handle.clone(),
                badge: item.badge.clone(),
                // Image cards: theme bg for transparent margins. Fallback cards:
                // their own flat colour.
                tint: if item.handle.is_some() {
                    card_bg
                } else {
                    item.tint
                },
                d: i as f32 - scroll,
            })
            .collect();

        // Table: solid white floor under the cards (toggleable via `.table`).
        let table = if self.table {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };

        // Wall: the spotlight's centre colour (a neutral light grey), always on.
        // The shader fades it radially to a dark edge so the focused card pops.
        let wall = [0.58, 0.60, 0.63, 1.0];

        renderer.draw_primitive(
            bounds,
            CoverFlowPrimitive {
                cards,
                reflection: self.reflection,
                table,
                wall,
            },
        );
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
        let n = self.items.len();
        if n == 0 || bounds.width <= 0.0 {
            return;
        }
        let state = tree.state.downcast_mut::<State>();
        let target = self.active_clamped();

        if !state.initialized {
            state.scroll = target;
            state.initialized = true;
        }

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                let dt = state
                    .last
                    .map(|t| (*now - t).as_secs_f32())
                    .unwrap_or(0.0)
                    .clamp(0.0, 0.05);
                state.last = Some(*now);

                if (state.scroll - target).abs() > 0.001 {
                    let k = 1.0 - (-dt * EASE_SPEED).exp();
                    state.scroll += (target - state.scroll) * k;
                    if (state.scroll - target).abs() <= 0.001 {
                        state.scroll = target;
                    }
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position() else { return };
                if !bounds.contains(p) {
                    return;
                }
                if let Some(idx) = self.nearest_card(state.scroll, bounds, p.x) {
                    if let Some(cb) = &self.on_select {
                        shell.publish(cb(idx));
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position().is_none_or(|p| !bounds.contains(p)) {
                    return;
                }
                let d = match delta {
                    mouse::ScrollDelta::Lines { x, y } | mouse::ScrollDelta::Pixels { x, y } => {
                        if x.abs() > 0.0 { *x } else { *y }
                    }
                };
                if d != 0.0 {
                    let step: i64 = if d > 0.0 { -1 } else { 1 };
                    let next = (self.active as i64 + step).clamp(0, n as i64 - 1) as usize;
                    if next != self.active {
                        if let Some(cb) = &self.on_select {
                            shell.publish(cb(next));
                            shell.capture_event();
                        }
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

impl<'a, Message: 'a> CoverFlow<'a, Message> {
    /// Index of the card whose projected centre is nearest `screen_x`.
    fn nearest_card(&self, scroll: f32, bounds: Rectangle, screen_x: f32) -> Option<usize> {
        let aspect = (bounds.width / bounds.height).max(0.01);
        let vp = camera::view_proj(aspect);
        let mut best = None;
        let mut best_dist = f32::MAX;
        for i in 0..self.items.len() {
            let d = i as f32 - scroll;
            if d.abs() > VISIBLE_SPAN {
                continue;
            }
            let p = camera::placement(d);
            let sx = camera::project_center_x(&vp, &p, bounds.x, bounds.width);
            let dist = (sx - screen_x).abs();
            if dist < best_dist {
                best_dist = dist;
                best = Some(i);
            }
        }
        best
    }
}

impl<'a, Message: 'a> From<CoverFlow<'a, Message>> for Element<'a, Message> {
    fn from(w: CoverFlow<'a, Message>) -> Self {
        Element::new(w)
    }
}

pub fn cover_flow<'a, Message>() -> CoverFlow<'a, Message> {
    CoverFlow::new()
}
