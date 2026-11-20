//! Reader and writer for the `.aloha` image container format.
//!
//! This crate implements encoding and decoding of the `.aloha` file format —
//! a compact, self-describing container for raster images. It is intended as
//! a low-level building block: it focuses on faithful serialization of pixel
//! data together with all information required to interpret it correctly
//! (color management, palettes, metadata, and optional HDR gain map).

use std::io::{Read, Write};

use crc32fast::Hasher;

use crate::{
    color::{ColorProfileDesc, ColorType},
    delta::{delta_decode, delta_encode},
    error::{AlohaError, AlohaResult},
    metadata::Metadata,
};

pub mod color;
pub mod delta;
pub mod error;
pub mod metadata;

const MAGIC: &[u8; 5] = b"ALOHA";
const VERSION: u16 = 0;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Compression {
    None,
    DeltaZlib,
    DeltaZstd,
}

impl TryFrom<u8> for Compression {
    type Error = AlohaError;

    fn try_from(value: u8) -> AlohaResult<Self> {
        Ok(match value {
            0 => Self::None,
            1 => Self::DeltaZlib,
            2 => Self::DeltaZstd,
            _ => Err(AlohaError::DecodingError(format!(
                "invalid compression algorithm: {value}"
            )))?,
        })
    }
}

/// A fully decoded in-memory representation of an `.aloha` image.
///
/// An `AlohaImage` holds the pixel data along with all associated color
/// information (ICC or CICP profile), optional palette, JSON metadata, and an
/// optional gain map. Use [`write_aloha`] to encode an instance to disk and
/// [`read_aloha`] to decode a file back into an `AlohaImage`.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct AlohaImage {
    /// The width of the image in pixels. Must be at least 1.
    pub width: u32,
    /// The height of the image in pixels. Must be at least 1.
    pub height: u32,
    /// The color type that describes the channel layout of the pixel data.
    pub color_type: ColorType,
    /// The number of bits used to represent a single color sample.
    ///
    /// For `Indexed` and `IndexedAlpha` color types this value must not exceed 8.
    pub bit_depth: u8,
    /// Describes which color profile representation is used (`CICP`, `ICC` or none).
    pub color_profile_desc: ColorProfileDesc,
    /// The compression algorithm applied to the pixel data.
    pub compression: Compression,
    /// Whether the image contains an optional gain map.
    pub has_gain_map: bool,
    /// The CICP color profile, required when `color_profile_desc` is [`ColorProfileDesc::Cicp`].
    pub cicp: Option<zencodec::Cicp>,
    /// The raw ICC profile bytes, required when `color_profile_desc` is [`ColorProfileDesc::Icc`].
    pub icc_profile: Option<Vec<u8>>,
    /// The color palette, required when `color_type` is `Indexed` (4) or `IndexedAlpha` (5).
    pub palette: Option<Vec<u8>>,
    /// The transparency palette, required when `color_type` is `IndexedAlpha` (5).
    pub transparency_palette: Option<Vec<u8>>,
    /// Optional JSON metadata attached to the image.
    pub metadata: Metadata,
    /// The raw (decompressed) pixel data.
    pub data: Vec<u8>,
    /// Metadata describing how to interpret the gain map, if `has_gain_map` is `true`.
    pub gain_map_metadata: Option<ultrahdr_core::GainMapMetadata>,
    /// The gain map image data, if `has_gain_map` is `true`.
    pub gain_map: Option<ultrahdr_core::GainMap>,
}

impl AlohaImage {
    /// Returns an instance of `AlohaImage`.
    ///
    /// # Errors
    ///
    /// Returns an [`AlohaError::ValidationError`] if validation was failed.
    pub fn new(
        width: u32,
        height: u32,
        color_type: ColorType,
        bit_depth: u8,
        compression: Compression,
        cicp: Option<zencodec::Cicp>,
        icc_profile: Option<Vec<u8>>,
        palette: Option<Vec<u8>>,
        transparency_palette: Option<Vec<u8>>,
        metadata: Metadata,
        data: Vec<u8>,
        gain_map_metadata: Option<ultrahdr_core::GainMapMetadata>,
        gain_map: Option<ultrahdr_core::GainMap>,
    ) -> AlohaResult<Self> {
        let image = Self {
            width,
            height,
            color_type,
            bit_depth,
            color_profile_desc: if cicp.is_some() {
                ColorProfileDesc::Cicp
            } else if icc_profile.is_some() {
                ColorProfileDesc::Icc
            } else {
                ColorProfileDesc::None
            },
            compression,
            has_gain_map: gain_map_metadata.is_some() && gain_map.is_some(),
            cicp,
            icc_profile,
            palette,
            transparency_palette,
            metadata,
            data,
            gain_map_metadata,
            gain_map,
        };

        image.validate()?;
        Ok(image)
    }

    /// Validates the internal consistency of this `AlohaImage`.
    ///
    /// The following invariants are checked:
    ///
    /// - Width and height are both non-zero.
    /// - `Indexed` and `IndexedAlpha` color types have a palette and a bit
    ///   depth of at most 8.
    /// - Image's color profile descriptor does not match the actual one.
    /// - The length of `data` matches `width * height * bytes_per_pixel()`.
    /// - If a gain map is present, its dimensions and data length are valid,
    ///   and its metadata passes validation
    ///
    /// # Errors
    ///
    /// Returns an [`AlohaError::ValidationError`] if any of the above checks fail.
    pub fn validate(&self) -> AlohaResult {
        if self.width < 1 || self.height < 1 {
            return Err(AlohaError::ValidationError(format!(
                "invalid image resolution: {}x{}",
                self.width, self.height
            )));
        }

        if self.color_type.is_indexed() {
            if self.palette.is_none() {
                return Err(AlohaError::ValidationError(
                    "the color palette not found".to_owned(),
                ));
            }

            if self.color_type.has_alpha() && self.transparency_palette.is_none() {
                return Err(AlohaError::ValidationError(
                    "the transparency palette not found".to_owned(),
                ));
            }

            if self.bit_depth > 8 {
                return Err(AlohaError::ValidationError(format!(
                    "invalid bit depth for indexed color type: {}",
                    self.bit_depth as u8
                )));
            }
        }

        if matches!(self.color_profile_desc, ColorProfileDesc::Cicp) {
            if self.icc_profile.is_some() {
                return Err(AlohaError::ValidationError(
                    "image uses the CICP but the ICC profile isn't empty".to_owned(),
                ));
            }
        }

        if matches!(self.color_profile_desc, ColorProfileDesc::Icc) {
            if self.cicp.is_some() {
                return Err(AlohaError::ValidationError(
                    "image uses the ICC profile but the CICP isn't empty".to_owned(),
                ));
            }
        }

        if matches!(self.color_profile_desc, ColorProfileDesc::None) {
            if self.cicp.is_some() {
                return Err(AlohaError::ValidationError(
                    "image does not use any color profile descriptor but the CICP isn't empty"
                        .to_owned(),
                ));
            }

            if self.icc_profile.is_some() {
                return Err(AlohaError::ValidationError(
                    "image does not use any color profile descriptor but the ICC profile isn't empty"
                        .to_owned()
                ));
            }
        }

        let actual_size = self.data.len();
        let expected_size = self.width as usize
            * self.height as usize
            * (self.bit_depth as usize / 8)
            * self.color_type.channels();
        if actual_size != expected_size {
            return Err(AlohaError::ValidationError(format!(
                "the image's data size {actual_size} does not match the expected {expected_size}"
            )));
        }

        if let Some(gain_map) = &self.gain_map {
            if gain_map.width < 1 || gain_map.height < 1 {
                return Err(AlohaError::ValidationError(format!(
                    "invalid gain map resolution: {}x{}",
                    gain_map.width, gain_map.height
                )));
            }

            let actual_size = gain_map.data.len();
            let expected_size =
                gain_map.width as usize * gain_map.height as usize * gain_map.channels as usize;
            if actual_size != expected_size {
                return Err(AlohaError::ValidationError(format!(
                    "the gain map's data size {actual_size} does not match the expected {expected_size}"
                )));
            }
        }

        if let Some(gain_map_metadata) = &self.gain_map_metadata {
            gain_map_metadata.validate()?;
        }

        Ok(())
    }

    fn write_header(&self, writer: &mut impl Write) -> AlohaResult {
        let mut hasher = Hasher::new();

        write_bytes(MAGIC, writer, &mut hasher)?;
        write_bytes(&VERSION.to_le_bytes(), writer, &mut hasher)?;

        write_bytes(&self.width.to_le_bytes(), writer, &mut hasher)?;
        write_bytes(&self.height.to_le_bytes(), writer, &mut hasher)?;

        write_bytes(
            &[
                self.color_type as u8,
                self.bit_depth,
                self.color_profile_desc as u8,
                self.compression as u8,
                self.has_gain_map as u8,
            ],
            writer,
            &mut hasher,
        )?;

        writer.write_all(&hasher.finalize().to_le_bytes())?;
        Ok(())
    }

    fn write_color_profile(&self, writer: &mut impl Write) -> AlohaResult {
        let mut hasher = Hasher::new();

        // Writing the CICP section
        if matches!(self.color_profile_desc, ColorProfileDesc::Cicp) {
            let Some(cicp) = &self.cicp else {
                return Err(AlohaError::EncodingError(
                    "image must have coding-independent code points but they're absent".to_owned(),
                ));
            };

            write_bytes(
                &[
                    cicp.color_primaries,
                    cicp.transfer_characteristics,
                    cicp.matrix_coefficients,
                    cicp.full_range as u8,
                ],
                writer,
                &mut hasher,
            )?;
        }

        // Writing the ICC profile section
        if matches!(self.color_profile_desc, ColorProfileDesc::Icc) {
            let Some(icc) = &self.icc_profile else {
                return Err(AlohaError::EncodingError(
                    "image must have an ICC profile but it's absent".to_owned(),
                ));
            };

            writer.write_all(&(icc.len() as u32).to_le_bytes())?;
            write_bytes(icc, writer, &mut hasher)?;
        }

        writer.write_all(&hasher.finalize().to_le_bytes())?;
        Ok(())
    }

    fn write_palette(&self, writer: &mut impl Write) -> AlohaResult {
        let mut hasher = Hasher::new();

        let Some(p) = &self.palette else {
            return Err(AlohaError::EncodingError(
                "image must have a color palette but it's absent".to_owned(),
            ));
        };

        writer.write_all(&(p.len() as u16).to_le_bytes())?;
        write_bytes(p, writer, &mut hasher)?;

        if self.color_type.has_alpha() {
            let Some(tp) = &self.transparency_palette else {
                return Err(AlohaError::EncodingError(
                    "image must have a transparency palette but it's absent".to_owned(),
                ));
            };

            writer.write_all(&[tp.len() as u8])?;
            write_bytes(tp, writer, &mut hasher)?;
        }

        writer.write_all(&hasher.finalize().to_le_bytes())?;
        Ok(())
    }

    fn write_metadata(&self, writer: &mut impl Write) -> AlohaResult {
        let mut hasher = Hasher::new();

        let meta_xmp = self.metadata.to_xmp()?;
        let meta_bytes = meta_xmp.as_bytes();

        writer.write_all(&(meta_bytes.len() as u32).to_le_bytes())?;
        write_bytes(&meta_bytes, writer, &mut hasher)?;

        writer.write_all(&hasher.finalize().to_le_bytes())?;
        Ok(())
    }

    fn write_data(&self, writer: &mut impl Write) -> AlohaResult {
        let compressed = match self.compression {
            Compression::None => self.data.clone(),

            Compression::DeltaZlib => {
                let filtered = delta_encode(
                    &self.data,
                    self.width as usize,
                    self.height as usize,
                    &self.color_type,
                    self.bit_depth,
                )?;

                let mut enc =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
                enc.write_all(&filtered)?;
                enc.finish()?
            }

            Compression::DeltaZstd => {
                let filtered = delta_encode(
                    &self.data,
                    self.width as usize,
                    self.height as usize,
                    &self.color_type,
                    self.bit_depth,
                )?;

                let mut enc = zstd::Encoder::new(Vec::new(), 6)?;
                enc.write_all(&filtered)?;
                enc.finish()?
            }
        };

        writer.write_all(&(compressed.len() as u64).to_le_bytes())?;
        writer.write_all(&compressed)?;
        writer.write_all(&crc32fast::hash(&compressed).to_le_bytes())?;

        Ok(())
    }

    fn write_gain_map(&self, writer: &mut impl Write) -> AlohaResult {
        let mut hasher = Hasher::new();

        // Writing the gain map metadata section
        let Some(gain_map_params) = &self.gain_map_metadata else {
            return Err(AlohaError::EncodingError(
                "image must have a gain map metadata but it's absent".to_owned(),
            ));
        };

        for i in 0..3 {
            write_bytes(
                &gain_map_params.gain_map_max[i].to_le_bytes(),
                writer,
                &mut hasher,
            )?;
            write_bytes(
                &gain_map_params.gain_map_min[i].to_le_bytes(),
                writer,
                &mut hasher,
            )?;
            write_bytes(&gain_map_params.gamma[i].to_le_bytes(), writer, &mut hasher)?;
            write_bytes(
                &gain_map_params.base_offset[i].to_le_bytes(),
                writer,
                &mut hasher,
            )?;
            write_bytes(
                &gain_map_params.alternate_offset[i].to_le_bytes(),
                writer,
                &mut hasher,
            )?;
        }

        write_bytes(
            &gain_map_params.base_hdr_headroom.to_le_bytes(),
            writer,
            &mut hasher,
        )?;
        write_bytes(
            &gain_map_params.alternate_hdr_headroom.to_le_bytes(),
            writer,
            &mut hasher,
        )?;
        write_bytes(
            &[
                gain_map_params.use_base_color_space as u8,
                gain_map_params.backward_direction as u8,
            ],
            writer,
            &mut hasher,
        )?;

        writer.write_all(&hasher.finalize().to_le_bytes())?;

        // Writing the gain map section
        let mut hasher = Hasher::new();

        let Some(gain_map) = &self.gain_map else {
            return Err(AlohaError::EncodingError(
                "image must have a gain map but it's absent".to_owned(),
            ));
        };

        let filtered = delta_encode(
            &gain_map.data,
            gain_map.width as usize,
            gain_map.height as usize,
            &ColorType::from_channels_count(gain_map.channels as usize)?,
            8, // bit depth for gain map is always 8
        )?;

        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&filtered)?;
        let compressed = enc.finish()?;

        write_bytes(&gain_map.width.to_le_bytes(), writer, &mut hasher)?;
        write_bytes(&gain_map.height.to_le_bytes(), writer, &mut hasher)?;
        write_bytes(&[gain_map.channels], writer, &mut hasher)?;

        writer.write_all(&(compressed.len() as u64).to_le_bytes())?;
        write_bytes(&compressed, writer, &mut hasher)?;

        writer.write_all(&hasher.finalize().to_le_bytes())?;
        Ok(())
    }

    /// Encodes an `AlohaImage` to the `.aloha` binary format and writes it to the
    /// provided `writer`.
    ///
    /// The function first calls [`AlohaImage::validate`] to ensure the image is
    /// internally consistent, then serializes the header, optional CICP/ICC/palette/
    /// metadata sections, compressed pixel data, optional gain map and its metadata,
    /// and a trailing CRC32 checksum.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Validation of `image` fails.
    /// - Writing to `writer` fails.
    /// - A required section (CICP, ICC, palette, or gain map with its metadata)
    ///   is missing when the corresponding flag indicates it should be present.
    /// - The compression algorithm in `image.compression` is unsupported.
    pub fn write_to(&self, mut writer: impl Write) -> AlohaResult {
        self.validate()?;

        self.write_header(&mut writer)?;

        if !matches!(self.color_profile_desc, ColorProfileDesc::None) {
            self.write_color_profile(&mut writer)?;
        }

        if self.color_type.is_indexed() {
            self.write_palette(&mut writer)?;
        }

        self.write_metadata(&mut writer)?;
        self.write_data(&mut writer)?;

        if self.has_gain_map {
            self.write_gain_map(&mut writer)?;
        }

        Ok(())
    }

    /// Decodes an `.aloha` stream from the provided `reader` into an `AlohaImage`.
    ///
    /// The function reads the magic bytes and version, parses all sections
    /// (header, optional CICP/ICC/palette/metadata, compressed pixel data, optional
    /// gain map and its metadata), verifies the trailing CRC32 checksum, and finally
    /// validates the decoded image via [`AlohaImage::validate`].
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The stream does not start with the [`MAGIC`] bytes.
    /// - The version is newer than the supported [`VERSION`].
    /// - A section cannot be parsed correctly.
    /// - The stored CRC32 checksum does not match the computed checksum.
    /// - The compression algorithm is unsupported.
    /// - The decoded image fails validation.
    pub fn read_from(mut reader: impl Read) -> AlohaResult<Self> {
        // Reading the header
        let mut header_hasher = Hasher::new();

        let mut magic = [0u8; 5];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(AlohaError::DecodingError("not an *.aloha file".to_owned()));
        }
        header_hasher.update(&magic);

        let version = read_u16_hash(&mut reader, &mut header_hasher)?;
        if version > VERSION {
            return Err(AlohaError::DecodingError(format!(
                "unsupported version: {version}"
            )));
        }

        let width = read_u32_hash(&mut reader, &mut header_hasher)?;
        let height = read_u32_hash(&mut reader, &mut header_hasher)?;

        let mut buf = [0u8; 5];
        reader.read_exact(&mut buf)?;
        header_hasher.update(&buf);

        let color_type = ColorType::try_from(buf[0])?;
        let bit_depth = buf[1];
        let color_profile_desc = ColorProfileDesc::try_from(buf[2])?;
        let compression = Compression::try_from(buf[3])?;
        let has_gain_map = buf[4] != 0;

        let stored_crc = read_u32(&mut reader)?;
        let actual_crc = header_hasher.finalize();
        if stored_crc != actual_crc {
            return Err(AlohaError::DecodingError(format!(
                "the header's checksum does not match: {actual_crc:08x} (actual) =/= {stored_crc:08x} (stored)"
            )));
        }

        // Reading the color profile section
        let mut cicp = None;
        let mut icc_profile = None;

        match color_profile_desc {
            ColorProfileDesc::None => { /* do nothing */ }

            ColorProfileDesc::Cicp => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;

                let color_primaries = buf[0];
                let transfer_characteristics = buf[1];
                let matrix_coefficients = buf[2];
                let full_range = buf[3] != 0;

                cicp = Some(zencodec::Cicp::new(
                    color_primaries,
                    transfer_characteristics,
                    matrix_coefficients,
                    full_range,
                ));

                let stored_crc = read_u32(&mut reader)?;
                let actual_crc = crc32fast::hash(&buf);
                if stored_crc != actual_crc {
                    return Err(AlohaError::DecodingError(format!(
                        "the CICP's checksum does not match: {actual_crc:08x} (actual) =/= {stored_crc:08x} (stored)"
                    )));
                }
            }

            ColorProfileDesc::Icc => {
                let mut icc_bytes = vec![0u8; read_u32(&mut reader)? as usize];
                reader.read_exact(&mut icc_bytes)?;
                let actual_crc = crc32fast::hash(&icc_bytes);

                icc_profile = Some(icc_bytes);

                let stored_crc = read_u32(&mut reader)?;
                if stored_crc != actual_crc {
                    return Err(AlohaError::DecodingError(format!(
                        "the ICC profile's checksum does not match: {actual_crc:08x} (actual) =/= {stored_crc:08x} (stored)"
                    )));
                }
            }
        }

        // Reading the palette section
        let mut palette = None;
        let mut transparency_palette = None;

        if color_type.is_indexed() {
            let mut hasher = Hasher::new();

            let mut palette_bytes = vec![0u8; read_u16(&mut reader)? as usize];
            reader.read_exact(&mut palette_bytes)?;
            hasher.update(&palette_bytes);

            palette = Some(palette_bytes);

            if color_type.has_alpha() {
                let mut tp_bytes = vec![0u8; read_u8(&mut reader)? as usize];
                reader.read_exact(&mut tp_bytes)?;
                hasher.update(&tp_bytes);

                transparency_palette = Some(tp_bytes);
            }

            let stored_crc = read_u32(&mut reader)?;
            let actual_crc = hasher.finalize();
            if stored_crc != actual_crc {
                return Err(AlohaError::DecodingError(format!(
                    "the palette's checksum does not match: {actual_crc:08x} (actual) =/= {stored_crc:08x} (stored)"
                )));
            }
        }

        // Reading the metadata section
        let mut meta_hasher = Hasher::new();

        let mut meta_bytes = vec![0u8; read_u32(&mut reader)? as usize];
        reader.read_exact(&mut meta_bytes)?;
        meta_hasher.update(&meta_bytes);

        let xmp_str = std::str::from_utf8(&meta_bytes)
            .map_err(|e| AlohaError::DecodingError(e.to_string()))?;
        let metadata = Metadata::from_xmp(xmp_str)?;

        let stored_crc = read_u32(&mut reader)?;
        let actual_crc = meta_hasher.finalize();
        if stored_crc != actual_crc {
            return Err(AlohaError::DecodingError(format!(
                "the metadata's checksum does not match: {actual_crc:08x} (actual) =/= {stored_crc:08x} (stored)"
            )));
        }

        // Reading the compressed data section
        let data_len = read_u64(&mut reader)?;
        let mut compressed = vec![0u8; data_len as usize];
        reader.read_exact(&mut compressed)?;

        let stored_hash = read_u32(&mut reader)?;
        let actual_hash = crc32fast::hash(&compressed);
        if actual_hash != stored_hash {
            return Err(AlohaError::DecodingError("help me pls".to_owned()));
        }

        let data = match compression {
            Compression::None => compressed.clone(),

            Compression::DeltaZlib => {
                let mut dec = flate2::read::ZlibDecoder::new(compressed.as_slice());
                let mut filtered = Vec::new();
                dec.read_to_end(&mut filtered)?;
                delta_decode(
                    &filtered,
                    width as usize,
                    height as usize,
                    &color_type,
                    bit_depth,
                )?
            }

            Compression::DeltaZstd => {
                let mut dec = zstd::Decoder::new(compressed.as_slice())?;
                let mut filtered = Vec::new();
                dec.read_to_end(&mut filtered)?;
                delta_decode(
                    &filtered,
                    width as usize,
                    height as usize,
                    &color_type,
                    bit_depth,
                )?
            }
        };

        // Reading the gain map section
        let mut gain_map_metadata = None;
        let mut gain_map = None;

        if has_gain_map {
            let mut hasher = Hasher::new();

            let mut gain_map_max = [0.0f64; 3];
            let mut gain_map_min = [0.0f64; 3];
            let mut gamma = [0.0f64; 3];
            let mut base_offset = [0.0f64; 3];
            let mut alternate_offset = [0.0f64; 3];

            for i in 0..3 {
                gain_map_max[i] = read_f64_hash(&mut reader, &mut hasher)?;
                gain_map_min[i] = read_f64_hash(&mut reader, &mut hasher)?;
                gamma[i] = read_f64_hash(&mut reader, &mut hasher)?;
                base_offset[i] = read_f64_hash(&mut reader, &mut hasher)?;
                alternate_offset[i] = read_f64_hash(&mut reader, &mut hasher)?;
            }

            let base_hdr_headroom = read_f64_hash(&mut reader, &mut hasher)?;
            let alternate_hdr_headroom = read_f64_hash(&mut reader, &mut hasher)?;

            let mut buf = [0u8; 2];
            reader.read_exact(&mut buf)?;
            hasher.update(&buf);

            let use_base_color_space = buf[0] != 0;
            let backward_direction = buf[1] != 0;

            // this mess is due to the fact that the `GainMapMetadata` is non-exhaustive
            let mut gmmd = ultrahdr_core::GainMapMetadata::default();
            gmmd.gain_map_max = gain_map_max;
            gmmd.gain_map_min = gain_map_min;
            gmmd.gamma = gamma;
            gmmd.base_offset = base_offset;
            gmmd.alternate_offset = alternate_offset;
            gmmd.base_hdr_headroom = base_hdr_headroom;
            gmmd.alternate_hdr_headroom = alternate_hdr_headroom;
            gmmd.use_base_color_space = use_base_color_space;
            gmmd.backward_direction = backward_direction;
            gain_map_metadata = Some(gmmd);

            let stored_crc = read_u32(&mut reader)?;
            let actual_crc = hasher.finalize();
            if stored_crc != actual_crc {
                return Err(AlohaError::DecodingError(format!(
                    "the gain map metadata's checksum does not match: {actual_crc:08x} (actual) =/= {actual_crc:08x} (stored)"
                )));
            }

            let mut hasher = Hasher::new();

            let width = read_u32_hash(&mut reader, &mut hasher)?;
            let height = read_u32_hash(&mut reader, &mut hasher)?;
            let channels = read_u8_hash(&mut reader, &mut hasher)?;

            let mut gain_map_bytes = vec![0u8; read_u64(&mut reader)? as usize];
            reader.read_exact(&mut gain_map_bytes)?;
            hasher.update(&gain_map_bytes);

            let mut dec = flate2::read::ZlibDecoder::new(gain_map_bytes.as_slice());
            let mut filtered = Vec::new();
            dec.read_to_end(&mut filtered)?;

            let data = delta_decode(
                &filtered,
                width as usize,
                height as usize,
                &ColorType::from_channels_count(channels as usize)?,
                8,
            )?;

            gain_map = Some(ultrahdr_core::GainMap {
                width,
                height,
                channels,
                data,
            });

            let stored_crc = read_u32(&mut reader)?;
            let actual_crc = hasher.finalize();
            if stored_crc != actual_crc {
                return Err(AlohaError::DecodingError(format!(
                    "the gain map's checksum does not match: {actual_crc:08x} (actual) =/= {actual_crc:08x} (stored)"
                )));
            }
        }

        let image = AlohaImage {
            width,
            height,
            color_type,
            bit_depth,
            color_profile_desc,
            compression,
            has_gain_map,
            cicp,
            icc_profile,
            palette,
            transparency_palette,
            metadata,
            data,
            gain_map_metadata,
            gain_map,
        };

        image.validate()?;
        Ok(image)
    }

    /// Changes [`AlohaImage::bit_depth`] from 16 to 8 and rewrites its data.
    pub fn reduce_bit_depth(&mut self) {
        if self.bit_depth == 8 {
            return;
        }

        let channels = self.color_type.channels();
        self.data = self
            .data
            .chunks_exact(channels * 2)
            .flat_map(|s| {
                let mut out = vec![0u8; channels];
                for c in 0..channels {
                    out[c] = (u16::from_be_bytes([s[c * 2], s[c * 2 + 1]]) / 256) as u8;
                }
                out
            })
            .collect();

        self.bit_depth = 8;
    }

    /// Removes alpha-channel from the `AlohaImage`, rewrites its data, and changes
    /// its `color_type`.
    pub fn discard_alpha(&mut self) {
        match self.color_type {
            ColorType::GrayscaleAlpha => {
                self.data = if self.bit_depth == 8 {
                    self.data.chunks_exact(2).map(|s| s[0]).collect()
                } else {
                    self.data
                        .chunks_exact(4)
                        .flat_map(|s| s[..2].to_owned())
                        .collect()
                };

                self.color_type = ColorType::Grayscale;
            }

            ColorType::Rgba => {
                self.data = if self.bit_depth == 8 {
                    self.data
                        .chunks_exact(4)
                        .flat_map(|s| s[..2].to_owned())
                        .collect()
                } else {
                    self.data
                        .chunks_exact(8)
                        .flat_map(|s| s[..6].to_owned())
                        .collect()
                };

                self.color_type = ColorType::Rgb;
            }

            ColorType::IndexedAlpha => {
                self.data = self.data.chunks_exact(2).map(|s| s[0]).collect();
                self.color_type = ColorType::Indexed;
                self.transparency_palette = None;
            }

            _ => {}
        }
    }
}

fn write_bytes(bytes: &[u8], w: &mut impl Write, h: &mut Hasher) -> AlohaResult {
    w.write_all(bytes)?;
    h.update(bytes);
    Ok(())
}

fn read_u8(r: &mut impl Read) -> AlohaResult<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u8_hash(r: &mut impl Read, h: &mut Hasher) -> AlohaResult<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    h.update(&b);
    Ok(b[0])
}

fn read_u16(r: &mut impl Read) -> AlohaResult<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u16_hash(r: &mut impl Read, h: &mut Hasher) -> AlohaResult<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    h.update(&b);
    Ok(u16::from_le_bytes(b))
}

fn read_u32(r: &mut impl Read) -> AlohaResult<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u32_hash(r: &mut impl Read, h: &mut Hasher) -> AlohaResult<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    h.update(&b);
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> AlohaResult<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_f64_hash(r: &mut impl Read, h: &mut Hasher) -> AlohaResult<f64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    h.update(&b);
    Ok(f64::from_le_bytes(b))
}
