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
    error::{AlohaError, AlohaResult},
};

pub mod color;
pub mod error;

const MAGIC: &[u8; 5] = b"ALOHA";
const VERSION: u16 = 0;

/// Optional metadata associated with an [`AlohaImage`].
///
/// This metadata is serialized as JSON within the `.aloha` container.
/// All fields are optional and omitted from the serialized output when `None`.
#[non_exhaustive]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Metadata {
    /// The name or identifier of the image author, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    ///A description of the image source or origin, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Metadata {
    pub fn from_source(src: impl ToString) -> Self {
        Self {
            author: None,
            source: Some(src.to_string()),
        }
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
    ///
    /// - `0` — no compression (raw bytes)
    /// - `1` — delta encoding followed by zlib compression
    pub compression: u8,
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
        compression: u8,
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
    /// - `Indexed` and `IndexedAlpha` color types have a palette and a bit
    ///   depth of at most 8.
    /// - Width and height are both non-zero.
    /// - The length of `data` matches `width * height * bytes_per_pixel()`.
    /// - If a gain map is present, its dimensions and data length are valid,
    ///   and its metadata passes validation
    ///
    /// # Errors
    ///
    /// Returns an [`AlohaError::ValidationError`] if any of the above checks fail.
    pub fn validate(&self) -> AlohaResult {
        if self.color_type.is_indexed() {
            if self.palette.is_none() {
                return Err(AlohaError::ValidationError(
                    "the palette not found".to_owned(),
                ));
            }

            if self.color_type.has_alpha() && self.transparency_palette.is_none() {
                return Err(AlohaError::ValidationError(
                    "the transparency palette not found".to_owned(),
                ));
            }
        }

        if self.color_type.is_indexed() && self.bit_depth > 8 {
            return Err(AlohaError::ValidationError(format!(
                "invalid bit depth for indexed color type: {}",
                self.bit_depth as u8
            )));
        }

        if self.width < 1 || self.height < 1 {
            return Err(AlohaError::ValidationError(format!(
                "invalid image resolution: {}x{}",
                self.width, self.height
            )));
        }

        let actual_size = self.data.len();
        let expected_size = self.width as usize * self.height as usize * self.bytes_per_pixel();
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

    /// Returns the number of bytes required to store a single pixel.
    pub fn bytes_per_pixel(&self) -> usize {
        (self.bit_depth as usize / 8) * self.color_type.channels()
    }

    /// Reads a single color sample from `data` at the given sample index.
    ///
    /// The sample is interpreted as a little-endian unsigned integer and widened
    /// to `u16`. Only 1-byte and 2-byte samples are supported
    ///
    /// # Errors
    ///
    /// Returns [`AlohaError::InvalidBytesPerSample`] if `bytes_per_sample` is neither 1 nor 2.
    pub fn read_sample(
        bytes_per_sample: usize,
        data: &[u8],
        sample_index: usize,
    ) -> AlohaResult<u16> {
        let off = sample_index * bytes_per_sample as usize;
        Ok(match bytes_per_sample {
            1 => data[off] as u16,
            2 => u16::from_le_bytes([data[off], data[off + 1]]),
            _ => return Err(AlohaError::InvalidBytesPerSample(bytes_per_sample)),
        })
    }

    /// Writes a `u16` value as a little-endian sample into `data`.
    ///
    /// # Errors
    ///
    /// Returns [`AlohaError::InvalidBytesPerSample`] if `bytes_per_sample` is neither 1 nor 2.
    pub fn write_sample(
        bytes_per_sample: usize,
        data: &mut [u8],
        sample_index: usize,
        value: u16,
    ) -> AlohaResult {
        let off = sample_index * bytes_per_sample as usize;
        Ok(match bytes_per_sample {
            1 => data[off] = value as u8,
            2 => {
                let b = value.to_le_bytes();
                data[off] = b[0];
                data[off + 1] = b[1];
            }
            _ => return Err(AlohaError::InvalidBytesPerSample(bytes_per_sample)),
        })
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

        let mut hasher = crc32fast::Hasher::new();

        // Writing the header
        write_bytes(MAGIC, &mut writer, &mut hasher)?;
        write_bytes(&VERSION.to_le_bytes(), &mut writer, &mut hasher)?;

        write_bytes(&self.width.to_le_bytes(), &mut writer, &mut hasher)?;
        write_bytes(&self.height.to_le_bytes(), &mut writer, &mut hasher)?;

        write_bytes(
            &[
                self.color_type as u8,
                self.bit_depth,
                self.color_profile_desc as u8,
                self.compression,
                self.has_gain_map as u8,
            ],
            &mut writer,
            &mut hasher,
        )?;

        // Writing the CICP section
        if matches!(self.color_profile_desc, ColorProfileDesc::Cicp) {
            let Some(cicp) = &self.cicp else {
                return Err(AlohaError::CicpNotFound);
            };

            write_bytes(
                &[
                    cicp.color_primaries,
                    cicp.transfer_characteristics,
                    cicp.matrix_coefficients,
                    cicp.full_range as u8,
                ],
                &mut writer,
                &mut hasher,
            )?;
        }

        // Writing the ICC profile section
        if matches!(self.color_profile_desc, ColorProfileDesc::Icc) {
            let Some(icc) = &self.icc_profile else {
                return Err(AlohaError::IccNotFound);
            };

            write_bytes(&(icc.len() as u32).to_le_bytes(), &mut writer, &mut hasher)?;
            write_bytes(icc, &mut writer, &mut hasher)?;
        }

        // Writing the palette section
        if self.color_type.is_indexed() {
            let Some(p) = &self.palette else {
                return Err(AlohaError::PaletteNotFound("color".to_owned()));
            };

            write_bytes(&(p.len() as u16).to_le_bytes(), &mut writer, &mut hasher)?;
            write_bytes(p, &mut writer, &mut hasher)?;

            if self.color_type.has_alpha() {
                let Some(tp) = &self.transparency_palette else {
                    return Err(AlohaError::PaletteNotFound("transparency".to_owned()));
                };

                write_bytes(&[tp.len() as u8], &mut writer, &mut hasher)?;
                write_bytes(tp, &mut writer, &mut hasher)?;
            }
        }

        // Writing the metadata section
        let meta_json = serde_json::to_vec(&self.metadata)?;
        write_bytes(
            &(meta_json.len() as u32).to_le_bytes(),
            &mut writer,
            &mut hasher,
        )?;
        write_bytes(&meta_json, &mut writer, &mut hasher)?;

        // Writing the compressed pixel data section
        let compressed = match self.compression {
            0 => self.data.clone(),
            1 => {
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
                let compressed = enc.finish()?;
                compressed
            }
            other => return Err(AlohaError::InvalidCompressionAlgorithm(other)),
        };

        write_bytes(
            &(compressed.len() as u64).to_le_bytes(),
            &mut writer,
            &mut hasher,
        )?;
        write_bytes(&compressed, &mut writer, &mut hasher)?;

        if self.has_gain_map {
            // Writing the gain map metadata section
            let Some(gain_map_params) = &self.gain_map_metadata else {
                return Err(AlohaError::GainMapMetadataNotFound);
            };

            for i in 0..3 {
                write_bytes(
                    &gain_map_params.gain_map_max[i].to_le_bytes(),
                    &mut writer,
                    &mut hasher,
                )?;
                write_bytes(
                    &gain_map_params.gain_map_min[i].to_le_bytes(),
                    &mut writer,
                    &mut hasher,
                )?;
                write_bytes(
                    &gain_map_params.gamma[i].to_le_bytes(),
                    &mut writer,
                    &mut hasher,
                )?;
                write_bytes(
                    &gain_map_params.base_offset[i].to_le_bytes(),
                    &mut writer,
                    &mut hasher,
                )?;
                write_bytes(
                    &gain_map_params.alternate_offset[i].to_le_bytes(),
                    &mut writer,
                    &mut hasher,
                )?;
            }

            write_bytes(
                &gain_map_params.base_hdr_headroom.to_le_bytes(),
                &mut writer,
                &mut hasher,
            )?;
            write_bytes(
                &gain_map_params.alternate_hdr_headroom.to_le_bytes(),
                &mut writer,
                &mut hasher,
            )?;
            write_bytes(
                &[
                    gain_map_params.use_base_color_space as u8,
                    gain_map_params.backward_direction as u8,
                ],
                &mut writer,
                &mut hasher,
            )?;

            // Writing the gain map section
            let Some(gain_map) = &self.gain_map else {
                return Err(AlohaError::GainMapNotFound);
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

            write_bytes(&gain_map.width.to_le_bytes(), &mut writer, &mut hasher)?;
            write_bytes(&gain_map.height.to_le_bytes(), &mut writer, &mut hasher)?;
            write_bytes(&[gain_map.channels], &mut writer, &mut hasher)?;

            write_bytes(
                &(compressed.len() as u64).to_le_bytes(),
                &mut writer,
                &mut hasher,
            )?;
            write_bytes(&compressed, &mut writer, &mut hasher)?;
        }

        // Writing the CRC32 section
        writer.write_all(&hasher.finalize().to_le_bytes())?;
        writer.flush()?;

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
        let mut hasher = crc32fast::Hasher::new();

        // Reading the header
        let mut magic = [0u8; 5];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(AlohaError::DecodingError("not an *.aloha file".to_owned()));
        }
        hasher.update(&magic);

        let version = read_u16(&mut reader, &mut hasher)?;
        if version > VERSION {
            return Err(AlohaError::DecodingError(format!(
                "unsupported version: {version}"
            )));
        }

        let width = read_u32(&mut reader, &mut hasher)?;
        let height = read_u32(&mut reader, &mut hasher)?;

        let mut buf = [0u8; 5];
        reader.read_exact(&mut buf)?;
        hasher.update(&buf);

        let color_type = ColorType::try_from(buf[0])?;
        let bit_depth = buf[1];
        let color_profile_desc = ColorProfileDesc::try_from(buf[2])?;
        let compression = buf[3];
        let has_gain_map = buf[4] != 0;

        // Reading the CICP section
        let cicp = if matches!(color_profile_desc, ColorProfileDesc::Cicp) {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            hasher.update(&buf);

            let color_primaries = buf[0];
            let transfer_characteristics = buf[1];
            let matrix_coefficients = buf[2];
            let full_range = buf[3] != 0;

            Some(zencodec::Cicp::new(
                color_primaries,
                transfer_characteristics,
                matrix_coefficients,
                full_range,
            ))
        } else {
            None
        };

        // Reading the ICC profile section
        let icc_profile = if matches!(color_profile_desc, ColorProfileDesc::Icc) {
            let icc_profile_len = read_u32(&mut reader, &mut hasher)?;

            let mut icc_bytes = vec![0u8; icc_profile_len as usize];
            reader.read_exact(&mut icc_bytes)?;
            hasher.update(&icc_bytes);

            Some(icc_bytes)
        } else {
            None
        };

        // Reading the palette section
        let mut palette = None;
        let mut transparency_palette = None;
        if color_type.is_indexed() {
            let palette_len = read_u16(&mut reader, &mut hasher)?;

            let mut palette_bytes = vec![0u8; palette_len as usize];
            reader.read_exact(&mut palette_bytes)?;
            hasher.update(&palette_bytes);

            palette = Some(palette_bytes);

            if color_type.has_alpha() {
                let tp_len = read_u8(&mut reader, &mut hasher)?;

                let mut tp_bytes = vec![0u8; tp_len as usize];
                reader.read_exact(&mut tp_bytes)?;
                hasher.update(&tp_bytes);

                transparency_palette = Some(tp_bytes);
            }
        }

        // Reading the metadata section
        let meta_len = read_u32(&mut reader, &mut hasher)?;

        let mut meta_bytes = vec![0u8; meta_len as usize];
        reader.read_exact(&mut meta_bytes)?;
        hasher.update(&meta_bytes);

        let metadata = serde_json::from_slice::<Metadata>(&meta_bytes)?;

        // Reading the compressed data section
        let data_len = read_u64(&mut reader, &mut hasher)?;

        let mut pixels_bytes = vec![0u8; data_len as usize];
        reader.read_exact(&mut pixels_bytes)?;
        hasher.update(&pixels_bytes);

        let data = match compression {
            0 => pixels_bytes,
            1 => {
                let mut dec = flate2::read::ZlibDecoder::new(pixels_bytes.as_slice());
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
            _ => return Err(AlohaError::InvalidCompressionAlgorithm(compression)),
        };

        // Reading the gain map section
        let mut gain_map_metadata = None;
        let mut gain_map = None;

        if has_gain_map {
            let mut gain_map_max = [0.0f64; 3];
            let mut gain_map_min = [0.0f64; 3];
            let mut gamma = [0.0f64; 3];
            let mut base_offset = [0.0f64; 3];
            let mut alternate_offset = [0.0f64; 3];

            for i in 0..3 {
                gain_map_max[i] = read_f64(&mut reader, &mut hasher)?;
                gain_map_min[i] = read_f64(&mut reader, &mut hasher)?;
                gamma[i] = read_f64(&mut reader, &mut hasher)?;
                base_offset[i] = read_f64(&mut reader, &mut hasher)?;
                alternate_offset[i] = read_f64(&mut reader, &mut hasher)?;
            }

            let base_hdr_headroom = read_f64(&mut reader, &mut hasher)?;
            let alternate_hdr_headroom = read_f64(&mut reader, &mut hasher)?;

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

            let width = read_u32(&mut reader, &mut hasher)?;
            let height = read_u32(&mut reader, &mut hasher)?;
            let channels = read_u8(&mut reader, &mut hasher)?;

            let gain_map_len = read_u64(&mut reader, &mut hasher)?;
            let mut gain_map_bytes = vec![0u8; gain_map_len as usize];
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
        }

        // Reading the CRC32 section
        let mut crc32_bytes = [0u8; 4];
        reader.read_exact(&mut crc32_bytes)?;

        let stored_crc = u32::from_le_bytes(crc32_bytes);
        let actual_crc = hasher.finalize();

        if stored_crc != actual_crc {
            return Err(AlohaError::DecodingError(format!(
                "the checksum does not match: {stored_crc:08x} != {actual_crc:08x}"
            )));
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

/// Applies delta encoding to `data` in-place and returns the encoded bytes.
///
/// For each channel, the first sample is stored as-is, and every subsequent
/// sample is replaced by the wrapping difference from the previous sample in the
/// same channel. `Indexed` and `IndexedAlpha` color types are returned unchanged.
///
/// # Errors
///
/// Propagates errors from [`AlohaImage::read_sample`] and [`AlohaImage::write_sample`].
fn delta_encode(
    data: &[u8],
    width: usize,
    height: usize,
    color_type: &ColorType,
    bit_depth: u8,
) -> AlohaResult<Vec<u8>> {
    if color_type.is_indexed() {
        return Ok(data.to_vec());
    }

    let channels = color_type.channels();
    let bytes_per_sample = bit_depth as usize / 8;
    let mask = color::get_sample_mask(bit_depth);
    let mut out = vec![0u8; data.len()];

    for y in 0..height as usize {
        let mut prev = vec![0u16; channels];
        for x in 0..width as usize {
            for c in 0..channels {
                let s = (y * width as usize + x) * channels + c;
                let v = AlohaImage::read_sample(bytes_per_sample, data, s)?;
                let d = v.wrapping_sub(prev[c]) & mask;
                AlohaImage::write_sample(bytes_per_sample, &mut out, s, d)?;
                prev[c] = v;
            }
        }
    }

    Ok(out)
}

/// Reverses the delta encoding applied by [`delta_decode`].
///
/// Each sample is reconstructed by wrapping-adding the delta value to the
/// previous sample in the same channel. `Indexed` and `IndexedAlpha` color types
/// are returned unchanged.
///
/// # Errors
///
/// Propagates errors from [`AlohaImage::read_sample`] and [`AlohaImage::write_sample`].
fn delta_decode(
    data: &[u8],
    width: usize,
    height: usize,
    color_type: &ColorType,
    bit_depth: u8,
) -> AlohaResult<Vec<u8>> {
    if color_type.is_indexed() {
        return Ok(data.to_vec());
    }

    let channels = color_type.channels();
    let bytes_per_sample = bit_depth as usize / 8;
    let mask = crate::color::get_sample_mask(bit_depth);
    let mut out = vec![0u8; data.len()];

    for y in 0..height as usize {
        let mut prev = vec![0u16; channels];
        for x in 0..width as usize {
            for c in 0..channels {
                let s = (y * width as usize + x) * channels + c;
                let d = AlohaImage::read_sample(bytes_per_sample, data, s)?;
                let v = (prev[c].wrapping_add(d)) & mask;
                AlohaImage::write_sample(bytes_per_sample, &mut out, s, v)?;
                prev[c] = v;
            }
        }
    }

    Ok(out)
}

fn write_bytes(bytes: &[u8], w: &mut impl Write, hasher: &mut Hasher) -> AlohaResult {
    w.write_all(bytes)?;
    hasher.update(bytes);
    Ok(())
}

fn read_u8(r: &mut impl Read, hasher: &mut Hasher) -> AlohaResult<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    hasher.update(&b);
    Ok(b[0])
}

fn read_u16(r: &mut impl Read, hasher: &mut Hasher) -> AlohaResult<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    hasher.update(&b);
    Ok(u16::from_le_bytes(b))
}

fn read_u32(r: &mut impl Read, hasher: &mut Hasher) -> AlohaResult<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    hasher.update(&b);
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read, hasher: &mut Hasher) -> AlohaResult<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    hasher.update(&b);
    Ok(u64::from_le_bytes(b))
}

fn read_f64(r: &mut impl Read, hasher: &mut Hasher) -> AlohaResult<f64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    hasher.update(&b);
    Ok(f64::from_le_bytes(b))
}
