use dream_archive::{Ba2Dx10Builder, ba2::Ba2CompressionFormat};
use std::ffi::OsString;

fn archive_path_bytes(path: OsString) -> Vec<u8> {
    if let Ok(path) = path.into_string() {
        path.into_bytes()
    } else {
        eprintln!("<archive-path.dds> must be valid UTF-8 in this example");
        std::process::exit(2);
    }
}

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
    let archive_path = archive_path_bytes(archive_path);
    builder.add_dds_file(&archive_path, source_dds)?;
    builder.write_path(output_ba2)
}
