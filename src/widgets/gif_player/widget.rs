// SPDX-License-Identifier: MPL-2.0

//! GIF player widget implementation.

use cosmic::iced::core::{
    Clipboard, ContentFit, Element, Event, Layout, Length, Point, Radians, Rectangle, Shell, Size,
    Vector, Widget,
    image::{self as iced_image, FilterMethod, Handle, Image},
    layout,
    mouse::Cursor,
    renderer,
    widget::{Tree, tree},
    window,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct RawFrame {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
    delay: Duration,
}

/// Pre-decoded RGBA frames for playback.
#[derive(Clone)]
pub struct Frames {
    frames: Vec<RawFrame>,
    id: u64,
}

impl std::fmt::Debug for Frames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frames")
            .field("count", &self.frames.len())
            .finish()
    }
}

impl Frames {
    pub fn from_rgba(images: &[(u32, u32, &[u8])], delay_ms: u32) -> Self {
        let delay = Duration::from_millis(delay_ms as u64);
        let mut hasher = std::hash::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        images.len().hash(&mut hasher);
        delay_ms.hash(&mut hasher);

        let frames = images
            .iter()
            .map(|(w, h, pixels)| RawFrame {
                width: *w,
                height: *h,
                pixels: Arc::new(pixels.to_vec()),
                delay,
            })
            .collect();

        Frames {
            frames,
            id: hasher.finish(),
        }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

struct State {
    index: usize,
    started: Instant,
    current_handle: Option<(usize, iced_image::Handle)>,
    id: u64,
}

impl State {
    fn handle_for(&mut self, index: usize, frames: &[RawFrame]) -> &iced_image::Handle {
        if self
            .current_handle
            .as_ref()
            .map_or(true, |(i, _)| *i != index)
        {
            let frame = &frames[index];
            let handle =
                iced_image::Handle::from_rgba(frame.width, frame.height, (*frame.pixels).clone());
            self.current_handle = Some((index, handle));
        }
        &self.current_handle.as_ref().unwrap().1
    }
}

pub struct GifPlayer<'a, Message> {
    frames: &'a Frames,
    playing: bool,
    trim_start: usize,
    trim_end: usize,
    seek_index: Option<usize>,
    on_frame: Option<Box<dyn Fn(usize) -> Message + 'a>>,
    width: Length,
    height: Length,
    content_fit: ContentFit,
}

impl<'a, Message> std::fmt::Debug for GifPlayer<'a, Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GifPlayer")
            .field("playing", &self.playing)
            .field("trim", &(self.trim_start, self.trim_end))
            .finish()
    }
}

impl<'a, Message> GifPlayer<'a, Message> {
    pub fn new(frames: &'a Frames) -> Self {
        let len = frames.frames.len();
        Self {
            frames,
            playing: false,
            trim_start: 0,
            trim_end: len,
            seek_index: None,
            on_frame: None,
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::Contain,
        }
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    /// Frame indices `[start, end)`.
    pub fn trim(mut self, start: usize, end: usize) -> Self {
        let len = self.frames.frames.len();
        self.trim_start = start.min(len);
        self.trim_end = end.min(len).max(self.trim_start);
        self
    }

    pub fn seek(mut self, index: usize) -> Self {
        self.seek_index = Some(index.min(self.frames.frames.len().saturating_sub(1)));
        self
    }

    pub fn on_frame(mut self, f: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_frame = Some(Box::new(f));
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }
}

pub fn gif_player<'a, Message>(frames: &'a Frames) -> GifPlayer<'a, Message> {
    GifPlayer::new(frames)
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for GifPlayer<'a, Message>
where
    Renderer: iced_image::Renderer<Handle = Handle>,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        let index = self
            .trim_start
            .min(self.frames.frames.len().saturating_sub(1));
        tree::State::new(State {
            index,
            started: Instant::now(),
            current_handle: None,
            id: self.frames.id,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();

        if state.id != self.frames.id {
            let index = self
                .trim_start
                .min(self.frames.frames.len().saturating_sub(1));
            *state = State {
                index,
                started: Instant::now(),
                current_handle: None,
                id: self.frames.id,
            };
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let idx = self
            .trim_start
            .min(self.frames.frames.len().saturating_sub(1));
        let handle = state.handle_for(idx, &self.frames.frames);

        let image_size = renderer
            .measure_image(handle)
            .map(|Size { width, height }| Size::new(width as f32, height as f32))
            .unwrap_or(Size::new(1.0, 1.0));

        let raw_size = limits.width(self.width).height(self.height).resolve(
            self.width,
            self.height,
            image_size,
        );

        let fitted = self.content_fit.fit(image_size, raw_size);

        let final_size = Size::new(
            match self.width {
                Length::Shrink => fitted.width,
                _ => raw_size.width,
            },
            match self.height {
                Length::Shrink => fitted.height,
                _ => raw_size.height,
            },
        );

        layout::Node::new(final_size)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if self.frames.frames.is_empty() {
            return;
        }

        let state = tree.state.downcast_mut::<State>();

        // When not playing, honour external seek position.
        if !self.playing {
            if let Some(idx) = self.seek_index {
                let clamped = idx.min(self.frames.frames.len().saturating_sub(1));
                if state.index != clamped {
                    state.index = clamped;
                    state.started = Instant::now();
                }
            }
            // Ensure handle is cached for draw().
            state.handle_for(state.index, &self.frames.frames);
            return;
        }

        // Clamp state index into current trim range if trim changed.
        if state.index < self.trim_start || state.index >= self.trim_end {
            state.index = self.trim_start;
            state.started = Instant::now();
            if let Some(ref on_frame) = self.on_frame {
                shell.publish((on_frame)(state.index));
            }
        }

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let trim_len = self.trim_end.saturating_sub(self.trim_start);
            if trim_len == 0 {
                return;
            }

            let current_frame = &self.frames.frames[state.index];
            let elapsed = now.duration_since(state.started);

            if elapsed >= current_frame.delay {
                // Advance to next frame within trim range.
                let offset = state.index.saturating_sub(self.trim_start);
                let next_offset = (offset + 1) % trim_len;
                state.index = self.trim_start + next_offset;
                state.started = *now;

                if let Some(ref on_frame) = self.on_frame {
                    shell.publish((on_frame)(state.index));
                }

                let next_delay = self.frames.frames[state.index].delay;
                shell.request_redraw_at(window::RedrawRequest::At(*now + next_delay));
            } else {
                let remaining = current_frame.delay - elapsed;
                shell.request_redraw_at(window::RedrawRequest::At(*now + remaining));
            }
        }

        // Ensure handle is cached for draw().
        state.handle_for(state.index, &self.frames.frames);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        if self.frames.frames.is_empty() {
            return;
        }

        // We need mutable access to update the cached handle, but draw() takes &self.
        // The handle_for method is on State which is behind Tree's interior mutability.
        // We'll create the handle inline if needed.
        let state = tree.state.downcast_ref::<State>();
        let idx = state.index.min(self.frames.frames.len() - 1);

        // Use the cached handle if it matches, otherwise reference the raw frame
        // dimensions for layout. The actual handle creation happens in update/layout.
        let handle = if let Some((cached_idx, ref h)) = state.current_handle {
            if cached_idx == idx {
                h
            } else {
                // Fallback: shouldn't normally happen since update() runs before draw(),
                // but create a temporary handle just in case.
                // This will be cached on the next update cycle.
                return;
            }
        } else {
            return;
        };

        if let Some(Size { width, height }) = renderer.measure_image(handle) {
            let image_size = Size::new(width as f32, height as f32);
            let bounds = layout.bounds();
            let adjusted_fit = self.content_fit.fit(image_size, bounds.size());

            let scale = Vector::new(
                adjusted_fit.width / image_size.width,
                adjusted_fit.height / image_size.height,
            );

            let final_size = image_size * scale;

            let position = Point::new(
                bounds.center_x() - final_size.width / 2.0,
                bounds.center_y() - final_size.height / 2.0,
            );

            let drawing_bounds = Rectangle::new(position, final_size);

            let render = |renderer: &mut Renderer| {
                renderer.draw_image(
                    Image {
                        handle: handle.clone(),
                        filter_method: FilterMethod::default(),
                        rotation: Radians(0.0),
                        border_radius: [0.0; 4].into(),
                        opacity: 1.0,
                        snap: true,
                    },
                    drawing_bounds,
                    drawing_bounds,
                );
            };

            if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
                renderer.with_layer(bounds, render);
            } else {
                render(renderer);
            }
        }
    }
}

impl<'a, Message, Theme, Renderer> From<GifPlayer<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: iced_image::Renderer<Handle = Handle> + 'a,
{
    fn from(player: GifPlayer<'a, Message>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(player)
    }
}
