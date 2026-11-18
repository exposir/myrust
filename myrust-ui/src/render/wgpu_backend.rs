//! Optional wgpu offscreen renderer.
//!
//! The backend owns a device, an offscreen RGBA texture and a
//! [`GpuScene`](super::gpu_scene::GpuScene) that draws the whole display list:
//! shapes, gradients, text, SVG paths and shadows. It needs a
//! working graphics driver at runtime; compiling the feature does not imply
//! that one is present.

use std::fmt;
use std::time::Duration;

use wgpu::{
    AdapterInfo, CommandEncoderDescriptor, DeviceDescriptor, Extent3d, InstanceDescriptor,
    PollType, PowerPreference, RequestAdapterOptions, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
};

use super::gpu_scene::GpuScene;
use super::{
    Damage, RenderError, RenderSize, RenderStats, Renderer, RendererBackend,
};
use crate::paint::{Color, DisplayList};

const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
const ADAPTER_TIMEOUT: Duration = Duration::from_secs(5);
const DEVICE_TIMEOUT: Duration = Duration::from_secs(5);
const RENDER_TIMEOUT: Duration = Duration::from_secs(5);

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

/// Offscreen wgpu renderer.
///
/// Initialization has bounded adapter/device timeouts and returns
/// [`RenderError::NoAdapter`] when no compatible adapter exists. Each
/// [`Renderer::render`] draws the whole display list into an RGBA texture.
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    scene: GpuScene,
    texture: wgpu::Texture,
    view: TextureView,
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
    /// Initializes an adapter, device, scene and offscreen texture.
    pub fn new(size: RenderSize) -> Result<Self, RenderError> {
        super::validate_size(size)?;
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

        let scene = GpuScene::new(&device, &queue, COLOR_FORMAT);
        let (texture, view) = create_color_target(&device, size);

        Ok(Self {
            device,
            queue,
            scene,
            texture,
            view,
            device_size: size,
            surface_size: size,
            gpu_info: adapter_info,
            last_stats: RenderStats::default(),
        })
    }

    /// Reads the rendered texture back as RGBA rows, for tests.
    #[cfg(test)]
    pub(crate) fn read_rgba(&self) -> Option<Vec<u8>> {
        read_texture_rgba(
            &self.device,
            &self.queue,
            &self.texture,
            self.surface_size,
        )
    }
}

/// Copies `texture` into a mapped buffer and returns unpadded RGBA rows.
#[cfg(test)]
fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: RenderSize,
) -> Option<Vec<u8>> {
    let row_bytes = size.width * 4;
    let padded = row_bytes.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("myrust-ui-readback"),
        size: u64::from(padded) * u64::from(size.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("myrust-ui-readback-encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size.height),
            },
        },
        Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    let _ = device.poll(PollType::Wait {
        submission_index: None,
        timeout: Some(RENDER_TIMEOUT),
    });
    let _ = receiver.recv_timeout(RENDER_TIMEOUT).ok()?;
    let mapped = slice.get_mapped_range().ok()?;
    let mut pixels = Vec::with_capacity((row_bytes * size.height) as usize);
    for row in 0..size.height {
        let start = (row * padded) as usize;
        pixels.extend_from_slice(&mapped[start..start + row_bytes as usize]);
    }
    Some(pixels)
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
        super::validate_size(size)?;
        let (texture, view) = create_color_target(&self.device, size);
        self.texture = texture;
        self.view = view;
        self.device_size = size;
        self.surface_size = size;
        self.last_stats = RenderStats::default();
        Ok(())
    }

    fn render(&mut self, display_list: &DisplayList) -> Result<RenderStats, RenderError> {
        super::validate_size(self.surface_size)?;
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-ui-encoder"),
            });
        let stats = self.scene.render(
            &mut encoder,
            &self.view,
            self.surface_size,
            display_list,
            &Damage::full(),
            Color::TRANSPARENT,
        )?;
        let submission = self.queue.submit([encoder.finish()]);
        self.device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(RENDER_TIMEOUT),
            })
            .map_err(|error| RenderError::BackendFailure {
                message: format!("wgpu render poll failed: {error:?}"),
            })?;
        self.last_stats.clone_from(&stats);
        Ok(stats)
    }
}

fn create_color_target(
    device: &wgpu::Device,
    size: RenderSize,
) -> (wgpu::Texture, TextureView) {
    create_color_target_with_format(device, size, COLOR_FORMAT)
}

fn create_color_target_with_format(
    device: &wgpu::Device,
    size: RenderSize,
    format: TextureFormat,
) -> (wgpu::Texture, TextureView) {
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
        format,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::DrawCommand;

    fn renderer() -> Option<WgpuRenderer> {
        WgpuRenderer::new(RenderSize::new(64, 64)).ok()
    }

    #[test]
    fn offscreen_renderer_draws_colored_pixels() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let display = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 8.0,
            y: 8.0,
            width: 16.0,
            height: 16.0,
            color: Color::rgb(255, 0, 0),
        }]);
        renderer.render(&display).unwrap();
        let pixels = renderer.read_rgba().expect("rendered texture reads back");
        let at = |x: u32, y: u32| {
            let index = ((y * 64 + x) * 4) as usize;
            [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
        };
        assert_eq!(at(12, 12), [255, 0, 0, 255]);
        assert_eq!(at(48, 48), [0, 0, 0, 0]);
    }

    #[test]
    fn blitting_copies_the_frame_onto_another_texture() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let display = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 8.0,
            y: 8.0,
            width: 16.0,
            height: 16.0,
            color: Color::rgb(255, 0, 0),
        }]);
        renderer.render(&display).unwrap();
        let size = renderer.surface_size;
        let (target, target_view) = create_color_target(&renderer.device, size);
        let mut encoder = renderer
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-ui-blit-test"),
            });
        renderer
            .scene
            .blit(&mut encoder, &renderer.view, &target_view);
        renderer.queue.submit([encoder.finish()]);
        let pixels = read_texture_rgba(&renderer.device, &renderer.queue, &target, size)
            .expect("the blit target reads back");
        let at = |x: u32, y: u32| {
            let index = ((y * size.width + x) * 4) as usize;
            [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
        };
        assert_eq!(at(12, 12), [255, 0, 0, 255]);
        assert_eq!(at(48, 48), [0, 0, 0, 0]);
    }

    #[test]
    fn srgb_targets_do_not_double_encode_colors() {
        let Some(renderer) = renderer() else {
            return;
        };
        let size = RenderSize::new(32, 32);
        let display = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 32.0,
            color: Color::rgb(128, 128, 128),
        }]);
        for format in [TextureFormat::Rgba8Unorm, TextureFormat::Rgba8UnormSrgb] {
            let (target, target_view) =
                create_color_target_with_format(&renderer.device, size, format);
            let mut scene = GpuScene::new(&renderer.device, &renderer.queue, format);
            let mut encoder = renderer
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("myrust-ui-srgb-test"),
                });
            scene
                .render(
                    &mut encoder,
                    &target_view,
                    size,
                    &display,
                    &Damage::full(),
                    Color::TRANSPARENT,
                )
                .expect("the scene fits its caches");
            renderer.queue.submit([encoder.finish()]);
            let pixels = read_texture_rgba(&renderer.device, &renderer.queue, &target, size)
                .expect("the color target reads back");
            let gray = pixels[((16 * size.width + 16) * 4) as usize];
            assert!(
                i32::from(gray).abs_diff(128) <= 2,
                "{format:?} must store the sRGB byte as written, got {gray}"
            );
        }
    }

    #[test]
    fn partial_damage_erases_the_pixel_aligned_region() {
        use crate::paint::Rect;

        let Some(renderer) = renderer() else {
            return;
        };
        let size = RenderSize::new(64, 64);
        let (target, view) = create_color_target_with_format(&renderer.device, size, COLOR_FORMAT);
        let mut scene = GpuScene::new(&renderer.device, &renderer.queue, COLOR_FORMAT);
        let red = DisplayList::from_commands(vec![DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
            color: Color::rgb(255, 0, 0),
        }]);
        let mut encoder = renderer
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-ui-damage-test"),
            });
        scene
            .render(&mut encoder, &view, size, &red, &Damage::full(), Color::TRANSPARENT)
            .expect("the red frame fits");
        renderer.queue.submit([encoder.finish()]);

        let empty = DisplayList::from_commands(Vec::new());
        let damage = Damage::partial(vec![Rect::new(10.4, 10.4, 19.2, 19.2)]);
        let mut encoder = renderer
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-ui-damage-test"),
            });
        scene
            .render(
                &mut encoder,
                &view,
                size,
                &empty,
                &damage,
                Color::rgb(128, 128, 128),
            )
            .expect("the damage frame fits");
        renderer.queue.submit([encoder.finish()]);
        let pixels = read_texture_rgba(&renderer.device, &renderer.queue, &target, size)
            .expect("the damage target reads back");
        let at = |x: u32, y: u32| {
            let index = ((y * size.width + x) * 4) as usize;
            [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
        };
        // The scissor floors/ceils to 10..30; every pixel it covers must be
        // fully new background, including the fractional-edge pixels.
        for (x, y) in [(10, 10), (29, 29), (10, 29), (29, 10)] {
            assert_eq!(at(x, y), [128, 128, 128, 255], "pixel ({x},{y})");
        }
        assert_eq!(at(9, 9), [255, 0, 0, 255]);
        assert_eq!(at(30, 30), [255, 0, 0, 255]);
    }

    #[test]
    fn srgb_full_clear_stores_the_background_byte() {
        let Some(renderer) = renderer() else {
            return;
        };
        let size = RenderSize::new(32, 32);
        let empty = DisplayList::from_commands(Vec::new());
        for format in [TextureFormat::Rgba8Unorm, TextureFormat::Rgba8UnormSrgb] {
            let (target, view) =
                create_color_target_with_format(&renderer.device, size, format);
            let mut scene = GpuScene::new(&renderer.device, &renderer.queue, format);
            let mut encoder = renderer
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("myrust-ui-clear-test"),
                });
            scene
                .render(
                    &mut encoder,
                    &view,
                    size,
                    &empty,
                    &Damage::full(),
                    Color::rgb(128, 128, 128),
                )
                .expect("an empty frame fits");
            renderer.queue.submit([encoder.finish()]);
            let pixels = read_texture_rgba(&renderer.device, &renderer.queue, &target, size)
                .expect("the clear target reads back");
            let index = ((16 * size.width + 16) * 4) as usize;
            let gray = pixels[index];
            assert!(
                i32::from(gray).abs_diff(128) <= 2,
                "{format:?} clear must store 128, got {gray}"
            );
        }
    }

    #[test]
    fn text_is_not_clipped_by_approximate_bounds() {
        if crate::font::default_font().is_none() {
            return;
        }
        let size = RenderSize::new(200, 40);
        let Some(mut renderer) = WgpuRenderer::new(size).ok() else {
            return;
        };
        let display = DisplayList::from_commands(vec![DrawCommand::Text {
            x: 2.0,
            y: 4.0,
            text: "WWWWWWWWWW".to_string(),
            font_size: 14.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: crate::paint::TextAlignment::Start,
        }]);
        renderer.render(&display).unwrap();
        let pixels = renderer.read_rgba().expect("the text target reads back");
        let rightmost = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, pixel)| pixel[3] > 0)
            .map(|(index, _)| index as u32 % size.width)
            .max()
            .unwrap_or(0);
        // Ten wide glyphs advance past the old approximate scissor (~91px).
        assert!(rightmost >= 100, "text ink must reach past 100px, got {rightmost}");
    }

    #[test]
    fn frames_that_exceed_the_gradient_cache_fail_explicitly() {
        use crate::layout::Point;
        use crate::paint::{GradientStop, Radius};

        let Some(renderer) = renderer() else {
            return;
        };
        let size = RenderSize::new(32, 32);
        let (_target, view) = create_color_target_with_format(&renderer.device, size, COLOR_FORMAT);
        let mut scene = GpuScene::new(&renderer.device, &renderer.queue, COLOR_FORMAT);
        let mut commands = Vec::new();
        for index in 0..300_u16 {
            commands.push(DrawCommand::LinearGradient {
                x: 0.0,
                y: 0.0,
                width: 32.0,
                height: 32.0,
                radius: Radius::all(0.0),
                start: Point::new(0.0, 0.0),
                end: Point::new(1.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb((index % 256) as u8, (index / 256) as u8, 0),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(255, 255, 255),
                    },
                ],
            });
        }
        let display = DisplayList::from_commands(commands);
        let mut encoder = renderer
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-ui-capacity-test"),
            });
        let result = scene.render(
            &mut encoder,
            &view,
            size,
            &display,
            &Damage::full(),
            Color::TRANSPARENT,
        );
        assert!(
            result.is_err(),
            "a frame that recycles the gradient cache must fail instead of drawing wrong colors"
        );
    }

    #[test]
    fn offscreen_renderer_draws_text_and_rounded_boxes() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let display = DisplayList::from_commands(vec![
            DrawCommand::RoundedRect {
                x: 4.0,
                y: 40.0,
                width: 40.0,
                height: 16.0,
                radius: crate::paint::Radius::all(6.0),
                color: Color::rgb(0, 255, 0),
            },
            DrawCommand::Text {
                x: 4.0,
                y: 4.0,
                text: "Ag".to_string(),
                font_size: 16.0,
                weight: 400,
                italic: false,
                mono: false,
                family: None,
                color: Color::WHITE,
                max_width: None,
                alignment: crate::paint::TextAlignment::Start,
            },
        ]);
        let stats = renderer.render(&display).unwrap();
        assert!(stats.drawn_vertices > 0);
        let pixels = renderer.read_rgba().expect("rendered texture reads back");
        let has_green = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[1] > 200 && pixel[3] > 0);
        let has_light = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] > 40 && pixel[1] > 40 && pixel[2] > 40 && pixel[3] > 0);
        assert!(has_green, "the rounded box must draw");
        assert!(has_light, "text must draw");
    }
}
