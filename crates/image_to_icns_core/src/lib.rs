mod decode;
mod error;
mod icns;
mod model;
mod render;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use decode::{MAX_INPUT_BYTES, SourceFormat, decode_bytes, decode_source};
pub use error::CoreError;
pub use icns::{IcnsReport, IcnsRepresentation, encode_icns, verify_icns};
pub use model::{CanvasOptions, CropTransform};
pub use render::render_square;

#[cfg(target_arch = "wasm32")]
pub use wasm::{WasmImage, wasm_decode, wasm_encode, wasm_render, wasm_verify};
