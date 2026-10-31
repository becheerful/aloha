//! Color-related types for the `.aloha` image format.
//!
//! This module defines [`ColorProfileDesc`] (how the color profile is stored)
//! and [`ColorType`] (the channel layout of the pixel data), along with
//! conversions from `u8` and helper methods used by the encoder and decoder.

use crate::error::{AlohaError, AlohaResult};

/// Describes how the color profile of an image is represented.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProfileDesc {
    /// No color profile is present.
    None,
    /// The color profile is stored as cICP (Coding-Independent Code Points)
    Cicp,
    /// The color profile is stored as raw ICC profile bytes.
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

/// Describes the channel layout and meaning of the pixel data.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    /// Single-channel grayscale data.
    Grayscale,
    /// Grayscale data followed by an alpha channel.
    GrayscaleAlpha,
    /// Red, green, and blue channels.
    Rgb,
    /// Red, green, blue, and alpha channels.
    Rgba,
    /// Each sample is an index into a color palette.
    Indexed,
}

impl ColorType {
    /// Creates a `ColorType` from a raw channel count.
    ///
    /// This is the inverse of [`ColorType::channels`] for non-`Indexed` types.
    ///
    /// # Errors
    ///
    /// Returns [`AlohaError::InvalidChannelsCount`] if `channels` is 0 or greater than 4,
    /// or if the count does not map to a supported non-`Indexed` layout.
    pub fn from_channels_count(channels: usize) -> AlohaResult<Self> {
        Ok(match channels {
            1 => Self::Grayscale,
            2 => Self::GrayscaleAlpha,
            3 => Self::Rgb,
            4 => Self::Rgba,
            _ => return Err(AlohaError::InvalidChannelsCount(channels)),
        })
    }

    /// Returns the number of channels (samples) per pixel for this color type.
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

/// Returns a bitmask for extracting a single sample from a packed pixel value.
pub(crate) fn get_sample_mask(bit_depth: u8) -> u16 {
    if bit_depth == 8 { 0xFF } else { 0xFFFF }
}
