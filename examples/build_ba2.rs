use dream_archive::{Ba2Builder, ba2::Ba2CompressionFormat};

fn main() -> dream_archive::ba2::Result<()> {
    let Some(output_path) = std::env::args_os().nth(1) else {
        eprintln!("usage: build_ba2 <output.ba2>");
        std::process::exit(2);
    };

    let mut builder = Ba2Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder.add_bytes("meshes/example.txt", b"hello from BA2\n")?;
    builder.write_path(output_path)?;
    Ok(())
}
