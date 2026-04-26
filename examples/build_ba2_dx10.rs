use dream_archive::{Ba2Dx10Builder, ba2::Ba2CompressionFormat};

fn main() -> dream_archive::ba2::Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let (Some(source_dds), Some(archive_path), Some(output_ba2)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: build_ba2_dx10 <source.dds> <archive-path.dds> <output.ba2>");
        eprintln!("example: build_ba2_dx10 foo.dds textures/foo.dds Textures.ba2");
        std::process::exit(2);
    };

    let mut builder = Ba2Dx10Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder.add_dds_file(archive_path.to_string_lossy().as_bytes(), source_dds)?;
    builder.write_path(output_ba2)
}
