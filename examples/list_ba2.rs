use dream_archive::ba2::Archive;

fn main() -> dream_archive::ba2::Result<()> {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: list_ba2 <archive.ba2>");
        std::process::exit(2);
    };

    let archive = Archive::open_path(path)?;
    for entry in archive.entries() {
        println!("{}", entry.name());
    }
    Ok(())
}
