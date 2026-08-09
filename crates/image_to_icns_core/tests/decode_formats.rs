use std::fs::{self, File};
use std::io::Cursor;
use std::path::Path;

#[cfg(target_os = "macos")]
use std::io::Write;

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use image_to_icns_core::{SourceFormat, decode_bytes, decode_source};
use tempfile::tempdir;

#[test]
fn decode_accepts_png_jpeg_and_svg() {
    let directory = tempdir().expect("create temp dir");
    let png = directory.path().join("sample.png");
    let jpeg = directory.path().join("sample.jpg");
    let svg = directory.path().join("sample.svg");

    write_raster(&png, ImageFormat::Png);
    write_raster(&jpeg, ImageFormat::Jpeg);
    fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="64"><rect width="96" height="64" fill="#31b56a"/></svg>"##,
    )
    .expect("write SVG");

    for (path, expected_size) in [(&png, (48, 32)), (&jpeg, (48, 32)), (&svg, (96, 64))] {
        let decoded = decode_source(path).expect("format should be readable");
        assert_eq!(decoded.dimensions(), expected_size, "{}", path.display());
    }
}

#[test]
fn decode_rejects_unknown_and_corrupted_inputs_with_stable_codes() {
    let directory = tempdir().expect("create temp dir");
    let unknown = directory.path().join("sample.txt");
    let corrupted = directory.path().join("broken.png");
    fs::write(&unknown, b"not an image").expect("write unknown format");
    fs::write(&corrupted, b"not a png").expect("write corrupted image");

    assert_eq!(
        decode_source(&unknown)
            .expect_err("unknown format should fail")
            .code(),
        "UNSUPPORTED_FORMAT"
    );
    assert_eq!(
        decode_source(&corrupted)
            .expect_err("corrupted should fail")
            .code(),
        "DECODE_FAILED"
    );
}

#[test]
fn decode_rejects_missing_and_oversized_files_before_parsing() {
    let directory = tempdir().expect("create temp dir");
    let missing = directory.path().join("missing.png");
    let oversized = directory.path().join("oversized.png");
    File::create(&oversized)
        .expect("create sparse file")
        .set_len(64 * 1024 * 1024 + 1)
        .expect("extend sparse file");

    assert_eq!(
        decode_source(&missing)
            .expect_err("nonexistent file should fail")
            .code(),
        "INPUT_IO"
    );
    assert_eq!(
        decode_source(&oversized)
            .expect_err("oversized file should fail")
            .code(),
        "INPUT_TOO_LARGE"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn decode_accepts_the_first_page_of_a_pdf() {
    let directory = tempdir().expect("create temp dir");
    let pdf = directory.path().join("sample.pdf");
    write_minimal_pdf(&pdf);

    let decoded = decode_source(&pdf).expect("PDF first page readable");
    assert!(decoded.width() > 0);
    assert!(decoded.height() > 0);
    assert!(decoded.width() >= decoded.height());
}

fn write_raster(path: &Path, format: ImageFormat) {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(48, 32, Rgba([210, 45, 80, 255])));
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, format)
        .expect("encode test image");
    fs::write(path, bytes.into_inner()).expect("write test image");
}

// ── decode_bytes byte path tests ─────────────────────────────────────────

#[test]
fn decode_bytes_accepts_png_jpeg_and_svg() {
    let png_bytes = encode_test_image(ImageFormat::Png);
    let jpeg_bytes = encode_test_image(ImageFormat::Jpeg);
    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="64"><rect width="96" height="64" fill="#31b56a"/></svg>"##;

    let decoded_png = decode_bytes(&png_bytes, SourceFormat::Png).expect("PNG bytes readable");
    let decoded_jpeg = decode_bytes(&jpeg_bytes, SourceFormat::Jpeg).expect("JPEG bytes readable");
    let decoded_svg = decode_bytes(svg_bytes, SourceFormat::Svg).expect("SVG bytes readable");

    assert_eq!(decoded_png.dimensions(), (48, 32));
    assert_eq!(decoded_jpeg.dimensions(), (48, 32));
    assert_eq!(decoded_svg.dimensions(), (96, 64));
}

#[test]
fn decode_bytes_rejects_oversized_input() {
    let huge = vec![0u8; 64 * 1024 * 1024 + 1];
    assert_eq!(
        decode_bytes(&huge, SourceFormat::Png)
            .expect_err("exceeding bytes should fail")
            .code(),
        "INPUT_TOO_LARGE"
    );
}

#[test]
fn decode_bytes_rejects_corrupted_input() {
    assert_eq!(
        decode_bytes(b"not an image", SourceFormat::Png)
            .expect_err("corrupted PNG should fail")
            .code(),
        "DECODE_FAILED"
    );
}

#[test]
fn decode_bytes_rejects_oversized_embedded_png_before_rendering() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image visibility="hidden" width="1" height="1" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAQAEAAAABCAYAAADJXd1mAAAADUlEQVR42mP8/58BAQEDAQF/lT8AAAAASUVORK5CYII="/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("embedded PNG dimensions must share the raster safety budget");

    assert_eq!(error.code(), "IMAGE_TOO_LARGE");
    assert_eq!(
        error.to_string(),
        "image dimensions 16385x1 exceed safety limit"
    );
}

#[test]
fn decode_bytes_rejects_oversized_embedded_jpeg_before_rendering() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image visibility="hidden" width="1" height="1" href="data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////2wBDAf//////////////////////////////////////////////////////////////////////////////////////wAARCEABQAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAf/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIQAxAAAAF//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABBQJ//8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAwEBPwF//8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAgEBPwF//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQAGPwJ//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPyF//9oADAMBAAIAAwAAABD/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAEDAQE/EB//xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAECAQE/EB//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/EB//2Q=="/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("embedded JPEG dimensions must share the raster safety budget");

    assert_eq!(error.code(), "IMAGE_TOO_LARGE");
    assert_eq!(
        error.to_string(),
        "image dimensions 16385x16385 exceed safety limit"
    );
}

#[test]
fn decode_bytes_accepts_safe_embedded_png_and_jpeg() {
    let png = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1">
        <image width="1" height="1" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/58BAQEDAQF/lT8AAAAASUVORK5CYII="/>
    </svg>"##;
    let jpeg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1">
        <image width="1" height="1" href="data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////2wBDAf//////////////////////////////////////////////////////////////////////////////////////wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAf/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIQAxAAAAF//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABBQJ//8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAwEBPwF//8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAgEBPwF//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQAGPwJ//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPyF//9oADAMBAAIAAwAAABD/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAEDAQE/EB//xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAECAQE/EB//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/EB//2Q=="/>
    </svg>"##;

    assert_eq!(
        decode_bytes(png, SourceFormat::Svg)
            .expect("safe embedded PNG should decode")
            .dimensions(),
        (2, 1)
    );
    assert_eq!(
        decode_bytes(jpeg, SourceFormat::Svg)
            .expect("safe embedded JPEG should decode")
            .dimensions(),
        (2, 1)
    );
}

#[test]
fn decode_bytes_rejects_nested_svg_images() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1' height='1'%3E%3Crect width='1' height='1' fill='red'/%3E%3C/svg%3E"/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("nested SVG images must not create a recursive parser path");

    assert_eq!(error.code(), "DECODE_FAILED");
    assert_eq!(
        error.to_string(),
        "SVG decode failed: embedded SVG and SVGZ images are not supported"
    );
}

#[test]
fn decode_bytes_rejects_nested_svgz_images() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="data:image/svg+xml;base64,H4sIAAAAAAAAA22MSQ6AIAwAv9L0ARb1ZoDPCFISXAKN9fnK3dskMxnb7gTPXo7mkEWuhUhVB52HsyaajDH0FQiag7DDEYFjTiwdva1xlV8FWy7FYY0Bydu+8C9JJfVYagAAAA=="/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("nested SVGZ images must not create a decompression parser path");

    assert_eq!(error.code(), "DECODE_FAILED");
    assert_eq!(
        error.to_string(),
        "SVG decode failed: embedded SVG and SVGZ images are not supported"
    );
}

#[test]
fn decode_bytes_rejects_unsupported_embedded_raster_types() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw=="/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("unbudgeted embedded raster formats must be rejected");

    assert_eq!(error.code(), "DECODE_FAILED");
    assert_eq!(
        error.to_string(),
        "SVG decode failed: unsupported embedded image type"
    );
}

#[test]
fn decode_bytes_rejects_embedded_raster_with_invalid_header() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="data:image/png;base64,bm90IGEgcG5n"/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("declared embedded raster type must have a verifiable header");

    assert_eq!(error.code(), "DECODE_FAILED");
    assert_eq!(
        error.to_string(),
        "SVG decode failed: embedded PNG header is invalid"
    );
}

#[test]
fn decode_bytes_rejects_external_svg_image_references() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
        <image width="1" height="1" href="file:///private/example.png"/>
    </svg>"##;

    let error = decode_bytes(svg, SourceFormat::Svg)
        .expect_err("external SVG image references must not be ignored silently");

    assert_eq!(error.code(), "DECODE_FAILED");
    assert_eq!(
        error.to_string(),
        "SVG decode failed: external SVG image references are not supported"
    );
}

#[test]
fn decode_bytes_rejects_outer_svgz_before_decompression() {
    let svgz = [
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x6d, 0x8c, 0x49, 0x0e, 0x80,
        0x20, 0x0c, 0x00, 0xbf, 0xd2, 0xf4, 0x01, 0x16, 0xf5, 0x66, 0x80, 0xcf, 0x08, 0x52, 0x12,
        0x5c, 0x02, 0x8d, 0xf5, 0xf9, 0xca, 0xdd, 0xdb, 0x24, 0x33, 0x19, 0xdb, 0xee, 0x04, 0xcf,
        0x5e, 0x8e, 0xe6, 0x90, 0x45, 0xae, 0x85, 0x48, 0x55, 0x07, 0x9d, 0x87, 0xb3, 0x26, 0x9a,
        0x8c, 0x31, 0xf4, 0x15, 0x08, 0x9a, 0x83, 0xb0, 0xc3, 0x11, 0x81, 0x63, 0x4e, 0x2c, 0x1d,
        0xbd, 0xad, 0x71, 0x95, 0x5f, 0x05, 0x5b, 0x2e, 0xc5, 0x61, 0x8d, 0x01, 0xc9, 0xdb, 0xbe,
        0xf0, 0x2f, 0x49, 0x25, 0xf5, 0x58, 0x6a, 0x00, 0x00, 0x00,
    ];

    let error = decode_bytes(&svgz, SourceFormat::Svg)
        .expect_err("SVGZ must not bypass the encoded input budget through decompression");

    assert_eq!(error.code(), "DECODE_FAILED");
    assert_eq!(
        error.to_string(),
        "SVG decode failed: compressed SVG input is not supported"
    );
}

fn encode_test_image(format: ImageFormat) -> Vec<u8> {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(48, 32, Rgba([210, 45, 80, 255])));
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, format)
        .expect("encode test image");
    bytes.into_inner()
}

#[cfg(target_os = "macos")]
fn write_minimal_pdf(path: &Path) {
    let stream = "q 1 0 0 rg 0 0 120 80 re f Q\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 80] /Contents 4 0 R >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            stream.len(),
            stream
        ),
    ];

    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        write!(&mut bytes, "{} 0 obj\n{}\nendobj\n", index + 1, object)
            .expect("construct PDF object");
    }

    let xref_offset = bytes.len();
    write!(
        &mut bytes,
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    )
    .expect("write PDF xref");
    for offset in offsets {
        writeln!(&mut bytes, "{offset:010} 00000 n ").expect("write PDF offsets");
    }
    write!(
        &mut bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_offset
    )
    .expect("write PDF trailer");

    fs::write(path, bytes).expect("write PDF fixture");
}
