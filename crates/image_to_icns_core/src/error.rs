use thiserror::Error;

/// Stable error type for the core library.
///
/// The CLI exposes `code()` to agents, so new errors must preserve existing code semantics.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("cannot read input file: {0}")]
    InputIo(String),

    #[error("unsupported input format: {0}")]
    UnsupportedFormat(String),

    #[error("input file is {size} bytes, exceeding {max} bytes limit")]
    InputTooLarge { size: u64, max: u64 },

    #[error("image dimensions {width}x{height} exceed safety limit")]
    ImageTooLarge { width: u32, height: u32 },

    #[error("{format} decode failed: {reason}")]
    DecodeFailed {
        format: &'static str,
        reason: String,
    },

    #[error("PDF input is not supported on this platform")]
    PdfUnsupportedOnPlatform,

    #[error("PDF decode failed: {0}")]
    PdfDecodeFailed(String),

    #[error("ICNS master canvas must be a non-empty square, got {width}x{height}")]
    InvalidMasterImage { width: u32, height: u32 },

    #[error("ICNS encode failed: {0}")]
    IcnsEncodeFailed(String),

    #[error("ICNS read-back verification failed: {0}")]
    IcnsDecodeFailed(String),

    #[error("invalid crop parameters: {0}")]
    InvalidTransform(&'static str),

    #[error("canvas size must be greater than 0")]
    InvalidCanvasSize,

    #[error("source image has no renderable pixels")]
    EmptySource,

    #[error("current zoom level produces a crop area smaller than one pixel")]
    CropTooSmall,
}

impl CoreError {
    /// Returns a stable error code for CLI, skill, and log consumption.
    /// Human-readable messages are not part of the protocol.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InputIo(_) => "INPUT_IO",
            Self::UnsupportedFormat(_) => "UNSUPPORTED_FORMAT",
            Self::InputTooLarge { .. } => "INPUT_TOO_LARGE",
            Self::ImageTooLarge { .. } => "IMAGE_TOO_LARGE",
            Self::DecodeFailed { .. } => "DECODE_FAILED",
            Self::PdfUnsupportedOnPlatform => "PDF_UNSUPPORTED_ON_PLATFORM",
            Self::PdfDecodeFailed(_) => "PDF_DECODE_FAILED",
            Self::InvalidMasterImage { .. } => "INVALID_MASTER_IMAGE",
            Self::IcnsEncodeFailed(_) => "ICNS_ENCODE_FAILED",
            Self::IcnsDecodeFailed(_) => "ICNS_DECODE_FAILED",
            Self::InvalidTransform(_) => "INVALID_TRANSFORM",
            Self::InvalidCanvasSize => "INVALID_CANVAS_SIZE",
            Self::EmptySource => "EMPTY_SOURCE",
            Self::CropTooSmall => "CROP_TOO_SMALL",
        }
    }
}
