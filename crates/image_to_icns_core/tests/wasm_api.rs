//! WASM entry point integration tests.
//!
//! These tests run in Node.js (or browser) via `wasm-bindgen-test`.
//! Only compiled on wasm32 target due to `wasm-bindgen` dependency.

#![cfg(target_arch = "wasm32")]

use image::{Rgba, RgbaImage};
use image_to_icns_core::{WasmImage, wasm_decode, wasm_encode, wasm_render, wasm_verify};
use wasm_bindgen_test::*;

fn encode_test_png() -> Vec<u8> {
    let image = RgbaImage::from_pixel(64, 64, Rgba([31, 181, 106, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("encode PNG");
    bytes.into_inner()
}

fn encode_test_jpeg() -> Vec<u8> {
    let image = RgbaImage::from_pixel(64, 64, Rgba([210, 45, 80, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .expect("encode JPEG");
    bytes.into_inner()
}

#[wasm_bindgen_test]
fn wasm_decode_png_round_trip() {
    let png = encode_test_png();
    let img = wasm_decode(png, "png".into()).expect("PNG decode");
    assert_eq!(img.width(), 64);
    assert_eq!(img.height(), 64);
}

#[wasm_bindgen_test]
fn wasm_decode_jpeg_round_trip() {
    let jpeg = encode_test_jpeg();
    let img = wasm_decode(jpeg, "jpeg".into()).expect("JPEG decode");
    assert_eq!(img.width(), 64);
    assert_eq!(img.height(), 64);
}

#[wasm_bindgen_test]
fn wasm_decode_svg() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect width="80" height="40" fill="#31b56a"/></svg>"##;
    let img = wasm_decode(svg.to_vec(), "svg".into()).expect("SVG decode");
    assert_eq!(img.width(), 80);
    assert_eq!(img.height(), 40);
}

#[wasm_bindgen_test]
fn wasm_decode_rejects_unknown_format() {
    let png = encode_test_png();
    let err = wasm_decode(png, "tiff".into()).expect_err("unknown format should fail");
    let msg = err.as_string().unwrap();
    assert!(
        msg.contains("unsupported format"),
        "expected unsupported format error: {msg}"
    );
}

#[wasm_bindgen_test]
fn wasm_decode_rejects_corrupted_data() {
    let err = wasm_decode(b"not an image".to_vec(), "png".into())
        .expect_err("corrupted data should fail");
    let msg = err.as_string().unwrap();
    assert!(
        msg.contains("code") || msg.contains("DECODE_FAILED"),
        "{msg}"
    );
}

#[wasm_bindgen_test]
fn wasm_render_default_center_produces_square() {
    let png = encode_test_png();
    let img = wasm_decode(png, "png".into()).expect("decode");

    let transform = r#"{"zoom":1.0,"center_x":0.5,"center_y":0.5}"#;
    let options = r#"{"size":128,"background":[0,0,0,0]}"#;
    let rendered = wasm_render(&img, transform.into(), options.into()).expect("render");
    assert_eq!(rendered.width(), 128);
    assert_eq!(rendered.height(), 128);
}

#[wasm_bindgen_test]
fn wasm_encode_and_verify_round_trip() {
    // Create square master canvas
    let master = RgbaImage::from_pixel(256, 256, Rgba([100, 150, 200, 255]));
    let img = WasmImage::from(master);

    let icns_bytes = wasm_encode(&img).expect("ICNS encode");
    assert!(!icns_bytes.is_empty());

    let report_json = wasm_verify(icns_bytes).expect("ICNS verify");
    let report: serde_json::Value =
        serde_json::from_str(&report_json).expect("parse verify report");
    assert!(report["byte_len"].as_u64().unwrap() > 0);
    let reps = report["representations"]
        .as_array()
        .expect("expected representation list");
    assert_eq!(reps.len(), 10, "expected 10 standard representations");
}

#[wasm_bindgen_test]
fn wasm_encode_rejects_non_square() {
    let master = RgbaImage::from_pixel(64, 32, Rgba([255, 0, 0, 255]));
    let img = WasmImage::from(master);
    let err = wasm_encode(&img).expect_err("non-square should fail");
    let msg = err.as_string().unwrap();
    assert!(msg.contains("INVALID_MASTER_IMAGE"), "{msg}");
}

#[wasm_bindgen_test]
fn wasm_verify_rejects_truncated_data() {
    let err = wasm_verify(b"not icns".to_vec()).expect_err("truncated data should fail");
    let msg = err.as_string().unwrap();
    assert!(msg.contains("ICNS_DECODE_FAILED"), "{msg}");
}
