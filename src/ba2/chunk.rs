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
                read_decompressed(&mut decoder, expected, out, |error| {
                    Error::Zlib(error.to_string())
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

    pub(crate) fn extract_to_writer(
        &self,
        archive: &[u8],
        compression: Ba2CompressionFormat,
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        let stored = self.stored_bytes(archive)?;
        if self.packed_size == 0 {
            out.write_all(stored)?;
            return Ok(stored.len().try_into()?);
        }

        let expected: usize = self.size.try_into()?;
        match compression {
            Ba2CompressionFormat::Zip => {
                let mut decoder = ZlibDecoder::new(stored);
                let mut buffer = Vec::new();
                read_decompressed(&mut decoder, expected, &mut buffer, |error| {
                    Error::Zlib(error.to_string())
                })?;
                if decoder.total_in() == u64::try_from(stored.len())? {
                    out.write_all(&buffer)?;
                    Ok(buffer.len().try_into()?)
                } else {
                    Err(Error::TrailingCompressedData)
                }
            }
            Ba2CompressionFormat::LZ4 => {
                let mut buffer = Vec::new();
                buffer.try_reserve_exact(expected)?;
                buffer.resize(expected, 0);
                let actual = lz4_flex::block::decompress_into(stored, &mut buffer)
                    .map_err(|error| Error::Lz4(error.to_string()))?;
                if actual != expected {
                    return Err(Error::DecompressionSizeMismatch { expected, actual });
                }
                out.write_all(&buffer)?;
                Ok(actual.try_into()?)
            }
        }
    }

    pub(crate) fn extract_to_writer_streaming(
        &self,
        archive: &[u8],
        compression: Ba2CompressionFormat,
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        let stored = self.stored_bytes(archive)?;
        if self.packed_size == 0 {
            out.write_all(stored)?;
            return Ok(stored.len().try_into()?);
        }

        let expected: usize = self.size.try_into()?;
        match compression {
            Ba2CompressionFormat::Zip => {
                let mut decoder = ZlibDecoder::new(stored);
                let actual = copy_decompressed(&mut decoder, expected, out, |error| {
                    Error::Zlib(error.to_string())
                })?;
                if decoder.total_in() != u64::try_from(stored.len())? {
                    return Err(Error::TrailingCompressedData);
                }
                Ok(actual.try_into()?)
            }
            Ba2CompressionFormat::LZ4 => self.extract_to_writer(archive, compression, out),
        }
    }
}

fn read_decompressed(
    decoder: &mut impl std::io::Read,
    expected: usize,
    out: &mut Vec<u8>,
    map_error: impl FnOnce(std::io::Error) -> Error,
) -> Result<()> {
    let before = out.len();
    out.try_reserve_exact(expected)?;
    let mut limited = decoder.take(expected as u64 + 1);
    if let Err(error) = limited.read_to_end(out) {
        out.truncate(before);
        return Err(map_error(error));
    }
    let actual = out.len() - before;
    if actual == expected {
        Ok(())
    } else {
        out.truncate(before);
        Err(Error::DecompressionSizeMismatch { expected, actual })
    }
}

fn copy_decompressed(
    decoder: &mut impl std::io::Read,
    expected: usize,
    out: &mut impl std::io::Write,
    map_error: impl FnOnce(std::io::Error) -> Error,
) -> Result<usize> {
    let mut limited = decoder.take(expected as u64 + 1);
    let actual = std::io::copy(&mut limited, out)
        .map_err(map_error)
        .and_then(|actual| usize::try_from(actual).map_err(Error::from))?;
    if actual == expected {
        Ok(actual)
    } else {
        Err(Error::DecompressionSizeMismatch { expected, actual })
    }
}
