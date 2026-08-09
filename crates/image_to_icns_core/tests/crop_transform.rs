use image::{Rgba, RgbaImage};
use image_to_icns_core::{CanvasOptions, CoreError, CropTransform, render_square};

#[test]
fn crop_uses_the_selected_source_center() {
    let mut source = RgbaImage::from_pixel(200, 100, Rgba([0, 0, 255, 255]));
    for x in 0..100 {
        for y in 0..100 {
            source.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }

    let rendered = render_square(
        &source,
        CropTransform::new(1.0, 0.25, 0.5).expect("valid crop"),
        CanvasOptions::transparent(64).expect("valid canvas"),
    )
    .expect("renderable");

    assert_eq!(rendered.dimensions(), (64, 64));
    assert_eq!(rendered.get_pixel(32, 32), &Rgba([255, 0, 0, 255]));
}

#[test]
fn crop_center_is_clamped_to_keep_the_selection_inside_the_source() {
    let mut source = RgbaImage::from_pixel(200, 100, Rgba([0, 0, 255, 255]));
    for x in 100..200 {
        for y in 0..100 {
            source.put_pixel(x, y, Rgba([0, 255, 0, 255]));
        }
    }

    let rendered = render_square(
        &source,
        CropTransform::new(1.0, 2.0, -1.0).expect("center point will be normalized"),
        CanvasOptions::transparent(32).expect("valid canvas"),
    )
    .expect("renderable");

    assert_eq!(rendered.get_pixel(16, 16), &Rgba([0, 255, 0, 255]));
}

#[test]
fn transform_rejects_non_finite_or_zoom_below_one() {
    assert!(CropTransform::new(0.0, 0.5, 0.5).is_err());
    assert!(CropTransform::new(0.99, 0.5, 0.5).is_err());
    assert!(CropTransform::new(f32::NAN, 0.5, 0.5).is_err());
    assert!(CropTransform::new(1.0, f32::INFINITY, 0.5).is_err());
}

#[test]
fn render_rejects_an_empty_source() {
    let error = render_square(
        &RgbaImage::new(0, 0),
        CropTransform::default(),
        CanvasOptions::transparent(32).expect("valid canvas"),
    )
    .expect_err("empty image cannot render");

    assert!(matches!(error, CoreError::EmptySource));
    assert_eq!(error.code(), "EMPTY_SOURCE");
}

#[test]
fn background_is_composited_under_transparent_pixels() {
    let source = RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 128]));
    let rendered = render_square(
        &source,
        CropTransform::default(),
        CanvasOptions::new(8, Rgba([0, 0, 255, 255])).expect("valid canvas"),
    )
    .expect("renderable");

    let pixel = rendered.get_pixel(4, 4);
    assert_eq!(pixel[3], 255);
    assert!((pixel[0] as i16 - 128).abs() <= 1);
    assert!((pixel[2] as i16 - 127).abs() <= 1);
}
