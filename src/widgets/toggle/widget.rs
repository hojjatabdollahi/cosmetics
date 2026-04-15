// SPDX-License-Identifier: MPL-2.0

//! Toggle widget implementation with icon support, N-item segments, and built-in animation.

use cosmic::Element;
use cosmic::anim;
use cosmic::iced::Size;
use cosmic::iced::core::{
    Background, Border, Color, Layout, Length, Rectangle, Shadow, layout,
    mouse::{self, Cursor},
    renderer::Quad,
    widget::tree::{self, Tree},
    window,
};
use cosmic::widget::icon;
use std::rc::Rc;
use std::time::{Duration, Instant};

// Default layout constants
const DEFAULT_PILL_THICKNESS: f32 = 38.0;
const DEFAULT_CIRCLE_SIZE: f32 = 32.0;
const DEFAULT_ICON_SIZE: f32 = 24.0;
const DEFAULT_SEGMENT_LENGTH: f32 = 40.0;
const DEFAULT_DURATION: Duration = Duration::from_millis(200);

#[derive(Debug)]
struct State {
    anim_from: f32,
    anim_start: Option<Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            anim_from: 0.0,
            anim_start: None,
        }
    }
}

impl State {
    fn position(&self, target: f32, duration: Duration) -> f32 {
        let Some(start) = self.anim_start else {
            return target;
        };
        let elapsed = start.elapsed().as_millis() as f32 / duration.as_millis() as f32;
        if elapsed >= 1.0 {
            return target;
        }
        anim::slerp(self.anim_from, target, elapsed)
    }

    fn start(&mut self, current_visual: f32) {
        self.anim_from = current_visual;
        self.anim_start = Some(Instant::now());
    }

    fn is_animating(&self, duration: Duration) -> bool {
        self.anim_start.is_some_and(|t| t.elapsed() < duration)
    }

    fn finish_if_done(&mut self, duration: Duration) {
        if self.anim_start.is_some_and(|t| t.elapsed() >= duration) {
            self.anim_start = None;
        }
    }
}

/// Segmented toggle with pill background, sliding indicator, and optional icons.
pub struct Toggle<'a, Msg> {
    icons: Vec<Option<&'a str>>,
    labels: Vec<Option<String>>,
    selected: usize,
    is_vertical: bool,
    on_select: Option<Box<dyn Fn(usize) -> Msg + 'a>>,
    content_opacity: f32,
    pill_thickness: f32,
    pill_length: Option<f32>,
    circle_size: f32,
    icon_size: f32,
    duration: Duration,
}

impl<'a, Msg> Toggle<'a, Msg> {
    fn new(icons: Vec<Option<&'a str>>, labels: Vec<Option<String>>, selected: usize) -> Self {
        let n = icons.len();
        assert!(n >= 2, "Toggle requires at least 2 items");
        Self {
            icons,
            labels,
            selected: selected.min(n - 1),
            is_vertical: false,
            on_select: None,
            content_opacity: 1.0,
            pill_thickness: DEFAULT_PILL_THICKNESS,
            pill_length: None,
            circle_size: DEFAULT_CIRCLE_SIZE,
            icon_size: DEFAULT_ICON_SIZE,
            duration: DEFAULT_DURATION,
        }
    }

    pub fn with_icons(icons: &[&'a str], selected: usize) -> Self {
        Self::new(
            icons.iter().map(|&s| Some(s)).collect(),
            vec![None; icons.len()],
            selected,
        )
    }

    pub fn with_optional_icons(icons: &[Option<&'a str>], selected: usize) -> Self {
        Self::new(icons.to_vec(), vec![None; icons.len()], selected)
    }

    pub fn plain(count: usize, selected: usize) -> Self {
        Self::new(vec![None; count], vec![None; count], selected)
    }

    pub fn with_labels(labels: &[&str], selected: usize) -> Self {
        let n = labels.len();
        Self::new(
            vec![None; n],
            labels.iter().map(|s| Some(s.to_string())).collect(),
            selected,
        )
    }

    pub fn on_select(mut self, callback: impl Fn(usize) -> Msg + 'a) -> Self {
        self.on_select = Some(Box::new(callback));
        self
    }

    /// Convenience for 2-item toggles: `true` for item 1, `false` for item 0.
    pub fn on_toggle(self, callback: impl Fn(bool) -> Msg + 'a) -> Self {
        self.on_select(move |i| callback(i != 0))
    }

    pub fn vertical(mut self) -> Self {
        self.is_vertical = true;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.content_opacity = opacity;
        self
    }

    pub fn pill_thickness(mut self, thickness: f32) -> Self {
        self.pill_thickness = thickness;
        self
    }

    pub fn pill_length(mut self, length: f32) -> Self {
        self.pill_length = Some(length);
        self
    }

    pub fn circle_size(mut self, size: f32) -> Self {
        self.circle_size = size;
        self
    }

    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    fn count(&self) -> usize {
        self.icons.len()
    }

    fn effective_pill_length(&self) -> f32 {
        self.pill_length
            .unwrap_or(self.count() as f32 * DEFAULT_SEGMENT_LENGTH)
    }

    fn total_width(&self) -> f32 {
        if self.is_vertical {
            self.pill_thickness
        } else {
            self.effective_pill_length()
        }
    }

    fn total_height(&self) -> f32 {
        if self.is_vertical {
            self.effective_pill_length()
        } else {
            self.pill_thickness
        }
    }

    fn position_for(&self, index: usize) -> f32 {
        let n = self.count();
        if n <= 1 {
            return 0.0;
        }
        index as f32 / (n - 1) as f32
    }

    fn segment_center(&self, i: usize, bounds: Rectangle) -> (f32, f32) {
        let n = self.count() as f32;
        let pill = self.effective_pill_length();
        let segment_len = pill / n;
        let offset = segment_len * (i as f32 + 0.5);
        if self.is_vertical {
            (bounds.x + self.pill_thickness / 2.0, bounds.y + offset)
        } else {
            (bounds.x + offset, bounds.y + self.pill_thickness / 2.0)
        }
    }

    fn segment_bounds(&self, i: usize, bounds: Rectangle) -> Rectangle {
        let n = self.count() as f32;
        let pill = self.effective_pill_length();
        let seg = pill / n;
        if self.is_vertical {
            Rectangle {
                x: bounds.x,
                y: bounds.y + seg * i as f32,
                width: self.pill_thickness,
                height: seg,
            }
        } else {
            Rectangle {
                x: bounds.x + seg * i as f32,
                y: bounds.y,
                width: seg,
                height: self.pill_thickness,
            }
        }
    }

    fn circle_center(&self, pos: f32, bounds: Rectangle) -> (f32, f32) {
        let first = self.segment_center(0, bounds);
        let last = self.segment_center(self.count() - 1, bounds);
        if self.is_vertical {
            (first.0, first.1 + (last.1 - first.1) * pos)
        } else {
            (first.0 + (last.0 - first.0) * pos, first.1)
        }
    }
}

impl<'a, Msg: Clone + 'a> cosmic::widget::Widget<Msg, cosmic::Theme, cosmic::Renderer>
    for Toggle<'a, Msg>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.total_width()),
            Length::Fixed(self.total_height()),
        )
    }

    fn diff(&mut self, _tree: &mut Tree) {}

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &cosmic::Renderer,
        limits: &cosmic::iced::Limits,
    ) -> layout::Node {
        let width = self.total_width();
        let height = self.total_height();
        let size = limits
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .resolve(width, height, Size::new(width, height));
        layout::Node::new(size)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &cosmic::iced::core::renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        use cosmic::iced::core::Renderer as _;

        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let cosmic_theme = theme.cosmic();
        let opacity = self.content_opacity;

        let mut accent_color: Color = cosmic_theme.accent_color().into();
        accent_color.a *= opacity;
        let pill_color = Color::from_rgba(0.3, 0.3, 0.3, 0.6 * opacity);
        let hover_color = Color::from_rgba(
            accent_color.r,
            accent_color.g,
            accent_color.b,
            0.3 * opacity,
        );

        let pill_radius = self.pill_thickness / 2.0;
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: bounds.x,
                    y: bounds.y,
                    width: self.total_width(),
                    height: self.total_height(),
                },
                border: Border {
                    radius: pill_radius.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(pill_color),
        );

        let target = self.position_for(self.selected);
        let pos = state.position(target, self.duration);
        let (circle_cx, circle_cy) = self.circle_center(pos, bounds);
        let segment_len = self.effective_pill_length() / self.count() as f32;

        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: circle_cx - segment_len / 2.0,
                    y: circle_cy - self.circle_size / 2.0,
                    width: segment_len,
                    height: self.circle_size,
                },
                border: Border {
                    radius: (self.circle_size / 2.0).into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Shadow::default(),
                snap: false,
            },
            Background::Color(accent_color),
        );

        let hovered = cursor
            .position()
            .and_then(|p| (0..self.count()).find(|&i| self.segment_bounds(i, bounds).contains(p)));
        if let Some(hi) = hovered
            && hi != self.selected
        {
            let (hx, hy) = self.segment_center(hi, bounds);
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: hx - segment_len / 2.0,
                        y: hy - self.circle_size / 2.0,
                        width: segment_len,
                        height: self.circle_size,
                    },
                    border: Border {
                        radius: (self.circle_size / 2.0).into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(hover_color),
            );
        }

        let n = self.count();
        let visual_index = if n <= 1 {
            0
        } else {
            (pos * (n - 1) as f32).round() as usize
        };
        let on_accent_color: Color = cosmic_theme.accent.on.into();
        let icon_sz = self.icon_size;

        for (i, icon_name) in self.icons.iter().enumerate() {
            let Some(name) = icon_name else { continue };
            let Some(handle) =
                icon::Icon::from(icon::from_name(*name).size(icon_sz as u16)).into_svg_handle()
            else {
                continue;
            };

            let is_selected = i == visual_index;
            let svg = if is_selected {
                cosmic::widget::svg::Svg::new(handle)
                    .width(Length::Fixed(icon_sz))
                    .height(Length::Fixed(icon_sz))
                    .opacity(opacity)
                    .symbolic(true)
                    .class(cosmic::theme::Svg::Custom(Rc::new(move |_| {
                        cosmic::widget::svg::Style {
                            color: Some(on_accent_color),
                        }
                    })))
            } else {
                cosmic::widget::svg::Svg::new(handle)
                    .width(Length::Fixed(icon_sz))
                    .height(Length::Fixed(icon_sz))
                    .opacity(opacity)
                    .symbolic(true)
            };

            let (cx, cy) = self.segment_center(i, bounds);
            let node = layout::Node::new(Size::new(icon_sz, icon_sz)).move_to(
                cosmic::iced::Point::new(cx - icon_sz / 2.0, cy - icon_sz / 2.0),
            );
            let element: Element<'_, Msg> = svg.into();
            element.as_widget().draw(
                &Tree::empty(),
                renderer,
                theme,
                style,
                Layout::new(&node),
                cursor,
                viewport,
            );
        }

        use cosmic::iced::core::text::Renderer as TextRenderer;
        let label_color: Color = cosmic_theme.palette.neutral_10.into();
        let font = renderer.default_font();
        for (i, label) in self.labels.iter().enumerate() {
            let Some(text) = label else { continue };
            let is_selected = i == visual_index;
            let color = if is_selected {
                on_accent_color
            } else {
                label_color
            };

            let (cx, cy) = self.segment_center(i, bounds);
            let text_size = cosmic::iced::Pixels(11.0);
            let text_bounds = Size::new(segment_len, 14.0);

            renderer.fill_text(
                cosmic::iced::core::Text {
                    content: text.to_string(),
                    bounds: text_bounds,
                    size: text_size,
                    line_height: cosmic::iced::core::text::LineHeight::default(),
                    font,
                    align_x: cosmic::iced::Alignment::Center.into(),
                    align_y: cosmic::iced::alignment::Vertical::Center,
                    shaping: cosmic::iced::core::text::Shaping::Basic,
                    wrapping: cosmic::iced::core::text::Wrapping::None,
                    ellipsize: cosmic::iced::core::text::Ellipsize::None,
                },
                cosmic::iced::Point::new(cx, cy),
                color,
                bounds,
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &cosmic::iced::core::Event,
        layout: Layout<'_>,
        cursor: Cursor,
        _renderer: &cosmic::Renderer,
        _clipboard: &mut dyn cosmic::iced::core::Clipboard,
        shell: &mut cosmic::iced::core::Shell<'_, Msg>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        match event {
            cosmic::iced::core::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(ref on_select) = self.on_select else {
                    return;
                };
                if let Some(pos) = cursor.position() {
                    let bounds = layout.bounds();
                    for i in 0..self.count() {
                        if i != self.selected && self.segment_bounds(i, bounds).contains(pos) {
                            let target = self.position_for(self.selected);
                            let current = state.position(target, self.duration);
                            state.start(current);

                            shell.publish(on_select(i));
                            shell.capture_event();
                            return;
                        }
                    }
                }
            }
            cosmic::iced::core::Event::Window(window::Event::RedrawRequested(_)) => {
                state.finish_if_done(self.duration);
                if state.is_animating(self.duration) {
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
        if let Some(pos) = cursor.position() {
            let bounds = layout.bounds();
            for i in 0..self.count() {
                if i != self.selected && self.segment_bounds(i, bounds).contains(pos) {
                    return mouse::Interaction::Pointer;
                }
            }
        }
        mouse::Interaction::default()
    }
}

impl<'a, Msg: Clone + 'a> From<Toggle<'a, Msg>> for Element<'a, Msg> {
    fn from(toggle: Toggle<'a, Msg>) -> Self {
        Element::new(toggle)
    }
}

pub fn toggle<'a, Msg: Clone + 'a>(
    icon_a: &'a str,
    icon_b: &'a str,
    is_b_selected: bool,
) -> Toggle<'a, Msg> {
    Toggle::with_icons(&[icon_a, icon_b], if is_b_selected { 1 } else { 0 })
}

pub fn toggle3<'a, Msg: Clone + 'a>(
    icon_a: &'a str,
    icon_b: &'a str,
    icon_c: &'a str,
    selected: usize,
) -> Toggle<'a, Msg> {
    Toggle::with_icons(&[icon_a, icon_b, icon_c], selected)
}
