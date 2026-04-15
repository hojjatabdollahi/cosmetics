// SPDX-License-Identifier: MPL-2.0

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
use cosmic::iced::widget::slider::{Catalog, Handle, HandleShape, Status, Style, StyleFn};
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveHandle {
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct State {
    dragging: Option<ActiveHandle>,
    last_active: Option<ActiveHandle>,
    keyboard_modifiers: keyboard::Modifiers,
}

/// Dual-handle slider for selecting a `(low, high)` range.
///
/// Uses cosmic/iced `Slider` styling types for visual consistency.
pub struct RangeSlider<'a, T, Message, Theme = cosmic::Theme>
where
    Theme: Catalog,
{
    range: RangeInclusive<T>,
    low: T,
    high: T,
    step: T,
    shift_step: Option<T>,
    breakpoints: &'a [T],
    on_change: Box<dyn Fn((T, T)) -> Message + 'a>,
    on_release: Option<Message>,
    width: Length,
    height: f32,
    class: Theme::Class<'a>,
    status: Option<Status>,
}

impl<'a, T, Message, Theme> RangeSlider<'a, T, Message, Theme>
where
    T: Copy + From<u8> + PartialOrd,
    Message: Clone,
    Theme: Catalog,
{
    pub const DEFAULT_HEIGHT: f32 = 16.0;

    pub fn new<F>(range: RangeInclusive<T>, values: (T, T), on_change: F) -> Self
    where
        F: 'a + Fn((T, T)) -> Message,
    {
        let clamp = |v: T| -> T {
            if v < *range.start() {
                *range.start()
            } else if v > *range.end() {
                *range.end()
            } else {
                v
            }
        };
        let (mut lo, mut hi) = (clamp(values.0), clamp(values.1));
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }

        Self {
            range,
            low: lo,
            high: hi,
            step: T::from(1),
            shift_step: None,
            breakpoints: &[],
            on_change: Box::new(on_change),
            on_release: None,
            width: Length::Fill,
            height: Self::DEFAULT_HEIGHT,
            class: Theme::default(),
            status: None,
        }
    }

    pub fn step(mut self, step: impl Into<T>) -> Self {
        self.step = step.into();
        self
    }

    pub fn shift_step(mut self, step: impl Into<T>) -> Self {
        self.shift_step = Some(step.into());
        self
    }

    pub fn breakpoints(mut self, breakpoints: &'a [T]) -> Self {
        self.breakpoints = breakpoints;
        self
    }

    pub fn on_release(mut self, msg: Message) -> Self {
        self.on_release = Some(msg);
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

fn handle_dims(handle: &Handle, bounds: &Rectangle) -> (f32, f32, core::border::Radius) {
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

fn offset_for<T: Copy + Into<f64>>(
    value: T,
    range: &RangeInclusive<T>,
    track_width: f32,
    handle_width: f32,
) -> f32 {
    let start = (*range.start()).into() as f32;
    let end = (*range.end()).into() as f32;
    if start >= end {
        0.0
    } else {
        (track_width - handle_width) * (value.into() as f32 - start) / (end - start)
    }
}

fn locate<T: Copy + Into<f64> + num_traits::FromPrimitive>(
    x: f32,
    bounds: &Rectangle,
    range: &RangeInclusive<T>,
    step: f64,
) -> Option<T> {
    let start = (*range.start()).into();
    let end = (*range.end()).into();

    if x <= bounds.x {
        Some(*range.start())
    } else if x >= bounds.x + bounds.width {
        Some(*range.end())
    } else {
        let percent = f64::from(x - bounds.x) / f64::from(bounds.width);
        let steps = (percent * (end - start) / step).round();
        let value = steps * step + start;
        T::from_f64(value.min(end))
    }
}

fn nearest_handle<T: Copy + Into<f64>>(
    x: f32,
    bounds: &Rectangle,
    range: &RangeInclusive<T>,
    low: T,
    high: T,
) -> ActiveHandle {
    let start = (*range.start()).into() as f32;
    let end = (*range.end()).into() as f32;
    let span = end - start;
    if span <= 0.0 {
        return ActiveHandle::Low;
    }
    let percent = (x - bounds.x) / bounds.width;
    let value = start + percent * span;
    let low_f = low.into() as f32;
    let high_f = high.into() as f32;
    if (value - low_f).abs() <= (value - high_f).abs() {
        ActiveHandle::Low
    } else {
        ActiveHandle::High
    }
}

impl<T, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for RangeSlider<'_, T, Message, Theme>
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

        let publish = |shell: &mut Shell<'_, Message>, lo: T, hi: T| {
            shell.publish((self.on_change)((lo, hi)));
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if let Some(pos) = cursor.position_over(bounds) {
                    let handle = nearest_handle(pos.x, &bounds, &self.range, self.low, self.high);
                    state.dragging = Some(handle);
                    state.last_active = Some(handle);

                    let step = active_step();
                    if let Some(val) = locate(pos.x, &bounds, &self.range, step) {
                        match handle {
                            ActiveHandle::Low => {
                                let clamped = clamp_low(val, self.high);
                                if !feq(clamped, self.low) {
                                    self.low = clamped;
                                    publish(shell, self.low, self.high);
                                }
                            }
                            ActiveHandle::High => {
                                let clamped = clamp_high(val, self.low);
                                if !feq(clamped, self.high) {
                                    self.high = clamped;
                                    publish(shell, self.low, self.high);
                                }
                            }
                        }
                    }
                    shell.capture_event();
                }
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                if state.dragging.is_some() {
                    if let Some(on_release) = self.on_release.clone() {
                        shell.publish(on_release);
                    }
                    state.dragging = None;
                }
            }

            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if let Some(handle) = state.dragging
                    && let Some(pos) = cursor.land().position()
                {
                    let step = active_step();
                    if let Some(val) = locate(pos.x, &bounds, &self.range, step) {
                        match handle {
                            ActiveHandle::Low => {
                                let clamped = clamp_low(val, self.high);
                                if !feq(clamped, self.low) {
                                    self.low = clamped;
                                    publish(shell, self.low, self.high);
                                }
                            }
                            ActiveHandle::High => {
                                let clamped = clamp_high(val, self.low);
                                if !feq(clamped, self.high) {
                                    self.high = clamped;
                                    publish(shell, self.low, self.high);
                                }
                            }
                        }
                    }
                    shell.capture_event();
                }
            }

            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if state.keyboard_modifiers.control() =>
            {
                if cursor.is_over(bounds) {
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y,
                        mouse::ScrollDelta::Pixels { y, .. } => y,
                    };

                    let handle = cursor
                        .position()
                        .map(|p| nearest_handle(p.x, &bounds, &self.range, self.low, self.high))
                        .unwrap_or(state.last_active.unwrap_or(ActiveHandle::Low));

                    let step = active_step();
                    let nudge = |val: T, positive: bool| -> Option<T> {
                        let v: f64 = val.into();
                        let steps = (v / step).round();
                        let new = step * (steps + if positive { 1.0 } else { -1.0 });
                        let start: f64 = (*self.range.start()).into();
                        let end: f64 = (*self.range.end()).into();
                        T::from_f64(new.clamp(start, end))
                    };

                    let positive = *dy > 0.0;
                    match handle {
                        ActiveHandle::Low => {
                            if let Some(val) = nudge(self.low, positive) {
                                let clamped = clamp_low(val, self.high);
                                if !feq(clamped, self.low) {
                                    self.low = clamped;
                                    publish(shell, self.low, self.high);
                                }
                            }
                        }
                        ActiveHandle::High => {
                            if let Some(val) = nudge(self.high, positive) {
                                let clamped = clamp_high(val, self.low);
                                if !feq(clamped, self.high) {
                                    self.high = clamped;
                                    publish(shell, self.low, self.high);
                                }
                            }
                        }
                    }
                    shell.capture_event();
                }
            }

            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if cursor.is_over(bounds) {
                    let handle = state.last_active.unwrap_or(ActiveHandle::Low);
                    let step = active_step();
                    let nudge = |val: T, positive: bool| -> Option<T> {
                        let v: f64 = val.into();
                        let steps = (v / step).round();
                        let new = step * (steps + if positive { 1.0 } else { -1.0 });
                        let start: f64 = (*self.range.start()).into();
                        let end: f64 = (*self.range.end()).into();
                        T::from_f64(new.clamp(start, end))
                    };

                    match key {
                        Key::Named(key::Named::ArrowUp | key::Named::ArrowRight) => {
                            match handle {
                                ActiveHandle::Low => {
                                    if let Some(val) = nudge(self.low, true) {
                                        let clamped = clamp_low(val, self.high);
                                        if !feq(clamped, self.low) {
                                            self.low = clamped;
                                            publish(shell, self.low, self.high);
                                        }
                                    }
                                }
                                ActiveHandle::High => {
                                    if let Some(val) = nudge(self.high, true) {
                                        let clamped = clamp_high(val, self.low);
                                        if !feq(clamped, self.high) {
                                            self.high = clamped;
                                            publish(shell, self.low, self.high);
                                        }
                                    }
                                }
                            }
                            shell.capture_event();
                        }
                        Key::Named(key::Named::ArrowDown | key::Named::ArrowLeft) => {
                            match handle {
                                ActiveHandle::Low => {
                                    if let Some(val) = nudge(self.low, false) {
                                        let clamped = clamp_low(val, self.high);
                                        if !feq(clamped, self.low) {
                                            self.low = clamped;
                                            publish(shell, self.low, self.high);
                                        }
                                    }
                                }
                                ActiveHandle::High => {
                                    if let Some(val) = nudge(self.high, false) {
                                        let clamped = clamp_high(val, self.low);
                                        if !feq(clamped, self.high) {
                                            self.high = clamped;
                                            publish(shell, self.low, self.high);
                                        }
                                    }
                                }
                            }
                            shell.capture_event();
                        }
                        Key::Named(key::Named::Tab) => {
                            state.last_active = Some(match handle {
                                ActiveHandle::Low => ActiveHandle::High,
                                ActiveHandle::High => ActiveHandle::Low,
                            });
                            shell.capture_event();
                        }
                        _ => {}
                    }
                }
            }

            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
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
        let (handle_w, handle_h, handle_br) = handle_dims(&style.handle, &bounds);

        let low_off = offset_for(self.low, &self.range, bounds.width, handle_w);
        let high_off = offset_for(self.high, &self.range, bounds.width, handle_w);
        let rail_y = bounds.y + bounds.height / 2.0;

        // Breakpoint markers
        const BP_WIDTH: f32 = 2.0;
        let range_start = (*self.range.start()).into() as f32;
        let range_end = (*self.range.end()).into() as f32;
        for &v in self.breakpoints {
            let v_f: f64 = v.into();
            let off = if range_start >= range_end {
                0.0
            } else {
                (bounds.width - BP_WIDTH) * (v_f as f32 - range_start) / (range_end - range_start)
            };
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x + off,
                        y: rail_y + 6.0,
                        width: BP_WIDTH,
                        height: 8.0,
                    },
                    border: Border::default(),
                    ..Quad::default()
                },
                Background::Color(style.breakpoint.color),
            );
        }

        let left_end = bounds.x + low_off + handle_w / 2.0;
        let right_start = bounds.x + high_off + handle_w / 2.0;

        // Rail: unfilled | filled range | unfilled
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: bounds.x,
                    y: rail_y - style.rail.width / 2.0,
                    width: left_end - bounds.x,
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
                    x: left_end,
                    y: rail_y - style.rail.width / 2.0,
                    width: (right_start - left_end).max(0.0),
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
                    x: right_start,
                    y: rail_y - style.rail.width / 2.0,
                    width: (bounds.x + bounds.width - right_start).max(0.0),
                    height: style.rail.width,
                },
                border: style.rail.border,
                ..Quad::default()
            },
            style.rail.backgrounds.1,
        );

        let draw_handle = |renderer: &mut Renderer, offset: f32| {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x + offset,
                        y: rail_y - handle_h / 2.0,
                        width: handle_w,
                        height: handle_h,
                    },
                    border: Border {
                        radius: handle_br,
                        width: style.handle.border_width,
                        color: style.handle.border_color,
                    },
                    ..Quad::default()
                },
                style.handle.background,
            );
        };

        draw_handle(renderer, low_off);
        draw_handle(renderer, high_off);
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
        } else if cursor.is_over(layout.bounds()) {
            if cfg!(target_os = "windows") {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::Grab
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, T, Message, Theme, Renderer> From<RangeSlider<'a, T, Message, Theme>>
    for Element<'a, Message, Theme, Renderer>
where
    T: Copy + Into<f64> + PartialOrd + num_traits::FromPrimitive + 'a,
    Message: Clone + 'a,
    Theme: Catalog + 'a,
    Renderer: core::Renderer + 'a,
{
    fn from(slider: RangeSlider<'a, T, Message, Theme>) -> Self {
        Element::new(slider)
    }
}

fn clamp_low<T: Copy + PartialOrd>(val: T, high: T) -> T {
    if val > high { high } else { val }
}

fn clamp_high<T: Copy + PartialOrd>(val: T, low: T) -> T {
    if val < low { low } else { val }
}

fn feq<T: Copy + Into<f64>>(a: T, b: T) -> bool {
    (a.into() - b.into()).abs() < f64::EPSILON
}

pub fn range_slider<'a, T, Message, Theme>(
    range: RangeInclusive<T>,
    values: (T, T),
    on_change: impl Fn((T, T)) -> Message + 'a,
) -> RangeSlider<'a, T, Message, Theme>
where
    T: Copy + From<u8> + PartialOrd,
    Message: Clone,
    Theme: Catalog,
{
    RangeSlider::new(range, values, on_change)
}
