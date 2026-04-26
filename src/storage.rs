use std::{fs::File, io, path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub(crate) enum Storage {
    Owned(Arc<[u8]>),
    Mapped(Arc<memmap2::Mmap>),
}

impl Storage {
    pub(crate) fn from_vec(bytes: Vec<u8>) -> Self {
        Self::Owned(Arc::from(bytes.into_boxed_slice()))
    }

    pub(crate) fn open_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        // SAFETY: Game archive files are treated as immutable while an Archive is
        // open. Mutating or truncating a mapped archive concurrently is outside
        // this API's contract and can make any mmap-backed reader unhappy in the
        // usual exciting platform-specific ways.
        let map = unsafe { memmap2::MmapOptions::new().map(&file)? };
        Ok(Self::Mapped(Arc::new(map)))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Owned(bytes) => bytes,
            Self::Mapped(map) => map,
        }
    }
}
