//! The 3D renderer, plugged into Rune as its compositor: each frame draws the sky, the world,
//! players, mobs and effects with wgpu, then has Rune draw the HUD and menus on top.

use crate::game::{Mode, Screen, State};
use crate::mesh::{SectionKey, Vertex, tile_uv};
use runecraft_common::block::tile;
use runecraft_common::chunk::CHUNK;
use runecraft_common::protocol::MobKind;
use glam::{Mat4, Vec3, Vec4};
use rune::compose::{Compositor, DeviceKey, Frame};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    vp: [[f32; 4]; 4],
    inv_vp: [[f32; 4]; 4],
    cam: [f32; 4],
    sun: [f32; 4],
    sky_top: [f32; 4],
    sky_hor: [f32; 4],
    fog: [f32; 4],
    misc: [f32; 4],
}

struct SectionGpu {
    opaque: Option<(wgpu::Buffer, u32)>,
    water: Option<(wgpu::Buffer, u32)>,
}

struct Dynamic {
    buf: wgpu::Buffer,
    cap: u64,
}

impl Dynamic {
    fn write(slot: &mut Option<Dynamic>, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8], usage: wgpu::BufferUsages) {
        let need = (data.len() as u64).max(256).next_multiple_of(256);
        if slot.as_ref().is_none_or(|d| d.cap < need) {
            let cap = need.next_power_of_two();
            *slot = Some(Dynamic {
                buf: device.create_buffer(&wgpu::BufferDescriptor { label: Some("dynamic"), size: cap, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
                cap,
            });
        }
        if !data.is_empty() {
            queue.write_buffer(&slot.as_ref().unwrap().buf, 0, data);
        }
    }
}

struct Inner {
    key: DeviceKey,
    format: wgpu::TextureFormat,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    sky: wgpu::RenderPipeline,
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    index: wgpu::Buffer,
    index_quads: u32,
    depth: Option<(wgpu::TextureView, (u32, u32))>,
    sections: HashMap<SectionKey, SectionGpu>,
    entities: Option<Dynamic>,
    overlay: Option<Dynamic>,
    outline: Option<Dynamic>,
}

#[derive(Default)]
pub struct Gfx {
    inner: Option<Inner>,
}

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn quad_indices(quads: u32) -> Vec<u32> {
    (0..quads).flat_map(|q| [q * 4, q * 4 + 1, q * 4 + 2, q * 4, q * 4 + 2, q * 4 + 3]).collect()
}

impl Inner {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, key: DeviceKey, atlas: &[u8]) -> Inner {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("world"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()) });
        let size = crate::textures::ATLAS as u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            atlas,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 4), rows_per_image: Some(size) },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pixels"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("world"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4];
        let vlayout = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs };
        let alpha = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent::OVER,
        };
        let pipe = |label: &str, fs: &str, blend: Option<wgpu::BlendState>, write: bool, compare: wgpu::CompareFunction, topology: wgpu::PrimitiveTopology, cull: Option<wgpu::Face>, buffers: &[Option<wgpu::VertexBufferLayout>], vs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), compilation_options: Default::default(), buffers },
                primitive: wgpu::PrimitiveState { topology, cull_mode: cull, front_face: wgpu::FrontFace::Ccw, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let vl = [Some(vlayout.clone())];
        use wgpu::CompareFunction as C;
        use wgpu::PrimitiveTopology as T;
        let sky = pipe("sky", "fs_sky", None, false, C::Always, T::TriangleList, None, &[], "vs_sky");
        let opaque = pipe("opaque", "fs_main", None, true, C::Less, T::TriangleList, Some(wgpu::Face::Back), &vl, "vs_main");
        let blend = pipe("blend", "fs_blend", Some(alpha), false, C::LessEqual, T::TriangleList, None, &vl, "vs_main");
        let lines = pipe("lines", "fs_main", None, false, C::LessEqual, T::LineList, None, &vl, "vs_main");
        let index_quads = 1 << 16;
        let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quads"),
            contents: bytemuck::cast_slice(&quad_indices(index_quads)),
            usage: wgpu::BufferUsages::INDEX,
        });
        Inner {
            key,
            format,
            uniforms,
            bind,
            sky,
            opaque,
            blend,
            lines,
            index,
            index_quads,
            depth: None,
            sections: HashMap::new(),
            entities: None,
            overlay: None,
            outline: None,
        }
    }

    fn ensure_quads(&mut self, device: &wgpu::Device, quads: u32) {
        if quads <= self.index_quads {
            return;
        }
        let n = quads.next_power_of_two();
        self.index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("quads"), contents: bytemuck::cast_slice(&quad_indices(n)), usage: wgpu::BufferUsages::INDEX });
        self.index_quads = n;
    }

    fn depth_view(&mut self, device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
        if self.depth.as_ref().is_none_or(|d| d.1 != size) {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((tex.create_view(&wgpu::TextureViewDescriptor::default()), size));
        }
        self.depth.as_ref().unwrap().0.clone()
    }

    fn upload(&mut self, device: &wgpu::Device, s: &mut State) {
        for key in s.world.removed.drain(..) {
            self.sections.remove(&key);
        }
        let ready: Vec<_> = s.world.ready.drain(..).collect();
        for out in ready {
            let make = |v: &[Vertex]| -> Option<(wgpu::Buffer, u32)> {
                if v.is_empty() {
                    return None;
                }
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("section"), contents: bytemuck::cast_slice(v), usage: wgpu::BufferUsages::VERTEX });
                Some((buf, (v.len() / 4) as u32))
            };
            let quads = (out.opaque.len().max(out.water.len()) / 4) as u32;
            self.ensure_quads(device, quads);
            let g = SectionGpu { opaque: make(&out.opaque), water: make(&out.water) };
            if g.opaque.is_none() && g.water.is_none() {
                self.sections.remove(&out.key);
            } else {
                self.sections.insert(out.key, g);
            }
        }
    }
}

/// The six planes of the view frustum, for culling sections.
fn frustum(vp: Mat4) -> [Vec4; 6] {
    let r = [vp.row(0), vp.row(1), vp.row(2), vp.row(3)];
    [r[3] + r[0], r[3] - r[0], r[3] + r[1], r[3] - r[1], r[2], r[3] - r[2]]
}

fn visible(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> bool {
    planes.iter().all(|p| {
        let v = Vec3::new(if p.x >= 0.0 { max.x } else { min.x }, if p.y >= 0.0 { max.y } else { min.y }, if p.z >= 0.0 { max.z } else { min.z });
        p.truncate().dot(v) + p.w >= 0.0
    })
}

fn mixv(a: Vec3, b: Vec3, k: f32) -> Vec3 {
    a + (b - a) * k.clamp(0.0, 1.0)
}

impl Compositor<State> for Gfx {
    fn animating(&self) -> bool {
        true
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let key = f.device_key();
        if self.inner.as_ref().is_none_or(|i| i.key != key || i.format != f.format) {
            let atlas = f.rt.state.atlas.clone();
            self.inner = Some(Inner::new(f.device, f.queue, f.format, key, &atlas.rgba));
            // A new device has none of the old buffers: mesh everything again.
            let s = &mut f.rt.state;
            let cols: Vec<_> = s.world.columns.iter().map(|(k, c)| (*k, (**c).clone())).collect();
            s.world.clear();
            for ((cx, cz), c) in cols {
                s.world.insert(cx, cz, c);
            }
        }
        let inner = self.inner.as_mut().unwrap();
        let size = f.size;
        {
            let s = &mut f.rt.state;
            s.screen_px = size;
        }
        inner.upload(f.device, &mut f.rt.state);
        let depth = inner.depth_view(f.device, size);
        let playing = f.rt.state.screen == Screen::Playing;

        if playing {
            let s = &f.rt.state;
            let vp = s.view_proj;
            let eye = s.eye();
            let tday = s.time_of_day;
            let ang = tday * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let sun = Vec3::new(ang.cos(), ang.sin(), 0.25).normalize();
            let day = s.daylight();
            let dusk = (1.0 - (sun.y.abs() * 4.0)).clamp(0.0, 1.0);
            let mut top = mixv(Vec3::new(0.02, 0.025, 0.07), Vec3::new(0.32, 0.56, 0.98), day);
            let mut hor = mixv(Vec3::new(0.05, 0.06, 0.12), Vec3::new(0.72, 0.84, 1.0), day);
            hor = mixv(hor, Vec3::new(1.0, 0.55, 0.3), dusk * 0.6);
            top = mixv(top, Vec3::new(0.35, 0.3, 0.55), dusk * 0.3);
            let underwater = {
                let e = eye.floor().as_ivec3();
                s.world.get(e.x, e.y, e.z) == runecraft_common::block::WATER
            };
            let in_lava = {
                let e = eye.floor().as_ivec3();
                s.world.get(e.x, e.y, e.z) == runecraft_common::block::LAVA
            };
            let far = (s.view_distance as f32 - 1.0) * CHUNK as f32;
            let (fog, near, far) = if in_lava {
                (Vec3::new(0.9, 0.3, 0.05), 0.0, 3.0)
            } else if underwater {
                (Vec3::new(0.08, 0.2, 0.45) * day.max(0.3), 2.0, 28.0)
            } else {
                (hor, far * 0.55, far)
            };
            let flash = s.explosions.iter().map(|(p, at)| ((1.0 - (s.t - at) as f32 * 3.0).max(0.0) * (1.0 - p.distance(eye) / 40.0).max(0.0)).min(0.6)).fold(0.0f32, f32::max);
            let u = Uniforms {
                vp: vp.to_cols_array_2d(),
                inv_vp: vp.inverse().to_cols_array_2d(),
                cam: [eye.x, eye.y, eye.z, s.t as f32],
                sun: [sun.x, sun.y, sun.z, day],
                sky_top: top.extend(1.0).to_array(),
                sky_hor: [hor.x, hor.y, hor.z, far],
                fog: [fog.x, fog.y, fog.z, near],
                misc: [if inner.format.is_srgb() { 1.0 } else { 0.0 }, if underwater || in_lava { 1.0 } else { 0.0 }, flash, 0.0],
            };
            f.queue.write_buffer(&inner.uniforms, 0, bytemuck::bytes_of(&u));

            let ents = entity_vertices(s);
            let overlay = overlay_vertices(s);
            let outline = outline_vertices(s);
            inner.ensure_quads(f.device, (ents.len().max(overlay.len()) / 4) as u32);
            Dynamic::write(&mut inner.entities, f.device, f.queue, bytemuck::cast_slice(&ents), wgpu::BufferUsages::VERTEX);
            Dynamic::write(&mut inner.overlay, f.device, f.queue, bytemuck::cast_slice(&overlay), wgpu::BufferUsages::VERTEX);
            Dynamic::write(&mut inner.outline, f.device, f.queue, bytemuck::cast_slice(&outline), wgpu::BufferUsages::VERTEX);

            let planes = frustum(vp);
            let mut order: Vec<(&SectionKey, &SectionGpu, f32)> = inner
                .sections
                .iter()
                .filter_map(|(k, g)| {
                    let min = Vec3::new((k.0 * CHUNK) as f32, (k.1 * CHUNK) as f32, (k.2 * CHUNK) as f32);
                    let max = min + Vec3::splat(CHUNK as f32);
                    let c = (min + max) * 0.5;
                    let d = c.distance(eye);
                    (d < far + 24.0 && visible(&planes, min, max)).then_some((k, g, d))
                })
                .collect();
            order.sort_by(|a, b| a.2.total_cmp(&b.2));

            let fogc = wgpu::Color { r: fog.x as f64, g: fog.y as f64, b: fog.z as f64, a: 1.0 };
            let mut pass = f.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: f.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(fogc), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &inner.bind, &[]);
            pass.set_pipeline(&inner.sky);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&inner.opaque);
            pass.set_index_buffer(inner.index.slice(..), wgpu::IndexFormat::Uint32);
            for (_, g, _) in &order {
                if let Some((buf, quads)) = &g.opaque {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..quads * 6, 0, 0..1);
                }
            }
            if !ents.is_empty() {
                pass.set_vertex_buffer(0, inner.entities.as_ref().unwrap().buf.slice(..));
                pass.draw_indexed(0..(ents.len() / 4) as u32 * 6, 0, 0..1);
            }
            pass.set_pipeline(&inner.blend);
            for (_, g, _) in order.iter().rev() {
                if let Some((buf, quads)) = &g.water {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..quads * 6, 0, 0..1);
                }
            }
            if !overlay.is_empty() {
                pass.set_vertex_buffer(0, inner.overlay.as_ref().unwrap().buf.slice(..));
                pass.draw_indexed(0..(overlay.len() / 4) as u32 * 6, 0, 0..1);
            }
            if !outline.is_empty() {
                pass.set_pipeline(&inner.lines);
                pass.set_vertex_buffer(0, inner.outline.as_ref().unwrap().buf.slice(..));
                pass.draw(0..outline.len() as u32, 0..1);
            }
        } else {
            let _ = f.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: f.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.06, b: 0.1, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        // Rune's HUD and menus on top.
        let n = f.rt.instances.len() as u32;
        f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        f.ui.encode_layers(f.encoder);
        f.ui.draw_target_range_with(f.encoder, f.target, f.format, wgpu::LoadOp::Load, 0..n, &mut |_, _| {});
    }
}

// ---------------------------------------------------------------------------------------------
// Entities

const FACES: [[[f32; 3]; 4]; 6] = [
    [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
    [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0]],
    [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
    [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
    [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
    [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
];
const SHADE: [f32; 6] = [0.62, 0.62, 1.0, 0.5, 0.8, 0.8];

/// A box from `min` to `min + size` in model space, faces +X, -X, +Y, -Y, +Z, -Z.
fn push_box(out: &mut Vec<Vertex>, m: Mat4, min: Vec3, size: Vec3, tiles: [u8; 6], light: [u8; 2], flags: u8) {
    for d in 0..6 {
        let mut quad = [Vertex { pos: [0.0; 3], uv: [0.0; 2], light: [0; 4] }; 4];
        for (k, c) in FACES[d].iter().enumerate() {
            let local = min + Vec3::from(*c) * size;
            let p = m.transform_point3(local);
            let (u, v) = match d / 2 {
                0 => (c[2], 1.0 - c[1]),
                1 => (c[0], c[2]),
                _ => (if d == 5 { 1.0 - c[0] } else { c[0] }, 1.0 - c[1]),
            };
            quad[k] = Vertex { pos: p.to_array(), uv: tile_uv(tiles[d], u, v), light: [light[0], light[1], (SHADE[d] * 255.0) as u8, flags] };
        }
        out.extend_from_slice(&quad);
    }
}

fn part(base: Mat4, pivot: Vec3, angle_x: f32) -> Mat4 {
    base * Mat4::from_translation(pivot) * Mat4::from_rotation_x(angle_x) * Mat4::from_translation(-pivot)
}

fn light_at(s: &State, p: Vec3) -> [u8; 2] {
    let b = p.floor().as_ivec3();
    let sky = if s.world.sky_exposed(b.x, b.y + 1, b.z) { 255 } else { 140 };
    [sky, 40]
}

fn humanoid(out: &mut Vec<Vertex>, pos: Vec3, yaw: f32, pitch: f32, walk: f32, speed: f32, skin: u8, zombie: bool, light: [u8; 2], flags: u8) {
    let base = Mat4::from_translation(pos) * Mat4::from_rotation_y(-yaw);
    let swing = walk.sin() * 0.7 * (speed / 4.0).min(1.0);
    let (skin_t, face, shirt, pants, hair) = if zombie {
        (tile::ZOMBIE, tile::ZOMBIE_FACE, tile::SHIRT + 2, tile::PANTS, tile::ZOMBIE)
    } else {
        let i = skin % 4;
        (tile::SKIN + i, tile::FACE + i, tile::SHIRT + (skin / 4) % 8, tile::PANTS, tile::HAIR)
    };
    let hip = Vec3::new(0.0, 0.75, 0.0);
    push_box(out, part(base, hip, swing), Vec3::new(-0.25, 0.0, -0.125), Vec3::new(0.25, 0.75, 0.25), [pants; 6], light, flags);
    push_box(out, part(base, hip, -swing), Vec3::new(0.0, 0.0, -0.125), Vec3::new(0.25, 0.75, 0.25), [pants; 6], light, flags);
    push_box(out, base, Vec3::new(-0.25, 0.75, -0.125), Vec3::new(0.5, 0.75, 0.25), [shirt; 6], light, flags);
    let shoulder = Vec3::new(0.0, 1.45, 0.0);
    let arm = if zombie { std::f32::consts::FRAC_PI_2 } else { 0.0 };
    let arm_tiles = [shirt, shirt, shirt, skin_t, shirt, shirt];
    push_box(out, part(base, shoulder, arm - swing), Vec3::new(-0.5, 0.75, -0.125), Vec3::new(0.25, 0.75, 0.25), arm_tiles, light, flags);
    push_box(out, part(base, shoulder, arm + swing), Vec3::new(0.25, 0.75, -0.125), Vec3::new(0.25, 0.75, 0.25), arm_tiles, light, flags);
    let neck = Vec3::new(0.0, 1.5, 0.0);
    push_box(out, part(base, neck, pitch), Vec3::new(-0.25, 1.5, -0.25), Vec3::new(0.5, 0.5, 0.5), [skin_t, skin_t, hair, skin_t, hair, face], light, flags);
}

fn quadruped(out: &mut Vec<Vertex>, pos: Vec3, yaw: f32, walk: f32, speed: f32, sheep: bool, light: [u8; 2], flags: u8) {
    // Mobs face +X at yaw 0 on the server (atan2(z, x)); turn the model to match.
    let base = Mat4::from_translation(pos) * Mat4::from_rotation_y(-yaw - std::f32::consts::FRAC_PI_2);
    let swing = walk.sin() * 0.6 * (speed / 2.0).min(1.0);
    let (body, face, leg) = if sheep { (tile::SHEEP, tile::SHEEP_FACE, tile::SHEEP_FACE) } else { (tile::PIG, tile::PIG_FACE, tile::PIG) };
    let w = if sheep { 0.7 } else { 0.6 };
    push_box(out, base, Vec3::new(-w / 2.0, 0.35, -0.45), Vec3::new(w, if sheep { 0.6 } else { 0.5 }, 0.9), [body; 6], light, flags);
    push_box(out, base, Vec3::new(-0.22, 0.5, -0.8), Vec3::new(0.44, 0.44, 0.38), [body, body, body, body, body, face], light, flags);
    let hip = 0.35;
    for (i, (x, z)) in [(-0.25, -0.35), (0.05, -0.35), (-0.25, 0.25), (0.05, 0.25)].iter().enumerate() {
        let a = if i % 3 == 0 { swing } else { -swing };
        push_box(out, part(base, Vec3::new(0.0, hip, *z + 0.1), a), Vec3::new(*x, 0.0, *z), Vec3::new(0.2, hip, 0.2), [leg; 6], light, flags);
    }
}

fn entity_vertices(s: &State) -> Vec<Vertex> {
    let mut out = Vec::new();
    for o in s.others.values() {
        humanoid(&mut out, o.pos, o.yaw, o.pitch, o.walk, o.speed, o.skin, false, light_at(s, o.pos), 0);
    }
    for m in s.mobs.values() {
        let flags = if m.hurt { 4 } else { 0 };
        let light = light_at(s, m.pos);
        match m.kind {
            MobKind::Zombie => humanoid(&mut out, m.pos, m.yaw + std::f32::consts::FRAC_PI_2, 0.0, m.walk, m.speed, 0, true, light, flags),
            MobKind::Pig => quadruped(&mut out, m.pos, m.yaw, m.walk, m.speed, false, light, flags),
            MobKind::Sheep => quadruped(&mut out, m.pos, m.yaw, m.walk, m.speed, true, light, flags),
        }
    }
    out
}

/// The cracks on a block being mined.
fn overlay_vertices(s: &State) -> Vec<Vertex> {
    let mut out = Vec::new();
    if let Some((p, prog)) = s.mining
        && s.mode == Mode::Survival
    {
        let stage = ((prog * 10.0) as u8).min(9);
        let e = 0.004;
        push_box(&mut out, Mat4::IDENTITY, p.as_vec3() - Vec3::splat(e), Vec3::splat(1.0 + 2.0 * e), [tile::CRACK + stage; 6], [255, 0], 3);
    }
    out
}

/// The outline around the targeted block.
fn outline_vertices(s: &State) -> Vec<Vertex> {
    let Some(hit) = s.target else { return Vec::new() };
    if s.panel != crate::game::Panel::None {
        return Vec::new();
    }
    let e = 0.003;
    let (a, b) = (hit.pos.as_vec3() - Vec3::splat(e), hit.pos.as_vec3() + Vec3::splat(1.0 + e));
    let c = |x: bool, y: bool, z: bool| Vec3::new(if x { b.x } else { a.x }, if y { b.y } else { a.y }, if z { b.z } else { a.z });
    let mut edges = Vec::new();
    for i in 0..4 {
        let (x, z) = (i & 1 == 1, i & 2 == 2);
        edges.push((c(x, false, z), c(x, true, z)));
        let (p, q) = (i & 1 == 1, i & 2 == 2);
        edges.push((c(false, p, q), c(true, p, q)));
        edges.push((c(p, q, false), c(p, q, true)));
    }
    let uv = tile_uv(tile::BLACK, 0.5, 0.5);
    edges
        .into_iter()
        .flat_map(|(p, q)| [p, q])
        .map(|p| Vertex { pos: p.to_array(), uv, light: [255, 255, 255, 3] })
        .collect()
}
