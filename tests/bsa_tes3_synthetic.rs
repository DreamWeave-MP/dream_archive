#![cfg(feature = "bsa-tes3")]

use dream_archive::bsa::{
    FilenameEncoding,
    tes3::{Archive, Builder, Error},
};
use std::io::Read as _;
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
    let archive = Archive::from_slice(&tiny_tes3_archive(b"Meshes/Foo.NIF", b"hello")).unwrap();

    assert_eq!(archive.info().file_count, 1);
    assert_eq!(archive.len(), 1);
    assert_eq!(archive.entries()[0].path(), "Meshes/Foo.NIF");
    assert_eq!(archive.entries()[0].file().size, 5);
    assert_eq!(archive.entries()[0].hash(), 0x0123_4567_89ab_cdef);
    assert!(archive.contains("meshes\\foo.nif"));
    assert!(archive.contains("/MESHES//foo.nif"));
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

    let mut reader = archive.open_file_required("meshes/foo.nif").unwrap();
    let mut streamed = Vec::new();
    reader.read_to_end(&mut streamed).unwrap();
    assert_eq!(streamed, b"hello");
}

#[test]
fn tes3_extract_entry_to_path_creates_parent_directories() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"Meshes/Foo.NIF", b"hello")).unwrap();
    let out = output_dir("tes3-entry-create-parent");
    let file_path = out.join("missing/parents/foo.nif");

    assert_eq!(
        archive
            .extract_entry_to_path(&archive.entries()[0], &file_path)
            .unwrap(),
        5
    );
    assert_eq!(std::fs::read(&file_path).unwrap(), b"hello");
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn writes_tes3_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();
    builder.add_bytes("textures/bar.dds", b"texture").unwrap();

    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.len(), 2);
    assert_eq!(
        archive.read_file("meshes/foo.nif").unwrap().unwrap(),
        b"mesh"
    );
    assert_eq!(
        archive.read_file("textures/bar.dds").unwrap().unwrap(),
        b"texture"
    );
    assert_eq!(archive.entries()[0].hash(), 0xECDD_AD85_071D_1701);
    assert_eq!(archive.entries()[0].path(), "textures\\bar.dds");
    assert_eq!(archive.entries()[1].path(), "meshes\\foo.nif");
}

#[test]
fn tes3_writer_encodes_legacy_text_paths() {
    let mut builder = Builder::new();
    builder
        .add_encoded_path("texts/María.txt", FilenameEncoding::Windows1252, b"hola")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(
        archive.read_file(b"texts/mar\xeda.txt").unwrap().unwrap(),
        b"hola"
    );
}

#[cfg(unix)]
#[test]
fn tes3_writer_add_dir_follows_file_symlinks_at_relative_path() {
    let root = output_dir("tes3-add-dir-symlink");
    let external = output_dir("tes3-add-dir-external");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(external.join("different.dds"), b"target bytes").unwrap();
    std::os::unix::fs::symlink(external.join("different.dds"), root.join("someThing.dds")).unwrap();

    let mut builder = Builder::new();
    builder.add_dir(&root).unwrap();
    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(
        archive.read_file("something.dds").unwrap().unwrap(),
        b"target bytes"
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(external).unwrap();
}

#[test]
fn tes3_lookup_uses_openmw_style_path_normalization() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"\\Meshes//Foo.NIF", b"hello")).unwrap();

    assert_eq!(archive.entries()[0].path(), "\\Meshes//Foo.NIF");
    assert!(archive.contains("meshes/foo.nif"));
    assert!(archive.contains("/MESHES\\\\FOO.NIF"));
    assert_eq!(
        archive.read_file("meshes//foo.nif").unwrap().unwrap(),
        b"hello"
    );
}

#[test]
fn writes_tes3_archive_to_path() {
    let mut builder = Builder::new();
    builder.add_bytes("file.txt", b"hello").unwrap();
    let out = output_dir("tes3-write-path");
    std::fs::create_dir_all(&out).unwrap();
    let archive_path = out.join("out.bsa");

    builder.write_path(&archive_path).unwrap();
    let archive = Archive::open_path(&archive_path).unwrap();

    assert_eq!(archive.read_file("file.txt").unwrap().unwrap(), b"hello");
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn tes3_writer_output_is_deterministic() {
    let mut first = Builder::new();
    first.add_bytes("b.txt", b"b").unwrap();
    first.add_bytes("a.txt", b"a").unwrap();

    let mut second = Builder::new();
    second.add_bytes("a.txt", b"a").unwrap();
    second.add_bytes("b.txt", b"b").unwrap();

    assert_eq!(first.to_vec().unwrap(), second.to_vec().unwrap());
}

#[test]
fn tes3_writer_rejects_duplicate_normalized_paths() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();

    assert!(matches!(
        builder.add_bytes("meshes\\foo.nif", b"other"),
        Err(Error::DuplicatePath)
    ));
}

#[test]
fn tes3_writer_rejects_unsafe_paths() {
    for path in ["", ".", "../evil.txt", "bad:name.txt", "bad\0name.txt"] {
        let mut builder = Builder::new();
        assert!(matches!(
            builder.add_bytes(path.as_bytes(), b"payload"),
            Err(Error::InvalidArchivePath)
        ));
    }
}

#[test]
fn extracts_tes3_archive_to_directory() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"Meshes/Foo.NIF", b"hello")).unwrap();
    let out = output_dir("tes3");

    assert_eq!(archive.extract_to(&out).unwrap(), 5);
    assert_eq!(
        std::fs::read(out.join("Meshes").join("Foo.NIF")).unwrap(),
        b"hello"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn extracts_tes3_archive_to_decoded_filesystem_paths() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"data/Mar\xeda.txt", b"hello")).unwrap();
    let out = output_dir("tes3-encoding");

    assert_eq!(
        archive
            .extract_to_with_encoding(&out, FilenameEncoding::Windows1252)
            .unwrap(),
        5
    );
    assert_eq!(
        std::fs::read(out.join("data").join("María.txt")).unwrap(),
        b"hello"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn tes3_extract_to_rejects_parent_directory_paths() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"../evil.txt", b"hello")).unwrap();
    let out = output_dir("tes3-traversal");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("evil.txt").exists());
}

#[test]
fn tes3_extract_to_rejects_colon_paths() {
    let archive =
        Archive::from_slice(&tiny_tes3_archive(b"Meshes/bad:name.nif", b"hello")).unwrap();
    let out = output_dir("tes3-colon");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("Meshes").join("bad:name.nif").exists());
}

#[test]
fn tes3_extract_to_rejects_empty_paths() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"", b"hello")).unwrap();
    let out = output_dir("tes3-empty");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
}

#[test]
fn rejects_invalid_tes3_version() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[0..4].copy_from_slice(&42_u32.to_le_bytes());
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::InvalidVersion(42))
    ));
}

#[test]
fn rejects_truncated_tes3_header() {
    assert!(
        matches!(Archive::from_slice(b""), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_tes3_hash_table_outside_archive() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[4..8].copy_from_slice(&10_000_u32.to_le_bytes());
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_tes3_name_offset_outside_name_blob() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[20..24].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_tes3_name_without_nul_before_hashes() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[32] = b'X';
    assert!(
        matches!(Archive::from_slice(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_tes3_file_data_outside_archive() {
    let mut bytes = tiny_tes3_archive(b"file.txt", b"hello");
    bytes[12..16].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn tes3_read_entry_into_appends_to_existing_output() {
    let archive = Archive::from_slice(&tiny_tes3_archive(b"file.txt", b"hello")).unwrap();
    let mut out = b"prefix".to_vec();
    archive
        .read_entry_into(&archive.entries()[0], &mut out)
        .unwrap();
    assert_eq!(out, b"prefixhello");
}

#[test]
fn add_file_reads_payload_when_written() {
    let root = output_dir("tes3-deferred-add-file");
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    std::fs::write(&source, b"first").unwrap();

    let mut builder = Builder::new();
    builder.add_file("data/source.txt", &source).unwrap();
    std::fs::write(&source, b"later").unwrap();

    let archive = Archive::from_vec(builder.to_vec().unwrap()).unwrap();
    assert_eq!(
        archive.read_file_required("data/source.txt").unwrap(),
        b"later"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn builder_can_defer_existing_archive_entry() {
    let source = std::sync::Arc::new(
        Archive::from_slice(&tiny_tes3_archive(b"data/source.txt", b"payload")).unwrap(),
    );
    let (id, _) = source.entries_with_ids().next().unwrap();

    let mut builder = Builder::new();
    builder
        .add_archive_entry("data/copied.txt", std::sync::Arc::clone(&source), id)
        .unwrap();
    let archive = Archive::from_vec(builder.to_vec().unwrap()).unwrap();
    assert_eq!(
        archive.read_file_required("data/copied.txt").unwrap(),
        b"payload"
    );
}
