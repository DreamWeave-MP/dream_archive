use dream_archive::Archive;
use std::ffi::OsString;

fn archive_path_bytes(path: OsString) -> Vec<u8> {
    if let Ok(path) = path.into_string() {
        path.into_bytes()
    } else {
        eprintln!("<member-path> must be valid UTF-8 in this example");
        std::process::exit(2);
    }
}

fn main() -> dream_archive::Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let (Some(archive_path), Some(member_path), Some(output_path)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: extract_archive <archive> <member-path> <output-path>");
        std::process::exit(2);
    };

    eprintln!("note: this demo treats <member-path> as exact UTF-8 archive bytes");
    let member_path = archive_path_bytes(member_path);
    let archive = Archive::open_path(archive_path)?;
    let output = std::fs::File::create(output_path)?;
    archive.extract_file_required(&member_path, output)?;
    Ok(())
}
