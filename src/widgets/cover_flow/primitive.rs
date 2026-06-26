// SPDX-License-Identifier: MPL-2.0

//! Custom wgpu primitive that renders the Cover Flow scene: textured cards in
//! perspective with a depth buffer and a glossy reflection beneath each.
//!
//! Structure mirrors `image_container::rounded_primitive` (the proven custom
//! `iced_wgpu::primitive` integration in this crate): a `Pipeline` owns the GPU
//! state and a per-frame queue of prepared draws built in `prepare()` and
//! consumed in `render()`, which records its own render pass over the widget's
//! clip rect.

use std::collections::HashMap;
use std::sync::Mutex;

use bytemuck::{Pod, Zeroable};
use cosmic::iced::core::Rectangle;
use cosmic::iced::core::image as iced_image;
use iced_wgpu::graphics::Viewport;
use iced_wgpu::primitive;
use iced_wgpu::wgpu;

use super::camera::{self, HALF_H, HALF_W, Mat4, VISIBLE_SPAN};

const SHADER: &str = r#"
struct Card {
    mvp:       mat4x4<f32>,
    uv_offset: vec2<f32>,
    uv_scale:  vec2<f32>,
    tint:      vec4<f32>,
    // x: opacity, y: has_texture (0/1), z: is_reflection (0/1), w: dim
    params:    vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Card;
@group(1) @binding(0) var t_img: texture_2d<f32>;
@group(1) @binding(1) var s_img: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0)       uv:  vec2<f32>,
    @location(1)       ly:  f32,
};

// Unit quad in the card's local plane (two triangles), z = 0.
var<private> P: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
    vec2<f32>(-1.0,  1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    let p = P[idx];
    var out: VsOut;
    out.pos = u.mvp * vec4<f32>(p, 0.0, 1.0);
    // Texture row 0 is the top; local y = +1 is the top of the card.
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    out.ly = p.y;
    return out;
}

// All outputs are PREMULTIPLIED (rgb already scaled by alpha): the pipeline
// uses premultiplied-alpha blending, and Wayland window buffers are themselves
// premultiplied. This keeps each window's real transparency — transparent
// regions stay transparent (the blur/table shows through) rather than being
// forced opaque (which produced black) or composited onto white.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = u.params.z; // 0 card, 1 reflection, 2 floor/table

    // Floor / table: a solid band filling the lower part with a hard top edge.
    if kind > 1.5 {
        let g = step(0.55, 0.5 - in.ly * 0.5);
        let a = u.tint.a * g;
        return vec4<f32>(u.tint.rgb * a, a);
    }

    var prgb: vec3<f32>; // premultiplied rgb
    var alpha: f32;
    if u.params.y > 0.5 {
        let s = textureSample(t_img, s_img, u.uv_offset + in.uv * u.uv_scale);
        prgb = s.rgb; // already premultiplied
        alpha = s.a;
    } else {
        // Fallback: straight tint treated as opaque → premultiply.
        prgb = u.tint.rgb * u.tint.a;
        alpha = u.tint.a;
    }

    // Dim cards (darken) as they rotate away from the focal point.
    prgb = prgb * u.params.w;

    if kind > 0.5 {
        // Reflection: strongest at the top (touching the card), fading downward.
        let g = clamp(0.5 - in.ly * 0.5, 0.0, 1.0);
        let f = u.params.x * g * g * 0.30;
        return vec4<f32>(prgb * f, alpha * f);
    }

    // Card face: keep the window's real alpha (faded near the visible edge).
    let f = u.params.x;
    return vec4<f32>(prgb * f, alpha * f);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CardUniforms {
    mvp: [f32; 16],
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    tint: [f32; 4],
    params: [f32; 4],
}

/// One window's card in the scene.
#[derive(Debug, Clone)]
pub struct CoverCard {
    /// `None` → render the flat `tint` fallback (window has no thumbnail yet).
    pub handle: Option<iced_image::Handle>,
    pub tint: [f32; 4],
    /// Continuous offset from the focal point: `index - scroll`.
    pub d: f32,
}

#[derive(Debug)]
pub struct CoverFlowPrimitive {
    pub cards: Vec<CoverCard>,
    pub reflection: bool,
    /// Floor / table colour (RGBA, premultiplied-friendly straight colour).
    /// Alpha 0 disables the floor.
    pub table: [f32; 4],
}

// 0 = card face, 1 = reflection, 2 = floor/table.
const KIND_CARD: f32 = 0.0;
const KIND_REFLECTION: f32 = 1.0;
const KIND_FLOOR: f32 = 2.0;

struct PreparedCard {
    #[allow(dead_code)]
    buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    tex_bg: wgpu::BindGroup,
    /// 0 card, 1 reflection, 2 floor.
    kind: u8,
    z: f32,
}

pub struct CoverFlowPipeline {
    card_pipeline: wgpu::RenderPipeline,
    reflection_pipeline: wgpu::RenderPipeline,
    uniform_bgl: wgpu::BindGroupLayout,
    tex_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    dummy_view: wgpu::TextureView,
    cache: HashMap<u64, (wgpu::Texture, wgpu::TextureView)>,
    depth: Option<(wgpu::TextureView, (u32, u32))>,
    frames: Mutex<Vec<PreparedCard>>,
}

impl std::fmt::Debug for CoverFlowPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CoverFlowPipeline")
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

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

fn handle_rgba(handle: &iced_image::Handle) -> Option<(u32, u32, Vec<u8>)> {
    match handle {
        iced_image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => Some((*width, *height, pixels.as_ref().to_vec())),
        _ => None,
    }
}

impl CoverFlowPipeline {
    fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cover_flow shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cover_flow uniform bgl"),
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

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cover_flow tex bgl"),
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
            label: Some("cover_flow sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cover_flow layout"),
            bind_group_layouts: &[&uniform_bgl, &tex_bgl],
            immediate_size: 0,
        });

        let make_pipeline = |depth_write: bool, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
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
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: depth_write,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        let card_pipeline = make_pipeline(true, "cover_flow card pipeline");
        let reflection_pipeline = make_pipeline(false, "cover_flow reflection pipeline");

        // 1x1 texture so the texture bind group is always satisfiable, even for
        // fallback (no-thumbnail) cards.
        let dummy = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cover_flow dummy tex"),
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
        let dummy_view = dummy.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            card_pipeline,
            reflection_pipeline,
            uniform_bgl,
            tex_bgl,
            sampler,
            dummy_view,
            cache: HashMap::new(),
            depth: None,
            frames: Mutex::new(Vec::new()),
        }
    }

    /// Upload (once) and cache a texture for `handle`; returns its cache id.
    fn ensure_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        handle: &iced_image::Handle,
    ) -> Option<u64> {
        let id = handle_hash(handle);
        if self.cache.contains_key(&id) {
            return Some(id);
        }
        let (w, h, pixels) = handle_rgba(handle)?;
        if w == 0 || h == 0 {
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cover_flow card tex"),
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
        self.cache.insert(id, (texture, view));
        Some(id)
    }

    fn ensure_depth(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        let size = (size.0.max(1), size.1.max(1));
        if self.depth.as_ref().map(|(_, s)| *s) == Some(size) {
            return;
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cover_flow depth"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.depth = Some((view, size));
    }

    /// Texture bind group for the cached id, or the dummy when absent.
    fn make_tex_bg(&self, device: &wgpu::Device, tex_id: Option<u64>) -> wgpu::BindGroup {
        let view = tex_id
            .and_then(|id| self.cache.get(&id))
            .map(|(_, v)| v)
            .unwrap_or(&self.dummy_view);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cover_flow tex bg"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_card(
        &self,
        out: &mut Vec<PreparedCard>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mvp: Mat4,
        kind: u8,
        uv_offset: [f32; 2],
        uv_scale: [f32; 2],
        tint: [f32; 4],
        params: [f32; 4],
        tex_id: Option<u64>,
        z: f32,
    ) {
        let uniforms = CardUniforms {
            mvp,
            uv_offset,
            uv_scale,
            tint,
            params,
        };
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cover_flow uniform buf"),
            size: std::mem::size_of::<CardUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buf, 0, bytemuck::bytes_of(&uniforms));
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cover_flow uniform bg"),
            layout: &self.uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        out.push(PreparedCard {
            buf,
            uniform_bg,
            tex_bg: self.make_tex_bg(device, tex_id),
            kind,
            z,
        });
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prim: &CoverFlowPrimitive,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let phys = viewport.physical_size();
        self.ensure_depth(device, (phys.width, phys.height));

        let aspect = (bounds.width / bounds.height).max(0.01);
        let vp = camera::view_proj(aspect);

        let mut prepared = Vec::new();

        // Floor / table: a screen-space quad behind everything.
        if prim.table[3] > 0.0 {
            self.push_card(
                &mut prepared,
                device,
                queue,
                camera::identity_mat(),
                2,
                [0.0, 0.0],
                [1.0, 1.0],
                prim.table,
                [1.0, 0.0, KIND_FLOOR, 1.0],
                None,
                f32::INFINITY,
            );
        }

        for card in &prim.cards {
            let ad = card.d.abs();
            if ad > VISIBLE_SPAN {
                continue;
            }
            let p = camera::placement(card.d);
            let opacity = 1.0 - smoothstep(VISIBLE_SPAN - 1.5, VISIBLE_SPAN, ad);
            let dim = 1.0 - 0.42 * (ad.min(1.5) / 1.5);

            let tex_id = card
                .handle
                .as_ref()
                .and_then(|h| self.ensure_texture(device, queue, h));

            // Each card keeps the window's own aspect ratio (uniform height,
            // width = HALF_H * aspect) so tall/portrait windows aren't cropped.
            let (has_tex, half_w) = match tex_id {
                Some(id) => {
                    let sz = self.cache.get(&id).unwrap().0.size();
                    let ar = (sz.width as f32 / sz.height as f32).clamp(0.4, 2.6);
                    (1.0f32, HALF_H * ar)
                }
                None => (0.0f32, HALF_W), // fallback: default aspect
            };
            // Full-frame sampling; no cover-fit crop.
            let uv_offset = [0.0, 0.0];
            let uv_scale = [1.0, 1.0];

            self.push_card(
                &mut prepared,
                device,
                queue,
                camera::mul(vp, camera::card_model(&p, half_w)),
                0, // kind: card face
                uv_offset,
                uv_scale,
                card.tint,
                [opacity, has_tex, KIND_CARD, dim],
                tex_id,
                p.z,
            );
            if prim.reflection {
                self.push_card(
                    &mut prepared,
                    device,
                    queue,
                    camera::mul(vp, camera::reflection_model(&p, half_w)),
                    1,
                    uv_offset,
                    uv_scale,
                    card.tint,
                    [opacity, has_tex, KIND_REFLECTION, dim],
                    tex_id,
                    p.z,
                );
            }
        }

        *self.frames.lock().unwrap() = prepared;
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some((depth_view, _)) = self.depth.as_ref() else {
            return;
        };
        let mut frames = self.frames.lock().unwrap();
        if frames.is_empty() {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cover_flow pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );

        // Far -> near so alpha blends correctly (camera looks down -Z, so a more
        // negative z is further away).
        let mut order: Vec<usize> = (0..frames.len()).collect();
        order.sort_by(|&a, &b| {
            frames[a]
                .z
                .partial_cmp(&frames[b].z)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Floor/table behind everything (no depth write), then opaque cards
        // (write depth), then reflections (depth-test only). `kind` order: 2,0,1.
        for (pipeline, kind) in [
            (&self.reflection_pipeline, 2u8),
            (&self.card_pipeline, 0u8),
            (&self.reflection_pipeline, 1u8),
        ] {
            pass.set_pipeline(pipeline);
            for &i in &order {
                if frames[i].kind != kind {
                    continue;
                }
                pass.set_bind_group(0, &frames[i].uniform_bg, &[]);
                pass.set_bind_group(1, &frames[i].tex_bg, &[]);
                pass.draw(0..6, 0..1);
            }
        }

        drop(pass);
        frames.clear();
    }
}

impl primitive::Pipeline for CoverFlowPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        CoverFlowPipeline::build(device, format)
    }

    fn trim(&mut self) {
        if self.cache.len() > 64 {
            self.cache.clear();
        }
    }
}

impl primitive::Primitive for CoverFlowPrimitive {
    type Pipeline = CoverFlowPipeline;

    fn prepare(
        &self,
        pipeline: &mut CoverFlowPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        pipeline.prepare(device, queue, self, bounds, viewport);
    }

    fn render(
        &self,
        pipeline: &CoverFlowPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.render(encoder, target, clip_bounds);
    }
}
