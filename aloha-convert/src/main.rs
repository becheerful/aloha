use std::path::Path;

use anyhow::bail;

mod convert;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "pack" => {
            let path = Path::new(&args[2]);
            let Some(ext) = path.extension() else {
                bail!("Not a file");
            };

            let Some(ext) = ext.to_str() else {
                bail!("Error converting the extension to &str");
            };

            match ext {
                "png" => convert::png_to_aloha(path, Path::new(&args[3])),
                "jpeg" | "jpg" => convert::jpeg_to_aloha(path, Path::new(&args[3])),
                _ => bail!("Unsupported file format: {ext}"),
            }?
        }

        "unpack" => {
            let path = Path::new(&args[3]);
            let Some(ext) = path.extension() else {
                bail!("Not a file");
            };

            let Some(ext) = ext.to_str() else {
                bail!("Error converting the extension to &str");
            };

            match ext {
                "png" => convert::aloha_to_png(Path::new(&args[2]), path),
                "jpeg" | "jpg" => convert::aloha_to_jpeg(Path::new(&args[2]), path),
                _ => bail!("Unsupported file format: {ext}"),
            }?
        }

        other => bail!("Unknown command: {}", other),
    }

    Ok(())
}
