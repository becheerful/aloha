use crate::error::{AlohaError, AlohaResult};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProfileDesc {
    None,
    Cicp,
    Icc,
}

impl TryFrom<u8> for ColorProfileDesc {
    type Error = AlohaError;

    fn try_from(value: u8) -> AlohaResult<Self> {
        Ok(match value {
            0 => Self::None,
            1 => Self::Cicp,
            2 => Self::Icc,
            _ => return Err(AlohaError::InvalidColorProfileDesc(value)),
        })
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    Grayscale,
    GrayscaleAlpha,
    Rgb,
    Rgba,
    Indexed,
}

impl ColorType {
    pub fn from_channels_count(channels: usize) -> AlohaResult<Self> {
        Ok(match channels {
            1 => Self::Grayscale,
            2 => Self::GrayscaleAlpha,
            3 => Self::Rgb,
            4 => Self::Rgba,
            _ => return Err(AlohaError::InvalidChannelsCount(channels)),
        })
    }

    pub fn channels(&self) -> usize {
        match self {
            Self::Grayscale | Self::Indexed => 1,
            Self::GrayscaleAlpha => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    pub fn is_indexed(&self) -> bool {
        matches!(self, Self::Indexed)
    }
}

impl TryFrom<u8> for ColorType {
    type Error = AlohaError;

    fn try_from(value: u8) -> AlohaResult<Self> {
        Ok(match value {
            0 => Self::Grayscale,
            1 => Self::GrayscaleAlpha,
            2 => Self::Rgb,
            3 => Self::Rgba,
            4 => Self::Indexed,
            _ => return Err(AlohaError::InvalidColorType(value)),
        })
    }
}

pub(crate) fn get_sample_mask(bit_depth: u8) -> u16 {
    if bit_depth == 8 { 0xFF } else { 0xFFFF }
}
