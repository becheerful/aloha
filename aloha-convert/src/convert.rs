use std::path::Path;

use anyhow::{Context, Result, bail};

use aloha_core::{
    AlohaImage, Metadata,
    color::{ColorProfileDesc, ColorType},
    read_aloha, write_aloha,
};

mod png_conv {
    pub(crate) fn color_type_from_aloha(ct: &super::ColorType) -> png::ColorType {
        use super::ColorType;
        use png::ColorType::*;

        match ct {
            ColorType::Grayscale => Grayscale,
            ColorType::GrayscaleAlpha => GrayscaleAlpha,
            ColorType::Rgb => Rgb,
            ColorType::Rgba => Rgba,
            ColorType::Indexed => Indexed,
        }
    }

    pub(crate) fn color_type_to_aloha(ct: &png::ColorType) -> super::ColorType {
        use super::ColorType;
        use png::ColorType::*;

        match ct {
            Grayscale => ColorType::Grayscale,
            Rgb => ColorType::Rgb,
            Indexed => ColorType::Indexed,
            GrayscaleAlpha => ColorType::GrayscaleAlpha,
            Rgba => ColorType::Rgba,
        }
    }
}

mod jpeg_conv {
    use anyhow::{Result, bail};

    pub(crate) fn color_type_from_aloha(ct: &super::ColorType) -> Result<jpeg_encoder::ColorType> {
        use super::ColorType;
        use jpeg_encoder::ColorType::*;

        Ok(match ct {
            ColorType::Grayscale => Luma,
            ColorType::Rgb => Rgb,
            _ => bail!("Unsupported color type: {}", *ct as u8),
        })
    }

    pub(crate) fn pixel_format_to_aloha(
        pf: &jpeg_decoder::PixelFormat,
    ) -> Result<super::ColorType> {
        use super::ColorType::*;
        use jpeg_decoder::PixelFormat::*;

        Ok(match pf {
            L8 => Grayscale,
            L16 => Grayscale,
            RGB24 => Rgb,
            _ => bail!("Unsupported pixel format: {pf:?}"),
        })
    }

    pub(crate) fn bit_depth_to_aloha(value: &jpeg_decoder::PixelFormat) -> Result<u8> {
        use jpeg_decoder::PixelFormat::*;
        Ok(match value {
            L8 | RGB24 => 8,
            L16 => 16,
            other => bail!("Unsupported pixel format: {other:?}"),
        })
    }
}

fn color_primaries_to_gamut(primaries: u8) -> Result<ultrahdr_core::ColorGamut> {
    use ultrahdr_core::ColorGamut::*;
    Ok(match primaries {
        1 => Bt709,
        9 => Bt2020,
        12 => DisplayP3,
        other => bail!("Unknown color gamut: {other}"),
    })
}

fn convert_transfer_func(tf: u8) -> Result<ultrahdr_core::ColorTransfer> {
    use ultrahdr_core::ColorTransfer::*;
    Ok(match tf {
        1 | 6 | 14 | 15 => Srgb,
        8 => Linear,
        16 => Pq,
        18 => Hlg,
        other => bail!("Unknown transfer function: {other}"),
    })
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

    let mut data = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut data)?;
    data.truncate(info.buffer_size());

    let mut aloha_cicp = None;

    let mut has_gain_map = false;
    if let Some(cicp) = cicp {
        has_gain_map = cicp.transfer_function == 16 || cicp.transfer_function == 18;
        aloha_cicp = Some(zencodec::Cicp::new(
            cicp.color_primaries,
            cicp.transfer_function,
            cicp.matrix_coefficients,
            cicp.is_video_full_range_image,
        ));
    }

    let mut gain_map_metadata = None;
    let mut gain_map = None;

    use png::BitDepth::*;
    if has_gain_map {
        let Some(cicp) = cicp else {
            bail!("Image must have coding-independent code points but they're absent");
        };

        let transfer = convert_transfer_func(cicp.transfer_function)?;
        let gamut = color_primaries_to_gamut(cicp.color_primaries)?;

        use png::ColorType::*;
        use ultrahdr_core::PixelFormat::*;
        let format = match (color_type, bit_depth) {
            (Rgb, Eight) => Rgb8,
            (Grayscale, Eight) => Gray8,
            (ct, bd) => bail!(
                "Unsupported pixel format for gain map: {ct:?}, {}-bit",
                bd as u8
            ),
        };

        let hdr_image =
            ultrahdr_core::RawImage::from_data(width, height, format, gamut, transfer, data)?;

        let sdr_data = ultrahdr_core::color::tonemap_image_to_srgb8(&hdr_image, gamut)?;
        data = sdr_data.clone();

        let sdr_image =
            ultrahdr_core::RawImage::from_data(width, height, format, gamut, transfer, sdr_data)?;

        (gain_map, gain_map_metadata) = Some(ultrahdr_core::gainmap::compute_gainmap(
            &hdr_image,
            &sdr_image,
            &ultrahdr_core::GainMapConfig {
                scale_factor: 1,
                gamma: 1.0,
                multi_channel: color_type.samples() > 1,
                ..Default::default()
            },
            ultrahdr_core::Unstoppable,
        )?)
        .unzip();
    }

    let image = AlohaImage {
        width,
        height,
        color_type: png_conv::color_type_to_aloha(&color_type),
        bit_depth: match bit_depth {
            Eight => 8,
            Sixteen => 16,
            _ => bail!("Unsupported bit depth: {}", bit_depth as u8),
        },
        color_profile_desc: if aloha_cicp.is_some() {
            ColorProfileDesc::Cicp
        } else if icc_profile.is_some() {
            ColorProfileDesc::Icc
        } else {
            ColorProfileDesc::None
        },
        compression: 1,
        has_gain_map,
        cicp: aloha_cicp,
        icc_profile,
        palette,
        metadata: Metadata::from_source(format!(
            "Converted from {}",
            png_path.to_str().unwrap_or("<NOT FOUND>")
        )),
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

    info.color_type = png_conv::color_type_from_aloha(&image.color_type);
    info.bit_depth = match image.bit_depth {
        8 => png::BitDepth::Eight,
        16 => png::BitDepth::Sixteen,
        other => bail!("Invalid bit depth: {other}"),
    };

    if matches!(image.color_profile_desc, ColorProfileDesc::Cicp) {
        let Some(cicp) = &image.cicp else {
            bail!("Image must have coding-independent code points but they're absent")
        };

        info.coding_independent_code_points = Some(png::CodingIndependentCodePoints {
            color_primaries: cicp.color_primaries,
            transfer_function: cicp.transfer_characteristics,
            matrix_coefficients: cicp.matrix_coefficients,
            is_video_full_range_image: cicp.full_range,
        });
    }

    if matches!(image.color_profile_desc, ColorProfileDesc::Icc) {
        let Some(icc) = &image.icc_profile else {
            bail!("Image must have an ICC profile but it's absent");
        };

        info.icc_profile = Some(std::borrow::Cow::Borrowed(icc));
    }

    if let Some(p) = &image.palette {
        info.palette = Some(std::borrow::Cow::Borrowed(p));
    }

    let hdr_data = if image.has_gain_map {
        let Some(cicp) = &image.cicp else {
            bail!("Image must have coding-independent code points but they're absent");
        };

        let Some(gain_map) = &image.gain_map else {
            bail!("Image must have a gain map but it's absent");
        };

        let Some(metadata) = &image.gain_map_metadata else {
            bail!("Image must have a gain map metadata but it's absent");
        };

        use ultrahdr_core::PixelFormat::*;
        Some(ultrahdr_core::gainmap::apply_gainmap(
            &ultrahdr_core::RawImage::from_data(
                image.width,
                image.height,
                match (image.color_type, image.bit_depth) {
                    (ColorType::Rgb, 8) => Rgb8,
                    (ColorType::Grayscale, 8) => Gray8,
                    (ct, bd) => {
                        bail!("Unsupported pixel format for a HDR image: {ct:?}, {bd}-bit",)
                    }
                },
                color_primaries_to_gamut(cicp.color_primaries)?,
                convert_transfer_func(cicp.transfer_characteristics)?,
                image.data.clone(),
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

    let mut writer = png::Encoder::with_info(
        std::io::BufWriter::new(std::fs::File::create(png_path)?),
        info,
    )?
    .write_header()?;

    if let Some(hdr) = hdr_data {
        writer.write_image_data(&hdr.data)?;
    } else {
        writer.write_image_data(&image.data)?;
    }

    Ok(())
}

pub fn jpeg_to_aloha(jpeg_path: &Path, aloha_path: &Path) -> Result<()> {
    let file_data = std::fs::read(jpeg_path)?;
    if let Ok(decoder) = ultrahdr_rs::Decoder::new(&file_data) {
        if decoder.is_ultrahdr() {
            let sdr = decoder.primary_jpeg().context("Image data is missing")?;
            let mut sdr_dec = jpeg_decoder::Decoder::new(sdr);
            let sdr_data = sdr_dec.decode()?;
            let sdr_info = sdr_dec.info().context("Image info is missing")?;
            let icc_profile = sdr_dec.icc_profile();

            let gain_map = decoder
                .gainmap_jpeg()
                .context("Image must have a gain map but it's absent")?;
            let mut gm_dec = jpeg_decoder::Decoder::new(gain_map);
            let gm_data = gm_dec.decode()?;
            let gm_info = gm_dec
                .info()
                .context("Image must have a gain map info but it's absent")?;

            use jpeg_decoder::PixelFormat::*;
            let gm = ultrahdr_rs::GainMap {
                width: gm_info.width as u32,
                height: gm_info.height as u32,
                channels: match gm_info.pixel_format {
                    L8 | L16 => 1,
                    RGB24 => 3,
                    other => bail!("Unsupported pixel format: {other:?}"),
                },
                data: gm_data,
            };

            let gmmd = decoder
                .metadata()
                .context("Image must have a gain map metadata but it's absent")?
                .to_owned();

            let image = AlohaImage {
                width: sdr_info.width as u32,
                height: sdr_info.height as u32,
                color_type: jpeg_conv::pixel_format_to_aloha(&sdr_info.pixel_format)?,
                bit_depth: jpeg_conv::bit_depth_to_aloha(&sdr_info.pixel_format)?,
                color_profile_desc: if icc_profile.is_some() {
                    ColorProfileDesc::Icc
                } else {
                    ColorProfileDesc::None
                },
                compression: 1,
                has_gain_map: true,
                cicp: None,
                icc_profile,
                palette: None,
                metadata: Metadata::from_source(format!(
                    "Converted from {}",
                    jpeg_path.to_str().unwrap_or("<NOT FOUND>")
                )),
                data: sdr_data,
                gain_map_metadata: Some(gmmd),
                gain_map: Some(gm),
            };

            write_aloha(aloha_path, &image)?;
            return Ok(());
        }
    }

    let mut reader = jpeg_decoder::Decoder::new(std::fs::File::open(jpeg_path)?);
    let data = reader.decode()?;

    // should be after `reader.decode()`
    let Some(info) = reader.info() else {
        bail!("Can't read info");
    };

    let icc_profile = reader.icc_profile();

    let image = AlohaImage {
        width: info.width as u32,
        height: info.height as u32,
        color_type: jpeg_conv::pixel_format_to_aloha(&info.pixel_format)?,
        bit_depth: jpeg_conv::bit_depth_to_aloha(&info.pixel_format)?,
        color_profile_desc: if icc_profile.is_some() {
            ColorProfileDesc::Icc
        } else {
            ColorProfileDesc::None
        },
        compression: 1,
        has_gain_map: false,
        cicp: None,
        icc_profile,
        palette: None,
        metadata: Metadata::from_source(format!(
            "Converted from {}",
            jpeg_path.to_str().unwrap_or("<NOT FOUND>")
        )),
        data,
        gain_map_metadata: None,
        gain_map: None,
    };

    write_aloha(aloha_path, &image)?;
    Ok(())
}

pub fn aloha_to_jpeg(aloha_path: &Path, jpeg_path: &Path) -> Result<()> {
    let image = read_aloha(aloha_path)?;

    if image.has_gain_map {
        let Some(gain_map) = &image.gain_map else {
            bail!("Image must have a gain map but it's absent");
        };

        let Some(gain_map_metadata) = &image.gain_map_metadata else {
            bail!("Image must have a gain map metadata but it's absent");
        };

        let mut sdr_jpeg = Vec::new();
        let mut sdr_enc = jpeg_encoder::Encoder::new(&mut sdr_jpeg, 90);
        if let Some(icc_profile) = &image.icc_profile {
            sdr_enc.add_icc_profile(icc_profile)?;
        }

        sdr_enc.encode(
            &image.data,
            image.width as u16,
            image.height as u16,
            jpeg_conv::color_type_from_aloha(&image.color_type)?,
        )?;

        let mut gm_jpeg = Vec::new();
        let gm_color = match gain_map.channels {
            1 => jpeg_encoder::ColorType::Luma,
            3 => jpeg_encoder::ColorType::Rgb,
            other => bail!("Unsupported channels count: {other}"),
        };

        let gm_enc = jpeg_encoder::Encoder::new(&mut gm_jpeg, 85);
        gm_enc.encode(
            &gain_map.data,
            gain_map.width as u16,
            gain_map.height as u16,
            gm_color,
        )?;

        let uhdr_bytes = ultrahdr_rs::encode_ultrahdr(
            &sdr_jpeg,
            &gm_jpeg,
            gain_map_metadata,
            ultrahdr_core::ColorGamut::default(),
        )?;

        std::fs::write(jpeg_path, uhdr_bytes)?;
        return Ok(());
    }

    let mut enc = jpeg_encoder::Encoder::new_file(jpeg_path, 90)?;

    if matches!(image.color_profile_desc, ColorProfileDesc::Icc) {
        let Some(icc) = image.icc_profile else {
            bail!("Image must have an ICC profile but it's absent");
        };

        enc.add_icc_profile(&icc)?;
    }

    enc.encode(
        &image.data,
        image.width as u16,
        image.height as u16,
        jpeg_conv::color_type_from_aloha(&image.color_type)?,
    )?;

    Ok(())
}
