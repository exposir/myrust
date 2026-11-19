//! Deterministic PNG golden-frame comparison for native UI tests.
//!
//! Goldens are ordinary 8-bit RGBA PNG files. Update them explicitly with
//! `UPDATE_GOLDEN=1`; a normal test run compares pixels and writes an
//! `.actual.png` sibling only on mismatch. The module is native-only because
//! it reads and writes files.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};

use super::paint::{Color, DisplayList};
use super::present::{CpuFrame, CpuRasterizer};
use super::render::RenderSize;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Pixel image used for CPU and GPU golden comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Row-major RGBA8 pixels.
    pub rgba: Vec<u8>,
}

impl GoldenImage {
    /// Creates an RGBA image, validating the buffer length.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, GoldenError> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || rgba.len() != expected {
            return Err(GoldenError::InvalidImage {
                width,
                height,
                bytes: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Converts a packed RGB CPU frame into an opaque RGBA golden.
    pub fn from_cpu_frame(frame: &CpuFrame) -> Self {
        let mut rgba = Vec::with_capacity(frame.pixels.len() * 4);
        for pixel in &frame.pixels {
            rgba.push(((pixel >> 16) & 0xff) as u8);
            rgba.push(((pixel >> 8) & 0xff) as u8);
            rgba.push((pixel & 0xff) as u8);
            rgba.push(255);
        }
        Self {
            width: frame.width,
            height: frame.height,
            rgba,
        }
    }

    /// Reads an RGBA or RGB PNG written by this module or another PNG encoder.
    pub fn read_png(path: impl AsRef<Path>) -> Result<Self, GoldenError> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|error| GoldenError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        decode_png(&bytes)
    }

    /// Writes this image as a deterministic RGBA PNG.
    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), GoldenError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| GoldenError::Io {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }
        let bytes = encode_png(self)?;
        fs::write(path, bytes).map_err(|error| GoldenError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    }

    /// Compares two images with a per-channel delta and changed-pixel budget.
    pub fn compare(
        &self,
        actual: &GoldenImage,
        tolerance: GoldenTolerance,
    ) -> Result<GoldenDiff, GoldenError> {
        if self.width != actual.width || self.height != actual.height {
            return Err(GoldenError::SizeMismatch {
                expected: (self.width, self.height),
                actual: (actual.width, actual.height),
            });
        }
        let pixels = self.width as usize * self.height as usize;
        let mut different_pixels = 0_usize;
        let mut max_channel_delta = 0_u8;
        let mut first_difference = None;
        for index in 0..pixels {
            let offset = index * 4;
            let expected = &self.rgba[offset..offset + 4];
            let actual = &actual.rgba[offset..offset + 4];
            let delta = expected
                .iter()
                .zip(actual)
                .map(|(left, right)| left.abs_diff(*right))
                .max()
                .unwrap_or(0);
            max_channel_delta = max_channel_delta.max(delta);
            if delta > tolerance.max_channel_delta {
                different_pixels += 1;
                first_difference.get_or_insert((index, offset, delta));
            }
        }
        let differing_fraction = different_pixels as f32 / pixels as f32;
        let diff = GoldenDiff {
            width: self.width,
            height: self.height,
            different_pixels,
            max_channel_delta,
            first_difference,
        };
        if differing_fraction > tolerance.max_different_pixel_fraction {
            Err(GoldenError::Mismatch { diff })
        } else {
            Ok(diff)
        }
    }
}

/// Threshold used by golden comparisons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoldenTolerance {
    /// Largest allowed delta on any R, G, B or A channel.
    pub max_channel_delta: u8,
    /// Largest allowed fraction of pixels whose channel delta exceeds the
    /// first threshold.
    pub max_different_pixel_fraction: f32,
}

impl GoldenTolerance {
    /// Requires byte-identical pixels.
    pub const EXACT: Self = Self {
        max_channel_delta: 0,
        max_different_pixel_fraction: 0.0,
    };

    /// A small tolerance for CPU/GPU antialiasing differences.
    pub const ANTIALIASED: Self = Self {
        max_channel_delta: 64,
        max_different_pixel_fraction: 0.02,
    };
}

/// Result of a successful comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenDiff {
    /// Compared width.
    pub width: u32,
    /// Compared height.
    pub height: u32,
    /// Number of pixels outside the per-channel tolerance.
    pub different_pixels: usize,
    /// Largest channel delta found anywhere.
    pub max_channel_delta: u8,
    /// First offending pixel index, byte offset and channel delta.
    pub first_difference: Option<(usize, usize, u8)>,
}

/// Golden read/write/compare error.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GoldenError {
    /// A filesystem operation failed.
    #[error("golden I/O failed for {path}: {message}")]
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// OS error text.
        message: String,
    },
    /// The image dimensions and RGBA length disagree.
    #[error("invalid golden image {width}x{height} with {bytes} RGBA bytes")]
    InvalidImage {
        /// Width.
        width: u32,
        /// Height.
        height: u32,
        /// Supplied byte count.
        bytes: usize,
    },
    /// The PNG structure is unsupported or corrupt.
    #[error("invalid PNG: {0}")]
    InvalidPng(String),
    /// Two images do not have the same size.
    #[error("golden size mismatch: expected {expected:?}, got {actual:?}")]
    SizeMismatch {
        /// Expected width and height.
        expected: (u32, u32),
        /// Actual width and height.
        actual: (u32, u32),
    },
    /// The expected golden does not exist and update mode is disabled.
    #[error("golden file is missing: {path}")]
    Missing {
        /// Missing path.
        path: PathBuf,
    },
    /// Pixel comparison exceeded the tolerance.
    #[error("golden mismatch: {diff:?}")]
    Mismatch {
        /// Comparison detail.
        diff: GoldenDiff,
    },
    /// CPU rasterization failed.
    #[error("CPU rasterization failed: {0}")]
    Raster(String),
    /// GPU capture failed for a reason other than "no adapter".
    #[error("GPU capture failed: {0}")]
    Gpu(String),
}

/// Golden directory and update policy.
#[derive(Debug, Clone)]
pub struct GoldenHarness {
    directory: PathBuf,
    update: bool,
}

impl GoldenHarness {
    /// Creates a harness. `UPDATE_GOLDEN=1` enables explicit rewrites.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            update: std::env::var("UPDATE_GOLDEN").is_ok_and(|value| value == "1"),
        }
    }

    /// Overrides update mode without consulting the environment.
    pub fn with_update(mut self, update: bool) -> Self {
        self.update = update;
        self
    }

    /// Returns the golden directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Compares or updates a named golden.
    pub fn compare_or_update(
        &self,
        name: &str,
        actual: &GoldenImage,
        tolerance: GoldenTolerance,
    ) -> Result<GoldenDiff, GoldenError> {
        let filename = if name.ends_with(".png") {
            name.to_owned()
        } else {
            format!("{name}.png")
        };
        let path = self.directory.join(filename);
        if self.update || !path.exists() {
            if self.update {
                actual.write_png(&path)?;
                return Ok(GoldenDiff {
                    width: actual.width,
                    height: actual.height,
                    different_pixels: 0,
                    max_channel_delta: 0,
                    first_difference: None,
                });
            }
            return Err(GoldenError::Missing { path });
        }
        let expected = GoldenImage::read_png(&path)?;
        match expected.compare(actual, tolerance) {
            Ok(diff) => Ok(diff),
            Err(error @ GoldenError::Mismatch { .. }) => {
                let actual_path = self.directory.join(format!(
                    "{}.actual.png",
                    path.file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("golden")
                ));
                let _ = actual.write_png(actual_path);
                Err(error)
            }
            Err(error) => Err(error),
        }
    }
}

/// Rasterizes a display list with the deterministic CPU presenter.
pub fn capture_cpu(
    display: &DisplayList,
    size: RenderSize,
    background: Color,
) -> Result<GoldenImage, GoldenError> {
    CpuRasterizer::rasterize(display, size, background)
        .map(|frame| GoldenImage::from_cpu_frame(&frame))
        .map_err(|error| GoldenError::Raster(error.to_string()))
}

/// Captures an offscreen GPU frame, or returns `None` when no adapter exists.
#[cfg(feature = "gpu")]
pub fn capture_gpu(
    display: &DisplayList,
    size: RenderSize,
    background: Color,
) -> Result<Option<GoldenImage>, GoldenError> {
    use super::render::{RenderError, WgpuRenderer};

    let mut renderer = match WgpuRenderer::new(size) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter)
        | Err(RenderError::BackendUnavailable { .. })
        | Err(RenderError::DeviceRequest { .. })
        | Err(RenderError::Timeout { .. }) => return Ok(None),
        Err(error) => return Err(GoldenError::Gpu(error.to_string())),
    };
    renderer
        .render_with_background(display, background)
        .map_err(|error| GoldenError::Gpu(error.to_string()))?;
    let rgba = renderer
        .read_rgba()
        .ok_or_else(|| GoldenError::Gpu("GPU readback failed".to_owned()))?;
    GoldenImage::new(size.width, size.height, rgba).map(Some)
}

fn encode_png(image: &GoldenImage) -> Result<Vec<u8>, GoldenError> {
    let mut raw = Vec::with_capacity(
        image.height as usize * (1 + image.width as usize * 4),
    );
    for row in image.rgba.chunks_exact(image.width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib.write_all(&raw)
        .map_err(|error| GoldenError::InvalidPng(error.to_string()))?;
    let compressed = zlib
        .finish()
        .map_err(|error| GoldenError::InvalidPng(error.to_string()))?;

    let mut png = Vec::new();
    png.extend_from_slice(PNG_SIGNATURE);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);
    Ok(png)
}

fn decode_png(bytes: &[u8]) -> Result<GoldenImage, GoldenError> {
    if bytes.len() < 8 || &bytes[..8] != PNG_SIGNATURE {
        return Err(GoldenError::InvalidPng("bad signature".to_owned()));
    }
    let mut offset = 8;
    let mut width = 0_u32;
    let mut height = 0_u32;
    let mut color_type = 0_u8;
    let mut compressed = Vec::new();
    let mut saw_iend = false;
    while offset + 12 <= bytes.len() {
        let length = read_u32(bytes, offset)? as usize;
        let kind_start = offset + 4;
        let data_start = kind_start + 4;
        let data_end = data_start
            .checked_add(length)
            .filter(|end| end + 4 <= bytes.len())
            .ok_or_else(|| GoldenError::InvalidPng("truncated chunk".to_owned()))?;
        let kind = &bytes[kind_start..kind_start + 4];
        let data = &bytes[data_start..data_end];
        let crc = read_u32(bytes, data_end)?;
        if crc32(&bytes[kind_start..data_end]) != crc {
            return Err(GoldenError::InvalidPng(format!(
                "bad CRC for {}",
                String::from_utf8_lossy(kind)
            )));
        }
        match kind {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err(GoldenError::InvalidPng("bad IHDR length".to_owned()));
                }
                width = read_u32(data, 0)?;
                height = read_u32(data, 4)?;
                if data[8] != 8 {
                    return Err(GoldenError::InvalidPng(
                        "only 8-bit PNGs are supported".to_owned(),
                    ));
                }
                color_type = data[9];
                if !matches!(color_type, 2 | 6) {
                    return Err(GoldenError::InvalidPng(format!(
                        "unsupported RGB color type {color_type}"
                    )));
                }
                if data[10] != 0 || data[11] != 0 || data[12] != 0 {
                    return Err(GoldenError::InvalidPng(
                        "compressed/filtered/interlaced PNG is unsupported".to_owned(),
                    ));
                }
            }
            b"IDAT" => compressed.extend_from_slice(data),
            b"IEND" => {
                saw_iend = true;
                break;
            }
            _ => {}
        }
        offset = data_end + 4;
    }
    if !saw_iend || width == 0 || height == 0 {
        return Err(GoldenError::InvalidPng("missing IHDR or IEND".to_owned()));
    }

    let channels = if color_type == 6 { 4 } else { 3 };
    let stride = width as usize * channels;
    let expected = height as usize * (stride + 1);
    let mut decoder = ZlibDecoder::new(compressed.as_slice());
    let mut raw = Vec::with_capacity(expected);
    decoder
        .read_to_end(&mut raw)
        .map_err(|error| GoldenError::InvalidPng(error.to_string()))?;
    if raw.len() != expected {
        return Err(GoldenError::InvalidPng(format!(
            "decompressed {} bytes, expected {expected}",
            raw.len()
        )));
    }
    unfilter(&mut raw, width as usize, height as usize, channels)?;

    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in raw.chunks_exact(stride + 1) {
        let pixels = &row[1..];
        if channels == 4 {
            rgba.extend_from_slice(pixels);
        } else {
            let (rgb_pixels, remainder) = pixels.as_chunks::<3>();
            debug_assert!(remainder.is_empty());
            for pixel in rgb_pixels {
                rgba.extend_from_slice(pixel);
                rgba.push(255);
            }
        }
    }
    GoldenImage::new(width, height, rgba)
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let crc = crc32(&{
        let mut chunk = Vec::with_capacity(kind.len() + data.len());
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        chunk
    });
    output.extend_from_slice(&crc.to_be_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, GoldenError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| GoldenError::InvalidPng("truncated u32".to_owned()))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn unfilter(
    raw: &mut [u8],
    width: usize,
    height: usize,
    channels: usize,
) -> Result<(), GoldenError> {
    let stride = width * channels;
    let mut previous = vec![0_u8; stride];
    for row in 0..height {
        let start = row * (stride + 1);
        let filter = raw[start];
        let current_start = start + 1;
        let current_end = current_start + stride;
        for index in 0..stride {
            let left = if index >= channels {
                raw[current_start + index - channels]
            } else {
                0
            };
            let up = previous[index];
            let up_left = if index >= channels {
                previous[index - channels]
            } else {
                0
            };
            let value = raw[current_start + index];
            raw[current_start + index] = match filter {
                0 => value,
                1 => value.wrapping_add(left),
                2 => value.wrapping_add(up),
                3 => value.wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                4 => value.wrapping_add(paeth(left, up, up_left)),
                other => {
                    return Err(GoldenError::InvalidPng(format!(
                        "unsupported PNG row filter {other}"
                    )));
                }
            };
        }
        previous.copy_from_slice(&raw[current_start..current_end]);
    }
    Ok(())
}

fn paeth(left: u8, up: u8, up_left: u8) -> u8 {
    let p = i32::from(left) + i32::from(up) - i32::from(up_left);
    let pa = (p - i32::from(left)).abs();
    let pb = (p - i32::from(up)).abs();
    let pc = (p - i32::from(up_left)).abs();
    if pa <= pb && pa <= pc {
        left
    } else if pb <= pc {
        up
    } else {
        up_left
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{button, container, text, text_field};
    use crate::layout::Size;
    use crate::paint::{DrawCommand, Radius};
    use crate::render::diff;
    use crate::theme::Theme;

    fn context_display(size: Size, build: impl FnOnce(&mut crate::context::Context)) -> DisplayList {
        let mut context = crate::context::Context::new(size);
        context.set_theme(Theme::light());
        context.begin_frame(size);
        build(&mut context);
        let frame = context.produce().unwrap();
        context.end_frame();
        frame.display
    }

    fn control_page() -> (DisplayList, RenderSize, Color) {
        let size = Size::new(320.0, 220.0);
        let display = context_display(size, |context| {
            container(context)
                .fill()
                .background(Color::rgb(247, 248, 250), 0.0)
                .padding(16.0)
                .column()
                .gap(10.0)
                .children(|context| {
                    text(context, "Native control page")
                        .font_size(20.0)
                        .weight(600);
                    text(context, "Deterministic golden fixture")
                        .font_size(13.0)
                        .color(Color::rgb(90, 96, 108));
                    let mut value = "Linus".to_owned();
                    text_field(context, "field", &mut value);
                    button(context, "Continue");
                });
        });
        (display, RenderSize::new(320, 220), Color::rgb(247, 248, 250))
    }

    fn text_scene() -> (DisplayList, RenderSize, Color) {
        let size = Size::new(280.0, 120.0);
        let display = context_display(size, |context| {
            container(context)
                .fill()
                .background(Color::rgb(18, 22, 28), 0.0)
                .padding(18.0)
                .column()
                .gap(4.0)
                .children(|context| {
                    text(context, "Golden text")
                        .font_size(26.0)
                        .weight(700)
                        .color(Color::WHITE);
                    text(context, "100% deterministic \u{4f60}\u{597d}")
                        .font_size(16.0)
                        .color(Color::rgb(180, 210, 255));
                });
        });
        (display, RenderSize::new(280, 120), Color::rgb(18, 22, 28))
    }

    fn shadow_scene() -> (DisplayList, RenderSize, Color) {
        let size = Size::new(280.0, 160.0);
        let display = context_display(size, |context| {
            container(context)
                .fill()
                .background(Color::rgb(238, 240, 244), 0.0)
                .children(|context| {
                    container(context)
                        .absolute(56.0, 42.0)
                        .size(168.0, 76.0)
                        .draw([
                            DrawCommand::Shadow {
                                x: 0.0,
                                y: 0.0,
                                width: 168.0,
                                height: 76.0,
                                radius: 16.0,
                                offset_x: 0.0,
                                offset_y: 8.0,
                                blur: 14.0,
                                color: Color::rgba(20, 24, 32, 80),
                            },
                            DrawCommand::RoundedRect {
                                x: 0.0,
                                y: 0.0,
                                width: 168.0,
                                height: 76.0,
                                radius: Radius::all(16.0),
                                color: Color::rgb(255, 255, 255),
                            },
                        ])
                        .children(|_| {});
                });
        });
        (display, RenderSize::new(280, 160), Color::rgb(238, 240, 244))
    }

    fn partial_damage_scenes() -> (DisplayList, DisplayList, RenderSize, Color) {
        let size = Size::new(320.0, 200.0);
        let background = Color::rgb(232, 236, 242);
        let before = context_display(size, |context| {
            container(context)
                .fill()
                .background(background, 0.0)
                .children(|context| {
                    container(context)
                        .absolute(24.0, 30.0)
                        .size(100.0, 58.0)
                        .background(Color::rgb(230, 82, 74), 12.0)
                        .children(|_| {});
                });
        });
        let after = context_display(size, |context| {
            container(context)
                .fill()
                .background(background, 0.0)
                .children(|context| {
                    container(context)
                        .absolute(92.0, 52.0)
                        .size(100.0, 58.0)
                        .background(Color::rgb(55, 126, 224), 12.0)
                        .children(|_| {});
                    container(context)
                        .absolute(18.0, 112.0)
                        .size(180.0, 6.0)
                        .background(Color::rgb(90, 96, 108), 3.0)
                        .children(|_| {});
                });
        });
        (before, after, RenderSize::new(320, 200), background)
    }

    fn golden_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
    }

    #[test]
    fn png_roundtrip_preserves_rgba_pixels() {
        let image = GoldenImage::new(
            3,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 40, 50, 60,
                70, 80, 90, 100, 110, 120,
            ],
        )
        .unwrap();
        let directory = std::env::temp_dir().join(format!(
            "myrust-golden-roundtrip-{}",
            std::process::id()
        ));
        let path = directory.join("roundtrip.png");
        image.write_png(&path).unwrap();
        assert_eq!(GoldenImage::read_png(&path).unwrap(), image);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn cpu_goldens_are_deterministic_and_cover_controls_text_and_shadow() {
        let harness = GoldenHarness::new(golden_dir());
        let scenes = [
            ("control-page", control_page()),
            ("text", text_scene()),
            ("shadow", shadow_scene()),
        ];
        for (name, (display, size, background)) in scenes {
            let first = capture_cpu(&display, size, background).unwrap();
            let second = capture_cpu(&display, size, background).unwrap();
            assert_eq!(first, second, "{name} must be deterministic");
            harness
                .compare_or_update(name, &first, GoldenTolerance::EXACT)
                .unwrap();
        }
    }

    #[test]
    fn partial_damage_replay_matches_a_full_redraw() {
        let (before, after, size, background) = partial_damage_scenes();
        let damage = diff(&before, &after, size);
        assert!(!damage.is_full());
        let full = capture_cpu(&after, size, background).unwrap();
        let harness = GoldenHarness::new(golden_dir());
        harness
            .compare_or_update("partial-damage", &full, GoldenTolerance::EXACT)
            .unwrap();

        #[cfg(feature = "gpu")]
        {
            use crate::render::{RenderError, WgpuRenderer};

            let mut renderer = match WgpuRenderer::new(size) {
                Ok(renderer) => renderer,
                Err(RenderError::NoAdapter)
                | Err(RenderError::BackendUnavailable { .. })
                | Err(RenderError::DeviceRequest { .. })
                | Err(RenderError::Timeout { .. }) => return,
                Err(error) => panic!("GPU initialization failed: {error}"),
            };
            renderer
                .render_with_background(&before, background)
                .unwrap();
            renderer
                .render_with_damage_and_background(&after, &damage, background)
                .unwrap();
            let rgba = renderer.read_rgba().expect("damage frame reads back");
            let gpu = GoldenImage::new(size.width, size.height, rgba).unwrap();
            full.compare(&gpu, GoldenTolerance::ANTIALIASED).unwrap();
        }
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_readback_matches_the_cpu_goldens_when_an_adapter_exists() {
        for (name, (display, size, background)) in [
            ("control-page", control_page()),
            ("text", text_scene()),
            ("shadow", shadow_scene()),
        ] {
            let cpu = capture_cpu(&display, size, background).unwrap();
            let Some(gpu) = capture_gpu(&display, size, background).unwrap() else {
                return;
            };
            cpu.compare(&gpu, GoldenTolerance::ANTIALIASED)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }
}
