use dream_archive::ba2::Archive;

fn main() -> dream_archive::ba2::Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let (Some(archive_path), Some(member_path), Some(output_path)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: extract_ba2 <archive.ba2> <member-path> <output-path>");
        std::process::exit(2);
    };

    let archive = Archive::open_path(archive_path)?;
    let Some(bytes) = archive.read_file(member_path.to_string_lossy().as_bytes())? else {
        eprintln!(
            "archive member not found: {}",
            member_path.to_string_lossy()
        );
        std::process::exit(1);
    };
    std::fs::write(output_path, bytes)?;
    Ok(())
}
