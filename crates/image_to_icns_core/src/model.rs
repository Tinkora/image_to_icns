use image::Rgba;
use serde::{Deserialize, Serialize};

use crate::CoreError;

/// Serializable crop state shared by all frontends with the same parameter semantics.
///
/// `zoom = 1.0` means using the source's short side as the square selection; the center
/// point uses normalized coordinates from 0 to 1. Reject non-finite values on
/// construction to avoid them leaking into pixel and dimension calculations.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CropTransform {
    zoom: f32,
    center_x: f32,
    center_y: f32,
}

impl CropTransform {
    pub fn new(zoom: f32, center_x: f32, center_y: f32) -> Result<Self, CoreError> {
        if !zoom.is_finite() || zoom < 1.0 {
            return Err(CoreError::InvalidTransform(
                "zoom must be a finite number not less than 1",
            ));
        }
        if !center_x.is_finite() || !center_y.is_finite() {
            return Err(CoreError::InvalidTransform(
                "center point must be a finite number",
            ));
        }

        Ok(Self {
            zoom,
            center_x: center_x.clamp(0.0, 1.0),
            center_y: center_y.clamp(0.0, 1.0),
        })
    }

    pub const fn zoom(self) -> f32 {
        self.zoom
    }

    pub const fn center_x(self) -> f32 {
        self.center_x
    }

    pub const fn center_y(self) -> f32 {
        self.center_y
    }
}

impl Default for CropTransform {
    fn default() -> Self {
        // SAFETY: 1.0, 0.5, 0.5 are all finite and zoom >= 1.0
        Self::new(1.0, 0.5, 0.5).expect("default CropTransform values are valid")
    }
}

/// Square export canvas dimensions and transparent pixel compositing background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasOptions {
    size: u32,
    background: Rgba<u8>,
}

impl Serialize for CanvasOptions {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CanvasOptions", 2)?;
        s.serialize_field("size", &self.size)?;
        s.serialize_field("background", &self.background.0)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for CanvasOptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            size: u32,
            background: [u8; 4],
        }
        let raw = Raw::deserialize(deserializer)?;
        CanvasOptions::new(raw.size, Rgba(raw.background)).map_err(serde::de::Error::custom)
    }
}

impl CanvasOptions {
    pub fn new(size: u32, background: Rgba<u8>) -> Result<Self, CoreError> {
        if size == 0 {
            return Err(CoreError::InvalidCanvasSize);
        }
        Ok(Self { size, background })
    }

    pub fn transparent(size: u32) -> Result<Self, CoreError> {
        Self::new(size, Rgba([0, 0, 0, 0]))
    }

    pub const fn size(self) -> u32 {
        self.size
    }

    pub const fn background(self) -> Rgba<u8> {
        self.background
    }
}
