// SPDX-License-Identifier: MPL-2.0

//! Custom GPU primitive for rendering an image clipped to rounded corners.
//!
//! [`RoundedImagePrimitive`] implements [`iced_wgpu::primitive::Primitive`]
//! so it gets its own wgpu render pipeline with a WGSL fragment shader that
//! performs SDF-based corner discarding , the only reliable way to clip an
//! image to rounded corners in this iced/wgpu backend.
//!
//! ## How the framework calls us
//!
//! The iced wgpu backend sets the render-pass **viewport** to the primitive's
//! physical-pixel bounds before calling `draw()`.  So the vertex shader just
//! needs to emit a unit NDC quad (−1..+1 in both axes) , the viewport
//! transform maps that onto the correct screen region automatically.
//!
//! `prepare()` receives `bounds` in **logical pixels** (the transformation
//! stack is IDENTITY at the top layer; scale is applied later by the
//! renderer when it sets up the viewport for the render pass).  We only
//! need `bounds` to compute `radius_norm`; we multiply by `scale_factor`
//! to get physical pixels for the normalisation denominator.

use cosmic::iced::core::{Rectangle, image as iced_image};
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive;
use iced_wgpu::wgpu;

use bytemuck::{Pod, Zeroable};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;


const SHADER: &str = r#"
// We create our own render pass in render(), so the vertex shader must
// position the quad explicitly using NDC coordinates from the uniforms.
struct Uniforms {
    // top-left NDC position of the widget (x in [-1,1], y in [-1,1], +1 = top)
    ndc_pos:      vec2<f32>,
    // NDC width/height (positive values)
    ndc_size:     vec2<f32>,
    // widget physical size in pixels , SDF runs in pixel space for a true circle
    widget_size:  vec2<f32>,
    // corner radius in physical pixels
    radius_px:      f32,
    opacity:        f32,
    // border stroke width in physical pixels (0 = no border)
    border_width:   f32,
    // overlay opacity multiplier (0 = disabled)
    overlay_opacity: f32,
    // UV transform for cover-fit: sample_uv = uv_offset + uv * uv_scale
    uv_offset:      vec2<f32>,
    uv_scale:       vec2<f32>,
    // pad to 16-byte alignment before vec4 fields
    _pad0:          vec2<f32>,
    // border and overlay colours in linear [0, 1]
    border_color:   vec4<f32>,
    overlay_color:  vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var t_image: texture_2d<f32>;
@group(1) @binding(1) var s_image: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
};

var<private> POSITIONS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    let uv = POSITIONS[idx];
    // Map UV [0,1] → NDC using widget position+size from uniforms.
    // NDC Y is flipped: uv.y=0 is top → ndc_y = ndc_pos.y (largest y value).
    let ndc = vec2<f32>(
        u.ndc_pos.x + uv.x * u.ndc_size.x,
        u.ndc_pos.y - uv.y * u.ndc_size.y,
    );
    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Box-SDF in physical pixel space.
// `px`   , fragment position in pixels within the widget (origin = top-left).
// `size` , widget size in pixels.
// `r`    , corner radius in pixels.
// Returns < 0 inside the rounded rect, > 0 outside.
fn rounded_rect_sdf_px(px: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let half = size * 0.5;
    let q    = abs(px - half) - (half - vec2<f32>(r, r));
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // px_coord: pixel position within the widget, origin at top-left.
    // uv.y = 0 is the screen top of the widget; px_coord.y = 0 is top.
    let px_coord = in.uv * u.widget_size;

    // Outer SDF: clips the widget to the rounded rectangle.
    let d = rounded_rect_sdf_px(px_coord, u.widget_size, u.radius_px);

    // AA over ±0.5 physical pixels at the outer edge.
    let alpha = 1.0 - smoothstep(-0.5, 0.5, d);
    if alpha <= 0.0 {
        discard;
    }

    // wgpu textures have row 0 at the top, matching uv.y=0=top.
    // However the NDC vertex positions flip Y (uv.y=0 → NDC top),
    // so the rasterised fragment uv.y=0 corresponds to the TOP of the widget
    // but the @builtin(position).y increases downward on screen.
    // Texture rows also increase downward, so NO V-flip is needed here.
    var img = textureSample(t_image, s_image, u.uv_offset + in.uv * u.uv_scale);

    // Overlay tint: blend color over the image before border composition so
    // the border remains visually on top.
    let overlay_alpha = clamp(u.overlay_opacity * u.overlay_color.a, 0.0, 1.0);
    if overlay_alpha > 0.0 {
        img = vec4<f32>(mix(img.rgb, u.overlay_color.rgb, overlay_alpha), img.a);
    }

    // Border: if border_width > 0, blend border_color over the image
    // in the annular region between the outer and inner rounded rects.
    var out_color = img;
    if u.border_width > 0.0 {
        let bw           = u.border_width;
        let inner_size   = u.widget_size - vec2<f32>(bw * 2.0, bw * 2.0);
        let inner_px     = px_coord - vec2<f32>(bw, bw);
        let inner_r      = max(u.radius_px - bw, 0.0);
        let d_inner      = rounded_rect_sdf_px(inner_px, inner_size, inner_r);
        // d_inner > 0  → outside the inner rect  → in the border zone
        // Blend smoothly over ±0.5px at the inner edge
        let border_frac  = smoothstep(-0.5, 0.5, d_inner);
        out_color = mix(img, u.border_color, border_frac);
    }

    return out_color * (u.opacity * alpha);
}

"#;


#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct Uniforms {
    ndc_pos: [f32; 2],     // NDC top-left
    ndc_size: [f32; 2],    // NDC width/height
    widget_size: [f32; 2], // physical pixel size of the widget
    radius_px: f32,        // corner radius in physical pixels
    opacity: f32,
    border_width: f32,       // stroke width in physical pixels (0 = none)
    overlay_opacity: f32,    // overlay intensity multiplier
    uv_offset: [f32; 2],     // cover-fit UV offset
    uv_scale: [f32; 2],      // cover-fit UV scale
    _pad0: [f32; 2],         // align vec4 fields to 16-byte boundary
    border_color: [f32; 4],  // RGBA
    overlay_color: [f32; 4], // RGBA
} // total: 96 bytes


/// A GPU primitive that draws `handle` clipped to rounded corners.
///
/// Submit via [`iced_wgpu::primitive::Renderer::draw_primitive`].
#[derive(Debug)]
pub struct RoundedImagePrimitive {
    pub handle: iced_image::Handle,
    /// Widget bounds in logical pixels; the framework passes this to `prepare`
    /// after transforming it to physical pixels via `bounds * transformation`.
    #[allow(dead_code)]
    pub bounds: Rectangle,
    pub radius: f32,
    pub opacity: f32,
    /// Border stroke width in logical pixels (0 = no border).
    pub border_width: f32,
    /// Border colour (RGBA components in linear [0,1] space).
    pub border_color: [f32; 4],
    /// Overlay tint colour (RGBA components in linear [0,1] space).
    pub overlay_color: [f32; 4],
    /// Overlay opacity multiplier in [0,1].
    pub overlay_opacity: f32,
}


/// One prepared draw call , created in `prepare()`, consumed in `render()`.
struct PreparedFrame {
    /// Kept alive so wgpu doesn't free the GPU buffer before the draw.
    #[allow(dead_code)]
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    image_bg: wgpu::BindGroup,
    /// Dummy 1×1 texture , the real image lives in `ImageCache`.
    #[allow(dead_code)]
    _texture: wgpu::Texture,
}

pub struct RoundedImagePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_bg_layout: wgpu::BindGroupLayout,
    image_bg_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Frames queued during `prepare()`, drained FIFO during `render()`.
    /// Wrapped in `Mutex` so `render()` can pop via `&Storage` (immutable ref).
    frames: Mutex<VecDeque<PreparedFrame>>,
    /// Cached decoded textures , reused while the handle id doesn't change.
    image_cache: HashMap<u64, ImageCache>,
}

struct ImageCache {
    #[allow(dead_code)]
    handle_id: u64,
    /// Kept alive so the `TextureView` and bind groups remain valid.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl std::fmt::Debug for RoundedImagePipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoundedImagePipeline")
    }
}

impl RoundedImagePipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rounded_image shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // Bind group 0 , uniforms
        let uniform_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rounded_image uniform bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Bind group 1 , image texture + sampler
        let image_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rounded_image image bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rounded_image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rounded_image pipeline layout"),
            bind_group_layouts: &[&uniform_bg_layout, &image_bg_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rounded_image pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_bg_layout,
            image_bg_layout,
            sampler,
            frames: Mutex::new(VecDeque::new()),
            image_cache: HashMap::new(),
        }
    }

    /// Build (or reuse) the image texture and push a new `PreparedFrame`.
    ///
    /// Called once per draw call per frame. Each call pushes exactly one entry
    /// onto `self.frames`; `render()` pops them FIFO in the same order.
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        primitive: &RoundedImagePrimitive,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let handle = &primitive.handle;
        let radius = primitive.radius;
        let opacity = primitive.opacity;
        let overlay_opacity = primitive.overlay_opacity.clamp(0.0, 1.0);
        let handle_id = handle_hash(handle);
        let cache_miss = !self.image_cache.contains_key(&handle_id);

        if cache_miss {
            // Check if the handle is memory-backed first, to avoid blocking disk I/O
            // This relies on the background task having already loaded it into memory!
            if let Some((pixels, w, h)) = try_decode(handle) {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("rounded_image tex"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * w),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.image_cache.insert(
                    handle_id,
                    ImageCache {
                        handle_id,
                        texture,
                        view,
                    },
                );
            } else {
                return; // can't decode , skip this frame
            }
        }

        let cache = match self.image_cache.get(&handle_id) {
            Some(c) => c,
            None => return,
        };

        let image_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rounded_image image bg"),
            layout: &self.image_bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cache.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // `bounds` is in logical pixels (transformation is IDENTITY at top
        // level). Multiply by scale to get physical pixels for NDC math.
        let scale = viewport.scale_factor() as f32;
        let physical = viewport.physical_size();
        let vp_w = physical.width as f32;
        let vp_h = physical.height as f32;

        let l = bounds.x * scale;
        let t = bounds.y * scale;
        let w = bounds.width * scale;
        let h = bounds.height * scale;

        // NDC: x in [-1, 1], y in [-1, 1] with +1 at top.
        let ndc_x = (l / vp_w) * 2.0 - 1.0;
        let ndc_y = 1.0 - (t / vp_h) * 2.0;
        let ndc_w = (w / vp_w) * 2.0;
        let ndc_h = (h / vp_h) * 2.0;

        // radius and border in physical pixels
        let radius_px = radius * scale;
        let border_width = primitive.border_width * scale;

        // Compute UV transform for cover-fit sampling using the actual decoded
        // texture dimensions. The shader quad covers `bounds` exactly; we need
        // to sample the sub-region of the image that a CSS cover-fit would show.
        //
        // cover-fit logic: scale the image uniformly so it fills the container
        // on the smaller ratio axis, then centre and crop the other axis.
        let (uv_offset, uv_scale) = {
            let (tex_w, tex_h) = match self.image_cache.get(&handle_id) {
                Some(c) => {
                    let sz = c.texture.size();
                    (sz.width as f32, sz.height as f32)
                }
                None => (w, h), // fallback: no crop (shouldn't happen; cache set above)
            };
            let widget_w = bounds.width; // logical pixels
            let widget_h = bounds.height;
            // Scale factor so the image fills the widget (cover = max of ratios)
            let sx = widget_w / tex_w;
            let sy = widget_h / tex_h;
            let s = sx.max(sy); // cover: larger scale fills the smaller dimension
            // Displayed image size in logical pixels
            let disp_w = tex_w * s;
            let disp_h = tex_h * s;
            // UV offset: how far into [0,1] UV space the top-left of the widget is
            let off_x = (disp_w - widget_w) / 2.0 / disp_w;
            let off_y = (disp_h - widget_h) / 2.0 / disp_h;
            // UV scale: what fraction of the full image [0,1] range the widget spans
            let sc_x = widget_w / disp_w;
            let sc_y = widget_h / disp_h;
            ([off_x, off_y], [sc_x, sc_y])
        };

        let uniforms = Uniforms {
            ndc_pos: [ndc_x, ndc_y],
            ndc_size: [ndc_w, ndc_h],
            widget_size: [w, h],
            radius_px,
            opacity,
            border_width,
            overlay_opacity,
            uv_offset,
            uv_scale,
            _pad0: [0.0, 0.0],
            border_color: primitive.border_color,
            overlay_color: primitive.overlay_color,
        };

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rounded_image uniform buf"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rounded_image uniform bg"),
            layout: &self.uniform_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        // Create a stub texture so the PreparedFrame owns something for _texture.
        // The actual texture lives in image_cache; we keep a reference-counted
        // copy by re-creating the view for the bind group above.
        // To satisfy the lifetime without Arc, we just clone a 1×1 placeholder.
        // (The bind group already holds the real view; the _texture field is
        //  only needed to keep wgpu happy if image_cache is evicted, which we
        //  won't do within a frame.)
        //
        // Actually, we DON'T need _texture in PreparedFrame at all , the bind
        // group already keeps the TextureView alive (wgpu ref-counts internally).
        // We keep the field as a dummy for future use.
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rounded_image dummy"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        self.frames.lock().unwrap().push_back(PreparedFrame {
            uniform_buf,
            uniform_bg,
            image_bg,
            _texture: dummy_texture,
        });
    }

    /// Pop the next queued frame and draw it. Called once per draw call per
    /// frame, in the same order as `prepare()`.
    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(frame) = self.frames.lock().unwrap().pop_front() else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rounded_image pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &frame.uniform_bg, &[]);
        pass.set_bind_group(1, &frame.image_bg, &[]);
        pass.draw(0..6, 0..1);
    }
}


fn try_decode(handle: &iced_image::Handle) -> Option<(Vec<u8>, u32, u32)> {
    let rgba = match handle {
        iced_image::Handle::Path(_, path) => image_crate::open(path).ok()?.into_rgba8(),
        iced_image::Handle::Bytes(_, bytes) => {
            image_crate::load_from_memory(bytes).ok()?.into_rgba8()
        }
        iced_image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => image_crate::RgbaImage::from_raw(*width, *height, pixels.to_vec())?,
    };
    let (w, h) = rgba.dimensions();
    Some((rgba.into_raw(), w, h))
}

/// Stable hash of a handle to detect changes between frames.
fn handle_hash(handle: &iced_image::Handle) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    match handle {
        iced_image::Handle::Path(id, _) => id.hash(&mut h),
        iced_image::Handle::Bytes(id, _) => id.hash(&mut h),
        iced_image::Handle::Rgba { id, .. } => id.hash(&mut h),
    }
    h.finish()
}


impl primitive::Pipeline for RoundedImagePipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        RoundedImagePipeline::new(device, format)
    }

    fn trim(&mut self) {
        // Just clear the cache for now, or implement an LRU cache eviction
        // based on last used time. For simplicity, we can just let it clear
        // if memory becomes an issue, but a better approach is to keep a bound.
        // If the cache grows too large (>100), trim it randomly or empty it.
        if self.image_cache.len() > 100 {
            self.image_cache.clear();
        }
    }
}

//
// Framework call order per frame:
//   1. `prepare()` , called with `&mut Self::Pipeline` directly.
//   2. `render()` , called with `&Self::Pipeline`.
//
// We use `Mutex<VecDeque<PreparedFrame>>` inside the pipeline so that
// `render()` can pop frames despite receiving `&Pipeline`.

impl primitive::Primitive for RoundedImagePrimitive {
    type Pipeline = RoundedImagePipeline;

    fn prepare(
        &self,
        pipeline: &mut RoundedImagePipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        pipeline.prepare(device, queue, self, bounds, viewport);
    }

    fn render(
        &self,
        pipeline: &RoundedImagePipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.render(encoder, target, clip_bounds);
    }
}
