use super::{Error, Result};
use flate2::read::ZlibDecoder;
use std::io::Read as _;

/// BA2 compression method.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Ba2CompressionFormat {
    #[default]
    Zip,
    LZ4,
}

/// One data chunk in a BA2 file record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    pub(crate) offset: u64,
    pub(crate) packed_size: u32,
    pub(crate) size: u32,
    /// Inclusive mip range for texture archives.
    pub mips: Option<std::ops::RangeInclusive<u16>>,
}

impl Chunk {
    pub(crate) fn new(
        offset: u64,
        packed_size: u32,
        size: u32,
        mips: Option<std::ops::RangeInclusive<u16>>,
    ) -> Self {
        Self {
            offset,
            packed_size,
            size,
            mips,
        }
    }

    /// Decompressed size declared by the archive.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Byte offset of this chunk's stored payload in the archive.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Compressed byte count stored in the archive. Zero means uncompressed.
    #[must_use]
    pub fn packed_size(&self) -> u32 {
        self.packed_size
    }

    /// Stored size. Zero packed size means the chunk is stored uncompressed.
    #[must_use]
    pub fn stored_size(&self) -> u32 {
        if self.packed_size == 0 {
            self.size
        } else {
            self.packed_size
        }
    }

    /// Whether this chunk is compressed on disk.
    #[must_use]
    pub fn is_compressed(&self) -> bool {
        self.packed_size != 0
    }

    pub(crate) fn stored_bytes<'a>(&self, archive: &'a [u8]) -> Result<&'a [u8]> {
        let start: usize = self.offset.try_into()?;
        let len: usize = self.stored_size().try_into()?;
        let end = start.checked_add(len).ok_or(Error::OutOfBounds)?;
        archive.get(start..end).ok_or(Error::OutOfBounds)
    }

    pub(crate) fn extract(
        &self,
        archive: &[u8],
        compression: Ba2CompressionFormat,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let stored = self.stored_bytes(archive)?;
        if self.packed_size == 0 {
            out.try_reserve_exact(stored.len())?;
            out.extend_from_slice(stored);
            return Ok(());
        }

        let expected: usize = self.size.try_into()?;
        let before = out.len();
        match compression {
            Ba2CompressionFormat::Zip => {
                let mut decoder = ZlibDecoder::new(stored);
                decoder.read_to_end(out).map_err(|e| {
                    out.truncate(before);
                    Error::Zlib(e.to_string())
                })?;
                if decoder.total_in() != u64::try_from(stored.len())? {
                    out.truncate(before);
                    return Err(Error::TrailingCompressedData);
                }
            }
            Ba2CompressionFormat::LZ4 => {
                out.try_reserve_exact(expected)?;
                out.resize(before + expected, 0);
                let actual = match lz4_flex::block::decompress_into(stored, &mut out[before..]) {
                    Ok(actual) => actual,
                    Err(error) => {
                        out.truncate(before);
                        return Err(Error::Lz4(error.to_string()));
                    }
                };
                out.truncate(before + actual);
            }
        }

        let actual = out.len() - before;
        if actual == expected {
            Ok(())
        } else {
            out.truncate(before);
            Err(Error::DecompressionSizeMismatch { expected, actual })
        }
    }
}
