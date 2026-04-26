#[cfg(feature = "parallel")]
use std::collections::HashSet;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Convert an archive-internal path to a filesystem path below `root`.
///
/// Both Bethesda separators are treated as directory separators. Empty and `.`
/// components are ignored, while `..`, NUL bytes, and Windows drive-ish `:`
/// components are rejected so extraction can not quietly scribble outside the
/// requested output directory. That would be an extraction API lying about what
/// it extracts to, which is impolite even by archive-tool standards.
pub(crate) fn output_path_into(
    out: &mut PathBuf,
    root: &Path,
    archive_path: &[u8],
) -> io::Result<()> {
    out.clear();
    out.push(root);
    let mut pushed = false;
    for component in archive_path.split(|byte| matches!(*byte, b'/' | b'\\')) {
        if component.is_empty() || component == b"." {
            continue;
        }
        if component == b".." || component.contains(&0) || component.contains(&b':') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive path can not be extracted safely",
            ));
        }
        #[cfg(unix)]
        {
            if let Ok(component) = std::str::from_utf8(component) {
                out.push(component);
            } else {
                use std::os::unix::ffi::OsStringExt as _;
                out.push(std::ffi::OsString::from_vec(component.to_vec()));
            }
        }
        #[cfg(not(unix))]
        {
            let component = std::str::from_utf8(component)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            out.push(component);
        }
        pushed = true;
    }
    if !pushed {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "archive entry has no file name",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_parent_dir(path: &Path, last_parent: &mut PathBuf) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if parent != last_parent.as_path() {
            fs::create_dir_all(parent)?;
            last_parent.clear();
            last_parent.push(parent);
        }
    }
    Ok(())
}

#[cfg(feature = "parallel")]
pub(crate) fn ensure_parent_dirs(paths: &[PathBuf]) -> io::Result<()> {
    let mut created_dirs = HashSet::new();
    for path in paths {
        if let Some(parent) = path.parent() {
            if created_dirs.insert(parent) {
                fs::create_dir_all(parent)?;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "parallel")]
pub(crate) fn has_duplicate_paths(paths: &[PathBuf]) -> bool {
    let mut seen = HashSet::new();
    paths.iter().any(|path| !seen.insert(path.as_path()))
}
