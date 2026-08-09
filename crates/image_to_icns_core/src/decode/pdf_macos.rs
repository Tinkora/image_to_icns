use std::path::Path;

use image::{ImageFormat, RgbaImage};
use objc2::AnyThread;
use objc2::rc::autoreleasepool;
use objc2_foundation::{NSSize, NSURL};
use objc2_pdf_kit::{PDFDisplayBox, PDFDocument};

use crate::CoreError;

const PDF_THUMBNAIL_SIZE: f64 = 2_048.0;

/// Rasterize the first page using macOS PDFKit, then hand the TIFF in-memory
/// representation back to the Rust `image` crate for decoding.
///
/// All Objective-C objects are managed by `Retained` and released inside a local
/// autorelease pool; the return value contains only RGBA pixels copied into Rust
/// ownership so that no system objects escape the FFI boundary.
pub(super) fn decode_first_page(path: &Path) -> Result<RgbaImage, CoreError> {
    autoreleasepool(|_| {
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| CoreError::PdfDecodeFailed("failed to construct file URL".to_owned()))?;

        // SAFETY: `url` is a file URL constructed by Foundation from a valid Path and
        // held within this scope; `PDFDocument::alloc()` and the init method follow
        // Objective-C's paired-initialization convention.
        let document =
            unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }.ok_or_else(|| {
                CoreError::PdfDecodeFailed("PDFKit failed to open document".to_owned())
            })?;

        // SAFETY: `document` is a successfully-initialized PDFDocument still held by
        // Retained; index 0 is only accessed when pageCount > 0, and the returned
        // page is kept alive by Retained until thumbnail generation completes.
        let page = unsafe {
            if document.pageCount() == 0 {
                return Err(CoreError::PdfDecodeFailed("PDF has no pages".to_owned()));
            }
            document.pageAtIndex(0)
        }
        .ok_or_else(|| CoreError::PdfDecodeFailed("failed to read PDF first page".to_owned()))?;

        // SAFETY: `page` is valid for the duration of the call; CGSize is a finite
        // positive number, CropBox is a PDFKit-defined enum value. The returned
        // NSImage is managed by Retained and holds no raw pointer to the page.
        let thumbnail = unsafe {
            page.thumbnailOfSize_forBox(
                NSSize::new(PDF_THUMBNAIL_SIZE, PDF_THUMBNAIL_SIZE),
                PDFDisplayBox::CropBox,
            )
        };
        let tiff = thumbnail.TIFFRepresentation().ok_or_else(|| {
            CoreError::PdfDecodeFailed("failed to get PDF thumbnail data".to_owned())
        })?;
        let bytes = tiff.to_vec();

        image::load_from_memory_with_format(&bytes, ImageFormat::Tiff)
            .map(|image| image.into_rgba8())
            .map_err(|error| CoreError::PdfDecodeFailed(error.to_string()))
    })
}
