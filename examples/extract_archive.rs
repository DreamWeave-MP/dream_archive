use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let (Some(archive_path), Some(member_path), Some(output_path)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: extract_archive <archive> <member-path> <output-path>");
        std::process::exit(2);
    };

    eprintln!("note: this demo interprets <member-path> as UTF-8/ASCII archive bytes");
    let archive = Archive::open_path(archive_path)?;
    let output = std::fs::File::create(output_path)?;
    archive.extract_file_required(member_path.to_string_lossy().as_bytes(), output)?;
    Ok(())
}
