//! WASM entry points for the browser editor.
//!
//! All functions marked `#[wasm_bindgen]`; parameters and return values are
//! limited to wasm-bindgen supported types. Errors are returned as
//! JSON-serialized strings, keeping stable error codes consistent with native
//! CoreError.

use image::RgbaImage;
use wasm_bindgen::prelude::*;

use crate::{
    CanvasOptions, CoreError, CropTransform, IcnsReport, SourceFormat, decode_bytes, encode_icns,
    render_square, verify_icns,
};

/// RGBA pixel buffer for JS-side ownership.
///
/// Pixels arranged as row-major RGBA (straight alpha), matching browser
/// `ImageData` order.
#[wasm_bindgen]
pub struct WasmImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl std::fmt::Debug for WasmImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("data_len", &self.data.len())
            .finish()
    }
}

impl Clone for WasmImage {
    fn clone(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            data: self.data.clone(),
        }
    }
}

#[wasm_bindgen]
impl WasmImage {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Return a copy of the RGBA pixel bytes for JavaScript ownership.
    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }
}

impl From<RgbaImage> for WasmImage {
    fn from(image: RgbaImage) -> Self {
        let (width, height) = image.dimensions();
        Self {
            width,
            height,
            data: image.into_raw(),
        }
    }
}

impl From<WasmImage> for RgbaImage {
    /// Consume the WASM wrapper and reuse its pixel allocation.
    fn from(img: WasmImage) -> Self {
        RgbaImage::from_raw(img.width, img.height, img.data)
            .expect("WasmImage dimensions and data length are always consistent")
    }
}

impl WasmImage {
    /// Clone pixel data and construct `RgbaImage` without consuming self.
    pub fn to_rgba(&self) -> RgbaImage {
        RgbaImage::from_raw(self.width, self.height, self.data.clone())
            .expect("WasmImage dimensions and data length are always consistent")
    }
}

/// Decode a byte buffer into RGBA pixels.
///
/// `format` must be one of `"png"`, `"jpeg"`, or `"svg"`.
#[wasm_bindgen]
pub fn wasm_decode(bytes: Vec<u8>, format: String) -> Result<WasmImage, JsValue> {
    let format = parse_format(&format)?;
    let image = decode_bytes(&bytes, format).map_err(core_error_to_js)?;
    Ok(WasmImage::from(image))
}

/// Apply crop, scale and background compositing to a decoded image.
///
/// `transform_json` and `options_json` are JSON representations of
/// `CropTransform` and `CanvasOptions` respectively.
#[wasm_bindgen]
pub fn wasm_render(
    image: &WasmImage,
    transform_json: String,
    options_json: String,
) -> Result<WasmImage, JsValue> {
    let source = RgbaImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| JsValue::from_str("WasmImage pixel data inconsistent"))?;

    let transform: CropTransform =
        serde_json::from_str(&transform_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let options: CanvasOptions =
        serde_json::from_str(&options_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let rendered = render_square(&source, transform, options).map_err(core_error_to_js)?;
    Ok(WasmImage::from(rendered))
}

/// Generate ICNS file bytes from a square RGBA master canvas.
#[wasm_bindgen]
pub fn wasm_encode(image: &WasmImage) -> Result<Vec<u8>, JsValue> {
    let master = RgbaImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| JsValue::from_str("WasmImage pixel data inconsistent"))?;
    encode_icns(&master).map_err(core_error_to_js)
}

/// Read back ICNS bytes and return a JSON verification report.
///
/// Report structure matches the serialization of `IcnsReport`.
#[wasm_bindgen]
pub fn wasm_verify(bytes: Vec<u8>) -> Result<String, JsValue> {
    let report: IcnsReport = verify_icns(&bytes).map_err(core_error_to_js)?;
    serde_json::to_string(&report).map_err(|e| JsValue::from_str(&e.to_string()))
}

// ── Internal helpers ────────────────────────────────────────────────────

fn parse_format(s: &str) -> Result<SourceFormat, JsValue> {
    match s {
        "png" => Ok(SourceFormat::Png),
        "jpeg" | "jpg" => Ok(SourceFormat::Jpeg),
        "svg" => Ok(SourceFormat::Svg),
        other => Err(JsValue::from_str(&format!(
            "unsupported format \"{other}\", expected png / jpeg / svg"
        ))),
    }
}

/// Convert CoreError to a `"CODE: message"` string for JS-side parsing.
fn core_error_to_js(error: CoreError) -> JsValue {
    JsValue::from_str(&format!("{}: {}", error.code(), error))
}
