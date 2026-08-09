use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use resvg::{tiny_skia, usvg};

use crate::CoreError;

#[cfg(target_os = "macos")]
mod pdf_macos;

/// Maximum encoded source size accepted by native and browser entry points.
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PIXELS: u64 = MAX_DECODED_BYTES / 4;

/// Supported source image formats, excluding PDF (PDF is only available from native file paths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    Png,
    Jpeg,
    Svg,
}

impl SourceFormat {
    /// Infer format from file extension.
    fn from_path(path: &Path) -> Result<Self, CoreError> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        match extension.as_str() {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "svg" => Ok(Self::Svg),
            _ => Err(CoreError::UnsupportedFormat(if extension.is_empty() {
                "missing extension".to_owned()
            } else {
                extension
            })),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Svg => "SVG",
        }
    }
}

/// Decode in-memory image bytes into unpremultiplied RGBA pixels.
///
/// Works in both native and WASM contexts; the caller is responsible for providing the correct
/// format. Input bytes are always treated as untrusted: the function first validates the byte
/// count upper bound, then restricts decode dimensions and memory per format. SVG will not
/// load external resources or access the filesystem.
pub fn decode_bytes(bytes: &[u8], format: SourceFormat) -> Result<RgbaImage, CoreError> {
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(CoreError::InputTooLarge {
            size: bytes.len() as u64,
            max: MAX_INPUT_BYTES,
        });
    }

    match format {
        SourceFormat::Png => decode_raster_from_bytes(bytes, format, ImageFormat::Png),
        SourceFormat::Jpeg => decode_raster_from_bytes(bytes, format, ImageFormat::Jpeg),
        SourceFormat::Svg => decode_svg_from_bytes(bytes),
    }
}

/// Decode a supported source file into unpremultiplied RGBA pixels.
///
/// The path comes from a user or agent and must be treated as untrusted: first limit the file
/// size, then restrict decode dimensions and memory per format. This function does not modify
/// the source file and does not allow SVG to load external resources.
///
/// PNG/JPEG/SVG are read into memory and delegated to `decode_bytes`; PDF is only supported
/// on macOS and requires a file path, taking a separate PDFKit path.
pub fn decode_source(path: &Path) -> Result<RgbaImage, CoreError> {
    let metadata = fs::metadata(path)
        .map_err(|error| CoreError::InputIo(format!("{}: {error}", path.to_string_lossy())))?;
    if !metadata.is_file() {
        return Err(CoreError::InputIo(format!(
            "{} is not a regular file",
            path.to_string_lossy()
        )));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(CoreError::InputTooLarge {
            size: metadata.len(),
            max: MAX_INPUT_BYTES,
        });
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    // PDF decoding requires a file path (PDFKit's initWithURL:) and does not go through decode_bytes.
    if extension == "pdf" {
        return decode_pdf(path);
    }

    let format = SourceFormat::from_path(path)?;
    let data = fs::read(path)
        .map_err(|error| CoreError::InputIo(format!("{}: {error}", path.to_string_lossy())))?;
    decode_bytes(&data, format)
}

fn decode_raster_from_bytes(
    bytes: &[u8],
    kind: SourceFormat,
    format: ImageFormat,
) -> Result<RgbaImage, CoreError> {
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes));
    reader.set_format(format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);

    reader
        .decode()
        .map_err(|error| CoreError::DecodeFailed {
            format: kind.label(),
            reason: error.to_string(),
        })
        .map(|image| image.into_rgba8())
}

fn decode_svg_from_bytes(data: &[u8]) -> Result<RgbaImage, CoreError> {
    if data.starts_with(&[0x1f, 0x8b]) {
        return Err(svg_decode_failed("compressed SVG input is not supported"));
    }

    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();

    // Resolver callbacks cannot return errors, so preserve the first rejection and surface it
    // after parsing. This prevents usvg from silently dropping an unsafe image and succeeding.
    let resolver_error = Arc::new(Mutex::new(None));
    let data_resolver_error = Arc::clone(&resolver_error);
    let string_resolver_error = Arc::clone(&resolver_error);
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: Box::new(
            move |mime, data, _| match resolve_embedded_raster(mime, data) {
                Ok(image) => Some(image),
                Err(error) => {
                    store_resolver_error(&data_resolver_error, error);
                    None
                }
            },
        ),
        resolve_string: Box::new(move |_, _| {
            store_resolver_error(
                &string_resolver_error,
                svg_decode_failed("external SVG image references are not supported"),
            );
            None
        }),
    };
    options.resources_dir = None;

    let tree_result = usvg::Tree::from_data(data, &options);
    if let Some(error) = take_resolver_error(&resolver_error) {
        return Err(error);
    }
    let tree = tree_result.map_err(|error| CoreError::DecodeFailed {
        format: SourceFormat::Svg.label(),
        reason: error.to_string(),
    })?;
    let size = tree.size().to_int_size();
    enforce_dimensions(size.width(), size.height())?;

    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).ok_or_else(|| {
        CoreError::DecodeFailed {
            format: SourceFormat::Svg.label(),
            reason: "failed to allocate SVG pixel buffer".to_owned(),
        }
    })?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );

    // tiny-skia uses premultiplied alpha; image::RgbaImage expects unpremultiplied,
    // so we must convert pixel by pixel — otherwise translucent SVG edges will
    // darken during subsequent scaling and background compositing.
    let mut rgba = Vec::with_capacity(size.width() as usize * size.height() as usize * 4);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }

    RgbaImage::from_raw(size.width(), size.height(), rgba).ok_or_else(|| CoreError::DecodeFailed {
        format: SourceFormat::Svg.label(),
        reason: "SVG pixel buffer size mismatch".to_owned(),
    })
}

fn resolve_embedded_raster(mime: &str, data: Arc<Vec<u8>>) -> Result<usvg::ImageKind, CoreError> {
    if mime.eq_ignore_ascii_case("image/svg+xml") {
        return Err(svg_decode_failed(
            "embedded SVG and SVGZ images are not supported",
        ));
    }

    let (format, invalid_header_reason, is_png) = if mime.eq_ignore_ascii_case("image/png") {
        (ImageFormat::Png, "embedded PNG header is invalid", true)
    } else if mime.eq_ignore_ascii_case("image/jpeg") || mime.eq_ignore_ascii_case("image/jpg") {
        (ImageFormat::Jpeg, "embedded JPEG header is invalid", false)
    } else {
        return Err(svg_decode_failed("unsupported embedded image type"));
    };

    let (width, height) = ImageReader::with_format(std::io::Cursor::new(data.as_slice()), format)
        .into_dimensions()
        .map_err(|_| svg_decode_failed(invalid_header_reason))?;
    enforce_dimensions(width, height)?;

    if is_png {
        Ok(usvg::ImageKind::PNG(data))
    } else {
        Ok(usvg::ImageKind::JPEG(data))
    }
}

fn svg_decode_failed(reason: &'static str) -> CoreError {
    CoreError::DecodeFailed {
        format: SourceFormat::Svg.label(),
        reason: reason.to_owned(),
    }
}

fn store_resolver_error(error_slot: &Mutex<Option<CoreError>>, error: CoreError) {
    let mut error_slot = error_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if error_slot.is_none() {
        *error_slot = Some(error);
    }
}

fn take_resolver_error(error_slot: &Mutex<Option<CoreError>>) -> Option<CoreError> {
    error_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

fn enforce_dimensions(width: u32, height: u32) -> Result<(), CoreError> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || pixels > MAX_PIXELS
    {
        return Err(CoreError::ImageTooLarge { width, height });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn decode_pdf(path: &Path) -> Result<RgbaImage, CoreError> {
    let image = pdf_macos::decode_first_page(path)?;
    enforce_dimensions(image.width(), image.height())?;
    Ok(image)
}

#[cfg(not(target_os = "macos"))]
fn decode_pdf(_path: &Path) -> Result<RgbaImage, CoreError> {
    Err(CoreError::PdfUnsupportedOnPlatform)
}
