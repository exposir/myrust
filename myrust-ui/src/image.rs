//! Decoded RGBA image drawing contract.
//!
//! The framework does not decode PNG/JPEG/WebP unless the optional
//! `image-decode` feature is enabled. The caller can provide a borrowed RGBA8
//! buffer through [`ImageData`], or retain and share decoded pixels through
//! [`ImageHandle`]. Drawing emits one image command regardless of the output
//! size.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::context::Context;

/// Decoded RGBA8 source pixels.
#[derive(Debug, Clone, Copy)]
pub struct ImageData<'a> {
    /// RGBA8 pixels, row-major, four bytes per pixel.
    pub pixels: &'a [u8],
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
}

impl<'a> ImageData<'a> {
    /// Creates image data from a decoded RGBA8 buffer.
    pub const fn new(pixels: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }

    /// Returns the number of bytes required by the declared dimensions.
    pub fn required_len(&self) -> Option<usize> {
        self.width
            .checked_mul(self.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .map(|bytes| bytes as usize)
    }

    /// Whether the buffer has enough bytes for the declared size.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .required_len()
                .is_some_and(|required| self.pixels.len() >= required)
    }

    /// Copies the decoded pixels into a shareable handle.
    pub fn into_handle(self, id: impl Into<String>) -> Result<ImageHandle, ImageDataError> {
        ImageHandle::from_data(id, self)
    }
}

/// Error returned when decoded pixels do not match their dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageDataError {
    /// Width or height was zero.
    #[error("image dimensions must be non-zero")]
    EmptyDimensions,
    /// The RGBA buffer was shorter than the declared dimensions require.
    #[error("image buffer is too short: {actual} bytes for {required} required")]
    BufferTooSmall {
        /// Bytes required by the dimensions.
        required: usize,
        /// Bytes supplied by the caller.
        actual: usize,
    },
    /// The dimensions overflowed the addressable byte count.
    #[error("image dimensions overflow the RGBA byte count")]
    SizeOverflow,
}

/// Owned RGBA8 pixels with a cheap content version.
///
/// Clones share the pixel allocation. The version is derived from the exact
/// dimensions and bytes, so GPU and damage caches can identify a changed
/// image without walking its pixels on every frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePixels {
    pixels: Arc<[u8]>,
    width: u32,
    height: u32,
    version: u64,
    opaque: bool,
}

impl ImagePixels {
    /// Creates owned pixels from an RGBA8 buffer.
    pub fn new(pixels: Arc<[u8]>, width: u32, height: u32) -> Result<Self, ImageDataError> {
        if width == 0 || height == 0 {
            return Err(ImageDataError::EmptyDimensions);
        }
        let required = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(ImageDataError::SizeOverflow)? as usize;
        if pixels.len() < required {
            return Err(ImageDataError::BufferTooSmall {
                required,
                actual: pixels.len(),
            });
        }
        let pixels = if pixels.len() == required {
            pixels
        } else {
            Arc::from(&pixels[..required])
        };
        let version = image_version(&pixels, width, height);
        let opaque = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255);
        Ok(Self {
            pixels,
            width,
            height,
            version,
            opaque,
        })
    }

    /// Returns the decoded RGBA8 bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns the source width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the source height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the content version derived from dimensions and pixels.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns whether every source pixel is fully opaque.
    pub const fn is_opaque(&self) -> bool {
        self.opaque
    }

    /// Returns a borrowed view of these pixels.
    pub fn as_data(&self) -> ImageData<'_> {
        ImageData::new(&self.pixels, self.width, self.height)
    }
}

/// Shareable image source carried by [`DrawCommand::Image`](crate::paint::DrawCommand::Image).
///
/// `ImageHandle::new` preserves the legacy unresolved-handle constructor.
/// Use [`ImageHandle::from_data`] or [`ImageHandle::from_pixels`] for a source
/// the presenters can draw.
#[derive(Debug, Clone)]
pub struct ImageHandle {
    id: Arc<str>,
    data: Option<Arc<ImagePixels>>,
}

impl ImageHandle {
    /// Creates an unresolved image handle from a stable source identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Arc::from(id.into()),
            data: None,
        }
    }

    /// Copies borrowed decoded pixels into a resolved handle.
    pub fn from_data(
        id: impl Into<String>,
        data: ImageData<'_>,
    ) -> Result<Self, ImageDataError> {
        if !data.is_valid() {
            if data.width == 0 || data.height == 0 {
                return Err(ImageDataError::EmptyDimensions);
            }
            let required = data.required_len().ok_or(ImageDataError::SizeOverflow)?;
            return Err(ImageDataError::BufferTooSmall {
                required,
                actual: data.pixels.len(),
            });
        }
        let pixels: Arc<[u8]> = Arc::from(data.pixels);
        Self::from_shared(id, pixels, data.width, data.height)
    }

    /// Creates a resolved handle over an existing shared RGBA8 allocation.
    pub fn from_shared(
        id: impl Into<String>,
        pixels: Arc<[u8]>,
        width: u32,
        height: u32,
    ) -> Result<Self, ImageDataError> {
        let data = ImagePixels::new(pixels, width, height)?;
        Ok(Self {
            id: Arc::from(id.into()),
            data: Some(Arc::new(data)),
        })
    }

    /// Creates a resolved handle from owned pixels without copying them again.
    pub fn from_pixels(id: impl Into<String>, data: ImagePixels) -> Self {
        Self {
            id: Arc::from(id.into()),
            data: Some(Arc::new(data)),
        }
    }

    /// Returns the stable source identifier.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// Returns the decoded pixels when this handle is resolved.
    pub fn data(&self) -> Option<&ImagePixels> {
        self.data.as_deref()
    }

    /// Returns whether this handle carries decoded pixels.
    pub const fn is_resolved(&self) -> bool {
        self.data.is_some()
    }

    /// Returns the content version, or zero for an unresolved handle.
    pub fn version(&self) -> u64 {
        self.data
            .as_ref()
            .map_or(0, |pixels| pixels.version())
    }

    /// Returns whether every pixel is opaque.
    pub fn is_opaque(&self) -> bool {
        self.data.as_ref().is_some_and(|pixels| pixels.is_opaque())
    }

    #[cfg(feature = "gpu")]
    pub(crate) fn cache_key(&self, grayscale: bool) -> Option<ImageTextureKey> {
        let data = self.data.as_ref()?;
        Some(ImageTextureKey {
            id: Arc::clone(&self.id),
            version: data.version(),
            format: if grayscale && data.is_opaque() {
                ImageFormat::R8
            } else {
                ImageFormat::Rgba8
            },
        })
    }
}

impl PartialEq for ImageHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && match (&self.data, &other.data) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    left.version == right.version
                        && left.width == right.width
                        && left.height == right.height
                }
                _ => false,
            }
    }
}

impl Eq for ImageHandle {}

impl Hash for ImageHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        match &self.data {
            Some(data) => {
                data.version.hash(state);
                data.width.hash(state);
                data.height.hash(state);
            }
            None => 0_u8.hash(state),
        }
    }
}

#[cfg(feature = "gpu")]
#[derive(Debug, Clone)]
pub(crate) struct ImageTextureKey {
    pub(crate) id: Arc<str>,
    pub(crate) version: u64,
    pub(crate) format: ImageFormat,
}

#[cfg(feature = "gpu")]
impl PartialEq for ImageTextureKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.version == other.version
            && self.format == other.format
    }
}

#[cfg(feature = "gpu")]
impl Eq for ImageTextureKey {}

#[cfg(feature = "gpu")]
impl Hash for ImageTextureKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.version.hash(state);
        self.format.hash(state);
    }
}

#[cfg(feature = "gpu")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImageFormat {
    R8,
    Rgba8,
}

#[derive(Serialize)]
struct ImageHandleWire<'a> {
    id: &'a str,
    width: u32,
    height: u32,
    version: u64,
    pixels: &'a [u8],
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ImageHandleInput {
    Id(String),
    Data(ImageHandleData),
}

#[derive(Deserialize)]
struct ImageHandleData {
    id: String,
    width: u32,
    height: u32,
    #[serde(default)]
    version: u64,
    pixels: Vec<u8>,
}

impl Serialize for ImageHandle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.data {
            None => serializer.serialize_str(&self.id),
            Some(data) => ImageHandleWire {
                id: &self.id,
                width: data.width,
                height: data.height,
                version: data.version,
                pixels: &data.pixels,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ImageHandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match ImageHandleInput::deserialize(deserializer)? {
            ImageHandleInput::Id(id) => Ok(Self::new(id)),
            ImageHandleInput::Data(data) => {
                let handle =
                    Self::from_shared(data.id, Arc::from(data.pixels), data.width, data.height)
                        .map_err(serde::de::Error::custom)?;
                if data.version != 0 && data.version != handle.version() {
                    return Err(serde::de::Error::custom(
                        "serialized image version does not match its pixels",
                    ));
                }
                Ok(handle)
            }
        }
    }
}

fn image_version(pixels: &[u8], width: u32, height: u32) -> u64 {
    // FNV-1a is a content identity, not a security primitive. The handle id
    // remains part of every cache key, so a collision only matters within one
    // source identifier.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in width
        .to_le_bytes()
        .into_iter()
        .chain(height.to_le_bytes())
        .chain(pixels.iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Result of drawing an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImageState {
    /// Whether a valid image with a positive target was drawn.
    pub drawn: bool,
    /// Logical target width.
    pub width: u32,
    /// Logical target height.
    pub height: u32,
    /// Number of visible output columns.
    pub columns: usize,
    /// Number of visible output rows.
    pub rows: usize,
    /// Declared source width.
    pub source_width: u32,
    /// Declared source height.
    pub source_height: u32,
}

/// Draws decoded RGBA8 pixels scaled into a target rectangle.
///
/// `target_width`/`target_height` are logical pixels. Zero or non-finite
/// targets emit no image command and report `drawn = false`.
pub fn image(
    context: &mut Context,
    id: &str,
    data: ImageData<'_>,
    target_width: f32,
    target_height: f32,
) -> ImageState {
    context.image(id, data, target_width, target_height)
}

/// Draws an image with MyGo's Fit, corner radius and grayscale.
pub fn image_styled(
    context: &mut Context,
    id: &str,
    data: ImageData<'_>,
    target_width: f32,
    target_height: f32,
    style: super::context::ImageStyle,
) -> ImageState {
    context.image_styled(id, data, target_width, target_height, style)
}

/// Draws an already-shared image handle without copying its pixels.
pub fn image_handle(
    context: &mut Context,
    id: &str,
    source: ImageHandle,
    target_width: f32,
    target_height: f32,
) -> ImageState {
    context.image_handle(
        id,
        source,
        target_width,
        target_height,
        super::context::ImageStyle::default(),
    )
}

/// Draws a shared image handle with Fit, corner radius and grayscale.
pub fn image_styled_handle(
    context: &mut Context,
    id: &str,
    source: ImageHandle,
    target_width: f32,
    target_height: f32,
    style: super::context::ImageStyle,
) -> ImageState {
    context.image_handle(id, source, target_width, target_height, style)
}

/// Decodes a PNG, JPEG or WebP into a shareable RGBA8 source.
///
/// This is available only with the optional `image-decode` feature. Animated
/// images use their first frame; GIF and other formats are not linked.
#[cfg(feature = "image-decode")]
pub fn decode_image(bytes: &[u8]) -> Result<ImagePixels, ImageDecodeError> {
    let decoded =
        image::load_from_memory(bytes).map_err(|error| ImageDecodeError::Decode(error.to_string()))?;
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    ImagePixels::new(Arc::from(rgba.into_raw()), width, height)
        .map_err(ImageDecodeError::InvalidPixels)
}

/// Error returned while decoding compressed image bytes.
#[cfg(feature = "image-decode")]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageDecodeError {
    /// The decoder rejected the input or its format is not enabled.
    #[error("image decode failed: {0}")]
    Decode(String),
    /// The decoded buffer did not match the decoded dimensions.
    #[error("decoded image pixels are invalid: {0}")]
    InvalidPixels(ImageDataError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Size;
    use crate::paint::{DisplayList, DrawCommand, ImageSourceRect};

    fn sample_pixels() -> [u8; 16] {
        [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]
    }

    #[test]
    fn image_scales_rgba_pixels_with_one_shared_command() {
        let pixels = sample_pixels();
        let data = ImageData::new(&pixels, 2, 2);
        let size = Size::new(80.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let drawn = image(&mut context, "image", data, 4.0, 4.0);
        let zero = image(&mut context, "image-zero", data, 0.0, 0.0);
        let invalid = image(
            &mut context,
            "image-invalid",
            ImageData::new(&pixels[..8], 2, 2),
            4.0,
            4.0,
        );
        let frame = context.produce().unwrap();
        assert!(drawn.drawn);
        assert_eq!(drawn.columns, 4);
        assert_eq!(drawn.rows, 4);
        assert_eq!(
            frame
                .display
                .commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::Image { .. }))
                .count(),
            1
        );
        assert_eq!(
            frame
                .display
                .commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::Rect { .. }))
                .count(),
            0
        );
        let image = frame
            .display
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Image { source, .. } => Some(source),
                _ => None,
            })
            .expect("one image command");
        assert!(image.is_resolved());
        assert_eq!(image.data().unwrap().pixels(), &pixels);
        assert!(!zero.drawn);
        assert_eq!(zero.width, 0);
        assert_eq!(zero.height, 0);
        assert!(!invalid.drawn);
    }

    #[test]
    fn shared_handles_compare_by_content_version() {
        let left = ImageHandle::from_data("same", ImageData::new(&sample_pixels(), 2, 2)).unwrap();
        let right = left.clone();
        assert_eq!(left, right);
        let mut changed = sample_pixels();
        changed[0] = 0;
        let other = ImageHandle::from_data("same", ImageData::new(&changed, 2, 2)).unwrap();
        assert_ne!(left, other);
        assert_ne!(left.version(), other.version());
    }

    #[test]
    fn serialized_images_round_trip_without_losing_pixels() {
        let pixels = sample_pixels();
        let handle = ImageHandle::from_data("round-trip", ImageData::new(&pixels, 2, 2)).unwrap();
        let json = serde_json::to_string(&handle).unwrap();
        let decoded: ImageHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, handle);
        assert_eq!(decoded.data().unwrap().pixels(), &pixels);
    }

    #[test]
    fn legacy_string_handle_still_deserializes() {
        let handle: ImageHandle = serde_json::from_str("\"legacy\"").unwrap();
        assert_eq!(handle.as_str(), "legacy");
        assert!(!handle.is_resolved());
    }

    #[test]
    fn display_list_translation_moves_image_state_commands() {
        let pixels = sample_pixels();
        let source = ImageHandle::from_data("move", ImageData::new(&pixels, 2, 2)).unwrap();
        let mut display = DisplayList::from_commands(vec![
            DrawCommand::ScissorPush {
                rect: crate::paint::Rect::new(0.0, 0.0, 4.0, 4.0),
            },
            DrawCommand::Image {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
                source,
                source_rect: ImageSourceRect::default(),
                radius: crate::paint::Radius::default(),
                grayscale: false,
            },
            DrawCommand::ScissorPop,
        ]);
        display.translate(crate::layout::Point::new(3.0, 5.0));
        let DrawCommand::Image { x, y, .. } = &display.commands[1] else {
            panic!("image remains second");
        };
        assert_eq!((*x, *y), (3.0, 5.0));
        let DrawCommand::ScissorPush { rect } = &display.commands[0] else {
            panic!("scissor remains first");
        };
        assert_eq!((rect.x, rect.y), (3.0, 5.0));
    }

    #[test]
    fn image_style_keeps_fit_radius_and_grayscale() {
        let pixels = [
            255_u8, 0, 0, 255, 0, 255, 0, 255,
        ];
        let size = Size::new(80.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let state = image_styled(
            &mut context,
            "contained",
            ImageData::new(&pixels, 2, 1),
            40.0,
            40.0,
            crate::context::ImageStyle {
                fit: crate::context::ImageFit::Contain,
                radius: 6.0,
                grayscale: true,
            },
        );
        assert!(state.drawn);
        let frame = context.produce().unwrap();
        let image = frame
            .display
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Image {
                    x,
                    y,
                    width,
                    height,
                    radius,
                    grayscale,
                    ..
                } => Some((*x, *y, *width, *height, *radius, *grayscale)),
                _ => None,
            })
            .expect("contained image command");
        assert_eq!(image.0, 0.0);
        assert_eq!(image.1, 10.0);
        assert_eq!(image.2, 40.0);
        assert_eq!(image.3, 20.0);
        assert_eq!(image.4, crate::paint::Radius::all(6.0));
        assert!(image.5);
    }

    #[test]
    fn opaque_flag_tracks_alpha() {
        let opaque =
            ImagePixels::new(Arc::from([255_u8, 0, 0, 255].as_slice()), 1, 1).unwrap();
        assert!(opaque.is_opaque());
        let translucent =
            ImagePixels::new(Arc::from([255_u8, 0, 0, 128].as_slice()), 1, 1).unwrap();
        assert!(!translucent.is_opaque());
    }

    #[cfg(feature = "image-decode")]
    #[test]
    fn optional_feature_decodes_png_to_shared_rgba() {
        use std::io::Cursor;

        let image = image::RgbaImage::from_raw(1, 1, vec![12, 34, 56, 78]).unwrap();
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_image(encoded.get_ref()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1, 1));
        assert_eq!(decoded.pixels(), &[12, 34, 56, 78]);
    }
}
