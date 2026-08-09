#![cfg(target_arch = "wasm32")]

//! image_to_icns Web editor — browser-side WASM module.
//!
//! This module provides browser-specific bindings on top of `image_to_icns_core`:
//! - Read and decode images from `File`/`Blob`
//! - Canvas interactive cropping and real-time preview
//! - ICNS generation and Blob download
//! - Optional Session state reporting

use wasm_bindgen::prelude::*;

mod editor;
mod import;

pub use editor::Editor;
pub use import::import_file;

/// Initialize the module, setting up a panic hook so Rust error stacks appear in the browser console.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}
