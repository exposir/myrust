//! GPU display-list scene.
//!
//! One instanced-quad pipeline draws every command kind: rounded boxes and
//! circles as signed-distance shapes, lines as capsules, linear gradients
//! through a small LUT texture, and text, SVG paths and shadows as coverage
//! masks from an R8 atlas. The scene owns no window; callers hand it a target
//! view and a [`Damage`](super::Damage) region and record the frame into their
//! own encoder.

use std::collections::HashMap;

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder, Extent3d,
    FragmentState, MultisampleState, Origin3d, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, TextureView, TextureViewDescriptor, VertexState,
};

use super::{Damage, RenderError, RenderSize, RenderStats, TransformMatrix};
use crate::font::{default_font, FontFace};
use crate::image::{ImageFormat, ImageHandle, ImageTextureKey};
use crate::layout::Point;
use crate::paint::{
    Color, DisplayList, DrawCommand, GradientStop, ImageSourceRect, Radius, Rect,
};
use crate::svg::parse_svg_path;
use crate::text::{layout_text_shaped, TextLayout};

/// Floats per instance: nine four-float vectors.
pub(crate) const FLOATS: usize = 36;
const ATLAS_SIZE: u32 = 2048;
const LUT_WIDTH: u32 = 1024;
const LUT_ROWS: u32 = 256;
const MAX_IMAGE_CACHE_BYTES: u64 = 64 * 1024 * 1024;

const SHADER: &str = r#"
struct Uniforms {
    size: vec2<f32>,
    pad: vec2<f32>,
};

@group(0) @binding(0) var atlas_tex: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;
@group(0) @binding(2) var lut_tex: texture_2d<f32>;
@group(0) @binding(3) var lut_sampler: sampler;
@group(0) @binding(4) var<uniform> uniforms: Uniforms;
@group(0) @binding(5) var image_tex: texture_2d<f32>;
@group(0) @binding(6) var image_sampler: sampler;

const TARGET_SRGB: bool = TARGET_SRGB_VALUE;

// Instance colors are sRGB-encoded bytes. An sRGB target format encodes
// shader output again, so convert to linear first; non-sRGB targets are
// written as-is, matching how the CPU and Canvas paths store pixels.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / vec3<f32>(12.92);
    let high = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

fn target_color(c: vec3<f32>) -> vec3<f32> {
    if (TARGET_SRGB) {
        return srgb_to_linear(c);
    }
    return c;
}

struct Instance {
    @location(0) origin_axis_x: vec4<f32>,
    @location(1) axis_y_rect: vec4<f32>,
    @location(2) rect_wh_radii: vec4<f32>,
    @location(3) radii_color: vec4<f32>,
    @location(4) color_uv: vec4<f32>,
    @location(5) uv_grad: vec4<f32>,
    @location(6) grad_end_kind: vec4<f32>,
    @location(7) pad0: vec4<f32>,
    @location(8) pad1: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) uv: vec2<f32>,
    @location(5) grad: vec4<f32>,
    @location(6) params: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: Instance) -> VertexOut {
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    let origin = instance.origin_axis_x.xy;
    let axis_x = instance.origin_axis_x.zw;
    let axis_y = instance.axis_y_rect.xy;
    let rect = vec4<f32>(instance.axis_y_rect.zw, instance.rect_wh_radii.xy);
    let uv0 = instance.color_uv.zw;
    let uv1 = instance.uv_grad.xy;
    let device = origin + axis_x * corner.x + axis_y * corner.y;

    var out: VertexOut;
    out.position = vec4<f32>(
        device.x / uniforms.size.x * 2.0 - 1.0,
        1.0 - device.y / uniforms.size.y * 2.0,
        0.0,
        1.0,
    );
    out.local = rect.xy + rect.zw * corner;
    out.rect = rect;
    out.radii = vec4<f32>(instance.rect_wh_radii.zw, instance.radii_color.xy);
    out.color = vec4<f32>(instance.radii_color.zw, instance.color_uv.xy);
    out.uv = mix(uv0, uv1, corner);
    out.grad = vec4<f32>(instance.uv_grad.zw, instance.grad_end_kind.xy);
    out.params = vec2<f32>(instance.grad_end_kind.z, instance.grad_end_kind.w);
    return out;
}

fn sd_round_box(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let half = rect.zw * 0.5;
    let q = p - (rect.xy + half);
    var radius = radii.x;
    if (q.x > 0.0) {
        radius = radii.y;
    }
    if (q.y > 0.0) {
        radius = select(radii.w, radii.z, q.x > 0.0);
    }
    let r = clamp(radius, 0.0, min(half.x, half.y));
    let d = abs(q) - (half - vec2<f32>(r, r));
    return length(max(d, vec2<f32>(0.0, 0.0))) + min(max(d.x, d.y), 0.0) - r;
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.000001), 0.0, 1.0);
    return length(pa - ba * h);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let kind = u32(input.params.x + 0.5);
    if (kind == 0u) {
        let d = sd_round_box(input.local, input.rect, input.radii);
        let aa = max(fwidth(d), 0.0001);
        let coverage = clamp(0.5 - d / aa, 0.0, 1.0);
        return vec4<f32>(target_color(input.color.rgb), input.color.a * coverage);
    }
    if (kind == 1u) {
        let d = sd_segment(input.position.xy, input.grad.xy, input.grad.zw) - input.params.y;
        let aa = max(fwidth(d), 0.0001);
        let coverage = clamp(0.5 - d / aa, 0.0, 1.0);
        return vec4<f32>(target_color(input.color.rgb), input.color.a * coverage);
    }
    if (kind == 2u) {
        let direction = input.grad.zw - input.grad.xy;
        let length_squared = max(dot(direction, direction), 0.000001);
        let t = clamp(dot(input.local - input.grad.xy, direction) / length_squared, 0.0, 1.0);
        let sampled = textureSample(lut_tex, lut_sampler, vec2<f32>(t, (input.params.y + 0.5) / LUT_ROWS_CONST));
        return vec4<f32>(target_color(sampled.rgb), sampled.a * input.color.a);
    }
    if (kind == 4u) {
        let sampled = textureSample(image_tex, image_sampler, clamp(input.uv, vec2<f32>(0.0), vec2<f32>(1.0)));
        let gray = dot(sampled.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        var rgb = sampled.rgb;
        var alpha = sampled.a;
        if (input.params.y > 0.5) {
            rgb = vec3<f32>(gray);
        }
        if (input.params.y > 1.5) {
            rgb = vec3<f32>(sampled.r);
            alpha = 1.0;
        }
        let d = sd_round_box(input.local, input.rect, input.radii);
        let aa = max(fwidth(d), 0.0001);
        let coverage = clamp(0.5 - d / aa, 0.0, 1.0);
        return vec4<f32>(target_color(rgb), alpha * coverage);
    }
    let mask = textureSample(atlas_tex, atlas_sampler, input.uv).r;
    return vec4<f32>(target_color(input.color.rgb), input.color.a * mask);
}
"#;

const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) index: u32) -> VertexOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let corner = corners[index];
    var out: VertexOut;
    out.position = vec4<f32>(corner, 0.0, 1.0);
    out.uv = vec2<f32>((corner.x + 1.0) * 0.5, 1.0 - (corner.y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs_blit(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(source, source_sampler, input.uv);
}
"#;

/// One batch of instances sharing a scissor rectangle and image texture.
#[derive(Debug, Clone)]
struct Batch {
    scissor: (u32, u32, u32, u32),
    first: u32,
    count: u32,
    image: Option<ImageTextureKey>,
}

/// A mask returned by a rasterization function.
struct Mask {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
    bearing_x: i32,
    bearing_y: i32,
}

/// Where a mask sits in the atlas.
#[derive(Debug, Clone, Copy)]
struct MaskEntry {
    uv: [f32; 4],
    width: u32,
    height: u32,
    bearing_x: i32,
    bearing_y: i32,
}

/// Keys the masks in the atlas. Content equality is enough: a changed text
/// run or path becomes another key, and the damage diff already covers the
/// pixels that changed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MaskKey {
    Glyph {
        font_id: u64,
        glyph_id: u16,
        size_quarters: u32,
        weight: u16,
    },
    SvgFill {
        path: String,
        width: u32,
        height: u32,
        scale_quarters: u32,
    },
    SvgStroke {
        path: String,
        width: u32,
        height: u32,
        scale_quarters: u32,
        stroke_quarters: u32,
    },
    Shadow {
        width: u32,
        height: u32,
        radius_quarters: u32,
        blur_quarters: u32,
    },
}

/// Shelf-packed R8 coverage atlas.
struct MaskAtlas {
    texture: Texture,
    view: TextureView,
    generation: u64,
    x: u32,
    y: u32,
    row_height: u32,
    entries: HashMap<MaskKey, MaskEntry>,
}

impl MaskAtlas {
    fn new(device: &wgpu::Device) -> Self {
        let (texture, view) = new_atlas_texture(device);
        Self {
            texture,
            view,
            generation: 0,
            x: 0,
            y: 0,
            row_height: 0,
            entries: HashMap::new(),
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn reset(&mut self, device: &wgpu::Device) {
        let (texture, view) = new_atlas_texture(device);
        self.texture = texture;
        self.view = view;
        self.generation = self.generation.wrapping_add(1);
        self.x = 0;
        self.y = 0;
        self.row_height = 0;
        self.entries.clear();
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 || width > ATLAS_SIZE || height > ATLAS_SIZE {
            return None;
        }
        if self.x + width > ATLAS_SIZE {
            self.y = self.y.saturating_add(self.row_height);
            self.x = 0;
            self.row_height = 0;
        }
        if self.y + height > ATLAS_SIZE {
            return None;
        }
        let position = (self.x, self.y);
        self.x += width + 1;
        self.row_height = self.row_height.max(height);
        Some(position)
    }

    fn get(&self, key: &MaskKey) -> Option<MaskEntry> {
        self.entries.get(key).copied()
    }

    fn entry(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: MaskKey,
        mask: Mask,
    ) -> Option<MaskEntry> {
        if let Some(entry) = self.entries.get(&key) {
            return Some(*entry);
        }
        let (x, y) = match self.allocate(mask.width, mask.height) {
            Some(position) => position,
            None => {
                self.reset(device);
                self.allocate(mask.width, mask.height)?
            }
        };
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d { x, y, z: 0 },
                aspect: TextureAspect::All,
            },
            &mask.coverage,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mask.width),
                rows_per_image: Some(mask.height),
            },
            Extent3d {
                width: mask.width,
                height: mask.height,
                depth_or_array_layers: 1,
            },
        );
        let size = ATLAS_SIZE as f32;
        let entry = MaskEntry {
            uv: [
                (x as f32 + 0.5) / size,
                (y as f32 + 0.5) / size,
                (x + mask.width) as f32 / size - 0.5 / size,
                (y + mask.height) as f32 / size - 0.5 / size,
            ],
            width: mask.width,
            height: mask.height,
            bearing_x: mask.bearing_x,
            bearing_y: mask.bearing_y,
        };
        self.entries.insert(key, entry);
        Some(entry)
    }
}

fn new_atlas_texture(device: &wgpu::Device) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("myrust-ui-mask-atlas"),
        size: Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

/// Rows of linear-gradient LUTs in one RGBA texture.
struct GradientCache {
    texture: Texture,
    view: TextureView,
    generation: u64,
    next_row: u32,
    rows: HashMap<Vec<u32>, u32>,
}

impl GradientCache {
    fn new(device: &wgpu::Device) -> Self {
        let (texture, view) = new_lut_texture(device);
        Self {
            texture,
            view,
            generation: 0,
            next_row: 0,
            rows: HashMap::new(),
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn reset(&mut self, device: &wgpu::Device) {
        let (texture, view) = new_lut_texture(device);
        self.texture = texture;
        self.view = view;
        self.generation = self.generation.wrapping_add(1);
        self.next_row = 0;
        self.rows.clear();
    }

    fn row(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, stops: &[GradientStop]) -> u32 {
        let key: Vec<u32> = stops
            .iter()
            .flat_map(|stop| {
                let color = stop.color;
                let offset = stop.offset.to_bits();
                [offset, u32::from(color.r), u32::from(color.g), u32::from(color.b), u32::from(color.a)]
            })
            .collect();
        if let Some(row) = self.rows.get(&key) {
            return *row;
        }
        if self.next_row >= LUT_ROWS {
            self.reset(device);
        }
        let row = self.next_row;
        self.next_row += 1;
        let mut data = vec![0_u8; (LUT_WIDTH * 4) as usize];
        for index in 0..LUT_WIDTH {
            let t = index as f32 / (LUT_WIDTH - 1) as f32;
            let color = sample_stops(stops, t);
            let base = index as usize * 4;
            data[base] = color.r;
            data[base + 1] = color.g;
            data[base + 2] = color.b;
            data[base + 3] = color.a;
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d { x: 0, y: row, z: 0 },
                aspect: TextureAspect::All,
            },
            &data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(LUT_WIDTH * 4),
                rows_per_image: Some(1),
            },
            Extent3d {
                width: LUT_WIDTH,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.rows.insert(key, row);
        row
    }
}

fn new_lut_texture(device: &wgpu::Device) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("myrust-ui-gradient-lut"),
        size: Extent3d {
            width: LUT_WIDTH,
            height: LUT_ROWS,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

fn new_fallback_image(device: &wgpu::Device) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("myrust-ui-image-fallback"),
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

struct ImageTextureEntry {
    _texture: Texture,
    view: TextureView,
}

/// Decoded image textures keyed by source id, content version and format.
///
/// The cache is byte-bounded. When a new image would overflow it, every entry
/// is dropped and the generation advances. The caller then re-emits the whole
/// frame so no already-emitted instance points at a recycled texture.
struct ImageTextureCache {
    entries: HashMap<ImageTextureKey, ImageTextureEntry>,
    bytes: u64,
    generation: u64,
    revision: u64,
    capacity: u64,
}

impl ImageTextureCache {
    fn new() -> Self {
        Self::with_capacity(MAX_IMAGE_CACHE_BYTES)
    }

    fn with_capacity(capacity: u64) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            generation: 0,
            revision: 0,
            capacity,
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn reset(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.generation = self.generation.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
    }

    fn texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &ImageHandle,
        grayscale: bool,
    ) -> Result<ImageTextureKey, RenderError> {
        let key = source.cache_key(grayscale).ok_or_else(|| {
            RenderError::BackendFailure {
                message: format!("image '{}' carries no decoded pixels", source.as_str()),
            }
        })?;
        if self.entries.contains_key(&key) {
            return Ok(key);
        }
        let data = source.data().ok_or_else(|| RenderError::BackendFailure {
            message: format!("image '{}' carries no decoded pixels", source.as_str()),
        })?;
        let max_dimension = device.limits().max_texture_dimension_2d;
        if data.width() > max_dimension || data.height() > max_dimension {
            return Err(RenderError::BackendFailure {
                message: format!(
                    "image '{}' is {}x{}, exceeding the GPU texture limit {max_dimension}",
                    source.as_str(),
                    data.width(),
                    data.height()
                ),
            });
        }
        let bytes = match key.format {
            ImageFormat::R8 => u64::from(data.width()) * u64::from(data.height()),
            ImageFormat::Rgba8 => {
                u64::from(data.width()) * u64::from(data.height()) * 4
            }
        };
        if bytes > self.capacity {
            return Err(RenderError::BackendFailure {
                message: format!(
                    "image '{}' needs {bytes} bytes, exceeding the {} byte image cache",
                    source.as_str(),
                    self.capacity
                ),
            });
        }
        if self.bytes.saturating_add(bytes) > self.capacity {
            self.reset();
        }

        let format = match key.format {
            ImageFormat::R8 => TextureFormat::R8Unorm,
            ImageFormat::Rgba8 => TextureFormat::Rgba8Unorm,
        };
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("myrust-ui-image"),
            size: Extent3d {
                width: data.width(),
                height: data.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        match key.format {
            ImageFormat::R8 => {
                let mut luma = Vec::with_capacity(bytes as usize);
                for pixel in data.pixels().as_chunks::<4>().0.iter() {
                    let value = 0.2126 * f32::from(pixel[0])
                        + 0.7152 * f32::from(pixel[1])
                        + 0.0722 * f32::from(pixel[2]);
                    luma.push(value.round().clamp(0.0, 255.0) as u8);
                }
                queue.write_texture(
                    TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    &luma,
                    TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(data.width()),
                        rows_per_image: Some(data.height()),
                    },
                    Extent3d {
                        width: data.width(),
                        height: data.height(),
                        depth_or_array_layers: 1,
                    },
                );
            }
            ImageFormat::Rgba8 => {
                queue.write_texture(
                    TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    data.pixels(),
                    TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(data.width() * 4),
                        rows_per_image: Some(data.height()),
                    },
                    Extent3d {
                        width: data.width(),
                        height: data.height(),
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.insert(
            key.clone(),
            ImageTextureEntry {
                _texture: texture,
                view,
            },
        );
        self.revision = self.revision.wrapping_add(1);
        Ok(key)
    }

    fn get(&self, key: &ImageTextureKey) -> Option<&ImageTextureEntry> {
        self.entries.get(key)
    }
}

/// GPU renderer for one target format.
pub struct GpuScene {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: RenderPipeline,
    blit_pipeline: RenderPipeline,
    layout: BindGroupLayout,
    blit_layout: BindGroupLayout,
    atlas: MaskAtlas,
    gradients: GradientCache,
    images: ImageTextureCache,
    _fallback_image: Texture,
    fallback_image_view: TextureView,
    image_sampler: wgpu::Sampler,
    srgb_target: bool,
    bind_group: Option<BindGroup>,
    image_bind_groups: HashMap<ImageTextureKey, BindGroup>,
    bound_generations: (u64, u64, u64, u64),
    uniform: wgpu::Buffer,
    instances: Vec<f32>,
    batches: Vec<Batch>,
    active_scissor: Option<(u32, u32, u32, u32)>,
    active_image: Option<ImageTextureKey>,
}

impl GpuScene {
    /// Builds the pipelines and caches for a target format.
    ///
    /// Callers record [`GpuScene::render`] into their own encoder and then
    /// [`GpuScene::blit`] the rendered texture onto a window surface.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: TextureFormat) -> Self {
        let atlas = MaskAtlas::new(device);
        let gradients = GradientCache::new(device);
        let images = ImageTextureCache::new();
        let (fallback_image, fallback_image_view) = new_fallback_image(device);
        let image_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("myrust-ui-image-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..SamplerDescriptor::default()
        });
        let layout = create_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("myrust-ui-scene-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let mut scene_source = SHADER.replace("LUT_ROWS_CONST", &format!("{LUT_ROWS}.0"));
        scene_source = scene_source.replace(
            "TARGET_SRGB_VALUE",
            if format.is_srgb() { "true" } else { "false" },
        );
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("myrust-ui-scene-shader"),
            source: ShaderSource::Wgsl(scene_source.into()),
        });
        let pipeline = create_pipeline(
            device,
            &shader,
            &pipeline_layout,
            format,
            "vs_main",
            "fs_main",
            true,
        );

        let blit_layout = create_blit_layout(device);
        let blit_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("myrust-ui-blit-layout"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("myrust-ui-blit-shader"),
            source: ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let blit_pipeline = create_pipeline(
            device,
            &blit_shader,
            &blit_pipeline_layout,
            format,
            "vs_blit",
            "fs_blit",
            false,
        );

        let uniform = device.create_buffer(&BufferDescriptor {
            label: Some("myrust-ui-scene-uniforms"),
            size: 16,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut scene = Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            blit_pipeline,
            layout,
            blit_layout,
            atlas,
            gradients,
            images,
            _fallback_image: fallback_image,
            fallback_image_view,
            image_sampler,
            srgb_target: format.is_srgb(),
            bind_group: None,
            image_bind_groups: HashMap::new(),
            bound_generations: (u64::MAX, u64::MAX, u64::MAX, u64::MAX),
            uniform,
            instances: Vec::new(),
            batches: Vec::new(),
            active_scissor: None,
            active_image: None,
        };
        scene.ensure_bind_groups();
        scene
    }

    /// Renders a display list into `target`, redrawing only `damage`.
    pub fn render(
        &mut self,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        size: RenderSize,
        display: &DisplayList,
        damage: &Damage,
        background: Color,
    ) -> Result<RenderStats, RenderError> {
        let mut stats = super::analyze_display_list(display).unwrap_or_default();
        if size.is_empty() {
            return Ok(stats);
        }
        self.instances.clear();
        self.batches.clear();
        self.active_scissor = None;
        self.active_image = None;

        let mut full = damage.is_full();
        if !full && damage.rects().is_empty() {
            stats.draw_calls = 0;
            stats.drawn_vertices = 0;
            return Ok(stats);
        }
        // A full mask atlas or gradient LUT recycles slots that instances
        // already emitted in this frame point at, so the whole frame has to be
        // emitted again against the fresh textures. Two attempts bound the
        // work; if the frame still does not fit, fail it so the window can
        // draw that frame on the CPU instead of showing recycled masks.
        let mut settled = false;
        for _ in 0..2 {
            let atlas_generation = self.atlas.generation();
            let gradient_generation = self.gradients.generation();
            let image_generation = self.images.generation();
            if full {
                self.emit(display, size, None, background)?;
            } else {
                for rect in damage.rects() {
                    self.emit(display, size, Some(*rect), background)?;
                }
            }
            settled = self.atlas.generation() == atlas_generation
                && self.gradients.generation() == gradient_generation
                && self.images.generation() == image_generation;
            if settled {
                break;
            }
            self.instances.clear();
            self.batches.clear();
            self.active_scissor = None;
            self.active_image = None;
            full = true;
        }
        if !settled {
            return Err(RenderError::BackendFailure {
                message: "display list exceeds the GPU mask, gradient or image cache for one frame"
                    .to_owned(),
            });
        }

        self.ensure_bind_groups();
        let mut uniform_bytes = [0_u8; 16];
        uniform_bytes[0..4].copy_from_slice(&(size.width as f32).to_le_bytes());
        uniform_bytes[4..8].copy_from_slice(&(size.height as f32).to_le_bytes());
        self.queue.write_buffer(&self.uniform, 0, &uniform_bytes);

        let vertex_buffer = if self.instances.is_empty() {
            None
        } else {
            let bytes: Vec<u8> = self
                .instances
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            let buffer = self.device.create_buffer(&BufferDescriptor {
                label: Some("myrust-ui-scene-instances"),
                size: bytes.len() as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buffer, 0, &bytes);
            Some(buffer)
        };

        let load = if full {
            wgpu::LoadOp::Clear(clear_color(background, self.srgb_target))
        } else {
            wgpu::LoadOp::Load
        };
        let attachment = Some(RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        });
        let attachments = [attachment];
        let descriptor = RenderPassDescriptor {
            label: Some("myrust-ui-scene-pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        };
        {
            let mut pass = encoder.begin_render_pass(&descriptor);
            pass.set_pipeline(&self.pipeline);
            if let (Some(buffer), Some(bind_group)) = (&vertex_buffer, &self.bind_group) {
                pass.set_vertex_buffer(0, buffer.slice(..));
                for batch in &self.batches {
                    let batch_bind_group = batch
                        .image
                        .as_ref()
                        .and_then(|key| self.image_bind_groups.get(key))
                        .unwrap_or(bind_group);
                    pass.set_bind_group(0, batch_bind_group, &[]);
                    pass.set_scissor_rect(
                        batch.scissor.0,
                        batch.scissor.1,
                        batch.scissor.2,
                        batch.scissor.3,
                    );
                    pass.draw(0..4, batch.first..batch.first + batch.count);
                }
            }
        }
        stats.draw_calls = self.batches.len();
        stats.drawn_vertices = self.instances.len() / FLOATS * 4;
        Ok(stats)
    }

    /// Copies the rendered target onto the window surface.
    pub fn blit(&mut self, encoder: &mut CommandEncoder, source: &TextureView, target: &TextureView) {
        let sampler = self.device.create_sampler(&SamplerDescriptor {
            label: Some("myrust-ui-blit-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..SamplerDescriptor::default()
        });
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("myrust-ui-blit-bind"),
            layout: &self.blit_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(source),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let attachment = Some(RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        });
        let attachments = [attachment];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("myrust-ui-blit-pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.blit_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn ensure_bind_groups(&mut self) {
        let generations = (
            self.atlas.generation(),
            self.gradients.generation(),
            self.images.generation(),
            self.images.revision(),
        );
        if self.bind_group.is_some() && self.bound_generations == generations {
            return;
        }
        self.image_bind_groups.clear();
        let sampler = self.device.create_sampler(&SamplerDescriptor {
            label: Some("myrust-ui-scene-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..SamplerDescriptor::default()
        });
        let lut_sampler = self.device.create_sampler(&SamplerDescriptor {
            label: Some("myrust-ui-lut-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..SamplerDescriptor::default()
        });
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("myrust-ui-scene-bind"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&self.atlas.view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&self.gradients.view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&lut_sampler),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::Buffer(self.uniform.as_entire_buffer_binding()),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: BindingResource::TextureView(&self.fallback_image_view),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: BindingResource::Sampler(&self.image_sampler),
                },
            ],
        });
        let keys: Vec<ImageTextureKey> = self
            .batches
            .iter()
            .filter_map(|batch| batch.image.clone())
            .collect();
        let mut image_groups = Vec::with_capacity(keys.len());
        for key in keys {
            let Some(entry) = self.images.get(&key) else {
                continue;
            };
            let group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("myrust-ui-image-bind"),
                layout: &self.layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&self.atlas.view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&sampler),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(&self.gradients.view),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Sampler(&lut_sampler),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::Buffer(
                            self.uniform.as_entire_buffer_binding(),
                        ),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: BindingResource::TextureView(&entry.view),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: BindingResource::Sampler(&self.image_sampler),
                    },
                ],
            });
            image_groups.push((key, group));
        }
        self.bind_group = Some(bind_group);
        self.image_bind_groups.extend(image_groups);
        self.bound_generations = generations;
    }

    fn emit(
        &mut self,
        display: &DisplayList,
        size: RenderSize,
        damage: Option<Rect>,
        background: Color,
    ) -> Result<(), RenderError> {
        let frame = Rect::new(0.0, 0.0, size.width as f32, size.height as f32);
        if let Some(rect) = damage {
            if let Some(scissor) = pixel_scissor(rect, size) {
                self.begin_batch(scissor, None);
                // Erase the pixel-aligned damage rectangle, not its float
                // bounds: an antialiased edge would leave old partial pixels
                // along the boundary.
                let aligned = Rect::new(
                    scissor.0 as f32,
                    scissor.1 as f32,
                    scissor.2 as f32,
                    scissor.3 as f32,
                );
                self.push_box(
                    TransformMatrix::IDENTITY,
                    aligned,
                    Radius::all(0.0),
                    background,
                );
            }
        }
        let mut transform = TransformMatrix::IDENTITY;
        let mut transforms = Vec::new();
        let mut clip: Option<Rect> = None;
        let mut clips = Vec::new();

        for command in &display.commands {
            match command {
                DrawCommand::TransformPush {
                    transform: local, ..
                } => {
                    transforms.push(transform);
                    transform = transform.then(TransformMatrix::from_transform(*local));
                }
                DrawCommand::TransformPop => {
                    if let Some(previous) = transforms.pop() {
                        transform = previous;
                    }
                }
                DrawCommand::ScissorPush { rect } => {
                    clips.push(clip);
                    let transformed = transform.transform_rect(*rect);
                    clip = Some(match clip {
                        Some(current) => super::intersect_rects(current, transformed),
                        None => transformed,
                    });
                }
                DrawCommand::ScissorPop => {
                    if let Some(previous) = clips.pop() {
                        clip = previous;
                    }
                }
                DrawCommand::Image {
                    x,
                    y,
                    width,
                    height,
                    source,
                    source_rect,
                    radius,
                    grayscale,
                } => {
                    if !source.is_resolved() {
                        continue;
                    }
                    let Some(bounds) =
                        super::damage::device_bounds(command, transform)
                    else {
                        continue;
                    };
                    let limiter = damage.unwrap_or(frame);
                    if bounds.intersection(limiter).is_none() {
                        continue;
                    }
                    let scissor_rect = match clip {
                        Some(clip) => match limiter.intersection(clip) {
                            Some(rect) => rect,
                            None => continue,
                        },
                        None => limiter,
                    };
                    let Some(scissor) = pixel_scissor(scissor_rect, size) else {
                        continue;
                    };
                    let key = self
                        .images
                        .texture(&self.device, &self.queue, source, *grayscale)?;
                    self.begin_batch(scissor, Some(key.clone()));
                    self.push_image(
                        transform,
                        Rect::new(*x, *y, *width, *height),
                        *source_rect,
                        *radius,
                        *grayscale,
                        key.format,
                    );
                }
                _ => {
                    let Some(bounds) = super::damage::device_bounds(command, transform) else {
                        continue;
                    };
                    let limiter = damage.unwrap_or(frame);
                    if bounds.intersection(limiter).is_none() {
                        continue;
                    }
                    let scissor_rect = match clip {
                        Some(clip) => match limiter.intersection(clip) {
                            Some(rect) => rect,
                            None => continue,
                        },
                        None => limiter,
                    };
                    let Some(scissor) = pixel_scissor(scissor_rect, size) else {
                        continue;
                    };
                    self.begin_batch(scissor, None);
                    self.emit_command(command, transform);
                }
            }
        }
        Ok(())
    }

    fn emit_command(&mut self, command: &DrawCommand, transform: TransformMatrix) {
        match command {
            DrawCommand::Rect {
                x,
                y,
                width,
                height,
                color,
            } => self.push_box(
                transform,
                Rect::new(*x, *y, *width, *height),
                Radius::all(0.0),
                *color,
            ),
            DrawCommand::RoundedRect {
                x,
                y,
                width,
                height,
                radius,
                color,
            } => self.push_box(
                transform,
                Rect::new(*x, *y, *width, *height),
                *radius,
                *color,
            ),
            DrawCommand::Circle {
                x,
                y,
                radius,
                color,
            } => self.push_box(
                transform,
                Rect::new(*x - *radius, *y - *radius, *radius * 2.0, *radius * 2.0),
                Radius::all(*radius),
                *color,
            ),
            DrawCommand::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                color,
            } => {
                let scale = transform.scale_factor();
                self.push_line(
                    transform,
                    Point::new(*x1, *y1),
                    Point::new(*x2, *y2),
                    width * scale / 2.0,
                    *color,
                );
            }
            DrawCommand::LinearGradient {
                x,
                y,
                width,
                height,
                radius,
                start,
                end,
                stops,
            } => {
                if stops.is_empty() {
                    return;
                }
                let mut ordered = stops.clone();
                ordered.sort_by(|left, right| left.offset.total_cmp(&right.offset));
                let row = self.gradients.row(&self.device, &self.queue, &ordered);
                self.push_gradient(
                    transform,
                    Rect::new(*x, *y, *width, *height),
                    *radius,
                    *start,
                    *end,
                    row,
                );
            }
            DrawCommand::Text {
                x,
                y,
                text,
                font_size,
                weight,
                italic,
                mono,
                color,
                max_width,
                alignment,
                ..
            } => {
                self.push_text(
                    transform,
                    *x,
                    *y,
                    text,
                    *font_size,
                    *weight,
                    *italic,
                    *mono,
                    *color,
                    *max_width,
                    *alignment,
                );
            }
            DrawCommand::SvgPath {
                x,
                y,
                width,
                height,
                path,
                fill,
                stroke,
                stroke_width,
            } => {
                if let Some(fill) = fill {
                    self.push_svg(
                        transform,
                        Rect::new(*x, *y, *width, *height),
                        path,
                        None,
                        *fill,
                    );
                }
                if let Some(stroke) = stroke {
                    self.push_svg(
                        transform,
                        Rect::new(*x, *y, *width, *height),
                        path,
                        Some(*stroke_width),
                        *stroke,
                    );
                }
            }
            DrawCommand::Shadow {
                x,
                y,
                width,
                height,
                radius,
                offset_x,
                offset_y,
                blur,
                color,
            } => self.push_shadow(
                transform,
                Rect::new(*x, *y, *width, *height),
                *radius,
                Point::new(*offset_x, *offset_y),
                *blur,
                *color,
            ),
            DrawCommand::Image { .. }
            | DrawCommand::ScissorPush { .. }
            | DrawCommand::ScissorPop
            | DrawCommand::TransformPush { .. }
            | DrawCommand::TransformPop => {}
        }
    }

    fn begin_batch(
        &mut self,
        scissor: (u32, u32, u32, u32),
        image: Option<ImageTextureKey>,
    ) {
        if self.active_scissor == Some(scissor) && self.active_image == image {
            return;
        }
        self.batches.push(Batch {
            scissor,
            first: (self.instances.len() / FLOATS) as u32,
            count: 0,
            image: image.clone(),
        });
        self.active_scissor = Some(scissor);
        self.active_image = image;
    }

    #[allow(clippy::too_many_arguments)]
    fn push_instance(
        &mut self,
        origin: [f32; 2],
        axis_x: [f32; 2],
        axis_y: [f32; 2],
        rect: [f32; 4],
        radii: [f32; 4],
        color: Color,
        uv: [f32; 4],
        gradient: [f32; 4],
        kind: f32,
        param: f32,
    ) {
        let alpha = f32::from(color.a) / 255.0;
        let values = [
            origin[0], origin[1], axis_x[0], axis_x[1],
            axis_y[0], axis_y[1], rect[0], rect[1],
            rect[2], rect[3], radii[0], radii[1],
            radii[2], radii[3], f32::from(color.r) / 255.0, f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0, alpha, uv[0], uv[1],
            uv[2], uv[3], gradient[0], gradient[1],
            gradient[2], gradient[3], kind, param,
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
        ];
        debug_assert_eq!(values.len(), FLOATS);
        self.instances.extend_from_slice(&values);
        if let Some(batch) = self.batches.last_mut() {
            batch.count += 1;
        }
    }

    fn push_box(&mut self, transform: TransformMatrix, rect: Rect, radius: Radius, color: Color) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let maximum = rect.width.min(rect.height) / 2.0;
        let radii = [
            radius.top_left.clamp(0.0, maximum),
            radius.top_right.clamp(0.0, maximum),
            radius.bottom_right.clamp(0.0, maximum),
            radius.bottom_left.clamp(0.0, maximum),
        ];
        let (origin, axis_x, axis_y) = quad_axes(transform, rect);
        self.push_instance(
            origin,
            axis_x,
            axis_y,
            [rect.x, rect.y, rect.width, rect.height],
            radii,
            color,
            [0.0; 4],
            [0.0; 4],
            0.0,
            0.0,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_image(
        &mut self,
        transform: TransformMatrix,
        rect: Rect,
        source_rect: ImageSourceRect,
        radius: Radius,
        grayscale: bool,
        format: ImageFormat,
    ) {
        if rect.width <= 0.0
            || rect.height <= 0.0
            || source_rect.width <= 0.0
            || source_rect.height <= 0.0
        {
            return;
        }
        let maximum = rect.width.min(rect.height) / 2.0;
        let radii = [
            radius.top_left.clamp(0.0, maximum),
            radius.top_right.clamp(0.0, maximum),
            radius.bottom_right.clamp(0.0, maximum),
            radius.bottom_left.clamp(0.0, maximum),
        ];
        let (origin, axis_x, axis_y) = quad_axes(transform, rect);
        let param = match (grayscale, format) {
            (true, ImageFormat::R8) => 2.0,
            (true, ImageFormat::Rgba8) => 1.0,
            (false, _) => 0.0,
        };
        self.push_instance(
            origin,
            axis_x,
            axis_y,
            [rect.x, rect.y, rect.width, rect.height],
            radii,
            Color::WHITE,
            [
                source_rect.x,
                source_rect.y,
                source_rect.x + source_rect.width,
                source_rect.y + source_rect.height,
            ],
            [0.0; 4],
            4.0,
            param,
        );
    }

    fn push_line(
        &mut self,
        transform: TransformMatrix,
        start: Point,
        end: Point,
        half_width: f32,
        color: Color,
    ) {
        if half_width <= 0.0 {
            return;
        }
        let a = transform.apply(start);
        let b = transform.apply(end);
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length <= f32::EPSILON {
            return;
        }
        let nx = -dy / length * half_width;
        let ny = dx / length * half_width;
        let origin = [a.x + nx, a.y + ny];
        let axis_x = [b.x - a.x, b.y - a.y];
        let axis_y = [-2.0 * nx, -2.0 * ny];
        self.push_instance(
            origin,
            axis_x,
            axis_y,
            [0.0; 4],
            [0.0; 4],
            color,
            [0.0; 4],
            [a.x, a.y, b.x, b.y],
            1.0,
            half_width,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_gradient(
        &mut self,
        transform: TransformMatrix,
        rect: Rect,
        radius: Radius,
        start: Point,
        end: Point,
        row: u32,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let maximum = rect.width.min(rect.height) / 2.0;
        let radii = [
            radius.top_left.clamp(0.0, maximum),
            radius.top_right.clamp(0.0, maximum),
            radius.bottom_right.clamp(0.0, maximum),
            radius.bottom_left.clamp(0.0, maximum),
        ];
        let (origin, axis_x, axis_y) = quad_axes(transform, rect);
        let start_px = [
            rect.x + start.x * rect.width,
            rect.y + start.y * rect.height,
        ];
        let end_px = [
            rect.x + end.x * rect.width,
            rect.y + end.y * rect.height,
        ];
        self.push_instance(
            origin,
            axis_x,
            axis_y,
            [rect.x, rect.y, rect.width, rect.height],
            radii,
            Color::WHITE,
            [0.0; 4],
            [start_px[0], start_px[1], end_px[0], end_px[1]],
            2.0,
            row as f32,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text(
        &mut self,
        transform: TransformMatrix,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        weight: u16,
        italic: bool,
        mono: bool,
        color: Color,
        max_width: Option<f32>,
        alignment: crate::paint::TextAlignment,
    ) {
        let Some(layout) = layout_text_shaped(text, font_size, max_width, alignment, mono) else {
            return;
        };
        let scale = transform.scale_factor().max(0.01);
        self.push_text_layout(transform, x, y, layout.as_ref(), weight, italic, color, scale);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_layout(
        &mut self,
        transform: TransformMatrix,
        x: f32,
        y: f32,
        layout: &TextLayout,
        weight: u16,
        italic: bool,
        color: Color,
        scale: f32,
    ) {
        let size_quarters = ((layout.font_size * scale * 4.0).round() as u32).max(4);
        for (line_index, line) in layout.lines.iter().enumerate() {
            let baseline = y + layout.baseline(line_index);
            let mut pen = x + line.x_offset;
            for run in &line.runs {
                for glyph in &run.glyphs {
                    let key = MaskKey::Glyph {
                        font_id: run.font.id(),
                        glyph_id: glyph.glyph_id,
                        size_quarters,
                        weight,
                    };
                    let entry = match self.atlas.get(&key) {
                        Some(entry) => entry,
                        None => {
                            let Some(mask) = glyph_mask(
                                &run.font,
                                glyph.glyph_id,
                                size_quarters as f32 / 4.0,
                                weight,
                            ) else {
                                pen += glyph.advance;
                                continue;
                            };
                            let Some(entry) =
                                self.atlas.entry(&self.device, &self.queue, key, mask)
                            else {
                                pen += glyph.advance;
                                continue;
                            };
                            entry
                        }
                    };
                    let local_x = pen + glyph.offset_x + entry.bearing_x as f32 / scale;
                    let local_y = baseline - glyph.offset_y + entry.bearing_y as f32 / scale;
                    if entry.width > 0 && entry.height > 0 {
                        let width = entry.width as f32 / scale;
                        let height = entry.height as f32 / scale;
                        let (origin, axis_x, axis_y) = if italic {
                            let slant = 0.2;
                            let sheared =
                                Point::new(local_x + slant * (baseline - local_y), local_y);
                            let origin = transform.apply(sheared);
                            (
                                [origin.x, origin.y],
                                transform.linear(Point::new(width, 0.0)),
                                transform.linear(Point::new(-slant * height, height)),
                            )
                        } else {
                            let origin = transform.apply(Point::new(local_x, local_y));
                            (
                                [origin.x, origin.y],
                                transform.linear(Point::new(width, 0.0)),
                                transform.linear(Point::new(0.0, height)),
                            )
                        };
                        self.push_instance(
                            origin,
                            [axis_x.x, axis_x.y],
                            [axis_y.x, axis_y.y],
                            [0.0, 0.0, 1.0, 1.0],
                            [0.0; 4],
                            color,
                            entry.uv,
                            [0.0; 4],
                            3.0,
                            0.0,
                        );
                    }
                    pen += glyph.advance;
                }
            }
        }
    }

    fn push_svg(
        &mut self,
        transform: TransformMatrix,
        rect: Rect,
        path: &str,
        stroke: Option<f32>,
        color: Color,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let scale = transform.scale_factor().clamp(0.25, 4.0);
        let scale_quarters = (scale * 4.0).round().max(1.0) as u32;
        let width = (rect.width * scale).ceil().max(1.0) as u32;
        let height = (rect.height * scale).ceil().max(1.0) as u32;
        if width > 2048 || height > 2048 {
            return;
        }
        let key = match stroke {
            Some(stroke_width) => MaskKey::SvgStroke {
                path: path.to_string(),
                width,
                height,
                scale_quarters,
                stroke_quarters: (stroke_width.max(0.0) * 4.0).round() as u32,
            },
            None => MaskKey::SvgFill {
                path: path.to_string(),
                width,
                height,
                scale_quarters,
            },
        };
        let entry = match self.atlas.get(&key) {
            Some(entry) => entry,
            None => {
                let Some(mask) = svg_mask(path, rect, scale_quarters as f32 / 4.0, stroke) else {
                    return;
                };
                let Some(entry) = self.atlas.entry(&self.device, &self.queue, key, mask) else {
                    return;
                };
                entry
            }
        };
        let (origin, axis_x, axis_y) = quad_axes(transform, rect);
        self.push_instance(
            origin,
            axis_x,
            axis_y,
            [0.0, 0.0, 1.0, 1.0],
            [0.0; 4],
            color,
            entry.uv,
            [0.0; 4],
            3.0,
            0.0,
        );
    }

    fn push_shadow(
        &mut self,
        transform: TransformMatrix,
        rect: Rect,
        radius: f32,
        offset: Point,
        blur: f32,
        color: Color,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || color.a == 0 {
            return;
        }
        let blur = blur.max(0.0);
        let scale = transform.scale_factor().clamp(0.25, 4.0);
        let reach = blur * 2.5 + 2.0;
        let local = Rect::new(
            rect.x + offset.x - reach,
            rect.y + offset.y - reach,
            rect.width + reach * 2.0,
            rect.height + reach * 2.0,
        );
        let width = (local.width * scale).ceil().max(1.0) as u32;
        let height = (local.height * scale).ceil().max(1.0) as u32;
        if width > 2048 || height > 2048 {
            return;
        }
        let key = MaskKey::Shadow {
            width,
            height,
            radius_quarters: (radius.max(0.0) * scale * 4.0).round() as u32,
            blur_quarters: (blur * scale * 4.0).round() as u32,
        };
        let entry = match self.atlas.get(&key) {
            Some(entry) => entry,
            None => {
                let Some(mask) = shadow_mask(
                    width,
                    height,
                    radius * scale,
                    blur * scale,
                    reach * scale,
                    rect.width * scale,
                    rect.height * scale,
                ) else {
                    return;
                };
                let Some(entry) = self.atlas.entry(&self.device, &self.queue, key, mask) else {
                    return;
                };
                entry
            }
        };
        let (origin, axis_x, axis_y) = quad_axes(transform, local);
        self.push_instance(
            origin,
            axis_x,
            axis_y,
            [0.0, 0.0, 1.0, 1.0],
            [0.0; 4],
            color,
            entry.uv,
            [0.0; 4],
            3.0,
            0.0,
        );
    }
}

fn quad_axes(
    transform: TransformMatrix,
    rect: Rect,
) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let origin = transform.apply(Point::new(rect.x, rect.y));
    let axis_x = transform.linear(Point::new(rect.width, 0.0));
    let axis_y = transform.linear(Point::new(0.0, rect.height));
    ([origin.x, origin.y], [axis_x.x, axis_x.y], [axis_y.x, axis_y.y])
}

fn pixel_scissor(rect: Rect, size: RenderSize) -> Option<(u32, u32, u32, u32)> {
    if !rect.is_valid() || rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let left = rect.x.max(0.0).floor() as u32;
    let top = rect.y.max(0.0).floor() as u32;
    let right = rect
        .right()
        .min(size.width as f32)
        .ceil()
        .clamp(0.0, size.width as f32) as u32;
    let bottom = rect
        .bottom()
        .min(size.height as f32)
        .ceil()
        .clamp(0.0, size.height as f32) as u32;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    (width > 0 && height > 0).then_some((left, top, width, height))
}

fn clear_color(color: Color, srgb_target: bool) -> wgpu::Color {
    // The shader encodes its output for an sRGB target format, so a clear
    // value has to be converted to linear the same way. Alpha stays linear.
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if srgb_target {
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        } else {
            value
        }
    };
    wgpu::Color {
        r: channel(color.r),
        g: channel(color.g),
        b: channel(color.b),
        a: f64::from(color.a) / 255.0,
    }
}

fn glyph_mask(font: &FontFace, glyph_id: u16, size: f32, weight: u16) -> Option<Mask> {
    let bitmap = font.rasterize_glyph_id(glyph_id, size);
    if bitmap.width == 0 || bitmap.height == 0 {
        return None;
    }
    if weight < 600 {
        return Some(Mask {
            width: bitmap.width,
            height: bitmap.height,
            coverage: bitmap.coverage,
            bearing_x: bitmap.bearing_x,
            bearing_y: bitmap.bearing_y,
        });
    }
    let offset = (size * 0.035).clamp(0.35, 1.0).round() as u32;
    let width = bitmap.width + offset;
    let mut coverage = vec![0_u8; width as usize * bitmap.height as usize];
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let value = bitmap.coverage[(y * bitmap.width + x) as usize];
            let index = y as usize * width as usize + x as usize;
            coverage[index] = coverage[index].max(value);
            coverage[index + offset as usize] = coverage[index + offset as usize].max(value);
        }
    }
    Some(Mask {
        width,
        height: bitmap.height,
        coverage,
        bearing_x: bitmap.bearing_x,
        bearing_y: bitmap.bearing_y,
    })
}

fn svg_mask(path: &str, viewport: Rect, scale: f32, stroke: Option<f32>) -> Option<Mask> {
    let width = (viewport.width * scale).ceil().max(1.0) as u32;
    let height = (viewport.height * scale).ceil().max(1.0) as u32;
    if width == 0 || height == 0 {
        return None;
    }
    let outline = parse_svg_path(path);
    if outline.contours.is_empty() {
        return None;
    }
    let mut coverage = vec![0_u8; width as usize * height as usize];
    let half_stroke = stroke.map(|width| width.max(0.0) / 2.0);
    for y in 0..height {
        for x in 0..width {
            let mut covered = 0_u32;
            for sy in 0..2 {
                for sx in 0..2 {
                    let px = viewport.x
                        + (x as f32 + (sx as f32 + 0.5) / 2.0) / scale;
                    let py = viewport.y
                        + (y as f32 + (sy as f32 + 0.5) / 2.0) / scale;
                    if let Some(half_stroke) = half_stroke {
                        if distance_to_contours(px, py, &outline.contours) <= half_stroke {
                            covered += 1;
                        }
                    } else if inside_contours(px, py, &outline.contours) {
                        covered += 1;
                    }
                }
            }
            coverage[(y * width + x) as usize] = (covered * 255 / 4) as u8;
        }
    }
    Some(Mask {
        width,
        height,
        coverage,
        bearing_x: 0,
        bearing_y: 0,
    })
}

fn inside_contours(x: f32, y: f32, contours: &[crate::svg::SvgContour]) -> bool {
    let mut inside = false;
    for contour in contours {
        let count = contour.points.len();
        if count < 2 {
            continue;
        }
        for index in 0..count {
            let first = contour.points[index];
            let second = contour.points[(index + 1) % count];
            if (first.y > y) != (second.y > y) {
                let crossing =
                    (second.x - first.x) * (y - first.y) / (second.y - first.y) + first.x;
                if x < crossing {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

fn distance_to_contours(x: f32, y: f32, contours: &[crate::svg::SvgContour]) -> f32 {
    let mut best = f32::INFINITY;
    for contour in contours {
        let count = contour.points.len();
        if count < 2 {
            continue;
        }
        for index in 0..count {
            let first = contour.points[index];
            let second = contour.points[(index + 1) % count];
            let dx = second.x - first.x;
            let dy = second.y - first.y;
            let length_squared = dx * dx + dy * dy;
            let amount = if length_squared <= f32::EPSILON {
                0.0
            } else {
                (((x - first.x) * dx + (y - first.y) * dy) / length_squared).clamp(0.0, 1.0)
            };
            let px = first.x + dx * amount;
            let py = first.y + dy * amount;
            best = best.min(((x - px) * (x - px) + (y - py) * (y - py)).sqrt());
        }
    }
    best
}

fn shadow_mask(
    width: u32,
    height: u32,
    radius: f32,
    blur: f32,
    reach: f32,
    shape_width: f32,
    shape_height: f32,
) -> Option<Mask> {
    if width == 0 || height == 0 {
        return None;
    }
    let rect = Rect::new(reach, reach, shape_width, shape_height);
    let maximum = shape_width.min(shape_height) / 2.0;
    let radii = [radius.clamp(0.0, maximum); 4];
    let mut mask = vec![0.0_f32; width as usize * height as usize];
    for y in 0..height {
        for x in 0..width {
            let mut covered = 0.0;
            for sy in 0..2 {
                for sx in 0..2 {
                    let px = x as f32 + (sx as f32 + 0.5) / 2.0;
                    let py = y as f32 + (sy as f32 + 0.5) / 2.0;
                    if crate::present::inside_rounded_rect(rect, radii, px, py) {
                        covered += 0.25;
                    }
                }
            }
            mask[(y * width + x) as usize] = covered;
        }
    }
    let pass = (blur * 0.5).round() as usize;
    if pass > 0 {
        for _ in 0..3 {
            crate::present::box_blur(&mut mask, width as usize, height as usize, pass);
        }
    }
    let mut coverage = vec![0_u8; mask.len()];
    for (index, value) in mask.iter().enumerate() {
        coverage[index] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    Some(Mask {
        width,
        height,
        coverage,
        bearing_x: 0,
        bearing_y: 0,
    })
}

fn sample_stops(stops: &[GradientStop], offset: f32) -> Color {
    let offset = offset.clamp(0.0, 1.0);
    let first = stops[0];
    if offset <= first.offset {
        return first.color;
    }
    for pair in stops.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if offset <= right.offset {
            let span = (right.offset - left.offset).max(f32::EPSILON);
            return mix_color(left.color, right.color, (offset - left.offset) / span);
        }
    }
    stops[stops.len() - 1].color
}

fn mix_color(left: Color, right: Color, amount: f32) -> Color {
    let mix = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        mix(left.r, right.r),
        mix(left.g, right.g),
        mix(left.b, right.b),
        mix(left.a, right.a),
    )
}

fn create_bind_group_layout(device: &wgpu::Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("myrust-ui-scene-bind-layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_blit_layout(device: &wgpu::Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("myrust-ui-blit-bind-layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: TextureFormat,
    vertex_entry: &str,
    fragment_entry: &str,
    instanced: bool,
) -> RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
        5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
    ];
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: (FLOATS * std::mem::size_of::<f32>()) as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    };
    // The blit shader reads only `vertex_index`; declaring the instance
    // layout on it would make wgpu require a vertex buffer at draw time.
    let layouts = [Some(vertex_layout)];
    let buffers: &[Option<wgpu::VertexBufferLayout>] = if instanced { &layouts } else { &[] };
    let target = ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    };
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("myrust-ui-scene-pipeline"),
        layout: Some(layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            ..PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(target)],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageData;

    #[test]
    fn instance_stride_is_nine_vectors() {
        assert_eq!(FLOATS, 9 * 4);
    }

    #[test]
    fn gradient_rows_sample_stops() {
        let stops = [
            GradientStop {
                offset: 0.0,
                color: Color::BLACK,
            },
            GradientStop {
                offset: 1.0,
                color: Color::WHITE,
            },
        ];
        assert_eq!(sample_stops(&stops, 0.0), Color::BLACK);
        assert_eq!(sample_stops(&stops, 1.0), Color::WHITE);
        let middle = sample_stops(&stops, 0.5);
        assert!((120..=135).contains(&middle.r));
    }

    #[test]
    fn scene_wgsl_parses_and_validates() {
        let shader = SHADER
            .replace("LUT_ROWS_CONST", &format!("{LUT_ROWS}.0"))
            .replace("TARGET_SRGB_VALUE", "false");
        let module = wgpu::naga::front::wgsl::parse_str(&shader).expect("scene WGSL parses");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        validator.validate(&module).expect("scene WGSL validates");
        let module = wgpu::naga::front::wgsl::parse_str(BLIT_SHADER).expect("blit WGSL parses");
        validator.validate(&module).expect("blit WGSL validates");
    }

    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .ok()?;
        runtime.block_on(async {
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle(),
            );
            let adapter = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                    apply_limit_buckets: false,
                }),
            )
            .await
            .ok()?
            .ok()?;
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("myrust-ui-image-cache-test"),
                    ..wgpu::DeviceDescriptor::default()
                })
                .await
                .ok()
        })
    }

    #[test]
    fn image_cache_reset_advances_generation_and_drops_old_textures() {
        let Some((device, queue)) = test_device() else {
            return;
        };
        let pixels = [255_u8, 0, 0, 255];
        let first =
            ImageHandle::from_data("first", ImageData::new(&pixels, 1, 1)).unwrap();
        let second =
            ImageHandle::from_data("second", ImageData::new(&pixels, 1, 1)).unwrap();
        let mut cache = ImageTextureCache::with_capacity(4);
        let first_key = cache
            .texture(&device, &queue, &first, false)
            .expect("first image fits");
        assert!(cache.get(&first_key).is_some());
        let second_key = cache
            .texture(&device, &queue, &second, false)
            .expect("second image resets the cache");
        assert_eq!(cache.generation(), 1);
        assert!(cache.get(&first_key).is_none());
        assert!(cache.get(&second_key).is_some());
        assert_eq!(cache.bytes, 4);
    }
}
