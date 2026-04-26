use dream_archive::{Archive, BStr};

fn main() -> dream_archive::Result<()> {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: list_archive <archive>");
        std::process::exit(2);
    };

    let archive = Archive::open_path(path)?;
    println!("format: {:?}", archive.format());
    for entry in archive.entries() {
        println!(
            "{}",
            entry.path().unwrap_or_else(|| BStr::new(b"<hash-only>"))
        );
    }
    Ok(())
}
