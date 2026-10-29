use std::path::Path;

use anyhow::{Context, Ok, Result, bail};
use ultrahdr_core::{PixelFormat, TransferFunction};

use crate::aloha::{AlohaImage, ColorType, Metadata, read_aloha, write_aloha};

fn png_info_to_uhdr_pf(
    color_type: &png::ColorType,
    bit_depth: &png::BitDepth,
) -> Result<PixelFormat> {
    use PixelFormat::*;
    use png::{BitDepth::*, ColorType::*};
    Ok(match (color_type, bit_depth) {
        (Rgb, Eight) => Rgb8,
        (Rgba, Eight) => Rgb8,
        (Rgb, Sixteen) => Rgb16,
        (Rgba, Sixteen) => Rgba16,
        _ => bail!("Unsupported image format"),
    })
}

fn is_hdr(tf: &TransferFunction) -> bool {
    matches!(tf, TransferFunction::Pq | TransferFunction::Hlg)
}

pub fn png_to_aloha(png_path: &Path, aloha_path: &Path) -> Result<()> {
    let mut reader = png::Decoder::new(std::fs::File::open(png_path)?).read_info()?;
    let (width, height, color_type, bit_depth, icc_profile, palette, cicp) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.bit_depth,
            info.icc_profile.clone().map(Vec::from),
            info.palette.clone().map(Vec::from),
            info.coding_independent_code_points,
        )
    };

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)?;
    let mut data = buf[..frame.buffer_size()].to_vec();

    let mut has_gain_map = false;
    let mut gain_map_metadata = None;
    let mut gain_map = None;

    if let Some(cicp) = cicp {
        let format = png_info_to_uhdr_pf(&color_type, &bit_depth)?;
        let primaries = ultrahdr_core::ColorPrimaries::from_cicp(cicp.color_primaries)
            .context("Unknown color primaries")?;
        let transfer = TransferFunction::from_cicp(cicp.transfer_function)
            .context("Unknown transfer function")?;
        has_gain_map = is_hdr(&transfer);

        if has_gain_map {
            let hdr = ultrahdr_core::new_pixel_buffer(
                width / 4,
                height / 4,
                format,
                primaries,
                transfer,
            )?;
            let sdr = ultrahdr_core::new_pixel_buffer(width, height, format, primaries, transfer)?;

            let config = ultrahdr_core::GainMapConfig::default();
            (gain_map, gain_map_metadata) = Some(ultrahdr_core::gainmap::compute_gainmap(
                &hdr,
                &sdr,
                &config,
                ultrahdr_core::Unstoppable,
            )?)
            .unzip();

            data = sdr.into_vec();
        }
    }

    let image = AlohaImage {
        width,
        height,
        color_type: ColorType::from(&color_type),
        bit_depth: crate::aloha::from_png_bit_depth(&bit_depth)?,
        compression: 1,
        has_gain_map,
        icc_profile,
        palette,
        metadata: crate::aloha::Metadata {
            author: None,
            source: Some(format!(
                "Converted from {}",
                png_path.to_str().unwrap_or("<NOT FOUND>")
            )),
        },
        data,
        gain_map_metadata,
        gain_map,
    };

    write_aloha(aloha_path, &image)?;
    Ok(())
}

pub fn aloha_to_png(aloha_path: &Path, png_path: &Path) -> Result<()> {
    let image = read_aloha(aloha_path)?;

    let mut info = png::Info::with_size(image.width, image.height);

    info.color_type = png::ColorType::from(&image.color_type);
    info.bit_depth = crate::aloha::to_png_bit_depth(image.bit_depth)?;

    if let Some(p) = &image.palette {
        info.palette = Some(std::borrow::Cow::Borrowed(p));
    }

    if let Some(icc) = &image.icc_profile {
        info.icc_profile = Some(std::borrow::Cow::Borrowed(icc));
    }

    let hdr_data = if let Some(metadata) = &image.gain_map_metadata {
        let Some(gain_map) = &image.gain_map else {
            bail!("Image must have gain map but it's absent");
        };

        info.coding_independent_code_points = Some(png::CodingIndependentCodePoints {
            color_primaries: 9,
            transfer_function: 16,
            matrix_coefficients: 0,
            is_video_full_range_image: true,
        });

        Some(ultrahdr_core::gainmap::apply_gainmap(
            &ultrahdr_core::PixelBuffer::from_vec(
                image.data.clone(),
                image.width,
                image.height,
                ultrahdr_core::descriptor_for(
                    png_info_to_uhdr_pf(&info.color_type, &info.bit_depth)?,
                    ultrahdr_core::ColorPrimaries::Bt2020,
                    ultrahdr_core::TransferFunction::Pq,
                ),
            )?,
            gain_map,
            metadata,
            4.0,
            ultrahdr_core::HdrOutputFormat::LinearFloat,
            ultrahdr_core::Unstoppable,
        )?)
    } else {
        None
    };

    let enc = png::Encoder::with_info(
        std::io::BufWriter::new(std::fs::File::create(png_path)?),
        info,
    )?;

    let mut writer = enc.write_header()?;
    if let Some(hdr) = hdr_data {
        writer.write_image_data(&hdr.as_slice().contiguous_bytes())?;
    } else {
        writer.write_image_data(&image.data)?;
    }

    Ok(())
}

// TODO: Implement gain map extraction from .jpeg
pub fn jpeg_to_aloha(jpeg_path: &Path, aloha_path: &Path) -> Result<()> {
    let mut reader = jpeg_decoder::Decoder::new(std::fs::File::open(jpeg_path)?);
    let data = reader.decode()?;

    // should be after `reader.decode()`
    let Some(info) = reader.info() else {
        bail!("Can't read info");
    };

    let image = AlohaImage {
        width: info.width as u32,
        height: info.height as u32,
        color_type: ColorType::try_from(&info.pixel_format)?,
        bit_depth: crate::aloha::from_jpeg_bit_depth(&info.pixel_format)?,
        compression: 1,
        has_gain_map: false,
        icc_profile: reader.icc_profile(),
        palette: None,
        metadata: Metadata {
            author: None,
            source: Some(format!(
                "Converted from {}",
                jpeg_path.to_str().unwrap_or("<NOT FOUND>")
            )),
        },
        data,
        gain_map_metadata: None,
        gain_map: None,
    };

    write_aloha(aloha_path, &image)?;
    Ok(())
}

pub fn aloha_to_jpeg(aloha_path: &Path, jpeg_path: &Path) -> Result<()> {
    let image = read_aloha(aloha_path)?;

    let mut enc = jpeg_encoder::Encoder::new_file(jpeg_path, 90)?;
    if let Some(icc) = image.icc_profile {
        enc.add_icc_profile(&icc)?;
    }

    enc.encode(
        &image.data,
        image.width as u16,
        image.height as u16,
        jpeg_encoder::ColorType::Rgb,
    )?;

    Ok(())
}
