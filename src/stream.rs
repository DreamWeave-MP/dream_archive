use std::io::{Cursor, Read};

pub(crate) type BoxReader<'a> = Box<dyn Read + 'a>;

pub(crate) fn borrowed_reader(bytes: &[u8]) -> BoxReader<'_> {
    Box::new(Cursor::new(bytes))
}

#[cfg(any(feature = "ba2", feature = "bsa-tes4"))]
pub(crate) fn owned_reader(bytes: Vec<u8>) -> BoxReader<'static> {
    Box::new(Cursor::new(bytes))
}

#[cfg(feature = "ba2")]
pub(crate) struct ChainReader<'a> {
    readers: Vec<BoxReader<'a>>,
    index: usize,
}

#[cfg(feature = "ba2")]
impl<'a> ChainReader<'a> {
    pub(crate) fn new(readers: Vec<BoxReader<'a>>) -> Self {
        Self { readers, index: 0 }
    }
}

#[cfg(feature = "ba2")]
impl Read for ChainReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while let Some(reader) = self.readers.get_mut(self.index) {
            let read = reader.read(out)?;
            if read != 0 || out.is_empty() {
                return Ok(read);
            }
            self.index += 1;
        }
        Ok(0)
    }
}
