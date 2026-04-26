#![cfg(feature = "bsa-tes3")]

use dream_archive::bsa::tes3::{Archive, Error};
use std::path::PathBuf;

const VERSION: u32 = 0x0000_0100;
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn tiny_tes3_archive(name: &[u8], payload: &[u8]) -> Vec<u8> {
    let file_count = 1u32;
    let names_len = u32::try_from(name.len() + 1).unwrap();
    let hash_offset = 8 * file_count + 4 * file_count + names_len;
    let mut bytes = Vec::new();
    push_u32(&mut bytes, VERSION);
    push_u32(&mut bytes, hash_offset);
    push_u32(&mut bytes, file_count);
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(name);
    bytes.push(0);
    push_u64(&mut bytes, 0x0123_4567_89ab_cdef);
    bytes.extend_from_slice(payload);
    bytes
}

fn output_dir(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target/test-extract");
    path.push(format!("{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

#[test]
fn extracts_synthetic_tes3_file() {
    let archive = Archive::read(&tiny_tes3_archive(b"Meshes/Foo.NIF", b"hello")).unwrap();

    assert_eq!(archive.info().file_count, 1);
    assert_eq!(archive.len(), 1);
    assert_eq!(archive.entries()[0].path(), "Meshes/Foo.NIF");
    assert_eq!(archive.entries()[0].file().size, 5);
    assert_eq!(archive.entries()[0].hash(), 0x0123_4567_89ab_cdef);
    assert!(archive.contains("meshes\\foo.nif"));
    assert_eq!(
        archive.read_file("meshes/foo.nif").unwrap().unwrap(),
        b"hello"
    );
    let mut out = Vec::new();
    assert_eq!(
        archive.extract_file("meshes/foo.nif", &mut out).unwrap(),
        Some(5)
    );
    assert_eq!(out, b"hello");
}

#[test]
fn extracts_tes3_archive_to_directory() {
    let archive = Archive::read(&tiny_tes3_archive(b"Meshes/Foo.NIF", b"hello")).unwrap();
    let out = output_dir("tes3");

    assert_eq!(archive.extract_to(&out).unwrap(), 5);
    assert_eq!(
        std::fs::read(out.join("Meshes").join("Foo.NIF")).unwrap(),
        b"hello"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn tes3_extract_to_rejects_parent_directory_paths() {
    let archive = Archive::read(&tiny_tes3_archive(b"../evil.txt", b"hello")).unwrap();
    let out = output_dir("tes3-traversal");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("evil.txt").exists());
}

#[test]
fn rejects_invalid_tes3_version() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[0..4].copy_from_slice(&42_u32.to_le_bytes());
    assert!(matches!(
        Archive::read(&bytes),
        Err(Error::InvalidVersion(42))
    ));
}

#[test]
fn rejects_truncated_tes3_header() {
    assert!(
        matches!(Archive::read(b""), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_tes3_hash_table_outside_archive() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[4..8].copy_from_slice(&10_000_u32.to_le_bytes());
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn rejects_tes3_name_offset_outside_name_blob() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[20..24].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn rejects_tes3_name_without_nul_before_hashes() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[32] = b'X';
    assert!(
        matches!(Archive::read(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_tes3_file_data_outside_archive() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[12..16].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(Archive::read(&bytes), Err(Error::OutOfBounds)));
}

#[test]
fn tes3_read_entry_into_appends_to_existing_output() {
    let archive = Archive::read(&tiny_tes3_archive(b"file.txt", b"hello")).unwrap();
    let mut out = b"prefix".to_vec();
    archive
        .read_entry_into(&archive.entries()[0], &mut out)
        .unwrap();
    assert_eq!(out, b"prefixhello");
}
