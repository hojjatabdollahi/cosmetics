// SPDX-License-Identifier: MPL-2.0

//! Widget implementation for [`ImageContainer`].

use cosmic::iced::core::image::Renderer as ImageRenderer;
use cosmic::iced::core::{
    self, Background, Border, Clipboard, Color, ContentFit, Element, Layout, Length, Point,
    Radians, Rectangle, Size, Vector, event::Event, image, layout, mouse, overlay, renderer,
    widget::Tree,
};
use iced_wgpu::primitive;

use super::rounded_primitive::RoundedImagePrimitive;

/// A container that draws an image as its background.
///
/// The image is drawn to fill the container's bounds without affecting the
/// layout or size of the container. The content is drawn on top of the image.
///
/// ## Rounded corners
///
/// Use [`Self::border_radius`] to set a corner radius. The image is clipped
/// via an SDF-based GPU shader with smooth anti-aliasing at the edge.
///
/// Use [`image_container`](super::image_container) for a convenient
/// constructor that returns an [`Element`] directly.
pub struct ImageContainer<'a, Message, Theme = cosmic::Theme, Renderer = cosmic::Renderer>
where
    Renderer: ImageRenderer<Handle = image::Handle>,
{
    handle: image::Handle,
    content: Element<'a, Message, Theme, Renderer>,
    width: Length,
    height: Length,
    border_radius: f32,
    opacity: f32,
    border_width: f32,
    border_color: Color,
    overlay_color: Color,
    overlay_opacity: f32,
}

impl<'a, Message, Theme, Renderer> ImageContainer<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + ImageRenderer<Handle = image::Handle>,
{
    pub fn new(
        handle: impl Into<image::Handle>,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            handle: handle.into(),
            content: content.into(),
            width: Length::Shrink,
            height: Length::Shrink,
            border_radius: 0.0,
            opacity: 1.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            overlay_color: Color::BLACK,
            overlay_opacity: 0.0,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn border_radius(mut self, radius: f32) -> Self {
        self.border_radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn overlay(mut self, color: Color, opacity: f32) -> Self {
        self.overlay_color = color;
        self.overlay_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self
    }
}

impl<'a, Message, Theme, Renderer> core::Widget<Message, Theme, Renderer>
    for ImageContainer<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + ImageRenderer<Handle = image::Handle> + primitive::Renderer,
    Message: 'a,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.children[0].diff(&mut self.content);
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Build child limits that:
        //   • cap width to our Fixed dimension so Length::Fill children
        //     expand to fill US, not the outer parent (prevents black overflow)
        //   • leave height limits from the parent untouched so Shrink height
        //     is determined by the child's natural content size, not collapsed
        //   • loose() zeroes minimums, matching iced's container behaviour
        //
        // IMPORTANT: do NOT use Limits::new(ZERO, self_size) with a pre-resolved
        // self_size , if height is Shrink, self_size.height is 0, which makes
        // child_limits.max.height = 0 and collapses everything to zero.
        let child_limits = limits
            .max_width(match self.width {
                Length::Fixed(w) => w,
                _ => f32::INFINITY,
            })
            .loose()
            .width(self.width);

        let child_node =
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &child_limits);

        // Resolve our own final size against the original parent limits:
        //   Fixed(n) → n px;  Shrink → child's natural size
        let size = limits.resolve(self.width, self.height, child_node.size());
        layout::Node::with_children(size, vec![child_node])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let child_layout = layout.children().next();

        // Guard: image not yet decoded (Handle::from_path is lazy; the renderer
        // may return a placeholder size such as 0×0 or 1×1 on the first frame).
        // Issue a zero-opacity draw_image to queue the texture upload; on the
        // next frame measure_image will return the real dimensions.
        //
        // IMPORTANT: this guard is only valid for the rectangular path
        // (border_radius == 0), which uses draw_image / iced's image pipeline.
        // For the rounded SDF path (border_radius > 0), the widget uses
        // draw_primitive → RoundedImagePipeline, which manages its own texture
        // cache independently and never registers handles with measure_image.
        // Applying the guard there would cause measure_image to return None
        // forever, making the image invisible on every frame.
        let img_px = if self.border_radius <= 0.0 {
            let img_px = renderer.measure_image(&self.handle);
            match img_px {
                Some(s) if s.width > 1 && s.height > 1 => s,
                _ => {
                    renderer.draw_image(
                        image::Image {
                            handle: self.handle.clone(),
                            filter_method: image::FilterMethod::Linear,
                            rotation: Radians(0.0),
                            opacity: 0.0,
                            border_radius: [0.0; 4].into(),
                            snap: false,
                        },
                        bounds,
                        bounds,
                    );

                    if self.border_width > 0.0 {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds,
                                border: Border {
                                    color: self.border_color,
                                    width: self.border_width,
                                    radius: 0.0.into(),
                                },
                                shadow: Default::default(),
                                snap: true,
                            },
                            Background::Color(Color::TRANSPARENT),
                        );
                    }

                    if let Some(child_layout) = child_layout {
                        renderer.with_layer(*viewport, |renderer| {
                            self.content.as_widget().draw(
                                &tree.children[0],
                                renderer,
                                theme,
                                style,
                                child_layout,
                                cursor,
                                viewport,
                            );
                        });
                    }

                    return;
                }
            }
        } else {
            // Rounded SDF path: RoundedImagePipeline decodes independently and
            // computes cover-fit UV from the real decoded texture dimensions.
            // Use container bounds as a size placeholder here (not used for UV).
            Size {
                width: bounds.width as u32,
                height: bounds.height as u32,
            }
        };

        // Safe: image has real decoded dimensions , no NaN possible.
        let image_size = Size::new(img_px.width as f32, img_px.height as f32);

        // Scale to Cover: fill the container, crop the excess, preserve aspect.
        let adjusted = ContentFit::Cover.fit(image_size, bounds.size());
        let scale_x = adjusted.width / image_size.width;
        let scale_y = adjusted.height / image_size.height;
        let draw_size = Size::new(image_size.width * scale_x, image_size.height * scale_y);

        // Centre the scaled image over the container.
        let drawing_bounds = Rectangle::new(
            Point::new(
                bounds.center_x() - draw_size.width / 2.0,
                bounds.center_y() - draw_size.height / 2.0,
            ),
            draw_size,
        );

        //
        // Background primitives are drawn first in this layer.
        // Child content is then drawn in its own top layer below, ensuring
        // child quads/images/text keep their internal painter ordering and are
        // never covered by this widget's background pass batching.
        let handle = self.handle.clone();
        let opacity = self.opacity;
        let border_radius_arr = [self.border_radius; 4];
        let overlay_alpha = (self.overlay_color.a * self.overlay_opacity).clamp(0.0, 1.0);
        let overlay_color = Color {
            a: overlay_alpha,
            ..self.overlay_color
        };

        if self.border_radius > 0.0 {
            // True rounded clipping + border via custom GPU primitive.
            // The SDF shader handles clipping + border ring in one pass.
            let bc = self.border_color;
            renderer.draw_primitive(
                bounds,
                RoundedImagePrimitive {
                    handle,
                    bounds,
                    radius: self.border_radius,
                    opacity,
                    border_width: self.border_width,
                    border_color: [bc.r, bc.g, bc.b, bc.a],
                    overlay_color: [
                        self.overlay_color.r,
                        self.overlay_color.g,
                        self.overlay_color.b,
                        self.overlay_color.a,
                    ],
                    overlay_opacity: self.overlay_opacity,
                },
            );
        } else {
            // Rectangular path: use with_layer only when image overflows bounds.
            let needs_layer = draw_size.width > bounds.width || draw_size.height > bounds.height;
            let img_struct = image::Image {
                handle,
                filter_method: image::FilterMethod::Linear,
                rotation: Radians(0.0),
                opacity,
                border_radius: border_radius_arr.into(),
                snap: false,
            };

            if needs_layer {
                renderer.with_layer(bounds, |renderer| {
                    renderer.draw_image(img_struct.clone(), drawing_bounds, bounds);
                    if overlay_alpha > 0.0 {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds,
                                border: Border::default(),
                                shadow: Default::default(),
                                snap: true,
                            },
                            Background::Color(overlay_color),
                        );
                    }
                });
            } else {
                renderer.draw_image(img_struct, drawing_bounds, bounds);
                if overlay_alpha > 0.0 {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            border: Border::default(),
                            shadow: Default::default(),
                            snap: true,
                        },
                        Background::Color(overlay_color),
                    );
                }
            }

            if self.border_width > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border {
                            color: self.border_color,
                            width: self.border_width,
                            radius: 0.0.into(),
                        },
                        shadow: Default::default(),
                        snap: true,
                    },
                    Background::Color(Color::TRANSPARENT),
                );
            }
        }

        // Draw children in a top layer so their quads/images are not occluded
        // by this widget's background primitive/image passes.
        if let Some(child_layout) = child_layout {
            renderer.with_layer(*viewport, |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut core::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let child_layout = layout.children().next().unwrap();
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let child_layout = layout.children().next().unwrap();
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn core::widget::Operation<()>,
    ) {
        let child_layout = layout.children().next().unwrap();
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout,
            renderer,
            operation,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let child_layout = layout.children().next().unwrap();
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message> From<ImageContainer<'a, Message>>
    for Element<'a, Message, cosmic::Theme, cosmic::Renderer>
where
    Message: 'a,
{
    fn from(widget: ImageContainer<'a, Message>) -> Self {
        Element::new(widget)
    }
}
