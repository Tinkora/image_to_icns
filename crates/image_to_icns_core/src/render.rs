use image::{Rgba, RgbaImage, imageops};

use crate::{CanvasOptions, CoreError, CropTransform};

/// Render an arbitrary-size RGBA image into a square canvas using the unified crop model.
///
/// Selection is always clamped within source bounds, so background color never masks
/// out-of-bounds drags. Background color only composites with the source image's own
/// transparent pixels; scaling uses Lanczos3, shared by GUI preview and final export.
pub fn render_square(
    source: &RgbaImage,
    transform: CropTransform,
    options: CanvasOptions,
) -> Result<RgbaImage, CoreError> {
    let (source_width, source_height) = source.dimensions();
    if source_width == 0 || source_height == 0 {
        return Err(CoreError::EmptySource);
    }

    let short_side = source_width.min(source_height) as f32;
    let crop_side = short_side / transform.zoom();
    if !crop_side.is_finite() || crop_side < 1.0 {
        return Err(CoreError::CropTooSmall);
    }

    let crop_size = (crop_side.round() as u32).clamp(1, source_width.min(source_height));
    let crop_x = crop_origin(source_width, crop_size, transform.center_x());
    let crop_y = crop_origin(source_height, crop_size, transform.center_y());
    let cropped = imageops::crop_imm(source, crop_x, crop_y, crop_size, crop_size).to_image();
    let mut rendered = imageops::resize(
        &cropped,
        options.size(),
        options.size(),
        imageops::FilterType::Lanczos3,
    );

    let background = options.background();
    if background != Rgba([0, 0, 0, 0]) {
        for pixel in rendered.pixels_mut() {
            *pixel = composite_over(*pixel, background);
        }
    }

    Ok(rendered)
}

/// First position by normalized center, then constrain the center with the current
/// selection radius to ensure the crop box never goes out of bounds.
fn crop_origin(source_size: u32, crop_size: u32, normalized_center: f32) -> u32 {
    let source_size = source_size as f32;
    let crop_size = crop_size as f32;
    let half_crop = crop_size / 2.0;
    let center = (normalized_center * source_size)
        .clamp(half_crop, (source_size - half_crop).max(half_crop));
    (center - half_crop).round() as u32
}

/// Use source-over formula with straight (non-premultiplied) RGBA, correctly preserving
/// semi-transparent background and transparent output.
fn composite_over(source: Rgba<u8>, destination: Rgba<u8>) -> Rgba<u8> {
    let source_alpha = u32::from(source[3]);
    let destination_alpha = u32::from(destination[3]);
    let inverse_source_alpha = 255 - source_alpha;
    let output_alpha = source_alpha + (destination_alpha * inverse_source_alpha + 127) / 255;

    if output_alpha == 0 {
        return Rgba([0, 0, 0, 0]);
    }

    let mut output = [0_u8; 4];
    for channel in 0..3 {
        let source_term = u32::from(source[channel]) * source_alpha * 255;
        let destination_term =
            u32::from(destination[channel]) * destination_alpha * inverse_source_alpha;
        output[channel] =
            ((source_term + destination_term + output_alpha * 127) / (output_alpha * 255)) as u8;
    }
    output[3] = output_alpha as u8;
    Rgba(output)
}
