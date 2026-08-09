//! File import module: read images from browser File API and decode to RGBA pixels.

use image_to_icns_core::{MAX_INPUT_BYTES, SourceFormat, WasmImage, decode_bytes};
use wasm_bindgen::prelude::*;
use web_sys::File;

/// Import image from browser File object.
///
/// Returns the decoded `WasmImage`, which JS can use to initialize an editor.
/// Format is inferred from the file extension (supports png / jpeg / svg).
#[wasm_bindgen]
pub async fn import_file(file: File) -> Result<WasmImage, JsValue> {
    if file.size() > MAX_INPUT_BYTES as f64 {
        return Err(JsValue::from_str(&format!(
            "input file exceeds the {} MiB limit",
            MAX_INPUT_BYTES / (1024 * 1024)
        )));
    }
    let name = file.name().to_lowercase();
    let format = if name.ends_with(".svg") {
        SourceFormat::Svg
    } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        SourceFormat::Jpeg
    } else if name.ends_with(".png") {
        SourceFormat::Png
    } else {
        return Err(JsValue::from_str(&format!(
            "unsupported file format, please use PNG / JPEG / SVG. received: {name}"
        )));
    };

    let bytes = read_file_as_bytes(&file).await?;
    let image = decode_bytes(&bytes, format)
        .map_err(|e| JsValue::from_str(&format!("{}: {}", e.code(), e)))?;
    Ok(WasmImage::from(image))
}

/// Read an entire browser `File` asynchronously using `Blob.arrayBuffer()`.
async fn read_file_as_bytes(file: &File) -> Result<Vec<u8>, JsValue> {
    let array_buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await?;
    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    let mut bytes = vec![0u8; uint8_array.length() as usize];
    uint8_array.copy_to(&mut bytes);
    Ok(bytes)
}
