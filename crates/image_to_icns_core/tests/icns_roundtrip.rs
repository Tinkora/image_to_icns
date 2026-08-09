use image::{Rgba, RgbaImage};
use image_to_icns_core::{encode_icns, verify_icns};

#[test]
fn exported_icns_round_trips_all_standard_representations() {
    let source = RgbaImage::from_fn(1024, 1024, |x, y| {
        Rgba([(x % 255) as u8, (y % 255) as u8, 96, 255])
    });
    let bytes = encode_icns(&source).expect("encode success");
    let report = verify_icns(&bytes).expect("readback success");

    assert!(bytes.starts_with(b"icns"));
    assert_eq!(report.byte_len, bytes.len() as u64);
    assert_eq!(report.representations.len(), 10);
    assert_eq!(
        report
            .representations
            .iter()
            .map(|item| (item.ostype.as_str(), item.pixels, item.scale))
            .collect::<Vec<_>>(),
        vec![
            ("icp4", 16, 1),
            ("ic11", 32, 2),
            ("icp5", 32, 1),
            ("ic12", 64, 2),
            ("ic07", 128, 1),
            ("ic13", 256, 2),
            ("ic08", 256, 1),
            ("ic14", 512, 2),
            ("ic09", 512, 1),
            ("ic10", 1024, 2),
        ]
    );
    assert!(report.representations.iter().all(|item| item.data_len > 0));
}

#[test]
fn encode_rejects_a_non_square_or_empty_master_image() {
    let non_square = RgbaImage::from_pixel(1024, 512, Rgba([0, 0, 0, 255]));
    let empty = RgbaImage::new(0, 0);

    assert_eq!(
        encode_icns(&non_square)
            .expect_err("non-square master should fail")
            .code(),
        "INVALID_MASTER_IMAGE"
    );
    assert_eq!(
        encode_icns(&empty)
            .expect_err("empty master should fail")
            .code(),
        "INVALID_MASTER_IMAGE"
    );
}

#[test]
fn verify_rejects_truncated_data() {
    assert_eq!(
        verify_icns(b"icns\0\0\0\x20")
            .expect_err("truncated data should fail")
            .code(),
        "ICNS_DECODE_FAILED"
    );
}
