//! Editor core: crop state, canvas preview, ICNS generation and download.

use image::RgbaImage;
use image_to_icns_core::{
    CanvasOptions, CropTransform, WasmImage, encode_icns, render_square, verify_icns,
};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Browser-side editor instance.
///
/// One Editor corresponds to one imported source image. JS side calls its methods for cropping, preview, and export.
#[wasm_bindgen]
pub struct Editor {
    source: RgbaImage,
    transform: CropTransform,
    canvas_options: CanvasOptions,
}

#[wasm_bindgen]
impl Editor {
    /// Create editor from a decoded source image.
    ///
    /// Default crop parameters: image center at 100% zoom; canvas is 512px with transparent background.
    #[wasm_bindgen(constructor)]
    pub fn new(source: WasmImage) -> Editor {
        Editor {
            source: source.into(),
            transform: CropTransform::default(),
            canvas_options: CanvasOptions::transparent(512).expect("512 is a valid size"),
        }
    }

    /// Return source image width in pixels.
    #[wasm_bindgen(getter)]
    pub fn source_width(&self) -> u32 {
        self.source.width()
    }

    /// Return source image height in pixels.
    #[wasm_bindgen(getter)]
    pub fn source_height(&self) -> u32 {
        self.source.height()
    }

    // ── Crop parameters ────────────────────────────────────

    /// Set zoom level (≥1.0).
    pub fn set_zoom(&mut self, zoom: f32) {
        if zoom.is_finite() && zoom >= 1.0 {
            self.transform =
                CropTransform::new(zoom, self.transform.center_x(), self.transform.center_y())
                    .unwrap_or_default();
        }
    }

    /// Get current zoom level.
    pub fn zoom(&self) -> f32 {
        self.transform.zoom()
    }

    /// Set crop center (0-1 normalized coordinates).
    pub fn set_center(&mut self, x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            self.transform = CropTransform::new(self.transform.zoom(), x, y).unwrap_or_default();
        }
    }

    /// Get current crop center X.
    pub fn center_x(&self) -> f32 {
        self.transform.center_x()
    }

    /// Get current crop center Y.
    pub fn center_y(&self) -> f32 {
        self.transform.center_y()
    }

    // ── Canvas preview ──────────────────────────────────

    /// Render current crop result to the given canvas.
    ///
    /// Preview uses the editor's `canvas_options.size` as output dimensions.
    pub fn preview(&self, canvas: &HtmlCanvasElement) -> Result<(), JsValue> {
        let rendered = render_square(&self.source, self.transform, self.canvas_options)
            .map_err(|e| JsValue::from_str(&format!("{}: {}", e.code(), e)))?;

        let ctx = canvas
            .get_context("2d")
            .map_err(|_| JsValue::from_str("failed to get Canvas 2D context"))?
            .ok_or_else(|| JsValue::from_str("Canvas 2D context is null"))?;
        let ctx: web_sys::CanvasRenderingContext2d = ctx.dyn_into().unwrap();

        let size = self.canvas_options.size();
        canvas.set_width(size);
        canvas.set_height(size);

        let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&rendered.into_raw()),
            size,
            size,
        )
        .map_err(|_| JsValue::from_str("failed to create ImageData"))?;
        ctx.put_image_data(&image_data, 0.0, 0.0)
            .map_err(|_| JsValue::from_str("failed to write to Canvas"))?;
        Ok(())
    }

    // ── ICNS export and download ─────────────────────────

    /// Generate ICNS file bytes from a 1024px master canvas.
    pub fn generate_icns(&self) -> Result<Vec<u8>, JsValue> {
        let master_options =
            CanvasOptions::transparent(1024).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let master = render_square(&self.source, self.transform, master_options)
            .map_err(|e| JsValue::from_str(&format!("{}: {}", e.code(), e)))?;
        let icns_bytes =
            encode_icns(&master).map_err(|e| JsValue::from_str(&format!("{}: {}", e.code(), e)))?;

        // Self-check
        let _report = verify_icns(&icns_bytes)
            .map_err(|e| JsValue::from_str(&format!("verification failed: {}", e)))?;
        Ok(icns_bytes)
    }
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    #[wasm_bindgen_test]
    fn editor_takes_ownership_of_source_rgba_allocation() {
        let source = RgbaImage::from_pixel(32, 24, Rgba([31, 181, 106, 255]));
        let source_pixels = source.as_ptr();

        let editor = Editor::new(WasmImage::from(source));

        assert_eq!(editor.source.as_ptr(), source_pixels);
    }
}
