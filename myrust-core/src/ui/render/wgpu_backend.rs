//! Optional wgpu offscreen renderer.
//!
//! The backend converts supported display-list geometry into transformed
//! triangle vertices and executes a real render pass into an offscreen RGBA
//! texture. It requires a working GPU or software graphics driver at runtime;
//! compiling the feature does not imply that one is present.

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use wgpu::{
    AdapterInfo, BlendState, BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, DeviceDescriptor, Extent3d, FragmentState, InstanceDescriptor,
    LoadOp, MultisampleState, Operations, PollType, PowerPreference, PrimitiveState,
    PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    RequestAdapterOptions, ShaderModuleDescriptor, ShaderSource, StoreOp, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, VertexState,
};

use super::{
    clip_bounds, color_to_array, command_bounds, include_bounds, intersect_rects, validate_size,
    RenderError, RenderSize, RenderStats, Renderer, RendererBackend, TransformMatrix,
};
use crate::ui::layout::Point;
use crate::ui::paint::{Color, DisplayList, DrawCommand, GradientStop, Radius, Rect};

const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;
const ADAPTER_TIMEOUT: Duration = Duration::from_secs(5);
const DEVICE_TIMEOUT: Duration = Duration::from_secs(5);
const RENDER_TIMEOUT: Duration = Duration::from_secs(5);
const VERTEX_BYTES: usize = std::mem::size_of::<[f32; 6]>();
const CIRCLE_SEGMENTS: usize = 64;
const CORNER_SEGMENTS: usize = 6;

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

/// Human-readable information about the selected wgpu adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    /// Adapter name.
    pub name: String,
    /// Graphics API backend.
    pub backend: String,
    /// Adapter device class.
    pub device_type: String,
    /// Driver name.
    pub driver: String,
    /// Driver-specific information.
    pub driver_info: String,
}

impl GpuInfo {
    fn from_adapter_info(info: &AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            backend: info.backend.to_str().to_owned(),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
        }
    }
}

/// Real wgpu renderer using an offscreen texture and triangle pipeline.
///
/// This renderer needs a graphics adapter and driver at runtime. Initialization
/// has bounded adapter/device timeouts and returns [`RenderError::NoAdapter`]
/// when no compatible adapter exists.
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    device_size: RenderSize,
    surface_size: RenderSize,
    gpu_info: GpuInfo,
    last_stats: RenderStats,
}

impl fmt::Debug for WgpuRenderer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WgpuRenderer")
            .field("device_size", &self.device_size)
            .field("surface_size", &self.surface_size)
            .field("gpu_info", &self.gpu_info)
            .finish_non_exhaustive()
    }
}

impl WgpuRenderer {
    /// Initializes an adapter, device, shader pipeline, and offscreen texture.
    ///
    /// This function blocks for at most ten seconds across adapter and device
    /// requests. It returns an explicit error rather than panicking when no
    /// graphics device is available.
    pub fn new(size: RenderSize) -> Result<Self, RenderError> {
        validate_size(size)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|error| RenderError::BackendFailure {
                message: format!("failed to create wgpu initialization runtime: {error}"),
            })?;
        runtime.block_on(Self::new_async(size))
    }

    /// Returns information about the selected GPU adapter.
    pub fn gpu_info(&self) -> &GpuInfo {
        &self.gpu_info
    }

    /// Returns statistics from the most recent successful render.
    pub fn last_stats(&self) -> &RenderStats {
        &self.last_stats
    }

    async fn new_async(size: RenderSize) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(InstanceDescriptor::new_without_display_handle());
        let adapter = tokio::time::timeout(
            ADAPTER_TIMEOUT,
            instance.request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            }),
        )
        .await
        .map_err(|_| RenderError::Timeout {
            operation: "request_adapter",
            milliseconds: ADAPTER_TIMEOUT.as_millis() as u64,
        })?
        .map_err(|_| RenderError::NoAdapter)?;

        let adapter_info = GpuInfo::from_adapter_info(&adapter.get_info());
        let (device, queue) = tokio::time::timeout(
            DEVICE_TIMEOUT,
            adapter.request_device(&DeviceDescriptor {
                label: Some("myrust-ui-device"),
                ..DeviceDescriptor::default()
            }),
        )
        .await
        .map_err(|_| RenderError::Timeout {
            operation: "request_device",
            milliseconds: DEVICE_TIMEOUT.as_millis() as u64,
        })?
        .map_err(|error| RenderError::DeviceRequest {
            message: error.to_string(),
        })?;

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("myrust-ui-shader"),
            source: ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });
        let pipeline = create_pipeline(&device, &shader);
        let (texture, view) = create_color_target(&device, size);

        Ok(Self {
            device,
            queue,
            pipeline,
            texture,
            view,
            device_size: size,
            surface_size: size,
            gpu_info: adapter_info,
            last_stats: RenderStats::default(),
        })
    }
}

impl Renderer for WgpuRenderer {
    fn backend(&self) -> RendererBackend {
        RendererBackend::Wgpu
    }

    fn device_size(&self) -> RenderSize {
        self.device_size
    }

    fn surface_size(&self) -> RenderSize {
        self.surface_size
    }

    fn resize(&mut self, size: RenderSize) -> Result<(), RenderError> {
        validate_size(size)?;
        let (texture, view) = create_color_target(&self.device, size);
        self.texture = texture;
        self.view = view;
        self.device_size = size;
        self.surface_size = size;
        self.last_stats = RenderStats::default();
        Ok(())
    }

    fn render(&mut self, display_list: &DisplayList) -> Result<RenderStats, RenderError> {
        validate_size(self.surface_size)?;
        let mut mesh = FrameMesh::build(display_list, self.surface_size)?;
        mesh.stats.drawn_vertices = mesh.vertex_count as usize;

        let vertex_buffer = if mesh.bytes.is_empty() {
            None
        } else {
            let buffer = self.device.create_buffer(&BufferDescriptor {
                label: Some("myrust-ui-vertices"),
                size: mesh.bytes.len() as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buffer, 0, &mesh.bytes);
            Some(buffer)
        };

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-ui-encoder"),
            });
        {
            let attachment = Some(RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            });
            let attachments = [attachment];
            let descriptor = RenderPassDescriptor {
                label: Some("myrust-ui-pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            };
            let mut render_pass = encoder.begin_render_pass(&descriptor);
            render_pass.set_pipeline(&self.pipeline);
            if let Some(buffer) = &vertex_buffer {
                render_pass.set_vertex_buffer(0, buffer.slice(..));
                for batch in &mesh.batches {
                    let (x, y, width, height) = match batch.scissor {
                        Some(rect) => match scissor_to_pixels(self.surface_size, rect) {
                            Some(scissor) => scissor,
                            None => continue,
                        },
                        None => (0, 0, self.surface_size.width, self.surface_size.height),
                    };
                    render_pass.set_scissor_rect(x, y, width, height);
                    render_pass.draw(
                        batch.first_vertex..batch.first_vertex + batch.vertex_count,
                        0..1,
                    );
                }
            }
        }

        let submission = self.queue.submit([encoder.finish()]);
        self.device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(RENDER_TIMEOUT),
            })
            .map_err(|error| RenderError::BackendFailure {
                message: format!("wgpu render poll failed: {error:?}"),
            })?;

        self.last_stats.clone_from(&mesh.stats);
        Ok(mesh.stats)
    }
}

fn create_pipeline(device: &wgpu::Device, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: VERTEX_BYTES as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    };
    let target = ColorTargetState {
        format: COLOR_FORMAT,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    };
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("myrust-ui-pipeline"),
        layout: None,
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(vertex_layout)],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            ..PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(target)],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_color_target(
    device: &wgpu::Device,
    size: RenderSize,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("myrust-ui-offscreen"),
        size: Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

#[derive(Debug, Clone, Copy)]
struct DrawBatch {
    first_vertex: u32,
    vertex_count: u32,
    scissor: Option<Rect>,
}

#[derive(Debug, Default)]
struct FrameMesh {
    bytes: Vec<u8>,
    vertex_count: u32,
    batches: Vec<DrawBatch>,
    stats: RenderStats,
}

impl FrameMesh {
    fn build(display_list: &DisplayList, size: RenderSize) -> Result<Self, RenderError> {
        let mut mesh = Self {
            stats: RenderStats {
                total_commands: display_list.len(),
                ..RenderStats::default()
            },
            ..Self::default()
        };
        let mut transform = TransformMatrix::IDENTITY;
        let mut transform_stack = Vec::new();
        let mut scissor = None;
        let mut scissor_stack = Vec::new();

        for command in &display_list.commands {
            mesh.stats.command_counts.record(command);
            match command {
                DrawCommand::TransformPush {
                    transform: local, ..
                } => {
                    transform_stack.push(transform);
                    transform = transform.then(TransformMatrix::from_transform(*local));
                    mesh.stats.max_transform_depth =
                        mesh.stats.max_transform_depth.max(transform_stack.len());
                }
                DrawCommand::TransformPop => {
                    transform = transform_stack.pop().ok_or(RenderError::UnbalancedState {
                        command: "TransformPop",
                    })?;
                }
                DrawCommand::ScissorPush { rect } => {
                    scissor_stack.push(scissor);
                    let transformed = transform.transform_rect(*rect);
                    scissor = Some(match scissor {
                        Some(current) => intersect_rects(current, transformed),
                        None => transformed,
                    });
                    mesh.stats.max_scissor_depth =
                        mesh.stats.max_scissor_depth.max(scissor_stack.len());
                }
                DrawCommand::ScissorPop => {
                    scissor = scissor_stack.pop().ok_or(RenderError::UnbalancedState {
                        command: "ScissorPop",
                    })?;
                }
                DrawCommand::Text { .. }
                | DrawCommand::SvgPath { .. }
                | DrawCommand::Image { .. } => {}
                _ => {
                    if let Some(bounds) = command_bounds(command) {
                        let transformed = transform.transform_rect(bounds);
                        if let Some(clipped) = clip_bounds(transformed, scissor) {
                            include_bounds(&mut mesh.stats.effective_bounds, clipped);
                        }
                    }
                    mesh.stats.effective_scissor = scissor;
                    if scissor.is_some_and(|rect| scissor_to_pixels(size, rect).is_none()) {
                        continue;
                    }
                    let first_vertex = mesh.vertex_count;
                    match command {
                        DrawCommand::Rect {
                            x,
                            y,
                            width,
                            height,
                            color,
                        } => mesh.add_rect(
                            transform,
                            Rect::new(*x, *y, *width, *height),
                            *color,
                            size,
                        ),
                        DrawCommand::RoundedRect {
                            x,
                            y,
                            width,
                            height,
                            radius,
                            color,
                        } => mesh.add_rounded_rect(
                            transform,
                            Rect::new(*x, *y, *width, *height),
                            *radius,
                            *color,
                            size,
                        ),
                        DrawCommand::Circle {
                            x,
                            y,
                            radius,
                            color,
                        } => mesh.add_circle(transform, Point::new(*x, *y), *radius, *color, size),
                        DrawCommand::Line {
                            x1,
                            y1,
                            x2,
                            y2,
                            width,
                            color,
                        } => mesh.add_line(
                            transform,
                            Point::new(*x1, *y1),
                            Point::new(*x2, *y2),
                            *width,
                            *color,
                            size,
                        ),
                        DrawCommand::LinearGradient {
                            x,
                            y,
                            width,
                            height,
                            start,
                            end,
                            stops,
                        } => mesh.add_gradient(
                            transform,
                            Rect::new(*x, *y, *width, *height),
                            *start,
                            *end,
                            stops,
                            size,
                        ),
                        _ => {}
                    }
                    if mesh.vertex_count > first_vertex {
                        mesh.batches.push(DrawBatch {
                            first_vertex,
                            vertex_count: mesh.vertex_count - first_vertex,
                            scissor,
                        });
                        mesh.stats.draw_calls += 1;
                    }
                }
            }
        }

        mesh.stats.unsupported_commands = mesh.stats.command_counts.unsupported();
        Ok(mesh)
    }

    fn add_rect(&mut self, transform: TransformMatrix, rect: Rect, color: Color, size: RenderSize) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let points = [
            Point::new(rect.x, rect.y),
            Point::new(rect.right(), rect.y),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.x, rect.bottom()),
        ];
        let color = color_to_array(color);
        self.add_triangle(
            transform,
            [points[0], points[1], points[2]],
            [color; 3],
            size,
        );
        self.add_triangle(
            transform,
            [points[0], points[2], points[3]],
            [color; 3],
            size,
        );
    }

    fn add_rounded_rect(
        &mut self,
        transform: TransformMatrix,
        rect: Rect,
        radius: Radius,
        color: Color,
        size: RenderSize,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let max_radius = rect.width.min(rect.height) / 2.0;
        let tl = radius.top_left.clamp(0.0, max_radius);
        let tr = radius.top_right.clamp(0.0, max_radius);
        let br = radius.bottom_right.clamp(0.0, max_radius);
        let bl = radius.bottom_left.clamp(0.0, max_radius);
        let segments = CORNER_SEGMENTS;
        let mut points = Vec::with_capacity((segments + 1) * 4);
        add_arc(
            &mut points,
            Point::new(rect.x + tl, rect.y + tl),
            tl,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            segments,
        );
        add_arc(
            &mut points,
            Point::new(rect.right() - tr, rect.y + tr),
            tr,
            std::f32::consts::PI * 1.5,
            std::f32::consts::PI * 2.0,
            segments,
        );
        add_arc(
            &mut points,
            Point::new(rect.right() - br, rect.bottom() - br),
            br,
            0.0,
            std::f32::consts::PI * 0.5,
            segments,
        );
        add_arc(
            &mut points,
            Point::new(rect.x + bl, rect.bottom() - bl),
            bl,
            std::f32::consts::PI * 0.5,
            std::f32::consts::PI,
            segments,
        );

        let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let color = color_to_array(color);
        for index in 0..points.len() {
            let next = (index + 1) % points.len();
            self.add_triangle(
                transform,
                [center, points[index], points[next]],
                [color; 3],
                size,
            );
        }
    }

    fn add_circle(
        &mut self,
        transform: TransformMatrix,
        center: Point,
        radius: f32,
        color: Color,
        size: RenderSize,
    ) {
        if radius <= 0.0 {
            return;
        }
        let color = color_to_array(color);
        for segment in 0..CIRCLE_SEGMENTS {
            let first_angle = segment as f32 * std::f32::consts::TAU / CIRCLE_SEGMENTS as f32;
            let second_angle =
                (segment + 1) as f32 * std::f32::consts::TAU / CIRCLE_SEGMENTS as f32;
            let first = Point::new(
                center.x + radius * first_angle.cos(),
                center.y + radius * first_angle.sin(),
            );
            let second = Point::new(
                center.x + radius * second_angle.cos(),
                center.y + radius * second_angle.sin(),
            );
            self.add_triangle(transform, [center, first, second], [color; 3], size);
        }
    }

    fn add_line(
        &mut self,
        transform: TransformMatrix,
        start: Point,
        end: Point,
        width: f32,
        color: Color,
        size: RenderSize,
    ) {
        if width <= 0.0 {
            return;
        }
        let delta = Point::new(end.x - start.x, end.y - start.y);
        let length = (delta.x * delta.x + delta.y * delta.y).sqrt();
        if length <= f32::EPSILON {
            return;
        }
        let half_width = width / 2.0;
        let normal = Point::new(
            -delta.y / length * half_width,
            delta.x / length * half_width,
        );
        let points = [
            Point::new(start.x + normal.x, start.y + normal.y),
            Point::new(end.x + normal.x, end.y + normal.y),
            Point::new(end.x - normal.x, end.y - normal.y),
            Point::new(start.x - normal.x, start.y - normal.y),
        ];
        let color = color_to_array(color);
        self.add_triangle(
            transform,
            [points[0], points[1], points[2]],
            [color; 3],
            size,
        );
        self.add_triangle(
            transform,
            [points[0], points[2], points[3]],
            [color; 3],
            size,
        );
    }

    fn add_gradient(
        &mut self,
        transform: TransformMatrix,
        rect: Rect,
        start: Point,
        end: Point,
        stops: &[GradientStop],
        size: RenderSize,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || stops.is_empty() {
            return;
        }
        let mut ordered = stops.to_vec();
        ordered.sort_by(|left, right| left.offset.total_cmp(&right.offset));
        let corners = [
            (Point::new(rect.x, rect.y), Point::new(0.0, 0.0)),
            (Point::new(rect.right(), rect.y), Point::new(1.0, 0.0)),
            (
                Point::new(rect.right(), rect.bottom()),
                Point::new(1.0, 1.0),
            ),
            (Point::new(rect.x, rect.bottom()), Point::new(0.0, 1.0)),
        ];
        let colors = corners.map(|(_, normalized)| {
            color_to_array(sample_gradient(
                &ordered,
                gradient_offset(start, end, normalized),
            ))
        });
        self.add_triangle(
            transform,
            [corners[0].0, corners[1].0, corners[2].0],
            [colors[0], colors[1], colors[2]],
            size,
        );
        self.add_triangle(
            transform,
            [corners[0].0, corners[2].0, corners[3].0],
            [colors[0], colors[2], colors[3]],
            size,
        );
    }

    fn add_triangle(
        &mut self,
        transform: TransformMatrix,
        points: [Point; 3],
        colors: [[f32; 4]; 3],
        size: RenderSize,
    ) {
        for (point, color) in points.into_iter().zip(colors) {
            let transformed = transform.apply(point);
            let ndc = to_ndc(size, transformed);
            self.bytes.extend_from_slice(&ndc[0].to_le_bytes());
            self.bytes.extend_from_slice(&ndc[1].to_le_bytes());
            for channel in color {
                self.bytes.extend_from_slice(&channel.to_le_bytes());
            }
            self.vertex_count += 1;
        }
    }
}

fn add_arc(
    points: &mut Vec<Point>,
    center: Point,
    radius: f32,
    start: f32,
    end: f32,
    segments: usize,
) {
    for segment in 0..=segments {
        let angle = start + (end - start) * segment as f32 / segments as f32;
        points.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
}

fn sample_gradient(stops: &[GradientStop], offset: f32) -> Color {
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
            let amount = (offset - left.offset) / span;
            return interpolate_color(left.color, right.color, amount);
        }
    }
    stops[stops.len() - 1].color
}

fn interpolate_color(left: Color, right: Color, amount: f32) -> Color {
    let mix = |a: u8, b: u8| {
        let value = f32::from(a) + (f32::from(b) - f32::from(a)) * amount;
        value.round().clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        mix(left.r, right.r),
        mix(left.g, right.g),
        mix(left.b, right.b),
        mix(left.a, right.a),
    )
}

fn gradient_offset(start: Point, end: Point, point: Point) -> f32 {
    let direction = Point::new(end.x - start.x, end.y - start.y);
    let length_squared = direction.x * direction.x + direction.y * direction.y;
    if length_squared <= f32::EPSILON {
        return 0.0;
    }
    ((point.x - start.x) * direction.x + (point.y - start.y) * direction.y) / length_squared
}

fn to_ndc(size: RenderSize, point: Point) -> [f32; 2] {
    [
        point.x / size.width as f32 * 2.0 - 1.0,
        1.0 - point.y / size.height as f32 * 2.0,
    ]
}

fn scissor_to_pixels(size: RenderSize, rect: Rect) -> Option<(u32, u32, u32, u32)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::paint::Transform;

    fn rect_command() -> DrawCommand {
        DrawCommand::Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            color: Color::WHITE,
        }
    }

    #[test]
    fn ndc_mapping_uses_top_left_origin() {
        let size = RenderSize::new(100, 50);
        assert_eq!(to_ndc(size, Point::new(0.0, 0.0)), [-1.0, 1.0]);
        assert_eq!(to_ndc(size, Point::new(50.0, 25.0)), [0.0, 0.0]);
        assert_eq!(to_ndc(size, Point::new(100.0, 50.0)), [1.0, -1.0]);
    }

    #[test]
    fn scissor_conversion_clamps_to_target() {
        let size = RenderSize::new(100, 50);
        assert_eq!(
            scissor_to_pixels(size, Rect::new(-5.0, -5.0, 20.0, 20.0)),
            Some((0, 0, 15, 15))
        );
        assert_eq!(
            scissor_to_pixels(size, Rect::new(90.0, 40.0, 30.0, 30.0)),
            Some((90, 40, 10, 10))
        );
    }

    #[test]
    fn rect_mesh_builds_one_batch_and_six_vertices() {
        let display = DisplayList::from_commands(vec![rect_command()]);
        let mesh = FrameMesh::build(&display, RenderSize::new(100, 100)).unwrap();
        assert_eq!(mesh.batches.len(), 1);
        assert_eq!(mesh.vertex_count, 6);
        assert_eq!(mesh.stats.draw_calls, 1);
        assert_eq!(mesh.bytes.len(), 6 * VERTEX_BYTES);
    }

    #[test]
    fn gradient_mesh_samples_distinct_corner_colors() {
        let gradient = DrawCommand::LinearGradient {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 10.0,
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: Color::BLACK,
                },
                GradientStop {
                    offset: 1.0,
                    color: Color::WHITE,
                },
            ],
        };
        let mesh = FrameMesh::build(
            &DisplayList::from_commands(vec![gradient]),
            RenderSize::new(100, 100),
        )
        .unwrap();
        assert_eq!(mesh.vertex_count, 6);
        let red = |vertex: usize| {
            let start = vertex * VERTEX_BYTES + 8;
            f32::from_le_bytes(
                mesh.bytes[start..start + 4]
                    .try_into()
                    .expect("four color bytes"),
            )
        };
        assert_eq!(red(0), 0.0);
        assert_eq!(red(1), 1.0);
    }

    #[test]
    fn transform_and_scissor_stacks_update_stats() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::TransformPush {
                transform: Transform::translate(Point::new(5.0, 5.0)),
            },
            DrawCommand::ScissorPush {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
            },
            rect_command(),
            DrawCommand::ScissorPop,
            DrawCommand::TransformPop,
        ]);
        let mesh = FrameMesh::build(&display, RenderSize::new(100, 100)).unwrap();
        assert_eq!(mesh.stats.max_transform_depth, 1);
        assert_eq!(mesh.stats.max_scissor_depth, 1);
        assert_eq!(mesh.stats.command_counts.rect, 1);
    }

    #[test]
    fn empty_scissor_produces_no_gpu_geometry() {
        let display = DisplayList::from_commands(vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(200.0, 200.0, 10.0, 10.0),
            },
            rect_command(),
            DrawCommand::ScissorPop,
        ]);
        let mesh = FrameMesh::build(&display, RenderSize::new(100, 100)).unwrap();
        assert_eq!(mesh.vertex_count, 0);
        assert!(mesh.batches.is_empty());
        assert_eq!(mesh.stats.draw_calls, 0);
        assert_eq!(mesh.stats.command_counts.rect, 1);
    }

    #[test]
    fn embedded_wgsl_parses_and_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(SHADER).expect("embedded WGSL must parse");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        validator
            .validate(&module)
            .expect("embedded WGSL must validate");
    }
}
