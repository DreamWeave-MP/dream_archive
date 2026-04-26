#![cfg(feature = "bsa-tes4")]

use dream_archive::bsa::tes4::{Archive, ArchiveVersion, Error};
use flate2::{Compression, write::ZlibEncoder};
use lz4_flex::frame::FrameEncoder;
use std::path::PathBuf;

const MAGIC: u32 = u32::from_le_bytes(*b"BSA\0");
const HEADER_SIZE: u32 = 0x24;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, bytes).unwrap();
    encoder.finish().unwrap()
}

fn lz4_frame_compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = FrameEncoder::new(Vec::new());
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
    tiny_tes4_index_with_version_and_payload(104, flags, payload)
}

fn tiny_tes4_index_with_version_and_payload(version: u32, flags: u32, payload: &[u8]) -> Vec<u8> {
    tiny_tes4_index_with_version_names_and_payload(version, flags, b"data", b"file.txt", payload)
}

fn tiny_tes4_index_with_version_names_and_payload(
    version: u32,
    flags: u32,
    folder: &[u8],
    name: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let folder_names_len: u32 = (folder.len() + 1).try_into().unwrap();
    let file_names_len: u32 = (name.len() + 1).try_into().unwrap();
    let folder_record_size = if version == 105 { 24 } else { 16 };
    let folder_record_offset = HEADER_SIZE + folder_record_size;
    let data_offset = folder_record_offset + 1 + folder_names_len + 16 + file_names_len;
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, version);
    push_u32(&mut bytes, HEADER_SIZE);
    push_u32(&mut bytes, 3 | flags);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, folder_names_len);
    push_u32(&mut bytes, file_names_len);
    push_u16(&mut bytes, 1 << 8);
    push_u16(&mut bytes, 0);
    bytes.extend_from_slice(&[0; 8]);
    push_u32(&mut bytes, 1);
    if version == 105 {
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, folder_record_offset);
        push_u32(&mut bytes, 0);
    } else {
        push_u32(&mut bytes, folder_record_offset);
    }
    bytes.push(folder_names_len.try_into().unwrap());
    bytes.extend_from_slice(folder);
    bytes.push(0);
    bytes.extend_from_slice(&[0; 8]);
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, data_offset);
    bytes.extend_from_slice(name);
    bytes.push(0);
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
fn extracts_tes4_archive_to_directory() {
    let archive = Archive::read(&tiny_tes4_index()).unwrap();
    let out = output_dir("tes4");

    assert_eq!(archive.extract_to(&out).unwrap(), 7);
    assert_eq!(
        std::fs::read(out.join("data").join("file.txt")).unwrap(),
        b"payload"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn tes4_extract_to_rejects_parent_directory_paths() {
    let archive = Archive::read(&tiny_tes4_index_with_version_names_and_payload(
        104,
        0,
        b"data",
        b"../evil.txt",
        b"payload",
    ))
    .unwrap();
    let out = output_dir("tes4-traversal");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("evil.txt").exists());
}

#[test]
fn tes4_extract_to_rejects_colon_paths() {
    let archive = Archive::read(&tiny_tes4_index_with_version_names_and_payload(
        104,
        0,
        b"data",
        b"bad:name.txt",
        b"payload",
    ))
    .unwrap();
    let out = output_dir("tes4-colon");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("data").join("bad:name.txt").exists());
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
fn rejects_file_name_table_that_bleeds_into_payload() {
    let mut bytes = tiny_tes4_index();
    bytes[82] = b'X';
    assert!(
        matches!(Archive::read(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_folder_file_count_that_disagrees_with_header() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 20, 2);
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn rejects_archives_without_file_name_strings() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 12, 1);
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::NotImplemented(
            "TES4 hash-only archives without file name strings"
        ))
    ));
}

#[test]
fn rejects_archives_without_directory_name_strings() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 12, 2);
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::NotImplemented(
            "TES4 hash-only archives without directory name strings"
        ))
    ));
}

#[test]
fn rejects_folder_name_without_trailing_nul() {
    let mut bytes = tiny_tes4_index();
    bytes[57] = b'X';
    assert!(
        matches!(Archive::read(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_folder_record_offset_before_folder_block() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 48, 40);
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn rejects_file_payload_that_overlaps_index() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 70, 36);
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn file_size_checked_flag_is_not_part_of_stored_size() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 66, 7 | (1 << 31));
    let archive = Archive::read(&bytes).unwrap();
    let record = archive.entries()[0].file();
    assert_eq!(record.stored_size, 7);
    assert!(record.checked);
}

#[test]
fn entry_path_preserves_archive_spelling_while_lookup_normalizes() {
    let mut bytes = tiny_tes4_index();
    bytes[53..58].copy_from_slice(b"Data\0");
    bytes[74..83].copy_from_slice(b"File.TXT\0");
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(archive.entries()[0].path(), "Data\\File.TXT");
    assert!(archive.get("data/file.txt").is_some());
}

#[test]
fn rejects_xmem_compressed_files_at_extraction_time() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 12, 3 | (1 << 2) | (1 << 9));
    let archive = Archive::read(&bytes).unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::NotImplemented("TES4 XMem compression"))
    ));
}

#[test]
fn embedded_name_skip_requires_compressed_size_prefix_after_name() {
    let archive = Archive::read(&tiny_tes4_index_with_payload(
        (1 << 2) | (1 << 8),
        b"\x04name",
    ))
    .unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_file_data_offset_outside_archive() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 70, 0x8000_0053);
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn compression_toggle_disables_archive_default_compression() {
    let mut bytes = tiny_tes4_index_with_payload(1 << 2, b"payload");
    write_u32(&mut bytes, 66, 7 | (1 << 30));
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn compression_toggle_enables_file_compression() {
    let compressed = zlib_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    let mut bytes = tiny_tes4_index_with_payload(0, &payload);
    write_u32(
        &mut bytes,
        66,
        u32::try_from(payload.len()).unwrap() | (1 << 30),
    );
    let archive = Archive::read(&bytes).unwrap();
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"compressed payload"
    );
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
    let mut out = Vec::new();
    assert_eq!(
        archive.extract_file("data/file.txt", &mut out).unwrap(),
        Some(18)
    );
    assert_eq!(out, b"compressed payload");
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

#[test]
fn rejects_synthetic_zlib_trailing_data() {
    let compressed = zlib_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    payload.push(0);
    let archive = Archive::read(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::TrailingCompressedData)
    ));
}

#[test]
fn failed_decompression_does_not_leave_partial_output() {
    let compressed = zlib_compress(b"short");
    let mut payload = Vec::new();
    push_u32(&mut payload, 99);
    payload.extend_from_slice(&compressed);
    let archive = Archive::read(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    let mut out = b"prefix".to_vec();
    assert!(
        archive
            .read_entry_into(&archive.entries()[0], &mut out)
            .is_err()
    );
    assert_eq!(out, b"prefix");
}

#[test]
fn extracts_synthetic_lz4_frame_tes4_file() {
    let compressed = lz4_frame_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    let archive = Archive::read(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"compressed payload"
    );
    let mut out = Vec::new();
    assert_eq!(
        archive.extract_file("data/file.txt", &mut out).unwrap(),
        Some(18)
    );
    assert_eq!(out, b"compressed payload");
}

#[test]
fn rejects_synthetic_lz4_block_when_frame_is_required() {
    let compressed = lz4_flex::block::compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    let archive = Archive::read(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::InvalidLz4Frame)
    ));
}

#[test]
fn detects_synthetic_lz4_frame_size_mismatch() {
    let compressed = lz4_frame_compress(b"short");
    let mut payload = Vec::new();
    push_u32(&mut payload, 99);
    payload.extend_from_slice(&compressed);
    let archive = Archive::read(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 5
        })
    ));
}

#[test]
fn rejects_synthetic_lz4_frame_trailing_data() {
    let compressed = lz4_frame_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    payload.push(0);
    let archive = Archive::read(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::TrailingCompressedData)
    ));
}
