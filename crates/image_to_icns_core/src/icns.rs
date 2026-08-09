use std::io::Cursor;

use icns::{IconFamily, IconType, Image as IcnsImage, PixelFormat};
use image::{RgbaImage, imageops};
use serde::{Deserialize, Serialize};

use crate::CoreError;

const STANDARD_REPRESENTATIONS: [(IconType, u32); 10] = [
    (IconType::RGBA32_16x16, 16),
    (IconType::RGBA32_16x16_2x, 32),
    (IconType::RGBA32_32x32, 32),
    (IconType::RGBA32_32x32_2x, 64),
    (IconType::RGBA32_128x128, 128),
    (IconType::RGBA32_128x128_2x, 256),
    (IconType::RGBA32_256x256, 256),
    (IconType::RGBA32_256x256_2x, 512),
    (IconType::RGBA32_512x512, 512),
    (IconType::RGBA32_512x512_2x, 1024),
];

/// Serializable verification result for a single ICNS representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcnsRepresentation {
    pub ostype: String,
    pub pixels: u32,
    pub logical_size: u32,
    pub scale: u32,
    pub data_len: usize,
}

/// Verification report for a complete ICNS file, shared by native and WASM consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcnsReport {
    pub byte_len: u64,
    pub representations: Vec<IcnsRepresentation>,
}

/// Generate ten modern macOS ICNS representations from a square RGBA master canvas.
///
/// Each representation is scaled independently from the same master canvas, avoiding
/// cumulative quality loss from successive downscaling. The output must still pass
/// through `verify_icns` before it can be considered deliverable.
pub fn encode_icns(master: &RgbaImage) -> Result<Vec<u8>, CoreError> {
    let (width, height) = master.dimensions();
    if width == 0 || height == 0 || width != height {
        return Err(CoreError::InvalidMasterImage { width, height });
    }

    let mut family = IconFamily::new();
    for (icon_type, size) in STANDARD_REPRESENTATIONS {
        let resized = imageops::resize(master, size, size, imageops::FilterType::Lanczos3);
        let icon = IcnsImage::from_data(PixelFormat::RGBA, size, size, resized.into_raw())
            .map_err(|error| CoreError::IcnsEncodeFailed(error.to_string()))?;
        family
            .add_icon_with_type(&icon, icon_type)
            .map_err(|error| CoreError::IcnsEncodeFailed(error.to_string()))?;
    }

    let mut bytes = Vec::new();
    family
        .write(&mut bytes)
        .map_err(|error| CoreError::IcnsEncodeFailed(error.to_string()))?;
    Ok(bytes)
}

/// Read back ICNS and verify each item's expected OSType, dimensions, pixel format, and data length.
///
/// Checking only the `icns` file header cannot detect missing Retina representations or
/// internal PNG corruption, so this function actually decodes all ten entries and produces
/// a machine-readable report in a fixed order.
pub fn verify_icns(bytes: &[u8]) -> Result<IcnsReport, CoreError> {
    let family = IconFamily::read(Cursor::new(bytes))
        .map_err(|error| CoreError::IcnsDecodeFailed(error.to_string()))?;
    let mut representations = Vec::with_capacity(STANDARD_REPRESENTATIONS.len());

    for (icon_type, expected_size) in STANDARD_REPRESENTATIONS {
        if !family.has_icon_with_type(icon_type) {
            return Err(CoreError::IcnsDecodeFailed(format!(
                "missing {} representation",
                icon_type.ostype()
            )));
        }
        let image = family
            .get_icon_with_type(icon_type)
            .map_err(|error| CoreError::IcnsDecodeFailed(error.to_string()))?;
        if image.width() != expected_size || image.height() != expected_size {
            return Err(CoreError::IcnsDecodeFailed(format!(
                "{} size should be {expected_size}x{expected_size}, but got {}x{}",
                icon_type.ostype(),
                image.width(),
                image.height()
            )));
        }
        if image.pixel_format() != PixelFormat::RGBA || image.data().is_empty() {
            return Err(CoreError::IcnsDecodeFailed(format!(
                "{} invalid pixel data",
                icon_type.ostype()
            )));
        }

        representations.push(IcnsRepresentation {
            ostype: icon_type.ostype().to_string(),
            pixels: expected_size,
            logical_size: icon_type.screen_width(),
            scale: icon_type.pixel_density(),
            data_len: image.data().len(),
        });
    }

    Ok(IcnsReport {
        byte_len: bytes.len() as u64,
        representations,
    })
}
