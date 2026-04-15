// SPDX-License-Identifier: MPL-2.0

use cosmic::iced::core::layout::{self, Layout};
use cosmic::iced::core::mouse::{self, Cursor};
use cosmic::iced::core::renderer;
use cosmic::iced::core::widget::Tree;
use cosmic::iced::core::{Color, Element, Length, Rectangle, Size, Widget};
use std::f32::consts::PI;
use std::time::Instant;

const TWO_PI: f32 = 2.0 * PI;

#[derive(Debug, Clone)]
pub struct SpinnerState {
    rotation: f32,
    sweep: f32,
    last_tick: Option<Instant>,
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

impl SpinnerState {
    pub fn new() -> Self {
        Self {
            rotation: 0.0,
            sweep: PI * 0.75,
            last_tick: None,
        }
    }

    pub fn tick(&mut self, now: Instant) {
        let dt = match self.last_tick {
            Some(prev) => (now - prev).as_secs_f32(),
            None => 0.0,
        };
        self.last_tick = Some(now);

        self.rotation = (self.rotation + dt * 3.0 * PI) % TWO_PI;

        // Sweep pulsation: oscillates between ~0.4pi and ~1.2pi over ~1.2s
        let elapsed = self.total_elapsed();
        self.sweep = PI * (0.8 + 0.4 * (elapsed * 5.2).sin());
    }

    pub fn reset(&mut self) {
        self.rotation = 0.0;
        self.sweep = PI * 0.75;
        self.last_tick = None;
    }

    fn total_elapsed(&self) -> f32 {
        self.last_tick
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }
}

/// Rotating arc spinner. Uses cosmic accent color by default.
pub struct Spinner<'a> {
    state: &'a SpinnerState,
    diameter: f32,
    stroke_width: f32,
    color: Option<Color>,
}

impl<'a> Spinner<'a> {
    pub fn new(state: &'a SpinnerState) -> Self {
        Self {
            state,
            diameter: 32.0,
            stroke_width: 3.0,
            color: None,
        }
    }

    pub fn size(mut self, diameter: f32) -> Self {
        self.diameter = diameter;
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Spinner<'a>
where
    Renderer: renderer::Renderer + cosmic::iced::advanced::graphics::geometry::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fixed(self.diameter),
            height: Length::Fixed(self.diameter),
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let side = Length::Fixed(self.diameter);
        let limits = limits.width(side).height(side);
        let size = limits.resolve(side, side, Size::ZERO);
        layout::Node::new(size)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        use cosmic::iced::advanced::graphics::geometry::{
            self, Style,
            path::Path,
            stroke::{LineCap, Stroke},
        };

        let bounds = layout.bounds();
        let mut frame = geometry::Frame::with_bounds(renderer, bounds);

        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;
        let radius = (bounds.width.min(bounds.height) / 2.0) - self.stroke_width;

        let color = self
            .color
            .unwrap_or_else(|| cosmic::theme::active().cosmic().accent_color().into());

        let start_angle = self.state.rotation;
        let sweep = self.state.sweep;

        let segments = 48;
        let path = Path::new(|b| {
            for i in 0..=segments {
                let t = i as f32 / segments as f32;
                let angle = start_angle + t * sweep;
                let x = cx + radius * angle.cos();
                let y = cy + radius * angle.sin();
                if i == 0 {
                    b.move_to(cosmic::iced::core::Point::new(x, y));
                } else {
                    b.line_to(cosmic::iced::core::Point::new(x, y));
                }
            }
        });

        frame.stroke(
            &path,
            Stroke {
                style: Style::Solid(color),
                width: self.stroke_width,
                line_cap: LineCap::Round,
                ..Default::default()
            },
        );

        renderer.draw_geometry(frame.into_geometry());
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        _layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

impl<'a, Message> From<Spinner<'a>> for Element<'a, Message, cosmic::Theme, cosmic::Renderer>
where
    Message: 'a,
{
    fn from(spinner: Spinner<'a>) -> Self {
        Element::new(spinner)
    }
}
