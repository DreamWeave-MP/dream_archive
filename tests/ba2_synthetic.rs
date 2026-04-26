use bstr::ByteSlice as _;
use dream_archive::ba2::{Archive, CompressionFormat, Error, Version};
use flate2::{Compression, write::ZlibEncoder};

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const GNRL: u32 = u32::from_le_bytes(*b"GNRL");
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;
const FILE_HEADER_SIZE_GNRL: u16 = 0x10;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy)]
struct TinyArchiveOptions<'a> {
    version: u32,
    format: u32,
    compression_code: Option<u32>,
    string_table_offset: u64,
    name: Option<&'a [u8]>,
    chunk_offset: u64,
    chunk_packed_size: u32,
    chunk_size: u32,
    sentinel: u32,
    payload: &'a [u8],
}

impl Default for TinyArchiveOptions<'_> {
    fn default() -> Self {
        Self {
            version: 1,
            format: GNRL,
            compression_code: None,
            string_table_offset: 60,
            name: Some(b"hello.txt"),
            chunk_offset: 60,
            chunk_packed_size: 0,
            chunk_size: 5,
            sentinel: CHUNK_SENTINEL,
            payload: b"hello",
        }
    }
}

fn tiny_archive(options: TinyArchiveOptions<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, options.version);
    push_u32(&mut bytes, options.format);
    push_u32(&mut bytes, 1);
    push_u64(&mut bytes, options.string_table_offset);
    if matches!(options.version, 2 | 3) {
        push_u64(&mut bytes, 1);
    }
    if let Some(code) = options.compression_code {
        push_u32(&mut bytes, code);
    }

    let (hash, _) = dream_archive::ba2::hash_file(b"hello.txt".as_bstr());
    push_u32(&mut bytes, hash.file);
    push_u32(&mut bytes, hash.extension);
    push_u32(&mut bytes, hash.directory);
    bytes.push(0);
    bytes.push(1);
    push_u16(&mut bytes, FILE_HEADER_SIZE_GNRL);
    push_u64(&mut bytes, options.chunk_offset);
    push_u32(&mut bytes, options.chunk_packed_size);
    push_u32(&mut bytes, options.chunk_size);
    push_u32(&mut bytes, options.sentinel);

    if let Some(name) = options.name {
        assert_eq!(
            u64::try_from(bytes.len()).unwrap(),
            options.string_table_offset
        );
        push_u16(&mut bytes, name.len().try_into().unwrap());
        bytes.extend_from_slice(name);
    }
    bytes.extend_from_slice(options.payload);
    bytes
}

fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn rejects_chunk_offsets_outside_archive() {
    let bytes = tiny_archive(TinyArchiveOptions {
        chunk_offset: 10_000,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn rejects_chunk_size_overflow() {
    let bytes = tiny_archive(TinyArchiveOptions {
        chunk_offset: u64::MAX,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::IntegralTruncation | Error::OutOfBounds)
    ));
}

#[test]
fn rejects_string_table_offset_outside_archive() {
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 10_000,
        name: None,
        chunk_offset: 60,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn rejects_truncated_string_table_entry() {
    let mut bytes = tiny_archive(TinyArchiveOptions::default());
    bytes.truncate(62 + 3);
    assert!(matches!(Archive::read(&bytes), Err(Error::Io(_))));
}

#[test]
fn accepts_v2_extra_header_field() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 2,
        string_table_offset: 68,
        chunk_offset: 68,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(archive.options().version, Version::v2);
    assert_eq!(archive.options().compression_format, CompressionFormat::Zip);
}

#[test]
fn v3_unknown_compression_code_means_zip() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(0xFFFF_FFFE),
        string_table_offset: 72,
        chunk_offset: 72,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(archive.options().version, Version::v3);
    assert_eq!(archive.options().compression_format, CompressionFormat::Zip);
}

#[test]
fn v3_compression_code_three_means_lz4() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 72,
        chunk_offset: 72,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(archive.options().compression_format, CompressionFormat::LZ4);
}

#[test]
fn extracts_synthetic_zlib_chunk() {
    let payload = zlib_compress(b"compressed hello");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 16,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(
        archive.read_entry(&archive.entries()[0]).unwrap(),
        b"compressed hello"
    );
}

#[test]
fn detects_zlib_decompression_size_mismatch() {
    let payload = zlib_compress(b"short");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 99,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert!(matches!(
        archive.read_entry(&archive.entries()[0]),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 5
        })
    ));
}

#[test]
fn extracts_synthetic_lz4_chunk() {
    let payload = lz4_flex::block::compress(b"lz4 says hello");
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 0,
        name: None,
        chunk_offset: 72,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 14,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(
        archive.read_entry(&archive.entries()[0]).unwrap(),
        b"lz4 says hello"
    );
}

#[test]
fn detects_lz4_decompression_size_mismatch() {
    let payload = lz4_flex::block::compress(b"lz4 short");
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 0,
        name: None,
        chunk_offset: 72,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 99,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert!(matches!(
        archive.read_entry(&archive.entries()[0]),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 9
        })
    ));
}
