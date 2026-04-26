use dream_archive::{Tes4BsaBuilder, bsa::tes4::ArchiveTypes};

fn main() -> dream_archive::bsa::Result<()> {
    let Some(output_path) = std::env::args_os().nth(1) else {
        eprintln!("usage: build_tes4_bsa <output.bsa>");
        std::process::exit(2);
    };

    let mut builder = Tes4BsaBuilder::skyrim_le();
    builder.set_archive_types(ArchiveTypes::MISC);
    builder.set_compressed(true);
    builder.add_bytes("meshes/example.txt", b"hello from TES4 BSA\n")?;
    builder.write_path(output_path)?;
    Ok(())
}
