fn to8(v: u16, bps: usize) -> u8 {
    if bps == 1 { v as u8 } else { (v / 256) as u8 }
}

pub fn aloha_to_rgba8(image: &aloha_core::AlohaImage) -> Result<Vec<u8>, String> {
    let w = image.width as usize;
    let h = image.height as usize;
    let pixels = w * h;
    let bps = image.bit_depth as usize / 8;

    let mut out = Vec::with_capacity(pixels * 4); // 4 because of four samples in RGBA

    if image.color_type.is_indexed() {
        let Some(palette) = &image.palette else {
            return Err("Indexed image without palette".to_owned());
        };

        let has_alpha = image.color_type.has_alpha();
        let tp = image.transparency_palette.as_ref();

        for i in 0..pixels {
            let index = image.data[i] as usize;
            let off = index * 3;

            if off + 3 > palette.len() {
                return Err(format!("Palette index out of bounds: {index}"));
            }

            if has_alpha {
                // `.unwrap()` is safe here if image validation passed
                if index > tp.unwrap().len() {
                    return Err(format!("Transparency palette index out of bounds: {index}"));
                }
            }

            out.extend_from_slice(&[
                palette[off],
                palette[off + 1],
                palette[off + 2],
                // `.unwrap()` is safe here if image validation passed
                if has_alpha { tp.unwrap()[off + 3] } else { 255 },
            ]);
        }

        return Ok(out);
    }

    let channels = image.color_type.channels();

    for i in 0..pixels {
        let mut samples = [0u16; 4];
        for c in 0..channels {
            let s = i * channels + c;
            samples[c] = if bps == 1 {
                image.data[s] as u16
            } else {
                let off = s * 2;
                u16::from_be_bytes([image.data[off], image.data[off + 1]])
            };
        }

        use aloha_core::color::ColorType::*;
        match image.color_type {
            Grayscale => {
                let g = to8(samples[0], bps);
                out.extend_from_slice(&[g, g, g, 255]);
            }

            GrayscaleAlpha => {
                let g = to8(samples[0], bps);
                out.extend_from_slice(&[g, g, g, to8(samples[1], bps)]);
            }

            Rgb => {
                out.extend_from_slice(&[
                    to8(samples[0], bps),
                    to8(samples[1], bps),
                    to8(samples[2], bps),
                    255,
                ]);
            }

            Rgba => {
                out.extend_from_slice(&[
                    to8(samples[0], bps),
                    to8(samples[1], bps),
                    to8(samples[2], bps),
                    to8(samples[3], bps),
                ]);
            }

            Indexed | IndexedAlpha => unreachable!(),
        }
    }

    Ok(out)
}
