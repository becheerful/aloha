use xmpkit::XmpValue;

pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
}

pub fn xmp_value_to_string(value: &XmpValue) -> String {
    match value {
        XmpValue::String(s) => s.clone(),
        XmpValue::Integer(i) => i.to_string(),
        XmpValue::Boolean(b) => b.to_string(),
        XmpValue::DateTime(dt) => dt.clone(),
        XmpValue::Array(xmp_values) => {
            let items: Vec<String> = xmp_values.iter().map(xmp_value_to_string).collect();
            format!("{}", items.join(", "))
        }
        XmpValue::Structure(hash_map) => {
            let items: Vec<String> = hash_map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}: {}",
                        k.rsplit(':').next().unwrap_or(k),
                        xmp_value_to_string(v)
                    )
                })
                .collect();
            format!("{}", items.join("\n"))
        }
    }
}

pub fn aloha_to_rgba8(image: &mut aloha_core::AlohaImage) -> Result<Vec<u8>, String> {
    image.reduce_bit_depth();

    let w = image.width as usize;
    let h = image.height as usize;
    let pixels = w * h;

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

    for i in 0..pixels {
        use aloha_core::color::ColorType::*;
        match image.color_type {
            Grayscale => {
                let g = image.data[i];
                out.extend_from_slice(&[g, g, g, 255]);
            }

            GrayscaleAlpha => {
                let g = image.data[i * 2];
                out.extend_from_slice(&[g, g, g, image.data[i * 2 + 1]]);
            }

            Rgb => {
                out.extend_from_slice(&[
                    image.data[i * 3],
                    image.data[i * 3 + 1],
                    image.data[i * 3 + 2],
                    255,
                ]);
            }

            Rgba => {
                out.extend_from_slice(&[
                    image.data[i * 4],
                    image.data[i * 4 + 1],
                    image.data[i * 4 + 2],
                    image.data[i * 4 + 3],
                ]);
            }

            Indexed | IndexedAlpha => unreachable!(),
        }
    }

    Ok(out)
}
