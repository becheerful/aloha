use std::io::{Read, Write};

use anyhow::{Ok, Result, bail};

const MAGIC: &[u8; 5] = b"ALOHA";
const VERSION: u16 = 0;

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
    pub fn from_channels_count(channels: usize) -> Result<Self> {
        Ok(match channels {
            1 => Self::Grayscale,
            2 => Self::GrayscaleAlpha,
            3 => Self::Rgb,
            4 => Self::Rgba,
            _ => bail!("Unsupported channels count: {channels}"),
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

impl From<&png::ColorType> for ColorType {
    fn from(value: &png::ColorType) -> Self {
        use png::ColorType::*;
        match value {
            Grayscale => Self::Grayscale,
            Rgb => Self::Rgb,
            Indexed => Self::Indexed,
            GrayscaleAlpha => Self::GrayscaleAlpha,
            Rgba => Self::Rgba,
        }
    }
}

impl TryFrom<&jpeg_decoder::PixelFormat> for ColorType {
    type Error = anyhow::Error;

    fn try_from(value: &jpeg_decoder::PixelFormat) -> Result<Self, Self::Error> {
        use jpeg_decoder::PixelFormat::*;
        Ok(match value {
            L8 => Self::Grayscale,
            L16 => Self::Grayscale,
            RGB24 => Self::Rgb,
            _ => bail!("Unsupported pixel format: {value:?}"),
        })
    }
}

impl TryFrom<u8> for ColorType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Grayscale,
            1 => Self::GrayscaleAlpha,
            2 => Self::Rgb,
            3 => Self::Rgba,
            4 => Self::Indexed,
            _ => bail!("Invalid color type: {value}"),
        })
    }
}

impl From<&ColorType> for png::ColorType {
    fn from(value: &ColorType) -> Self {
        use png::ColorType::*;
        match value {
            ColorType::Grayscale => Grayscale,
            ColorType::GrayscaleAlpha => GrayscaleAlpha,
            ColorType::Rgb => Rgb,
            ColorType::Rgba => Rgba,
            ColorType::Indexed => Indexed,
        }
    }
}

impl TryFrom<&ColorType> for jpeg_encoder::ColorType {
    type Error = anyhow::Error;

    fn try_from(value: &ColorType) -> Result<Self, Self::Error> {
        use jpeg_encoder::ColorType::*;
        Ok(match value {
            ColorType::Grayscale => Luma,
            ColorType::Rgb => Rgb,
            _ => bail!("Unsupported color type: {}", *value as u8),
        })
    }
}

pub fn get_sample_mask(bit_depth: u8) -> u16 {
    if bit_depth == 8 { 0xFF } else { 0xFFFF }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProfileDesc {
    None,
    Cicp,
    Icc,
}

impl TryFrom<u8> for ColorProfileDesc {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::None,
            1 => Self::Cicp,
            2 => Self::Icc,
            _ => bail!("Unknown color profile's descriptor: {value}"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AlohaImage {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: u8,
    pub color_profile_desc: ColorProfileDesc,
    pub compression: u8,
    pub has_gain_map: bool,
    pub cicp: Option<zencodec::Cicp>,
    pub icc_profile: Option<Vec<u8>>,
    pub palette: Option<Vec<u8>>,
    pub metadata: Metadata,
    pub data: Vec<u8>,
    pub gain_map_metadata: Option<ultrahdr_core::GainMapMetadata>,
    pub gain_map: Option<ultrahdr_core::GainMap>,
}

impl AlohaImage {
    pub fn validate(&self) -> Result<()> {
        if self.color_type.is_indexed() && self.palette.is_none() {
            bail!("The pallete not found");
        }

        if self.color_type.is_indexed() && self.bit_depth > 8 {
            bail!(
                "Invalid bit depth for indexed color type: {}",
                self.bit_depth as u8
            );
        }

        if self.width < 1 || self.height < 1 {
            bail!("Invalid image resolution: {}x{}", self.width, self.height);
        }

        let actual_size = self.data.len();
        let expected_size = self.width as usize * self.height as usize * self.bytes_per_pixel();
        if actual_size != expected_size {
            bail!(
                "The image's data size {actual_size} does not match the expected {expected_size}"
            );
        }

        if let Some(gain_map) = &self.gain_map {
            if gain_map.width < 1 || gain_map.height < 1 {
                bail!(
                    "Invalid gain map resolution: {}x{}",
                    gain_map.width,
                    gain_map.height
                );
            }

            let actual_size = gain_map.data.len();
            let expected_size =
                gain_map.width as usize * gain_map.height as usize * gain_map.channels as usize;
            if actual_size != expected_size {
                bail!(
                    "The gain map's data size {actual_size} does not match the expected {expected_size}"
                );
            }
        }

        if let Some(gain_map_metadata) = &self.gain_map_metadata {
            gain_map_metadata.validate()?;
        }

        Ok(())
    }

    pub fn bytes_per_pixel(&self) -> usize {
        (self.bit_depth as usize / 8) * self.color_type.channels()
    }

    pub fn read_sample(bytes_per_sample: usize, data: &[u8], sample_index: usize) -> u16 {
        let off = sample_index * bytes_per_sample as usize;
        match bytes_per_sample {
            1 => data[off] as u16,
            2 => u16::from_le_bytes([data[off], data[off + 1]]),
            _ => unreachable!(),
        }
    }

    pub fn write_sample(bytes_per_sample: usize, data: &mut [u8], sample_index: usize, value: u16) {
        let off = sample_index * bytes_per_sample as usize;
        match bytes_per_sample {
            1 => data[off] = value as u8,
            2 => {
                let b = value.to_le_bytes();
                data[off] = b[0];
                data[off + 1] = b[1];
            }
            _ => unreachable!(),
        }
    }
}

pub fn write_aloha(path: &std::path::Path, image: &AlohaImage) -> Result<()> {
    image.validate()?;

    let file = std::fs::File::create(path)?;
    let mut buffer: Vec<u8> = Vec::new();

    // Writing the header
    buffer.extend(MAGIC);
    buffer.extend(VERSION.to_le_bytes());

    buffer.extend(image.width.to_le_bytes());
    buffer.extend(image.height.to_le_bytes());

    buffer.push(image.color_type as u8);
    buffer.push(image.bit_depth);
    buffer.push(image.color_profile_desc as u8);
    buffer.push(image.compression);
    buffer.push(image.has_gain_map as u8);

    // Writing the cICP section
    if matches!(image.color_profile_desc, ColorProfileDesc::Cicp) {
        let Some(cicp) = &image.cicp else {
            bail!("Image must have coding-independent code points but they're absent");
        };

        buffer.push(cicp.color_primaries);
        buffer.push(cicp.transfer_characteristics);
        buffer.push(cicp.matrix_coefficients);
        buffer.push(cicp.full_range as u8);
    }

    // Writing the ICC profile section
    if matches!(image.color_profile_desc, ColorProfileDesc::Icc) {
        let Some(icc) = &image.icc_profile else {
            bail!("Image must have an ICC profile but it's absent");
        };

        buffer.extend((icc.len() as u32).to_le_bytes());
        buffer.extend(icc);
    }

    // Writing the palette section
    if image.color_type.is_indexed() {
        let Some(p) = &image.palette else {
            bail!("Image must have a palette but it's absent");
        };

        buffer.extend((p.len() as u16).to_le_bytes());
        buffer.extend(p);
    }

    // Writing the metadata section
    let meta_json = serde_json::to_vec(&image.metadata)?;
    buffer.extend((meta_json.len() as u32).to_le_bytes());
    buffer.extend(meta_json);

    // Writing the compressed pixel data section
    let compressed = match image.compression {
        0 => Ok(image.data.clone()),
        1 => {
            let filtered = delta_encode(
                &image.data,
                image.width as usize,
                image.height as usize,
                &image.color_type,
                image.bit_depth,
            );
            let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            enc.write_all(&filtered)?;
            let compressed = enc.finish()?;
            Ok(compressed)
        }
        _ => bail!("Unsupported compression algorithm: {}", image.compression),
    }?;

    buffer.extend((compressed.len() as u64).to_le_bytes());
    buffer.extend(compressed);

    if image.has_gain_map {
        // Writing the gain map metadata section
        let Some(gain_map_params) = &image.gain_map_metadata else {
            bail!("Image must have a gain map metadata but it's absent");
        };

        for i in 0..3 {
            buffer.extend(gain_map_params.gain_map_max[i].to_le_bytes());
            buffer.extend(gain_map_params.gain_map_min[i].to_le_bytes());
            buffer.extend(gain_map_params.gamma[i].to_le_bytes());
            buffer.extend(gain_map_params.base_offset[i].to_le_bytes());
            buffer.extend(gain_map_params.alternate_offset[i].to_le_bytes());
        }

        buffer.extend(gain_map_params.base_hdr_headroom.to_le_bytes());
        buffer.extend(gain_map_params.alternate_hdr_headroom.to_le_bytes());
        buffer.push(gain_map_params.use_base_color_space as u8);
        buffer.push(gain_map_params.backward_direction as u8);

        // Writing the gain map section
        let Some(gain_map) = &image.gain_map else {
            bail!("Image must have a gain map but it's absent");
        };

        let filtered = delta_encode(
            &gain_map.data,
            gain_map.width as usize,
            gain_map.height as usize,
            &ColorType::from_channels_count(gain_map.channels as usize)?,
            8, // bit depth for gain map is always 8
        );

        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&filtered)?;
        let compressed = enc.finish()?;

        buffer.extend(gain_map.width.to_le_bytes());
        buffer.extend(gain_map.height.to_le_bytes());
        buffer.push(gain_map.channels);

        buffer.extend((compressed.len() as u64).to_le_bytes());
        buffer.extend(compressed);
    }

    // Writing the CRC32 section
    let checksum = crc32fast::hash(&buffer);
    buffer.extend(checksum.to_le_bytes());

    // Writing the buffer to a file
    let mut w = std::io::BufWriter::new(file);
    w.write_all(&buffer)?;
    w.flush()?;

    Ok(())
}

pub fn read_aloha(path: &std::path::Path) -> Result<AlohaImage> {
    let file = std::fs::File::open(path)?;
    let mut buffer: Vec<u8> = Vec::new();
    let mut r = std::io::BufReader::new(file);

    // Reading the header
    let mut magic = [0u8; 5];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        bail!("Not an *.aloha file");
    }
    buffer.extend(magic);

    let version = read_u16(&mut r, &mut buffer)?;
    if version > VERSION {
        bail!("Unsupported version: {version}");
    }

    let width = read_u32(&mut r, &mut buffer)?;
    let height = read_u32(&mut r, &mut buffer)?;

    let mut one = [0u8; 1];

    r.read_exact(&mut one)?;
    let color_type = one[0];
    buffer.push(color_type);
    let color_type = ColorType::try_from(color_type)?;

    r.read_exact(&mut one)?;
    let bit_depth = one[0];
    buffer.push(bit_depth);

    r.read_exact(&mut one)?;
    buffer.extend(&one);
    let color_profile_desc = ColorProfileDesc::try_from(one[0])?;

    r.read_exact(&mut one)?;
    let compression = one[0];
    buffer.push(compression);

    r.read_exact(&mut one)?;
    buffer.extend(&one);
    let has_gain_map = one[0] != 0;

    // Reading the cICP section
    let cicp = if matches!(color_profile_desc, ColorProfileDesc::Cicp) {
        r.read_exact(&mut one)?;
        let color_primaries = one[0];
        buffer.push(color_primaries);

        r.read_exact(&mut one)?;
        let transfer_characteristics = one[0];
        buffer.push(transfer_characteristics);

        r.read_exact(&mut one)?;
        let matrix_coefficients = one[0];
        buffer.push(matrix_coefficients);

        r.read_exact(&mut one)?;
        buffer.extend(&one);
        let full_range = one[0] != 0;

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
        let icc_profile_len = read_u32(&mut r, &mut buffer)?;

        let mut icc_bytes = vec![0u8; icc_profile_len as usize];
        r.read_exact(&mut icc_bytes)?;
        buffer.extend(&icc_bytes);

        Some(icc_bytes)
    } else {
        None
    };

    // Reading the palette section
    let palette = if color_type.is_indexed() {
        let palette_len = read_u16(&mut r, &mut buffer)?;

        let mut palette_bytes = vec![0u8; palette_len as usize];
        r.read_exact(&mut palette_bytes)?;
        buffer.extend(&palette_bytes);

        Some(palette_bytes)
    } else {
        None
    };

    // Reading the metadata section
    let meta_len = read_u32(&mut r, &mut buffer)?;

    let mut meta_bytes = vec![0u8; meta_len as usize];
    r.read_exact(&mut meta_bytes)?;
    buffer.extend_from_slice(&meta_bytes);

    let metadata = serde_json::from_slice::<Metadata>(&meta_bytes)?;

    // Reading the compressed data section
    let data_len = read_u64(&mut r, &mut buffer)?;

    let mut pixels_bytes = vec![0u8; data_len as usize];
    r.read_exact(&mut pixels_bytes)?;
    buffer.extend(&pixels_bytes);

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
            )
        }
        _ => bail!("Invalid compression algorithm {compression}"),
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
            gain_map_max[i] = read_f64(&mut r, &mut buffer)?;
            gain_map_min[i] = read_f64(&mut r, &mut buffer)?;
            gamma[i] = read_f64(&mut r, &mut buffer)?;
            base_offset[i] = read_f64(&mut r, &mut buffer)?;
            alternate_offset[i] = read_f64(&mut r, &mut buffer)?;
        }

        let base_hdr_headroom = read_f64(&mut r, &mut buffer)?;
        let alternate_hdr_headroom = read_f64(&mut r, &mut buffer)?;

        let mut b = [0u8];

        r.read_exact(&mut b)?;
        buffer.extend(&b);
        let use_base_color_space = b[0] != 0;

        r.read_exact(&mut b)?;
        buffer.extend(&b);
        let backward_direction = b[0] != 0;

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

        let width = read_u32(&mut r, &mut buffer)?;
        let height = read_u32(&mut r, &mut buffer)?;

        r.read_exact(&mut b)?;
        let channels = b[0];
        buffer.push(channels);

        let gain_map_len = read_u64(&mut r, &mut buffer)?;
        let mut gain_map_bytes = vec![0u8; gain_map_len as usize];
        r.read_exact(&mut gain_map_bytes)?;
        buffer.extend(&gain_map_bytes);

        let mut dec = flate2::read::ZlibDecoder::new(gain_map_bytes.as_slice());
        let mut filtered = Vec::new();
        dec.read_to_end(&mut filtered)?;

        let data = delta_decode(
            &filtered,
            width as usize,
            height as usize,
            &ColorType::from_channels_count(channels as usize)?,
            8,
        );

        gain_map = Some(ultrahdr_core::GainMap {
            width,
            height,
            channels,
            data,
        });
    }

    // Reading the CRC32 section
    let mut crc32_bytes = [0u8; 4];
    r.read_exact(&mut crc32_bytes)?;

    let stored_crc = u32::from_le_bytes(crc32_bytes);
    let actual_crc = crc32fast::hash(&buffer);

    if stored_crc != actual_crc {
        bail!(
            "The CRC does not match: {:08x} != {:08x}",
            stored_crc,
            actual_crc
        );
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
        metadata,
        data,
        gain_map_metadata,
        gain_map,
    };

    image.validate()?;
    Ok(image)
}

fn delta_encode(
    data: &[u8],
    width: usize,
    height: usize,
    color_type: &ColorType,
    bit_depth: u8,
) -> Vec<u8> {
    if color_type.is_indexed() {
        return data.to_vec();
    }

    let channels = color_type.channels();
    let bytes_per_sample = bit_depth as usize / 8;
    let mask = get_sample_mask(bit_depth);
    let mut out = vec![0u8; data.len()];

    for y in 0..height as usize {
        let mut prev = vec![0u16; channels];
        for x in 0..width as usize {
            for c in 0..channels {
                let s = (y * width as usize + x) * channels + c;
                let v = AlohaImage::read_sample(bytes_per_sample, data, s);
                let d = v.wrapping_sub(prev[c]) & mask;
                AlohaImage::write_sample(bytes_per_sample, &mut out, s, d);
                prev[c] = v;
            }
        }
    }

    out
}

fn delta_decode(
    data: &[u8],
    width: usize,
    height: usize,
    color_type: &ColorType,
    bit_depth: u8,
) -> Vec<u8> {
    if color_type.is_indexed() {
        return data.to_vec();
    }

    let channels = color_type.channels();
    let bytes_per_sample = bit_depth as usize / 8;
    let mask = get_sample_mask(bit_depth);
    let mut out = vec![0u8; data.len()];

    for y in 0..height as usize {
        let mut prev = vec![0u16; channels];
        for x in 0..width as usize {
            for c in 0..channels {
                let s = (y * width as usize + x) * channels + c;
                let d = AlohaImage::read_sample(bytes_per_sample, data, s);
                let v = (prev[c].wrapping_add(d)) & mask;
                AlohaImage::write_sample(bytes_per_sample, &mut out, s, v);
                prev[c] = v;
            }
        }
    }

    out
}

fn read_u16(r: &mut impl Read, buf: &mut Vec<u8>) -> Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    buf.extend(b);
    Ok(u16::from_le_bytes(b))
}

fn read_u32(r: &mut impl Read, buf: &mut Vec<u8>) -> Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    buf.extend(b);
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read, buf: &mut Vec<u8>) -> Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    buf.extend(b);
    Ok(u64::from_le_bytes(b))
}

fn read_f64(r: &mut impl Read, buf: &mut Vec<u8>) -> Result<f64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    buf.extend(b);
    Ok(f64::from_le_bytes(b))
}
