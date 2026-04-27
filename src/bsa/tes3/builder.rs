use super::{Error, Result, hash::FileHash, hash::hash_normalized_file};
use crate::{
    bsa::{FilenameEncoding, encode_filename},
    builder_fs,
};
use bstr::{BString, ByteSlice as _};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, BufWriter, Cursor, Seek, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const VERSION: u32 = 0x0000_0100;
const HEADER_SIZE: usize = 12;
const FILE_RECORD_SIZE: usize = 8;
const NAME_OFFSET_SIZE: usize = 4;

/// Builder for TES3/Morrowind BSA archives.
///
/// File-backed entries are deferred: [`Self::add_file`] records the source path
/// and size, but payload bytes are not read until the archive is written.
#[derive(Clone, Debug, Default)]
pub struct Builder {
    entries: Vec<BuilderEntry>,
    paths: HashSet<BString>,
}

#[derive(Clone, Debug)]
struct BuilderEntry {
    path: BString,
    hash: FileHash,
    source: EntrySource,
}

#[derive(Clone, Debug)]
enum EntrySource {
    Bytes(Vec<u8>),
    File {
        path: PathBuf,
        len: u64,
    },
    ArchiveEntry {
        archive: Arc<super::Archive>,
        id: super::EntryId,
        len: u64,
    },
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
    /// Returns an error if the path is invalid, duplicated, or allocation fails.
    pub fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()> {
        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        self.add_source(path, EntrySource::Bytes(owned))
    }

    /// Encode a Unicode archive path with an explicit legacy filename encoding,
    /// then add the payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be encoded, is invalid, duplicated, or allocation fails.
    pub fn add_encoded_path(
        &mut self,
        path: &str,
        encoding: FilenameEncoding,
        bytes: impl AsRef<[u8]>,
    ) -> Result<()> {
        let encoded = encode_filename(path, encoding)?;
        self.add_bytes(encoded.as_ref(), bytes)
    }

    /// Record a filesystem file and store it at `archive_path` when written.
    ///
    /// Symlinked files are followed for size and later payload bytes. If the
    /// source changes size before writing, writing fails instead of producing a
    /// table that lies about payload length.
    ///
    /// # Errors
    ///
    /// Returns an error if source metadata lookup fails or the archive path is invalid or duplicated.
    pub fn add_file(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
    ) -> Result<()> {
        let source = source.as_ref();
        let len = fs::metadata(source)?.len();
        self.add_source(
            archive_path,
            EntrySource::File {
                path: source.to_path_buf(),
                len,
            },
        )
    }

    /// Preserve an entry from another TES3 archive without materializing it in
    /// the builder.
    ///
    /// # Errors
    ///
    /// Returns an error if the archive path is invalid, duplicated, or the source id is invalid.
    pub fn add_archive_entry(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        archive: Arc<super::Archive>,
        id: super::EntryId,
    ) -> Result<()> {
        let len = u64::from(archive.entry_by_id_required(id)?.file().size);
        self.add_source(archive_path, EntrySource::ArchiveEntry { archive, id, len })
    }

    /// Recursively add all files below `root` using paths relative to `root`.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, source metadata lookup, or adding an entry fails.
    pub fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()> {
        let root = root.as_ref();
        for path in builder_fs::collect_files(root)? {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::InvalidArchivePath)?;
            let archive_path =
                builder_fs::path_to_archive_bytes(relative).ok_or(Error::InvalidArchivePath)?;
            self.add_file(archive_path, &path)?;
        }
        Ok(())
    }

    /// Write the archive to a filesystem path.
    ///
    /// # Errors
    ///
    /// Returns an error if creating/writing the file fails, archive metadata overflows, or a deferred source can not be read.
    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        self.write_seek(BufWriter::new(file))
    }

    /// Write the archive to a byte vector.
    ///
    /// # Errors
    ///
    /// Returns an error if archive metadata overflows, output allocation fails, or a deferred source can not be read.
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        let mut out = Cursor::new(Vec::new());
        out.get_mut().try_reserve_exact(self.archive_size_hint()?)?;
        self.write_seek(&mut out)?;
        Ok(out.into_inner())
    }

    /// Write the archive to `out` using deferred payload sources.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails, archive metadata overflows, or a deferred source can not be read.
    pub fn write_seek<W: Write + Seek>(&self, mut out: W) -> Result<()> {
        self.write_streaming(&mut out)
    }

    fn add_source(&mut self, path: impl AsRef<[u8]>, source: EntrySource) -> Result<()> {
        let path = normalize_stored_path(path.as_ref())?;
        if self.paths.contains(path.as_bstr()) {
            return Err(Error::DuplicatePath);
        }
        let hash = hash_normalized_file(path.as_bstr());
        self.entries.try_reserve(1)?;
        self.paths.try_reserve(1)?;
        self.paths.insert(path.clone());
        self.entries.push(BuilderEntry { path, hash, source });
        Ok(())
    }

    fn write_streaming(&self, out: &mut impl Write) -> Result<()> {
        let entries = self.sorted_entries();
        let file_count: u32 = entries.len().try_into()?;
        let names_len = names_len(&entries)?;
        let hash_offset = FILE_RECORD_SIZE
            .checked_mul(entries.len())
            .and_then(|size| size.checked_add(NAME_OFFSET_SIZE * entries.len()))
            .and_then(|size| size.checked_add(names_len))
            .ok_or(Error::OutOfBounds)?;

        write_u32(out, VERSION)?;
        write_u32(out, hash_offset.try_into()?)?;
        write_u32(out, file_count)?;

        let mut data_offset = 0u32;
        for entry in &entries {
            let len: u32 = entry.source.len().try_into()?;
            write_u32(out, len)?;
            write_u32(out, data_offset)?;
            data_offset = data_offset.checked_add(len).ok_or(Error::OutOfBounds)?;
        }

        let mut name_offset = 0u32;
        for entry in &entries {
            write_u32(out, name_offset)?;
            name_offset = name_offset
                .checked_add((entry.path.len() + 1).try_into()?)
                .ok_or(Error::OutOfBounds)?;
        }

        for entry in &entries {
            out.write_all(&entry.path)?;
            out.write_all(&[0])?;
        }
        for entry in &entries {
            write_u32(out, entry.hash.lo)?;
            write_u32(out, entry.hash.hi)?;
        }
        for entry in &entries {
            entry.source.copy_to(out)?;
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
                entries.iter().try_fold(size, |sum, entry| {
                    usize::try_from(entry.source.len())
                        .ok()
                        .and_then(|len| sum.checked_add(len))
                })
            })
            .ok_or(Error::OutOfBounds)
    }
}

impl EntrySource {
    fn len(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::File { len, .. } | Self::ArchiveEntry { len, .. } => *len,
        }
    }

    fn copy_to(&self, out: &mut impl Write) -> Result<u64> {
        match self {
            Self::Bytes(bytes) => {
                out.write_all(bytes)?;
                Ok(bytes.len().try_into()?)
            }
            Self::File { path, len } => {
                let mut file = File::open(path)?;
                let copied = io::copy(&mut file, out)?;
                if copied == *len {
                    Ok(copied)
                } else {
                    Err(Error::Io(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "deferred source file size changed before archive write",
                    )))
                }
            }
            Self::ArchiveEntry { archive, id, len } => {
                let copied = archive.extract_entry_by_id(*id, out)?;
                if copied == *len {
                    Ok(copied)
                } else {
                    Err(Error::OutOfBounds)
                }
            }
        }
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
