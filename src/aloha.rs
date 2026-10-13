use std::io::{Read, Write};

use anyhow::{Ok, Result, bail};

const MAGIC: &[u8; 6] = b"ALOHA1";
const VERSION: u16 = 2;

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

impl TryFrom<u8> for ColorType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Grayscale),
            1 => Ok(Self::GrayscaleAlpha),
            2 => Ok(Self::Rgb),
            3 => Ok(Self::Rgba),
            4 => Ok(Self::Indexed),
            _ => bail!("Invalid color type: {value}"),
        }
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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BitDepth {
    Eight = 8,
    Sixteen = 16,
}

impl BitDepth {
    pub fn sample_mask(&self) -> u16 {
        if matches!(self, Self::Eight) {
            0xFF
        } else {
            0xFFFF
        }
    }

    pub fn bytes_per_sample(&self) -> usize {
        *self as usize / 8
    }
}

impl TryFrom<&png::BitDepth> for BitDepth {
    type Error = anyhow::Error;

    fn try_from(value: &png::BitDepth) -> Result<Self, Self::Error> {
        use png::BitDepth::*;
        match value {
            Eight => Ok(Self::Eight),
            Sixteen => Ok(Self::Sixteen),
            _ => bail!("Unsupported bit depth: {}", *value as u8),
        }
    }
}

impl TryFrom<u8> for BitDepth {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            8 => Ok(Self::Eight),
            16 => Ok(Self::Sixteen),
            _ => bail!("Invalid bit depth: {value}"),
        }
    }
}

impl From<&BitDepth> for png::BitDepth {
    fn from(value: &BitDepth) -> Self {
        use png::BitDepth::*;
        match value {
            BitDepth::Eight => Eight,
            BitDepth::Sixteen => Sixteen,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Metadata {
    pub author: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlohaImage {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: BitDepth,
    pub compression: u8,
    pub palette: Option<Vec<u8>>,
    pub metadata: Metadata,
    pub data: Vec<u8>,
}

impl AlohaImage {
    pub fn validate(&self) -> Result<()> {
        if self.color_type == ColorType::Indexed && self.bit_depth > BitDepth::Eight {
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
            bail!("The data size {actual_size} does not match the expected {expected_size}");
        }

        Ok(())
    }

    pub fn bytes_per_pixel(&self) -> usize {
        self.bit_depth.bytes_per_sample() * self.color_type.channels()
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
    let file = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::new(file);

    w.write_all(MAGIC)?;
    w.write_all(&VERSION.to_le_bytes())?;

    w.write_all(&image.width.to_le_bytes())?;
    w.write_all(&image.height.to_le_bytes())?;

    w.write_all(&[
        image.color_type as u8,
        image.bit_depth as u8,
        image.compression,
    ])?;

    if let Some(p) = &image.palette {
        w.write_all(&(p.len() as u16).to_le_bytes())?;
        w.write_all(p)?;
    }

    let meta_json = serde_json::to_vec(&image.metadata)?;
    w.write_all(&(meta_json.len() as u32).to_le_bytes())?;
    w.write_all(&meta_json)?;

    let compressed = match image.compression {
        0 => Ok(image.data.clone()),
        1 => {
            let filtered = delta_encode(&image);
            let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            enc.write_all(&filtered)?;
            let compressed = enc.finish()?;
            Ok(compressed)
        }
        _ => bail!("Unsupported compression algorithm: {}", image.compression),
    }?;

    w.write_all(&(compressed.len() as u64).to_le_bytes())?;
    w.write_all(&compressed)?;

    w.write_all(&crc32fast::hash(&compressed).to_le_bytes())?;

    w.flush()?;
    Ok(())
}

pub fn read_aloha(path: &std::path::Path) -> Result<AlohaImage> {
    let file = std::fs::File::open(path)?;
    let mut r = std::io::BufReader::new(file);

    let mut magic = [0u8; 6];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        bail!("Not an *.aloha file");
    }

    let version = read_u16(&mut r)?;
    if version > VERSION {
        bail!("Unsupported version: {version}");
    }

    let width = read_u32(&mut r)?;
    let height = read_u32(&mut r)?;

    let mut one = [0u8; 1];
    r.read_exact(&mut one)?;
    let color_type = ColorType::try_from(one[0])?;
    r.read_exact(&mut one)?;
    let bit_depth = BitDepth::try_from(one[0])?;
    r.read_exact(&mut one)?;
    let compression = one[0];

    let mut palette = None;
    if matches!(color_type, ColorType::Indexed) {
        let palette_len = read_u16(&mut r)? as usize;
        let mut palette_bytes = vec![0u8; palette_len];
        r.read_exact(&mut palette_bytes)?;
        palette = Some(palette_bytes);
    }

    let meta_len = read_u32(&mut r)? as usize;
    let mut meta_bytes = vec![0u8; meta_len];
    r.read_exact(&mut meta_bytes)?;
    let metadata = serde_json::from_slice::<Metadata>(&meta_bytes)?;

    let data_len = read_u64(&mut r)? as usize;
    let mut pixels_bytes = vec![0u8; data_len];
    r.read_exact(&mut pixels_bytes)?;

    let stored_crc = read_u32(&mut r)?;
    let actual_crc = crc32fast::hash(&pixels_bytes);
    if stored_crc != actual_crc {
        bail!(
            "The CRC does not match: {:08x} != {:08x}",
            stored_crc,
            actual_crc
        );
    }

    let data = match compression {
        0 => pixels_bytes,
        1 => {
            let mut dec = flate2::read::ZlibDecoder::new(&pixels_bytes[..]);
            let mut filtered = Vec::new();
            dec.read_to_end(&mut filtered)?;
            delta_decode(
                &filtered,
                &color_type,
                &bit_depth,
                width as usize,
                height as usize,
            )
        }
        _ => bail!("Invalid compression algorithm {compression}"),
    };

    let image = AlohaImage {
        width,
        height,
        color_type,
        bit_depth,
        palette,
        compression,
        metadata,
        data,
    };

    Ok(image)
}

fn delta_encode(image: &AlohaImage) -> Vec<u8> {
    if image.color_type.is_indexed() {
        return image.data.clone();
    }

    let ch = image.color_type.channels() as usize;
    let mask = image.bit_depth.sample_mask();
    let bytes_per_sample = image.bit_depth.bytes_per_sample();

    let mut out = vec![0u8; image.data.len()];

    for y in 0..image.height as usize {
        let mut prev = vec![0u16; ch];
        for x in 0..image.width as usize {
            for c in 0..ch {
                let s = (y * image.width as usize + x) * ch + c;
                let v = AlohaImage::read_sample(bytes_per_sample, &image.data, s);
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
    color_type: &ColorType,
    bit_depth: &BitDepth,
    width: usize,
    height: usize,
) -> Vec<u8> {
    if color_type.is_indexed() {
        return data.to_vec();
    }

    let channels = color_type.channels();
    let bytes_per_sample = bit_depth.bytes_per_sample();
    let mut out = vec![0u8; data.len()];

    for y in 0..height as usize {
        let mut prev = vec![0u16; channels];
        for x in 0..width as usize {
            for c in 0..channels {
                let s = (y * width as usize + x) * channels + c;
                let d = AlohaImage::read_sample(bytes_per_sample, data, s);
                let v = (prev[c].wrapping_add(d)) & bit_depth.sample_mask();
                AlohaImage::write_sample(bytes_per_sample, &mut out, s, v);
                prev[c] = v;
            }
        }
    }

    out
}

fn read_u16(r: &mut impl Read) -> Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32(r: &mut impl Read) -> Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}
