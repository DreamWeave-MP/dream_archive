use bstr::ByteSlice as _;
use dream_archive::ba2::{Archive, ArchiveVersion, Ba2CompressionFormat, Error, PayloadFormat};
use flate2::{Compression, write::ZlibEncoder};
use std::path::PathBuf;

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const GNRL: u32 = u32::from_le_bytes(*b"GNRL");
const DX10: u32 = u32::from_le_bytes(*b"DX10");
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;
const FILE_HEADER_SIZE_GNRL: u16 = 0x10;
const FILE_HEADER_SIZE_DX10: u16 = 0x18;

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

fn output_dir(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target/test-extract");
    path.push(format!("{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

#[derive(Clone, Copy)]
struct TinyTextureOptions<'a> {
    height: u16,
    width: u16,
    mip_count: u8,
    format: u8,
    flags: u8,
    tile_mode: u8,
    first_mip: u16,
    last_mip: u16,
    name: &'a [u8],
    payload: &'a [u8],
}

impl Default for TinyTextureOptions<'_> {
    fn default() -> Self {
        Self {
            height: 8,
            width: 16,
            mip_count: 4,
            format: 98,
            flags: 0,
            tile_mode: 0,
            first_mip: 0,
            last_mip: 3,
            name: b"tiny.dds",
            payload: b"texture-bytes",
        }
    }
}

fn tiny_texture_archive(options: TinyTextureOptions<'_>) -> Vec<u8> {
    let string_table_offset = 72_u64;
    let payload_offset = string_table_offset + 2 + u64::try_from(options.name.len()).unwrap();
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, DX10);
    push_u32(&mut bytes, 1);
    push_u64(&mut bytes, string_table_offset);

    let (hash, _) = dream_archive::ba2::hash_file(options.name.as_bstr());
    push_u32(&mut bytes, hash.file);
    push_u32(&mut bytes, hash.extension);
    push_u32(&mut bytes, hash.directory);
    bytes.push(0);
    bytes.push(1);
    push_u16(&mut bytes, FILE_HEADER_SIZE_DX10);
    push_u16(&mut bytes, options.height);
    push_u16(&mut bytes, options.width);
    bytes.push(options.mip_count);
    bytes.push(options.format);
    bytes.push(options.flags);
    bytes.push(options.tile_mode);
    push_u64(&mut bytes, payload_offset);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, options.payload.len().try_into().unwrap());
    push_u16(&mut bytes, options.first_mip);
    push_u16(&mut bytes, options.last_mip);
    push_u32(&mut bytes, CHUNK_SENTINEL);
    assert_eq!(u64::try_from(bytes.len()).unwrap(), string_table_offset);
    push_u16(&mut bytes, options.name.len().try_into().unwrap());
    bytes.extend_from_slice(options.name);
    bytes.extend_from_slice(options.payload);
    bytes
}

#[test]
fn rejects_invalid_magic() {
    let mut bytes = tiny_archive(TinyArchiveOptions::default());
    bytes[0..4].copy_from_slice(&u32::to_le_bytes(0x1234_5678));
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::InvalidMagic(0x1234_5678))
    ));
}

#[test]
fn rejects_invalid_format() {
    let bytes = tiny_archive(TinyArchiveOptions {
        format: u32::from_le_bytes(*b"NOPE"),
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::InvalidFormat(_))
    ));
}

#[test]
fn rejects_invalid_version() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 0x101,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::InvalidVersion(0x101))
    ));
}

#[test]
fn rejects_invalid_chunk_sentinel() {
    let bytes = tiny_archive(TinyArchiveOptions {
        sentinel: 0xDEAD_BEEF,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::InvalidChunkSentinel(0xDEAD_BEEF))
    ));
}

#[test]
fn rejects_truncated_headers_as_unexpected_eof() {
    for bytes in [&b""[..], &b"BTDX"[..], &b"BTDX\x01\0"[..]] {
        assert!(
            matches!(Archive::read(bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
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
    assert_eq!(archive.info().version, ArchiveVersion::v2);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
}

#[test]
fn synthetic_texture_archive_reconstructs_dx10_dds_header() {
    let bytes = tiny_texture_archive(TinyTextureOptions::default());
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(archive.info().format, PayloadFormat::DX10);
    let data = archive.read_file("tiny.dds").unwrap().unwrap();

    assert_eq!(&data[0..4], b"DDS ");
    assert_eq!(u32::from_le_bytes(data[12..16].try_into().unwrap()), 8);
    assert_eq!(u32::from_le_bytes(data[16..20].try_into().unwrap()), 16);
    assert_eq!(u32::from_le_bytes(data[28..32].try_into().unwrap()), 4);
    assert_eq!(&data[84..88], b"DX10");
    assert_eq!(u32::from_le_bytes(data[128..132].try_into().unwrap()), 98);
    assert_eq!(u32::from_le_bytes(data[132..136].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 1);
    assert_eq!(&data[148..], b"texture-bytes");
}

#[test]
fn synthetic_cubemap_sets_dds_cube_metadata() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        flags: 1,
        ..TinyTextureOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    let data = archive.read_file("tiny.dds").unwrap().unwrap();

    assert_eq!(
        u32::from_le_bytes(data[112..116].try_into().unwrap()),
        0xFE00
    );
    assert_eq!(u32::from_le_bytes(data[136..140].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 6);
}

#[test]
fn rejects_zero_sized_texture_during_extraction() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        width: 0,
        ..TinyTextureOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    assert!(matches!(
        archive.read_file("tiny.dds"),
        Err(Error::Dds("zero-sized texture"))
    ));
}

#[test]
fn rejects_unsupported_dxgi_format_without_partial_output() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        format: 255,
        ..TinyTextureOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    let mut out = b"prefix".to_vec();
    assert!(matches!(
        archive.read_entry_into(&archive.entries()[0], &mut out),
        Err(Error::Dds("unsupported DXGI format"))
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn block_compressed_dds_size_uses_rounded_blocks() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        width: 5,
        height: 5,
        format: 71,
        ..TinyTextureOptions::default()
    });
    let archive = Archive::read(&bytes).unwrap();
    let data = archive.read_file("tiny.dds").unwrap().unwrap();
    assert_eq!(u32::from_le_bytes(data[20..24].try_into().unwrap()), 32);
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
    assert_eq!(archive.info().version, ArchiveVersion::v3);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
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
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::LZ4);
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
    let mut out = Vec::new();
    assert_eq!(
        archive
            .extract_entry(&archive.entries()[0], &mut out)
            .unwrap(),
        16
    );
    assert_eq!(out, b"compressed hello");
}

#[test]
fn extracts_ba2_archive_to_directory() {
    let archive = Archive::read(&tiny_archive(TinyArchiveOptions {
        name: Some(b"textures/hello.txt"),
        chunk_offset: 60 + 2 + u64::try_from(b"textures/hello.txt".len()).unwrap(),
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2");

    assert_eq!(archive.extract_to(&out).unwrap(), 5);
    assert_eq!(
        std::fs::read(out.join("textures").join("hello.txt")).unwrap(),
        b"hello"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn ba2_extract_to_rejects_parent_directory_paths() {
    let archive = Archive::read(&tiny_archive(TinyArchiveOptions {
        name: Some(b"../evil.txt"),
        chunk_offset: 60 + 2 + u64::try_from(b"../evil.txt".len()).unwrap(),
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2-traversal");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("evil.txt").exists());
}

#[test]
fn ba2_extract_to_rejects_colon_paths() {
    let archive = Archive::read(&tiny_archive(TinyArchiveOptions {
        name: Some(b"textures/bad:name.txt"),
        chunk_offset: 60 + 2 + u64::try_from(b"textures/bad:name.txt".len()).unwrap(),
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2-colon");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("textures").join("bad:name.txt").exists());
}

#[test]
fn ba2_extract_to_rejects_unnamed_entries() {
    let archive = Archive::read(&tiny_archive(TinyArchiveOptions {
        name: None,
        string_table_offset: 0,
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2-unnamed");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
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
fn failed_zlib_decompression_does_not_leave_partial_output() {
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
    let mut out = b"prefix".to_vec();
    assert!(
        archive
            .read_entry_into(&archive.entries()[0], &mut out)
            .is_err()
    );
    assert_eq!(out, b"prefix");
}

#[test]
fn rejects_synthetic_zlib_trailing_data() {
    let mut payload = zlib_compress(b"compressed hello");
    payload.push(0);
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
    assert!(matches!(
        archive.read_entry(&archive.entries()[0]),
        Err(Error::TrailingCompressedData)
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
    let mut out = Vec::new();
    assert_eq!(
        archive
            .extract_entry(&archive.entries()[0], &mut out)
            .unwrap(),
        14
    );
    assert_eq!(out, b"lz4 says hello");
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
