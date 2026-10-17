use std::path::Path;

use anyhow::{Ok, Result, bail};

use crate::aloha::{AlohaImage, ColorType, Metadata, read_aloha};

pub fn png_to_aloha(png_path: &Path, aloha_path: &Path) -> Result<()> {
    let mut reader = png::Decoder::new(std::fs::File::open(png_path)?).read_info()?;
    let (width, height, color_type, palette, bit_depth) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.palette.clone().map(Vec::from),
            info.bit_depth,
        )
    };

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)?;

    let image = AlohaImage {
        width,
        height,
        color_type: ColorType::from(&color_type),
        bit_depth: crate::aloha::from_png_bit_depth(&bit_depth)?,
        compression: 1,
        palette,
        metadata: crate::aloha::Metadata {
            author: None,
            source: Some(format!(
                "Converted from {}",
                png_path.to_str().unwrap_or("<NOT FOUND>")
            )),
        },
        data: buf[..frame.buffer_size()].to_vec(),
    };

    image.validate()?;
    crate::aloha::write_aloha(aloha_path, &image)?;
    Ok(())
}

pub fn aloha_to_png(aloha_path: &Path, png_path: &Path) -> Result<()> {
    let image = read_aloha(aloha_path)?;
    image.validate()?;

    let color_type = png::ColorType::from(&image.color_type);
    let bit_depth = crate::aloha::to_png_bit_depth(image.bit_depth)?;

    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(std::fs::File::create(png_path)?),
        image.width,
        image.height,
    );

    if let Some(p) = image.palette {
        enc.set_palette(p);
    }

    enc.set_color(color_type);
    enc.set_depth(bit_depth);

    let mut writer = enc.write_header()?;
    writer.write_image_data(&image.data)?;

    Ok(())
}

pub fn jpeg_to_aloha(jpeg_path: &Path, aloha_path: &Path) -> Result<()> {
    let mut reader = jpeg_decoder::Decoder::new(std::fs::File::open(jpeg_path)?);
    let data = reader.decode()?;

    // should be after reader.decode()
    let Some(info) = reader.info() else {
        bail!("Can't read info");
    };

    let image = AlohaImage {
        width: info.width as u32,
        height: info.height as u32,
        color_type: ColorType::try_from(&info.pixel_format)?,
        bit_depth: crate::aloha::from_jpeg_bit_depth(&info.pixel_format),
        compression: 1,
        palette: None,
        metadata: Metadata {
            author: None,
            source: Some(format!(
                "Converted from {}",
                jpeg_path.to_str().unwrap_or("<NOT FOUND>")
            )),
        },
        data,
    };

    image.validate()?;
    crate::aloha::write_aloha(aloha_path, &image)?;
    Ok(())
}

pub fn aloha_to_jpeg(aloha_path: &Path, jpeg_path: &Path) -> Result<()> {
    let image = read_aloha(aloha_path)?;
    image.validate()?;

    let enc = jpeg_encoder::Encoder::new_file(jpeg_path, 100)?;
    enc.encode(
        &image.data,
        image.width as u16,
        image.height as u16,
        jpeg_encoder::ColorType::Rgb,
    )?;

    Ok(())
}
