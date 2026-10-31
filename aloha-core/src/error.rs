use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlohaError {
    // miscellaneous
    #[error("decoding error: {0}")]
    DecodingError(String),
    #[error("validation error: {0}")]
    ValidationError(String),
    #[error("inconvertible color type: {0:?}")]
    InconvertibleColorType(crate::color::ColorType),
    // "invalid" errors
    #[error("invalid color type: {0}")]
    InvalidColorType(u8),
    #[error("invalid bytes per sample count: {0}")]
    InvalidBytesPerSample(usize),
    #[error("invalid color profile's descriptor: {0}")]
    InvalidColorProfileDesc(u8),
    #[error("invalid compression algorithm: {0}")]
    InvalidCompressionAlgorithm(u8),
    #[error("invalid channels count: {0}")]
    InvalidChannelsCount(usize),
    // "not found" errors
    #[error("image must have coding-independent code points but they're absent")]
    CicpNotFound,
    #[error("image must have an ICC profile but it's absent")]
    IccNotFound,
    #[error("image must have a palette but it's absent")]
    PaletteNotFound,
    #[error("image must have a gain map metadata but it's absent")]
    GainMapMetadataNotFound,
    #[error("image must have a gain map but it's absent")]
    GainMapNotFound,
    // crate errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ultra HDR error: {0}")]
    UltraHdr(#[from] ultrahdr_core::Error),
    #[error("serde serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

pub type AlohaResult<T = ()> = Result<T, AlohaError>;
