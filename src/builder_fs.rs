use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) fn collect_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir)? {
            entries.push(entry?.path());
        }
        entries.sort();
        for path in entries {
            let symlink_metadata = fs::symlink_metadata(&path)?;
            if symlink_metadata.file_type().is_symlink() {
                if fs::metadata(&path)?.is_file() {
                    files.push(path);
                }
            } else if symlink_metadata.is_dir() {
                pending.push(path);
            } else if symlink_metadata.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn path_to_archive_bytes(path: &Path) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for component in path.components() {
        if !out.is_empty() {
            out.push(b'/');
        }
        let std::path::Component::Normal(part) = component else {
            return None;
        };
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            out.extend_from_slice(part.as_bytes());
        }
        #[cfg(not(unix))]
        {
            out.extend_from_slice(part.to_str()?.as_bytes());
        }
    }
    Some(out)
}
