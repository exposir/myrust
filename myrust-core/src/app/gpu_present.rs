//! GPU presentation for native UI windows.
//!
//! One [`GpuPresenter`] owns a wgpu surface for a tao window, a persistent
//! front texture, and a [`GpuScene`]. Each frame diffs the display list
//! against the previous one, redraws only the damaged rectangles into the
//! front texture, and blits that texture onto the window surface. The surface
//! is not assumed to preserve its contents between frames, so the front
//! texture is the source of truth.
//!
//! Creation and rendering both return errors instead of panicking; the
//! window falls back to CPU presentation when a driver lacks a usable
//! adapter or the surface is lost.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tao::{
    rwh_06::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle},
    window::Window,
};
use wgpu::{
    CommandEncoderDescriptor, CurrentSurfaceTexture, Device, DeviceDescriptor, Extent3d, Instance,
    InstanceDescriptor, PowerPreference, PresentMode, Queue, RequestAdapterOptions, Surface,
    SurfaceConfiguration, SurfaceTexture, Texture, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
};

use crate::ui::{diff, Color, Damage, DisplayList, GpuScene, RenderSize, RenderStats};

/// Bound on adapter and device creation so a broken driver cannot hang window
/// creation.
const INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Surface formats we prefer, in order.
///
/// Non-sRGB formats keep shader blending in the same sRGB-encoded space as the
/// CPU and Canvas paths. Lists are ordered; a platform that reports only
/// `*Srgb` formats still works, with the documented double-encoding caveat.
const PREFERRED_FORMATS: [TextureFormat; 2] = [TextureFormat::Bgra8Unorm, TextureFormat::Rgba8Unorm];

/// Owned window handle that wgpu can turn into a `Surface<'static>`.
///
/// `tao::Window` implements the raw-window-handle traits, but `Arc<Window>`
/// does not. Wrapping the `Arc` means the surface keeps the window alive and
/// we never pass a borrowed handle through `unsafe`.
struct GpuTarget(Arc<Window>);

impl HasWindowHandle for GpuTarget {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for GpuTarget {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// GPU surface presenter for one native UI window.
pub(crate) struct GpuPresenter {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    scene: GpuScene,
    config: SurfaceConfiguration,
    front: Texture,
    front_view: TextureView,
    size: RenderSize,
    last_display: Option<DisplayList>,
    last_stats: RenderStats,
    device_lost: Arc<AtomicBool>,
}

impl GpuPresenter {
    /// Creates a surface, adapter, device, scene and front texture.
    pub(crate) fn new(window: Arc<Window>, size: RenderSize) -> Result<Self> {
        validate_size(size)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .context("failed to create GPU initialization runtime")?;
        runtime.block_on(Self::new_async(window, size))
    }

    async fn new_async(window: Arc<Window>, size: RenderSize) -> Result<Self> {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(GpuTarget(window))
            .context("failed to create a GPU surface for the window")?;
        let adapter = tokio::time::timeout(
            INIT_TIMEOUT,
            instance.request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            }),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "GPU adapter request timed out after {} ms",
                INIT_TIMEOUT.as_millis()
            )
        })?
        .map_err(|error| anyhow!("no GPU adapter can present to this window: {error}"))?;

        let (device, queue) = tokio::time::timeout(
            INIT_TIMEOUT,
            adapter.request_device(&DeviceDescriptor {
                label: Some("myrust-native-window"),
                ..DeviceDescriptor::default()
            }),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "GPU device request timed out after {} ms",
                INIT_TIMEOUT.as_millis()
            )
        })?
        .context("failed to create a GPU device for native presentation")?;

        // A lost device makes every later call invalid; latch it so the
        // window's next render falls back to the CPU instead of panicking.
        let device_lost = Arc::new(AtomicBool::new(false));
        let lost_flag = Arc::clone(&device_lost);
        device.set_device_lost_callback(move |reason, message| {
            lost_flag.store(true, Ordering::SeqCst);
            tracing::error!(?reason, %message, "native GPU device lost");
        });

        let capabilities = surface.get_capabilities(&adapter);
        if capabilities.formats.is_empty() {
            bail!("GPU surface reports no supported texture formats");
        }
        let adapter_info = adapter.get_info();
        if std::env::var_os("MYRUST_GPU_DEBUG").is_some() {
            eprintln!(
                "myrust: GPU presentation via {} ({:?}, {:?})",
                adapter_info.name, adapter_info.backend, adapter_info.device_type
            );
        }
        let format = PREFERRED_FORMATS
            .iter()
            .copied()
            .find(|candidate| capabilities.formats.contains(candidate))
            .unwrap_or(capabilities.formats[0]);
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("GPU surface rejected a {}x{} configuration", size.width, size.height))?;
        config.format = format;
        config.present_mode = PresentMode::Fifo;
        surface.configure(&device, &config);

        let scene = GpuScene::new(&device, &queue, format);
        let (front, front_view) = create_front_texture(&device, size, format);
        Ok(Self {
            surface,
            device,
            queue,
            scene,
            config,
            front,
            front_view,
            size,
            last_display: None,
            last_stats: RenderStats::default(),
            device_lost,
        })
    }

    /// Reconfigures the surface for a new size and drops the previous frame.
    pub(crate) fn resize(&mut self, size: RenderSize) -> Result<()> {
        validate_size(size)?;
        if size == self.size {
            return Ok(());
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        let (front, front_view) = create_front_texture(&self.device, size, self.config.format);
        self.front = front;
        self.front_view = front_view;
        self.size = size;
        self.last_display = None;
        Ok(())
    }

    /// Renders `display` into the front texture, redrawing only the region
    /// that differs from the previous frame, then presents it.
    ///
    /// An unchanged display list still blits and presents: the platform window
    /// may have painted over the last presented frame.
    pub(crate) fn render(
        &mut self,
        display: &DisplayList,
        size: RenderSize,
        background: Color,
    ) -> Result<RenderStats> {
        if self.device_lost.load(Ordering::SeqCst) {
            bail!("the GPU device was lost");
        }
        self.resize(size)?;
        let damage = match &self.last_display {
            Some(previous) => diff(previous, display, size),
            None => Damage::full(),
        };
        if std::env::var_os("MYRUST_GPU_DEBUG").is_some() {
            let rects = damage.rects();
            eprintln!(
                "myrust: GPU damage full={} rects={} first={:?}",
                damage.is_full(),
                rects.len(),
                rects.first()
            );
        }

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("myrust-native-frame"),
            });
        let stats = self
            .scene
            .render(
                &mut encoder,
                &self.front_view,
                size,
                display,
                &damage,
                background,
            )
            .map_err(|error| anyhow!("{error}"))?;
        match self.acquire()? {
            Some(frame) => {
                let view = frame.texture.create_view(&TextureViewDescriptor::default());
                self.scene.blit(&mut encoder, &self.front_view, &view);
                self.queue.submit([encoder.finish()]);
                self.queue.present(frame);
            }
            None => {
                // Occluded or timed out: keep the front texture current for the
                // next successful acquire.
                self.queue.submit([encoder.finish()]);
            }
        }
        if self.device_lost.load(Ordering::SeqCst) {
            bail!("the GPU device was lost during presentation");
        }
        self.last_stats.clone_from(&stats);
        self.last_display = Some(display.clone());
        Ok(stats)
    }

    /// Acquires the next surface texture, reconfiguring once when the surface
    /// reports that it is stale.
    fn acquire(&mut self) -> Result<Option<SurfaceTexture>> {
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                Ok(Some(frame))
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => Ok(None),
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    CurrentSurfaceTexture::Success(frame)
                    | CurrentSurfaceTexture::Suboptimal(frame) => Ok(Some(frame)),
                    CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => Ok(None),
                    _ => Err(anyhow!(
                        "GPU surface is still unavailable after reconfiguration"
                    )),
                }
            }
            CurrentSurfaceTexture::Validation => Err(anyhow!("GPU surface validation failed")),
        }
    }
}

fn validate_size(size: RenderSize) -> Result<()> {
    if size.width == 0 || size.height == 0 {
        bail!(
            "GPU presentation needs a non-empty size, got {}x{}",
            size.width,
            size.height
        );
    }
    Ok(())
}

fn create_front_texture(
    device: &Device,
    size: RenderSize,
    format: TextureFormat,
) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("myrust-native-front"),
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
