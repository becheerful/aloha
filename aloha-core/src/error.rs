use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlohaError {
    #[error("encoding error: {0}")]
    EncodingError(String),
    #[error("decoding error: {0}")]
    DecodingError(String),
    #[error("validation error: {0}")]
    ValidationError(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ultra HDR error: {0}")]
    UltraHdr(#[from] ultrahdr_core::Error),
    #[error("XMPKit error: {0}")]
    XmpKit(#[from] xmpkit::XmpError),
}

pub type AlohaResult<T = ()> = Result<T, AlohaError>;
