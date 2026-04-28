use std::io::{self, Cursor, Read};

pub(crate) type BoxReader<'a> = Box<dyn Read + 'a>;

pub(crate) fn borrowed_reader(bytes: &[u8]) -> BoxReader<'_> {
    Box::new(Cursor::new(bytes))
}

pub(crate) fn owned_reader(bytes: Vec<u8>) -> BoxReader<'static> {
    Box::new(Cursor::new(bytes))
}

pub(crate) struct ChainReader<'a> {
    readers: Vec<BoxReader<'a>>,
    index: usize,
}

impl<'a> ChainReader<'a> {
    pub(crate) fn new(readers: Vec<BoxReader<'a>>) -> Self {
        Self { readers, index: 0 }
    }
}

impl Read for ChainReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
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
