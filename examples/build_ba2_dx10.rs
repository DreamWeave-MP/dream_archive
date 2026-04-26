use dream_archive::{Ba2Dx10Builder, ba2::Ba2CompressionFormat};

fn main() -> dream_archive::ba2::Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let (Some(source_dds), Some(output_ba2)) = (args.next(), args.next()) else {
        eprintln!("usage: build_ba2_dx10 <source.dds> <output.ba2>");
        eprintln!("stores the DDS at archive path textures/example.dds");
        std::process::exit(2);
    };

    let mut builder = Ba2Dx10Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder.add_dds_file("textures/example.dds", source_dds)?;
    builder.write_path(output_ba2)
}
