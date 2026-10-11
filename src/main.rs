use std::path::Path;

mod aloha;
mod convert;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "pack" => {
            convert::png_to_aloha(Path::new(&args[2]), Path::new(&args[3]))?;
        }

        "unpack" => {
            convert::aloha_to_png(Path::new(&args[2]), Path::new(&args[3]))?;
        }

        other => anyhow::bail!("Unknown command: {}", other),
    }

    Ok(())
}
