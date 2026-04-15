// SPDX-License-Identifier: MPL-2.0
#![allow(clippy::type_complexity)]

use cosmic::iced::core::{
    self, Background, Border, Element, Layout, Length, Pixels, Rectangle, Shell, Size, Widget,
    event::Event,
    keyboard::{
        self,
        key::{self, Key},
    },
    layout,
    mouse::{self, Cursor},
    renderer::{self, Quad},
    touch,
    widget::tree::{self, Tree},
    window,
};
use cosmic::iced::widget::slider::{Catalog, HandleShape, Status, Style, StyleFn};
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragTarget {
    Playhead,
    TrimStart,
    TrimEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct State {
    dragging: Option<DragTarget>,
    last_active: Option<DragTarget>,
    keyboard_modifiers: keyboard::Modifiers,
}

const HANDLE_WIDTH: f32 = 14.0;
const HANDLE_RADIUS: f32 = 4.0;
const HANDLE_HIT_PAD: f32 = 4.0;
const GRIP_LINE_WIDTH: f32 = 6.0;
const GRIP_LINE_THICKNESS: f32 = 1.5;
const FRAME_BORDER: f32 = 2.0;
const TRACK_INSET: f32 = HANDLE_WIDTH + HANDLE_HIT_PAD;

/// Player scrubber with playhead and trim bracket handles.
pub struct Scrubber<'a, T, Message, Theme = cosmic::Theme>
where
    Theme: Catalog,
{
    range: RangeInclusive<T>,
    position: T,
    trim_start: T,
    trim_end: T,
    step: T,
    shift_step: Option<T>,
    on_scrub: Option<Box<dyn Fn(T) -> Message + 'a>>,
    on_trim: Option<Box<dyn Fn((T, T)) -> Message + 'a>>,
    on_release: Option<Message>,
    width: Length,
    height: f32,
    class: Theme::Class<'a>,
    status: Option<Status>,
}

impl<'a, T, Message, Theme> Scrubber<'a, T, Message, Theme>
where
    T: Copy + From<u8> + PartialOrd,
    Message: Clone,
    Theme: Catalog,
{
    pub const DEFAULT_HEIGHT: f32 = 32.0;

    pub fn new(range: RangeInclusive<T>, position: T, trim: (T, T)) -> Self {
        let rs = *range.start();
        let re = *range.end();
        let clamp = |v: T| -> T {
            if v < rs {
                rs
            } else if v > re {
                re
            } else {
                v
            }
        };
        let (mut ts, mut te) = (clamp(trim.0), clamp(trim.1));
        if ts > te {
            std::mem::swap(&mut ts, &mut te);
        }

        Self {
            range,
            position: clamp(position),
            trim_start: ts,
            trim_end: te,
            step: T::from(1),
            shift_step: None,
            on_scrub: None,
            on_trim: None,
            on_release: None,
            width: Length::Fill,
            height: Self::DEFAULT_HEIGHT,
            class: Theme::default(),
            status: None,
        }
    }

    pub fn on_scrub(mut self, f: impl Fn(T) -> Message + 'a) -> Self {
        self.on_scrub = Some(Box::new(f));
        self
    }

    pub fn on_trim(mut self, f: impl Fn((T, T)) -> Message + 'a) -> Self {
        self.on_trim = Some(Box::new(f));
        self
    }

    pub fn on_release(mut self, msg: Message) -> Self {
        self.on_release = Some(msg);
        self
    }

    pub fn step(mut self, step: impl Into<T>) -> Self {
        self.step = step.into();
        self
    }

    pub fn shift_step(mut self, step: impl Into<T>) -> Self {
        self.shift_step = Some(step.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Pixels>) -> Self {
        self.height = height.into().0;
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }
}

fn val_to_x<T: Copy + Into<f64>>(val: T, range: &RangeInclusive<T>, bounds: &Rectangle) -> f32 {
    let start = (*range.start()).into() as f32;
    let end = (*range.end()).into() as f32;
    let span = end - start;
    if span <= 0.0 {
        return bounds.x + TRACK_INSET;
    }
    let usable = (bounds.width - 2.0 * TRACK_INSET).max(0.0);
    bounds.x + TRACK_INSET + usable * (val.into() as f32 - start) / span
}

fn x_to_val<T: Copy + Into<f64> + num_traits::FromPrimitive>(
    x: f32,
    bounds: &Rectangle,
    range: &RangeInclusive<T>,
    step: f64,
) -> Option<T> {
    let start = (*range.start()).into();
    let end = (*range.end()).into();
    let inner_left = bounds.x + TRACK_INSET;
    let usable = (bounds.width - 2.0 * TRACK_INSET).max(0.0) as f64;
    if x <= inner_left {
        return Some(*range.start());
    }
    if x >= inner_left + usable as f32 {
        return Some(*range.end());
    }
    let pct = f64::from(x - inner_left) / usable;
    let steps = (pct * (end - start) / step).round();
    let val = (steps * step + start).min(end);
    T::from_f64(val)
}

fn hit_test<T: Copy + Into<f64>>(
    x: f32,
    _y: f32,
    bounds: &Rectangle,
    range: &RangeInclusive<T>,
    trim_start: T,
    trim_end: T,
) -> DragTarget {
    let ts_x = val_to_x(trim_start, range, bounds);
    let te_x = val_to_x(trim_end, range, bounds);

    let start_left = ts_x - HANDLE_WIDTH - HANDLE_HIT_PAD;
    let start_right = ts_x + HANDLE_HIT_PAD;
    let end_left = te_x - HANDLE_HIT_PAD;
    let end_right = te_x + HANDLE_WIDTH + HANDLE_HIT_PAD;

    if x >= start_left && x <= start_right {
        DragTarget::TrimStart
    } else if x >= end_left && x <= end_right {
        DragTarget::TrimEnd
    } else {
        DragTarget::Playhead
    }
}

fn feq<T: Copy + Into<f64>>(a: T, b: T) -> bool {
    (a.into() - b.into()).abs() < f64::EPSILON
}

impl<T, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Scrubber<'_, T, Message, Theme>
where
    T: Copy + Into<f64> + PartialOrd + num_traits::FromPrimitive,
    Message: Clone,
    Theme: Catalog,
    Renderer: core::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: Length::Shrink,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn core::Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        let active_step = || -> f64 {
            if state.keyboard_modifiers.shift() {
                self.shift_step.unwrap_or(self.step)
            } else {
                self.step
            }
            .into()
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if let Some(pos) = cursor.position_over(bounds) {
                    let target = hit_test(
                        pos.x,
                        pos.y,
                        &bounds,
                        &self.range,
                        self.trim_start,
                        self.trim_end,
                    );
                    state.dragging = Some(target);
                    state.last_active = Some(target);

                    let step = active_step();
                    if let Some(val) = x_to_val(pos.x, &bounds, &self.range, step) {
                        self.apply(target, val, shell);
                    }
                    shell.capture_event();
                }
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                if state.dragging.is_some() {
                    if let Some(msg) = self.on_release.clone() {
                        shell.publish(msg);
                    }
                    state.dragging = None;
                }
            }

            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if let Some(target) = state.dragging
                    && let Some(pos) = cursor.land().position()
                {
                    let step = active_step();
                    if let Some(val) = x_to_val(pos.x, &bounds, &self.range, step) {
                        self.apply(target, val, shell);
                    }
                    shell.capture_event();
                }
            }

            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if cursor.is_over(bounds) {
                    let target = state.last_active.unwrap_or(DragTarget::Playhead);
                    let step = active_step();

                    let nudge = |val: T, positive: bool| -> Option<T> {
                        let v: f64 = val.into();
                        let steps = (v / step).round();
                        let new = step * (steps + if positive { 1.0 } else { -1.0 });
                        let s: f64 = (*self.range.start()).into();
                        let e: f64 = (*self.range.end()).into();
                        T::from_f64(new.clamp(s, e))
                    };

                    match key {
                        Key::Named(key::Named::ArrowRight | key::Named::ArrowUp) => {
                            let cur = self.value_for(target);
                            if let Some(val) = nudge(cur, true) {
                                self.apply(target, val, shell);
                            }
                            shell.capture_event();
                        }
                        Key::Named(key::Named::ArrowLeft | key::Named::ArrowDown) => {
                            let cur = self.value_for(target);
                            if let Some(val) = nudge(cur, false) {
                                self.apply(target, val, shell);
                            }
                            shell.capture_event();
                        }
                        Key::Named(key::Named::Tab) => {
                            state.last_active = Some(match target {
                                DragTarget::Playhead => DragTarget::TrimStart,
                                DragTarget::TrimStart => DragTarget::TrimEnd,
                                DragTarget::TrimEnd => DragTarget::Playhead,
                            });
                            shell.capture_event();
                        }
                        _ => {}
                    }
                }
            }

            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                state.keyboard_modifiers = *m;
            }

            _ => {}
        }

        let current_status = if state.dragging.is_some() {
            Status::Dragged
        } else if cursor.is_over(bounds) {
            Status::Hovered
        } else {
            Status::Active
        };

        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            self.status = Some(current_status);
        } else if self.status.is_some_and(|s| s != current_status) {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let style = theme.style(&self.class, self.status.unwrap_or(Status::Active));
        let rail_y = bounds.y + bounds.height / 2.0;

        let ts_x = val_to_x(self.trim_start, &self.range, &bounds);
        let te_x = val_to_x(self.trim_end, &self.range, &bounds);

        // Track rail: dimmed | active | dimmed
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: bounds.x,
                    y: rail_y - style.rail.width / 2.0,
                    width: (ts_x - bounds.x).max(0.0),
                    height: style.rail.width,
                },
                border: style.rail.border,
                ..Quad::default()
            },
            style.rail.backgrounds.1,
        );

        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: ts_x,
                    y: rail_y - style.rail.width / 2.0,
                    width: (te_x - ts_x).max(0.0),
                    height: style.rail.width,
                },
                border: style.rail.border,
                ..Quad::default()
            },
            style.rail.backgrounds.0,
        );

        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: te_x,
                    y: rail_y - style.rail.width / 2.0,
                    width: (bounds.x + bounds.width - te_x).max(0.0),
                    height: style.rail.width,
                },
                border: style.rail.border,
                ..Quad::default()
            },
            style.rail.backgrounds.1,
        );

        // Dim overlay on inactive regions
        let dim_color =
            Background::Color(cosmic::iced::core::Color::from_rgba(0.0, 0.0, 0.0, 0.15));

        if ts_x > bounds.x {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x,
                        y: bounds.y,
                        width: ts_x - bounds.x,
                        height: bounds.height,
                    },
                    ..Quad::default()
                },
                dim_color,
            );
        }
        if te_x < bounds.x + bounds.width {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: te_x,
                        y: bounds.y,
                        width: bounds.x + bounds.width - te_x,
                        height: bounds.height,
                    },
                    ..Quad::default()
                },
                dim_color,
            );
        }

        // Trim handles + frame
        {
            let cosmic_theme = cosmic::theme::active();
            let ct = cosmic_theme.cosmic();
            let handle_bg: cosmic::iced::core::Color = ct.button.base.into();
            let grip_color: cosmic::iced::core::Color = ct.button.on.into();
            let grip_color = cosmic::iced::core::Color {
                a: 0.4,
                ..grip_color
            };
            let frame_color: cosmic::iced::core::Color = ct.button.divider.into();

            let draw_trim_handle = |renderer: &mut Renderer, x: f32| {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x,
                            y: bounds.y,
                            width: HANDLE_WIDTH,
                            height: bounds.height,
                        },
                        border: Border {
                            radius: HANDLE_RADIUS.into(),
                            ..Border::default()
                        },
                        ..Quad::default()
                    },
                    Background::Color(handle_bg),
                );

                // Grip ridges (3 horizontal lines)
                let cx = x + HANDLE_WIDTH / 2.0;
                let cy = bounds.y + bounds.height / 2.0;
                let spacing = 4.0_f32;
                for i in [-1.0_f32, 0.0, 1.0] {
                    let ly = cy + i * spacing - GRIP_LINE_THICKNESS / 2.0;
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: cx - GRIP_LINE_WIDTH / 2.0,
                                y: ly,
                                width: GRIP_LINE_WIDTH,
                                height: GRIP_LINE_THICKNESS,
                            },
                            border: Border {
                                radius: (GRIP_LINE_THICKNESS / 2.0).into(),
                                ..Border::default()
                            },
                            ..Quad::default()
                        },
                        Background::Color(grip_color),
                    );
                }
            };

            draw_trim_handle(renderer, ts_x - HANDLE_WIDTH);
            draw_trim_handle(renderer, te_x);

            // Frame borders connecting handles along top + bottom
            let frame_w = (te_x - ts_x).max(0.0);
            if frame_w > 0.0 {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: ts_x,
                            y: bounds.y,
                            width: frame_w,
                            height: FRAME_BORDER,
                        },
                        ..Quad::default()
                    },
                    Background::Color(frame_color),
                );

                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: ts_x,
                            y: bounds.y + bounds.height - FRAME_BORDER,
                            width: frame_w,
                            height: FRAME_BORDER,
                        },
                        ..Quad::default()
                    },
                    Background::Color(frame_color),
                );
            }
        }

        // Playhead handle
        let (hw, hh, hr) = handle_dims(&style.handle, &bounds);
        let ph_x = val_to_x(self.position, &self.range, &bounds);

        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: ph_x - hw / 2.0,
                    y: rail_y - hh / 2.0,
                    width: hw,
                    height: hh,
                },
                border: Border {
                    radius: hr,
                    width: style.handle.border_width,
                    color: style.handle.border_color,
                },
                ..Quad::default()
            },
            style.handle.background,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();

        if state.dragging.is_some() {
            if cfg!(target_os = "windows") {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::Grabbing
            }
        } else if let Some(pos) = cursor.position_over(layout.bounds()) {
            match hit_test(
                pos.x,
                pos.y,
                &layout.bounds(),
                &self.range,
                self.trim_start,
                self.trim_end,
            ) {
                DragTarget::TrimStart | DragTarget::TrimEnd => {
                    mouse::Interaction::ResizingHorizontally
                }
                DragTarget::Playhead => {
                    if cfg!(target_os = "windows") {
                        mouse::Interaction::Pointer
                    } else {
                        mouse::Interaction::Grab
                    }
                }
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<T, Message, Theme> Scrubber<'_, T, Message, Theme>
where
    T: Copy + Into<f64> + PartialOrd + num_traits::FromPrimitive,
    Message: Clone,
    Theme: Catalog,
{
    fn value_for(&self, target: DragTarget) -> T {
        match target {
            DragTarget::Playhead => self.position,
            DragTarget::TrimStart => self.trim_start,
            DragTarget::TrimEnd => self.trim_end,
        }
    }

    fn apply(&mut self, target: DragTarget, val: T, shell: &mut Shell<'_, Message>) {
        match target {
            DragTarget::Playhead => {
                let clamped = if val < self.trim_start {
                    self.trim_start
                } else if val > self.trim_end {
                    self.trim_end
                } else {
                    val
                };
                if !feq(clamped, self.position) {
                    self.position = clamped;
                    if let Some(ref cb) = self.on_scrub {
                        shell.publish(cb(clamped));
                    }
                }
            }
            DragTarget::TrimStart => {
                let clamped = if val > self.trim_end {
                    self.trim_end
                } else {
                    val
                };
                if !feq(clamped, self.trim_start) {
                    self.trim_start = clamped;
                    if self.position < self.trim_start {
                        self.position = self.trim_start;
                        if let Some(ref cb) = self.on_scrub {
                            shell.publish(cb(self.position));
                        }
                    }
                    if let Some(ref cb) = self.on_trim {
                        shell.publish(cb((self.trim_start, self.trim_end)));
                    }
                }
            }
            DragTarget::TrimEnd => {
                let clamped = if val < self.trim_start {
                    self.trim_start
                } else {
                    val
                };
                if !feq(clamped, self.trim_end) {
                    self.trim_end = clamped;
                    if self.position > self.trim_end {
                        self.position = self.trim_end;
                        if let Some(ref cb) = self.on_scrub {
                            shell.publish(cb(self.position));
                        }
                    }
                    if let Some(ref cb) = self.on_trim {
                        shell.publish(cb((self.trim_start, self.trim_end)));
                    }
                }
            }
        }
    }
}

fn handle_dims(
    handle: &cosmic::iced::widget::slider::Handle,
    bounds: &Rectangle,
) -> (f32, f32, core::border::Radius) {
    let bw = handle
        .border_width
        .min(bounds.height / 2.0)
        .min(bounds.width / 2.0);
    match handle.shape {
        HandleShape::Circle { radius } => {
            let r = radius
                .max(2.0 * bw)
                .min(bounds.height / 2.0)
                .min(bounds.width / 2.0 + 2.0 * bw);
            (r * 2.0, r * 2.0, core::border::Radius::from(r))
        }
        HandleShape::Rectangle {
            width,
            height,
            border_radius,
        } => {
            let w = f32::from(width).max(2.0 * bw);
            let h = f32::from(height).max(2.0 * bw);
            let mut br: [f32; 4] = border_radius.into();
            for r in &mut br {
                *r = (*r).min(h / 2.0).min(w / 2.0).max(*r * (w + bw * 2.0) / w);
            }
            (
                w,
                h,
                core::border::Radius {
                    top_left: br[0],
                    top_right: br[1],
                    bottom_right: br[2],
                    bottom_left: br[3],
                },
            )
        }
    }
}

impl<'a, T, Message, Theme, Renderer> From<Scrubber<'a, T, Message, Theme>>
    for Element<'a, Message, Theme, Renderer>
where
    T: Copy + Into<f64> + PartialOrd + num_traits::FromPrimitive + 'a,
    Message: Clone + 'a,
    Theme: Catalog + 'a,
    Renderer: core::Renderer + cosmic::iced::advanced::graphics::geometry::Renderer + 'a,
{
    fn from(s: Scrubber<'a, T, Message, Theme>) -> Self {
        Element::new(s)
    }
}

pub fn scrubber<'a, T, Message, Theme>(
    range: RangeInclusive<T>,
    position: T,
    trim: (T, T),
) -> Scrubber<'a, T, Message, Theme>
where
    T: Copy + From<u8> + PartialOrd,
    Message: Clone,
    Theme: Catalog,
{
    Scrubber::new(range, position, trim)
}
