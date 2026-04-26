use std::io;

#[derive(Debug)]
pub(crate) enum Error {
    OutOfBounds,
    UnexpectedEof,
}

impl From<Error> for io::Error {
    fn from(value: Error) -> Self {
        match value {
            Error::OutOfBounds => Self::from(io::ErrorKind::InvalidData),
            Error::UnexpectedEof => Self::from(io::ErrorKind::UnexpectedEof),
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    #[cfg(feature = "ba2")]
    pub(crate) fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.bytes.len() {
            return Err(Error::OutOfBounds);
        }
        self.pos = pos;
        Ok(())
    }

    pub(crate) fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).ok_or(Error::OutOfBounds)?;
        let bytes = self.bytes.get(self.pos..end).ok_or(Error::UnexpectedEof)?;
        self.pos = end;
        Ok(bytes)
    }

    #[cfg(feature = "ba2")]
    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    #[cfg(feature = "ba2")]
    pub(crate) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}
