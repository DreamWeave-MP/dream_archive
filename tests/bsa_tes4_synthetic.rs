#![cfg(feature = "bsa-tes4")]

use dream_archive::bsa::{Archive, ArchiveVersion, Error};
use flate2::{Compression, write::ZlibEncoder};

const MAGIC: u32 = u32::from_le_bytes(*b"BSA\0");
const HEADER_SIZE: u32 = 0x24;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, bytes).unwrap();
    encoder.finish().unwrap()
}

fn tiny_tes4_header(version: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, version);
    push_u32(&mut bytes, HEADER_SIZE);
    push_u32(&mut bytes, 3);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u16(&mut bytes, 1 << 8);
    push_u16(&mut bytes, 0);
    bytes
}

fn tiny_tes4_index() -> Vec<u8> {
    tiny_tes4_index_with_payload(0, b"payload")
}

fn tiny_tes4_index_with_payload(flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 104);
    push_u32(&mut bytes, HEADER_SIZE);
    push_u32(&mut bytes, 3 | flags);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 5);
    push_u32(&mut bytes, 9);
    push_u16(&mut bytes, 1 << 8);
    push_u16(&mut bytes, 0);
    bytes.extend_from_slice(&[0; 8]);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 52);
    bytes.push(5);
    bytes.extend_from_slice(b"data\0");
    bytes.extend_from_slice(&[0; 8]);
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, 83);
    bytes.extend_from_slice(b"file.txt\0");
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn accepts_supported_tes4_versions() {
    for (raw, version) in [
        (103, ArchiveVersion::v103),
        (104, ArchiveVersion::v104),
        (105, ArchiveVersion::v105),
    ] {
        let archive = Archive::read(&tiny_tes4_header(raw)).unwrap();
        assert_eq!(archive.info().version, version);
    }
}

#[test]
fn rejects_truncated_tes4_headers_as_unexpected_eof() {
    for bytes in [&b""[..], &b"BSA\0"[..], &tiny_tes4_header(104)[..20]] {
        assert!(
            matches!(Archive::read(bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
}

#[test]
fn rejects_unsupported_tes4_version() {
    assert!(matches!(
        Archive::read(&tiny_tes4_header(42)),
        Err(Error::InvalidVersion(42))
    ));
}

#[test]
fn rejects_bad_tes4_header_size() {
    let mut bytes = tiny_tes4_header(104);
    bytes[8..12].copy_from_slice(&204_u32.to_le_bytes());
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::InvalidHeaderSize(204))
    ));
}

#[test]
fn parses_synthetic_tes4_index() {
    let archive = Archive::read(&tiny_tes4_index()).unwrap();
    let entry = &archive.entries()[0];
    assert_eq!(entry.path(), "data\\file.txt");
    assert_eq!(entry.folder(), "data");
    assert_eq!(entry.name(), "file.txt");
    assert_eq!(entry.file().stored_size, 7);
    assert_eq!(entry.file().data_offset, 83);
    assert!(archive.get("DATA/file.TXT").is_some());
}

#[test]
fn rejects_truncated_tes4_folder_record() {
    let mut bytes = tiny_tes4_index();
    bytes.truncate(40);
    assert!(
        matches!(Archive::read(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_truncated_tes4_file_name_block() {
    let mut bytes = tiny_tes4_index();
    bytes.truncate(64);
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::OutOfBounds | Error::Io(_))
    ));
}

#[test]
fn extracts_synthetic_zlib_tes4_file() {
    let compressed = zlib_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    let archive = Archive::read(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"compressed payload"
    );
}

#[test]
fn detects_synthetic_zlib_size_mismatch() {
    let compressed = zlib_compress(b"short");
    let mut payload = Vec::new();
    push_u32(&mut payload, 99);
    payload.extend_from_slice(&compressed);
    let archive = Archive::read(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 5
        })
    ));
}
