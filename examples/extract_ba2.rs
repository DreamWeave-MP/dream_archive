use bstr::ByteSlice as _;
use dream_archive::ba2::Archive;
use std::ffi::OsString;

fn archive_path_bytes(path: OsString) -> Vec<u8> {
    if let Ok(path) = path.into_string() {
        path.into_bytes()
    } else {
        eprintln!("<member-path> must be valid UTF-8 in this example");
        std::process::exit(2);
    }
}

fn main() -> dream_archive::ba2::Result<()> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let (Some(archive_path), Some(member_path), Some(output_path)) =
        (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: extract_ba2 <archive.ba2> <member-path> <output-path>");
        std::process::exit(2);
    };

    eprintln!(
        "note: this example only supports UTF-8 member names; legacy BSA paths need explicit encoding"
    );
    let member_path = archive_path_bytes(member_path);
    let archive = Archive::open_path(archive_path)?;
    let Some(bytes) = archive.read_file(&member_path)? else {
        eprintln!("archive member not found: {}", member_path.as_bstr());
        std::process::exit(1);
    };
    std::fs::write(output_path, bytes)?;
    Ok(())
}
