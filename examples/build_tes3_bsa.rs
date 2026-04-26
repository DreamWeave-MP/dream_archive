use dream_archive::Tes3BsaBuilder;

fn main() -> dream_archive::bsa::Result<()> {
    let Some(output_path) = std::env::args_os().nth(1) else {
        eprintln!("usage: build_tes3_bsa <output.bsa>");
        std::process::exit(2);
    };

    let mut builder = Tes3BsaBuilder::new();
    builder.add_bytes("meshes/example.txt", b"hello from TES3 BSA\n")?;
    builder.write_path(output_path)?;
    Ok(())
}
