#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
use std::borrow::Cow;
#[cfg(feature = "parallel")]
use std::collections::HashSet;
use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

/// Convert an archive-internal path to a filesystem path by first decoding each
/// archive component through an explicit filename encoding.
#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
pub(crate) fn output_path_decoded_into<'a>(
    out: &mut PathBuf,
    root: &Path,
    archive_path: &'a [u8],
    mut decode_component: impl FnMut(&'a [u8]) -> Cow<'a, str>,
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
        out.push(decode_component(component).as_ref());
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

pub(crate) fn write_file_atomically<E>(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> std::result::Result<u64, E>,
) -> std::result::Result<u64, E>
where
    E: From<io::Error>,
{
    let (temp_path, mut file) = create_temp_file(path)?;

    let result = write(&mut file).and_then(|written| {
        file.flush()?;
        Ok(written)
    });
    drop(file);

    match result {
        Ok(written) => {
            if let Err(error) = fs::rename(&temp_path, path) {
                let _ = fs::remove_file(&temp_path);
                return Err(error.into());
            }
            Ok(written)
        }
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            Err(error)
        }
    }
}

fn create_temp_file(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    for _ in 0..100 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(file_name);
        temp_name.push(format!(
            ".dream-archive-tmp-{}-{counter}",
            std::process::id()
        ));
        let temp_path = parent.join(temp_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate temporary extraction path",
    ))
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
    let mut exact_paths = HashSet::new();
    let mut collision_keys = HashSet::new();
    paths.iter().any(|path| {
        !exact_paths.insert(path.as_path())
            || !collision_keys.insert(extraction_collision_key(path))
    })
}

#[cfg(feature = "parallel")]
fn extraction_collision_key(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        path.as_os_str()
            .as_bytes()
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy()
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .collect()
    }
}

#[cfg(all(test, feature = "parallel"))]
mod tests {
    use super::has_duplicate_paths;
    use std::path::PathBuf;

    #[test]
    fn duplicate_detection_catches_exact_paths() {
        assert!(has_duplicate_paths(&[
            PathBuf::from("out/Data/Foo.txt"),
            PathBuf::from("out/Data/Foo.txt"),
        ]));
    }

    #[test]
    fn duplicate_detection_catches_ascii_case_conflicts() {
        assert!(has_duplicate_paths(&[
            PathBuf::from("out/Data/Foo.txt"),
            PathBuf::from("out/data/foo.txt"),
        ]));
    }
}
