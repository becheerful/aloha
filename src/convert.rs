use std::path::Path;

pub fn png_to_aloha(png_path: &Path, aloha_path: &Path) -> anyhow::Result<()> {
    let mut reader = png::Decoder::new(std::fs::File::open(png_path)?).read_info()?;
    let (width, height, _srgb, color_type, palette, bit_depth) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.srgb,
            info.color_type,
            info.palette.clone().map(Vec::from),
            info.bit_depth,
        )
    };

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf)?;
    let data = buf[..frame.buffer_size()].to_vec();

    if matches!(color_type, png::ColorType::Indexed) && palette.is_none() {
        anyhow::bail!("The pallete for an indexed PNG not found");
    }

    let color_type = crate::aloha::ColorType::from(&color_type);
    let bit_depth = crate::aloha::BitDepth::try_from(&bit_depth)?;
    let metadata = crate::aloha::Metadata {
        author: None,
        source: Some(format!(
            "Converted from {}",
            png_path.to_str().unwrap_or("<NOT FOUND>")
        )),
    };

    let imag = crate::aloha::AlohaImage {
        width,
        height,
        color_type,
        bit_depth,
        palette,
        compression: 1,
        metadata,
        data,
    };

    imag.validate()?;
    crate::aloha::write_aloha(aloha_path, &imag)?;
    anyhow::Ok(())
}

pub fn aloha_to_png(aloha_path: &Path, png_path: &Path) -> anyhow::Result<()> {
    let image = crate::aloha::read_aloha(aloha_path)?;
    image.validate()?;

    let color_type = png::ColorType::from(&image.color_type);
    let bit_depth = png::BitDepth::from(&image.bit_depth);

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

    anyhow::Ok(())
}
