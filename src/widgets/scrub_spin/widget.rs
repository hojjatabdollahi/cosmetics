// SPDX-License-Identifier: MPL-2.0

//! Scrub-spin numeric field. See [`super`] for usage.

use cosmic::iced::core::{
    Background, Border, Clipboard, Color, Event, Layout, Length, Pixels, Rectangle, Shadow, Shell,
    Size, Widget, alignment,
    event::Event as CoreEvent,
    keyboard::{self, key::Named},
    layout,
    mouse::{self, Cursor},
    renderer,
    text::Renderer as _,
    touch,
    widget::tree::{self, Tree},
    window,
};
use cosmic::iced::core::Renderer as _;
use std::time::{Duration, Instant};

const DEFAULT_HEIGHT: f32 = 34.0;
const DEFAULT_WIDTH: f32 = 168.0;
/// Pixels of horizontal drag per one `step` of value change.
const DRAG_PX_PER_STEP: f32 = 8.0;
/// Drag must exceed this many pixels before a press becomes a scrub (vs a click).
const DRAG_THRESHOLD: f32 = 3.0;
/// Delay before a held stepper button starts repeating, then the repeat floor.
const HOLD_DELAY: Duration = Duration::from_millis(400);
const HOLD_INTERVAL_MIN: Duration = Duration::from_millis(40);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Minus,
    Plus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Minus,
    Center,
    Plus,
}

#[derive(Debug, Clone)]
struct Edit {
    chars: Vec<char>,
    caret: usize,
    /// Whole buffer is selected; the next text input replaces it.
    select_all: bool,
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    start_x: f32,
    start_val: f64,
    moved: bool,
}

#[derive(Debug, Clone, Copy)]
struct Hold {
    side: Side,
    since: Instant,
    last: Instant,
}

#[derive(Debug, Default)]
struct State {
    drag: Option<DragState>,
    edit: Option<Edit>,
    hold: Option<Hold>,
    modifiers: keyboard::Modifiers,
}

pub struct ScrubSpin<'a, Message> {
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    shift_step: Option<f64>,
    decimals: usize,
    width: Length,
    height: f32,
    on_change: Option<Box<dyn Fn(f64) -> Message + 'a>>,
    on_release: Option<Box<dyn Fn(f64) -> Message + 'a>>,
}

impl<'a, Message> ScrubSpin<'a, Message> {
    pub fn new(range: std::ops::RangeInclusive<f64>, value: f64) -> Self {
        let (min, max) = (*range.start(), *range.end());
        Self {
            value: value.clamp(min, max),
            min,
            max,
            step: 1.0,
            shift_step: None,
            decimals: 0,
            width: Length::Fixed(DEFAULT_WIDTH),
            height: DEFAULT_HEIGHT,
            on_change: None,
            on_release: None,
        }
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step.max(0.0);
        self
    }

    /// Step used while Shift is held (fine adjustment). Defaults to `step / 10`.
    pub fn shift_step(mut self, step: f64) -> Self {
        self.shift_step = Some(step.max(0.0));
        self
    }

    /// Decimal places shown and used when formatting the value for editing.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(0.0);
        self
    }

    /// Emitted on every value change (drag, step, wheel, key, commit).
    pub fn on_change(mut self, f: impl Fn(f64) -> Message + 'a) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Emitted when an interaction settles (drag release, typed commit).
    pub fn on_release(mut self, f: impl Fn(f64) -> Message + 'a) -> Self {
        self.on_release = Some(Box::new(f));
        self
    }

    fn clamp(&self, v: f64) -> f64 {
        v.clamp(self.min, self.max)
    }

    fn snap(v: f64, step: f64) -> f64 {
        if step > 0.0 {
            (v / step).round() * step
        } else {
            v
        }
    }

    fn eff_step(&self, shift: bool) -> f64 {
        if shift {
            self.shift_step.unwrap_or(self.step / 10.0).max(f64::MIN_POSITIVE)
        } else {
            self.step.max(f64::MIN_POSITIVE)
        }
    }

    /// Apply a new value, publishing `on_change` if it actually moved.
    fn emit(&mut self, v: f64, shell: &mut Shell<'_, Message>) {
        let c = self.clamp(v);
        if (c - self.value).abs() > f64::EPSILON {
            self.value = c;
            if let Some(cb) = &self.on_change {
                shell.publish(cb(c));
            }
        }
    }

    fn release(&self, shell: &mut Shell<'_, Message>) {
        if let Some(cb) = &self.on_release {
            shell.publish(cb(self.value));
        }
    }

    fn formatted(&self) -> String {
        format!("{:.*}", self.decimals, self.value)
    }

    fn regions(bounds: Rectangle) -> (Rectangle, Rectangle, Rectangle) {
        let btn_w = bounds.height.min(bounds.width / 3.0).max(0.0);
        let left = Rectangle {
            width: btn_w,
            ..bounds
        };
        let right = Rectangle {
            x: bounds.x + bounds.width - btn_w,
            width: btn_w,
            ..bounds
        };
        let center = Rectangle {
            x: bounds.x + btn_w,
            width: (bounds.width - 2.0 * btn_w).max(0.0),
            ..bounds
        };
        (left, center, right)
    }

    fn region_at(bounds: Rectangle, x: f32) -> Region {
        let (left, _, right) = Self::regions(bounds);
        if x < left.x + left.width {
            Region::Minus
        } else if x >= right.x {
            Region::Plus
        } else {
            Region::Center
        }
    }

    fn enter_edit(&self, fresh: bool) -> Edit {
        if fresh {
            Edit {
                chars: Vec::new(),
                caret: 0,
                select_all: false,
            }
        } else {
            let chars: Vec<char> = self.formatted().chars().collect();
            let caret = chars.len();
            Edit {
                chars,
                caret,
                select_all: true,
            }
        }
    }

    fn step_by(&mut self, side: Side, mult: f64, shell: &mut Shell<'_, Message>) {
        let delta = self.step.max(f64::MIN_POSITIVE) * mult;
        let signed = match side {
            Side::Minus => -delta,
            Side::Plus => delta,
        };
        self.emit(Self::snap(self.value + signed, self.step), shell);
    }
}

impl<'a, Message: 'a> Widget<Message, cosmic::Theme, cosmic::Renderer> for ScrubSpin<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, Length::Fixed(self.height))
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
        let state = tree.state.downcast_mut::<State>();

        match event {
            CoreEvent::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | CoreEvent::Touch(touch::Event::FingerPressed { .. }) => {
                let Some(pos) = cursor.position() else { return };
                let inside = bounds.contains(pos);

                // A press anywhere other than inside an active editor commits it.
                if state.edit.is_some() && !inside {
                    self.commit_edit(state, shell);
                    return;
                }
                if !inside {
                    return;
                }

                match Self::region_at(bounds, pos.x) {
                    Region::Minus | Region::Plus => {
                        if state.edit.is_some() {
                            self.commit_edit(state, shell);
                        }
                        let side = if Self::region_at(bounds, pos.x) == Region::Minus {
                            Side::Minus
                        } else {
                            Side::Plus
                        };
                        let mult = if state.modifiers.control() { 10.0 } else { 1.0 };
                        self.step_by(side, mult, shell);
                        let now = Instant::now();
                        state.hold = Some(Hold {
                            side,
                            since: now,
                            last: now,
                        });
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    Region::Center => {
                        if state.edit.is_none() {
                            // Tentative drag; becomes a scrub past the threshold,
                            // or a click-to-edit if released in place.
                            state.drag = Some(DragState {
                                start_x: pos.x,
                                start_val: self.value,
                                moved: false,
                            });
                        }
                        shell.capture_event();
                    }
                }
            }

            CoreEvent::Mouse(mouse::Event::CursorMoved { .. })
            | CoreEvent::Touch(touch::Event::FingerMoved { .. }) => {
                if let Some(drag) = state.drag.as_mut()
                    && let Some(pos) = cursor.position()
                {
                    let dx = pos.x - drag.start_x;
                    if !drag.moved && dx.abs() >= DRAG_THRESHOLD {
                        drag.moved = true;
                    }
                    if drag.moved {
                        let shift = state.modifiers.shift();
                        let eff = self.eff_step(shift);
                        let units = f64::from(dx / DRAG_PX_PER_STEP) * eff;
                        let target = drag.start_val + units;
                        self.emit(Self::snap(target, eff), shell);
                        shell.capture_event();
                    }
                }
            }

            CoreEvent::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | CoreEvent::Touch(touch::Event::FingerLifted { .. })
            | CoreEvent::Touch(touch::Event::FingerLost { .. }) => {
                if let Some(drag) = state.drag.take() {
                    if drag.moved {
                        self.release(shell);
                    } else {
                        // Click in place → enter type-in mode (select all).
                        state.edit = Some(self.enter_edit(false));
                        shell.request_redraw();
                    }
                }
                if state.hold.take().is_some() {
                    shell.request_redraw();
                }
            }

            CoreEvent::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.is_over(bounds) && state.edit.is_none() {
                    let d = match delta {
                        mouse::ScrollDelta::Lines { y, x } | mouse::ScrollDelta::Pixels { y, x } => {
                            if y.abs() >= x.abs() { *y } else { *x }
                        }
                    };
                    if d != 0.0 {
                        let eff = self.eff_step(state.modifiers.shift());
                        let signed = if d > 0.0 { eff } else { -eff };
                        self.emit(Self::snap(self.value + signed, eff), shell);
                        shell.capture_event();
                    }
                }
            }

            CoreEvent::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                state.modifiers = *m;
            }

            CoreEvent::Keyboard(keyboard::Event::KeyPressed { key, text, .. }) => {
                // Keyboard input goes only to the field currently being edited.
                // Routing keys to whatever field is *hovered* lets a hovered field
                // steal keystrokes from the one you're editing (events are
                // delivered to siblings in layout order, and capture only blocks
                // later siblings) — so two fields would change at once. Click a
                // field to focus it before typing / using arrow keys.
                if state.edit.is_some() {
                    self.handle_edit_key(state, key, text.as_deref(), shell);
                }
            }

            CoreEvent::Window(window::Event::RedrawRequested(_)) => {
                if let Some(hold) = state.hold {
                    let now = Instant::now();
                    if now - hold.since >= HOLD_DELAY {
                        // Accelerate: repeat interval shrinks the longer it's held.
                        let held = (now - hold.since).as_secs_f32();
                        let interval = Duration::from_secs_f32(
                            (0.18 / (1.0 + held)).max(HOLD_INTERVAL_MIN.as_secs_f32()),
                        );
                        if now - hold.last >= interval {
                            let mult = if state.modifiers.control() { 10.0 } else { 1.0 };
                            self.step_by(hold.side, mult, shell);
                            if let Some(h) = state.hold.as_mut() {
                                h.last = now;
                            }
                        }
                    }
                    shell.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        if bounds.width <= 0.0 {
            return;
        }
        let (left, center, right) = Self::regions(bounds);

        let ct = theme.cosmic();
        let accent: Color = ct.accent_color().into();
        let text_color: Color = ct.on_bg_color().into();
        let field_bg: Color = ct.bg_component_color().into();
        let r_s = ct.corner_radii.radius_s[0];
        let editing = state.edit.is_some();

        let hover_region = cursor
            .position_over(bounds)
            .map(|p| Self::region_at(bounds, p.x));

        // Field background + border (accent border while editing).
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: r_s.into(),
                    width: if editing { 1.5 } else { 1.0 },
                    color: if editing {
                        accent
                    } else {
                        Color { a: 0.5, ..text_color }
                    },
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(field_bg),
        );

        // Stepper buttons.
        let glyph_fs = (bounds.height * 0.5).clamp(12.0, 24.0);
        let draw_btn = |renderer: &mut cosmic::Renderer, rect: Rectangle, side: Side| {
            let hovered = matches!(
                (hover_region, side),
                (Some(Region::Minus), Side::Minus) | (Some(Region::Plus), Side::Plus)
            );
            if hovered {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: rect,
                        border: Border {
                            radius: r_s.into(),
                            ..Border::default()
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    },
                    Background::Color(Color { a: 0.12, ..accent }),
                );
            }
            let glyph = match side {
                Side::Minus => "−",
                Side::Plus => "+",
            };
            renderer.fill_text(
                cosmic::iced::core::Text {
                    content: glyph.to_string(),
                    bounds: rect.size(),
                    size: Pixels(glyph_fs),
                    line_height: cosmic::iced::core::text::LineHeight::default(),
                    font: renderer.default_font(),
                    align_x: alignment::Horizontal::Center.into(),
                    align_y: alignment::Vertical::Center,
                    shaping: cosmic::iced::core::text::Shaping::Basic,
                    wrapping: cosmic::iced::core::text::Wrapping::None,
                    ellipsize: cosmic::iced::core::text::Ellipsize::None,
                },
                cosmic::iced::Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0),
                Color { a: 0.85, ..text_color },
                rect,
            );
        };
        draw_btn(renderer, left, Side::Minus);
        draw_btn(renderer, right, Side::Plus);

        // Value / editor text.
        let value_fs = (bounds.height * 0.42).clamp(11.0, 22.0);
        let cx = center.x + center.width / 2.0;
        let cy = center.y + center.height / 2.0;

        let (display, edit) = match &state.edit {
            Some(e) => (e.chars.iter().collect::<String>(), Some(e)),
            None => (self.formatted(), None),
        };

        // Selection highlight behind text when the whole buffer is selected.
        let char_w = value_fs * 0.6;
        let total_w = display.chars().count() as f32 * char_w;
        if let Some(e) = edit
            && e.select_all
            && total_w > 0.0
        {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: cx - total_w / 2.0 - 2.0,
                        y: cy - value_fs * 0.65,
                        width: total_w + 4.0,
                        height: value_fs * 1.3,
                    },
                    border: Border {
                        radius: 2.0.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(Color { a: 0.25, ..accent }),
            );
        }

        renderer.fill_text(
            cosmic::iced::core::Text {
                content: display,
                bounds: center.size(),
                size: Pixels(value_fs),
                line_height: cosmic::iced::core::text::LineHeight::default(),
                font: renderer.default_font(),
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Center,
                shaping: cosmic::iced::core::text::Shaping::Basic,
                wrapping: cosmic::iced::core::text::Wrapping::None,
                ellipsize: cosmic::iced::core::text::Ellipsize::None,
            },
            cosmic::iced::Point::new(cx, cy),
            text_color,
            center,
        );

        // Caret (approximate placement: number glyphs are near-monospace).
        if let Some(e) = edit
            && !e.select_all
        {
            let caret_x = cx - total_w / 2.0 + e.caret as f32 * char_w;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: caret_x,
                        y: cy - value_fs * 0.6,
                        width: 1.5,
                        height: value_fs * 1.2,
                    },
                    ..renderer::Quad::default()
                },
                Background::Color(accent),
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        if state.drag.is_some_and(|d| d.moved) {
            return mouse::Interaction::ResizingHorizontally;
        }
        match cursor.position_over(bounds).map(|p| Self::region_at(bounds, p.x)) {
            Some(Region::Minus | Region::Plus) => mouse::Interaction::Pointer,
            Some(Region::Center) => {
                if state.edit.is_some() {
                    mouse::Interaction::Text
                } else {
                    mouse::Interaction::ResizingHorizontally
                }
            }
            None => mouse::Interaction::default(),
        }
    }
}

impl<'a, Message: 'a> ScrubSpin<'a, Message> {
    fn commit_edit(&mut self, state: &mut State, shell: &mut Shell<'_, Message>) {
        if let Some(edit) = state.edit.take() {
            let s: String = edit.chars.iter().collect();
            if let Some(v) = parse_expr(&s) {
                self.emit(v, shell);
            }
            self.release(shell);
            shell.request_redraw();
        }
    }

    fn handle_edit_key(
        &mut self,
        state: &mut State,
        key: &keyboard::Key,
        text: Option<&str>,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(edit) = state.edit.as_mut() else {
            return;
        };
        match key {
            keyboard::Key::Named(Named::Enter) => {
                self.commit_edit(state, shell);
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::Escape) => {
                state.edit = None;
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::Backspace) => {
                if edit.select_all {
                    edit.chars.clear();
                    edit.caret = 0;
                    edit.select_all = false;
                } else if edit.caret > 0 {
                    edit.caret -= 1;
                    edit.chars.remove(edit.caret);
                }
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::Delete) => {
                edit.select_all = false;
                if edit.caret < edit.chars.len() {
                    edit.chars.remove(edit.caret);
                }
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::ArrowLeft) => {
                edit.select_all = false;
                edit.caret = edit.caret.saturating_sub(1);
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::ArrowRight) => {
                edit.select_all = false;
                edit.caret = (edit.caret + 1).min(edit.chars.len());
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::Home) => {
                edit.select_all = false;
                edit.caret = 0;
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            keyboard::Key::Named(Named::End) => {
                edit.select_all = false;
                edit.caret = edit.chars.len();
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            _ => {}
        }

        if let Some(t) = text {
            let printable: Vec<char> = t.chars().filter(|c| !c.is_control()).collect();
            if !printable.is_empty() {
                if edit.select_all {
                    edit.chars.clear();
                    edit.caret = 0;
                    edit.select_all = false;
                }
                for c in printable {
                    edit.chars.insert(edit.caret, c);
                    edit.caret += 1;
                }
                shell.request_redraw();
                shell.capture_event();
            }
        }
    }

    // Hover-based keyboard nudging is currently disabled (see KeyPressed) to
    // avoid a hovered field stealing keystrokes from the edited one. Kept for a
    // future focus-aware reintroduction.
    #[allow(dead_code)]
    fn handle_nudge_key(
        &mut self,
        state: &mut State,
        key: &keyboard::Key,
        text: Option<&str>,
        shell: &mut Shell<'_, Message>,
    ) {
        let shift = state.modifiers.shift();
        match key {
            keyboard::Key::Named(Named::ArrowUp | Named::ArrowRight) => {
                let eff = self.eff_step(shift);
                self.emit(Self::snap(self.value + eff, eff), shell);
                shell.capture_event();
            }
            keyboard::Key::Named(Named::ArrowDown | Named::ArrowLeft) => {
                let eff = self.eff_step(shift);
                self.emit(Self::snap(self.value - eff, eff), shell);
                shell.capture_event();
            }
            keyboard::Key::Named(Named::PageUp) => {
                self.emit(Self::snap(self.value + self.step * 10.0, self.step), shell);
                shell.capture_event();
            }
            keyboard::Key::Named(Named::PageDown) => {
                self.emit(Self::snap(self.value - self.step * 10.0, self.step), shell);
                shell.capture_event();
            }
            keyboard::Key::Named(Named::Home) => {
                self.emit(self.min, shell);
                shell.capture_event();
            }
            keyboard::Key::Named(Named::End) => {
                self.emit(self.max, shell);
                shell.capture_event();
            }
            _ => {
                // Typing a digit (or -, ., () begins type-in mode fresh.
                if let Some(t) = text
                    && t.chars().next().is_some_and(starts_number)
                {
                    let mut edit = self.enter_edit(true);
                    for c in t.chars().filter(|c| !c.is_control()) {
                        edit.chars.insert(edit.caret, c);
                        edit.caret += 1;
                    }
                    state.edit = Some(edit);
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
        }
    }
}

fn starts_number(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | '(')
}

impl<'a, Message: 'a> From<ScrubSpin<'a, Message>> for cosmic::Element<'a, Message> {
    fn from(w: ScrubSpin<'a, Message>) -> Self {
        cosmic::Element::new(w)
    }
}

pub fn scrub_spin<'a, Message>(
    range: std::ops::RangeInclusive<f64>,
    value: f64,
) -> ScrubSpin<'a, Message> {
    ScrubSpin::new(range, value)
}

// --- Tiny recursive-descent evaluator: numbers, + - * /, parens, unary +/-. ---

fn parse_expr(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let mut p = Parser {
        chars: t.chars().collect(),
        pos: 0,
    };
    let v = p.expr()?;
    p.skip_ws();
    (p.pos == p.chars.len() && v.is_finite()).then_some(v)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    v += self.term()?;
                }
                Some('-') => {
                    self.pos += 1;
                    v -= self.term()?;
                }
                _ => break,
            }
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    v *= self.factor()?;
                }
                Some('/') => {
                    self.pos += 1;
                    v /= self.factor()?;
                }
                _ => break,
            }
        }
        Some(v)
    }

    fn factor(&mut self) -> Option<f64> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let v = self.expr()?;
                self.skip_ws();
                if self.peek() == Some(')') {
                    self.pos += 1;
                    Some(v)
                } else {
                    None
                }
            }
            Some('-') => {
                self.pos += 1;
                Some(-self.factor()?)
            }
            Some('+') => {
                self.pos += 1;
                self.factor()
            }
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_ws();
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        s.parse::<f64>().ok()
    }
}
