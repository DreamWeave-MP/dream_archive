use super::{Error, Result, hash::FileHash, hash::hash_normalized_file};
use crate::bsa::{FilenameEncoding, encode_filename};
use bstr::{BString, ByteSlice as _};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const VERSION: u32 = 0x0000_0100;
const HEADER_SIZE: usize = 12;
const FILE_RECORD_SIZE: usize = 8;
const NAME_OFFSET_SIZE: usize = 4;

/// Builder for TES3/Morrowind BSA archives.
///
/// Entries are stored uncompressed. Paths are normalized to TES3 lookup form
/// (`/` to `\`, ASCII lowercase, no leading/trailing separators) before being
/// written, because that is the name the hash table describes. Preserving a
/// spelling that hashes as something else would be charmingly broken.
#[derive(Clone, Debug, Default)]
pub struct Builder {
    entries: Vec<BuilderEntry>,
}

#[derive(Clone, Debug)]
struct BuilderEntry {
    path: BString,
    hash: FileHash,
    bytes: Vec<u8>,
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add an owned copy of one file payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be represented safely, is a
    /// duplicate after TES3 normalization, or allocation fails.
    pub fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()> {
        let path = normalize_stored_path(path.as_ref())?;
        if self.entries.iter().any(|entry| entry.path == path) {
            return Err(Error::DuplicatePath);
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        let hash = hash_normalized_file(path.as_bstr());
        self.entries.push(BuilderEntry {
            path,
            hash,
            bytes: owned,
        });
        Ok(())
    }

    /// Encode a Unicode archive path with an explicit legacy filename encoding,
    /// then add the payload.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` can not be encoded losslessly, the encoded
    /// path is invalid for a TES3 archive, is a duplicate, or allocation fails.
    pub fn add_encoded_path(
        &mut self,
        path: &str,
        encoding: FilenameEncoding,
        bytes: impl AsRef<[u8]>,
    ) -> Result<()> {
        let encoded = encode_filename(path, encoding)?;
        self.add_bytes(encoded.as_ref(), bytes)
    }

    /// Read a filesystem file and store it at `archive_path`.
    ///
    /// Symlinked files are followed for payload bytes; the archive path is the
    /// path supplied by the caller.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the file fails or adding the archive entry fails.
    pub fn add_file(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
    ) -> Result<()> {
        let bytes = fs::read(source)?;
        self.add_bytes(archive_path, bytes)
    }

    /// Recursively add all files below `root` using paths relative to `root`.
    ///
    /// File symlinks are followed for payload bytes, but stored at the relative
    /// path where the symlink was found.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, file reading, or adding an entry fails.
    pub fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()> {
        let root = root.as_ref();
        for path in collect_files(root)? {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::InvalidArchivePath)?;
            let archive_path = path_to_archive_bytes(relative)?;
            self.add_file(archive_path, &path)?;
        }
        Ok(())
    }

    /// Write the archive to a filesystem path.
    ///
    /// # Errors
    ///
    /// Returns an error if creating/writing the file fails or archive integer
    /// fields overflow their TES3 on-disk sizes.
    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        self.write_to(BufWriter::new(file))
    }

    /// Write the archive to a byte vector.
    ///
    /// # Errors
    ///
    /// Returns an error if archive integer fields overflow their TES3 on-disk
    /// sizes or output allocation fails.
    pub fn into_vec(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.try_reserve_exact(self.archive_size_hint()?)?;
        self.write_to(&mut out)?;
        Ok(out)
    }

    /// Write the archive to `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or archive integer fields overflow
    /// their TES3 on-disk sizes.
    pub fn write_to(&self, mut out: impl Write) -> Result<()> {
        let entries = self.sorted_entries();
        let file_count: u32 = entries.len().try_into()?;
        let names_len = names_len(&entries)?;
        let hash_offset = FILE_RECORD_SIZE
            .checked_mul(entries.len())
            .and_then(|size| size.checked_add(NAME_OFFSET_SIZE * entries.len()))
            .and_then(|size| size.checked_add(names_len))
            .ok_or(Error::OutOfBounds)?;

        write_u32(&mut out, VERSION)?;
        write_u32(&mut out, hash_offset.try_into()?)?;
        write_u32(&mut out, file_count)?;

        let mut data_offset = 0u32;
        for entry in &entries {
            write_u32(&mut out, entry.bytes.len().try_into()?)?;
            write_u32(&mut out, data_offset)?;
            data_offset = data_offset
                .checked_add(entry.bytes.len().try_into()?)
                .ok_or(Error::OutOfBounds)?;
        }

        let mut name_offset = 0u32;
        for entry in &entries {
            write_u32(&mut out, name_offset)?;
            name_offset = name_offset
                .checked_add((entry.path.len() + 1).try_into()?)
                .ok_or(Error::OutOfBounds)?;
        }

        for entry in &entries {
            out.write_all(&entry.path)?;
            out.write_all(&[0])?;
        }
        for entry in &entries {
            write_u32(&mut out, entry.hash.lo)?;
            write_u32(&mut out, entry.hash.hi)?;
        }
        for entry in &entries {
            out.write_all(&entry.bytes)?;
        }
        Ok(())
    }

    fn sorted_entries(&self) -> Vec<&BuilderEntry> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|left, right| {
            left.hash
                .numeric()
                .cmp(&right.hash.numeric())
                .then_with(|| left.path.cmp(&right.path))
        });
        entries
    }

    fn archive_size_hint(&self) -> Result<usize> {
        let entries = self.sorted_entries();
        let names_len = names_len(&entries)?;
        HEADER_SIZE
            .checked_add(FILE_RECORD_SIZE * entries.len())
            .and_then(|size| size.checked_add(NAME_OFFSET_SIZE * entries.len()))
            .and_then(|size| size.checked_add(names_len))
            .and_then(|size| size.checked_add(8 * entries.len()))
            .and_then(|size| {
                entries
                    .iter()
                    .try_fold(size, |sum, entry| sum.checked_add(entry.bytes.len()))
            })
            .ok_or(Error::OutOfBounds)
    }
}

fn names_len(entries: &[&BuilderEntry]) -> Result<usize> {
    entries.iter().try_fold(0usize, |sum, entry| {
        sum.checked_add(entry.path.len() + 1)
            .ok_or(Error::OutOfBounds)
    })
}

fn normalize_stored_path(path: &[u8]) -> Result<BString> {
    if path.is_empty() {
        return Err(Error::InvalidArchivePath);
    }
    let mut normalized = Vec::new();
    normalized.try_reserve_exact(path.len())?;
    let mut pushed = false;
    for component in path.split(|byte| matches!(*byte, b'/' | b'\\')) {
        if component.is_empty() || component == b"." {
            continue;
        }
        if component == b".." || component.contains(&0) || component.contains(&b':') {
            return Err(Error::InvalidArchivePath);
        }
        if pushed {
            normalized.push(b'\\');
        }
        normalized.extend(component.iter().copied().map(|byte| match byte {
            b'A'..=b'Z' => byte + 32,
            _ => byte,
        }));
        pushed = true;
    }
    if !pushed {
        return Err(Error::InvalidArchivePath);
    }
    Ok(BString::from(normalized))
}

fn write_u32(out: &mut impl Write, value: u32) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
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

fn path_to_archive_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for component in path.components() {
        if !out.is_empty() {
            out.push(b'/');
        }
        let std::path::Component::Normal(part) = component else {
            return Err(Error::InvalidArchivePath);
        };
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            out.extend_from_slice(part.as_bytes());
        }
        #[cfg(not(unix))]
        {
            out.extend_from_slice(part.to_str().ok_or(Error::InvalidArchivePath)?.as_bytes());
        }
    }
    Ok(out)
}
