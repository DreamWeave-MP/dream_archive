#![cfg(feature = "bsa-tes4")]

use dream_archive::{
    CompressionOverride,
    bsa::{
        FilenameEncoding,
        tes4::{
            Archive, ArchiveFlags, ArchiveVersion, Builder, Error, GameProfile, NameMode,
            hash_directory, hash_file,
        },
    },
};
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

fn push_u64(out: &mut Vec<u8>, value: u64) {
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

fn assert_no_temp_extract_files(dir: &std::path::Path) {
    if !dir.exists() {
        return;
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        assert!(
            !name.to_string_lossy().contains(".dream-archive-tmp-"),
            "temporary extraction file was not cleaned up: {}",
            name.to_string_lossy()
        );
    }
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
    push_u64(&mut bytes, hash_directory(folder).0.numeric());
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
    push_u64(&mut bytes, hash_file(name).0.numeric());
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, data_offset);
    bytes.extend_from_slice(name);
    bytes.push(0);
    bytes.extend_from_slice(payload);
    bytes
}

fn tiny_hash_only_tes4_index() -> Vec<u8> {
    tiny_hash_only_tes4_index_with_hashes(b"data", b"file.txt", b"payload")
}

fn tiny_hash_only_tes4_index_with_hashes(folder: &[u8], name: &[u8], payload: &[u8]) -> Vec<u8> {
    let folder_record_offset = HEADER_SIZE + 16;
    let data_offset = folder_record_offset + 16;
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 104);
    push_u32(&mut bytes, HEADER_SIZE);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u16(&mut bytes, 1 << 8);
    push_u16(&mut bytes, 0);
    push_u64(&mut bytes, hash_directory(folder).0.numeric());
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, folder_record_offset);
    push_u64(&mut bytes, hash_file(name).0.numeric());
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, data_offset);
    bytes.extend_from_slice(payload);
    bytes
}

fn tiny_embedded_name_tes4_index() -> Vec<u8> {
    let embedded_name = b"data\\file.txt";
    let payload = b"payload";
    let folder_record_offset = HEADER_SIZE + 16;
    let data_offset = folder_record_offset + 16;
    let stored_size = 1 + embedded_name.len() + payload.len();
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 104);
    push_u32(&mut bytes, HEADER_SIZE);
    push_u32(&mut bytes, 1 << 8);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u16(&mut bytes, 1 << 8);
    push_u16(&mut bytes, 0);
    push_u64(&mut bytes, hash_directory(b"data").0.numeric());
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, folder_record_offset);
    push_u64(&mut bytes, hash_file(b"file.txt").0.numeric());
    push_u32(&mut bytes, stored_size.try_into().unwrap());
    push_u32(&mut bytes, data_offset);
    bytes.push(embedded_name.len().try_into().unwrap());
    bytes.extend_from_slice(embedded_name);
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
        let archive = Archive::from_slice(&tiny_tes4_header(raw)).unwrap();
        assert_eq!(archive.info().version, version);
    }
}

#[test]
fn rejects_truncated_tes4_headers_as_unexpected_eof() {
    for bytes in [&b""[..], &b"BSA\0"[..], &tiny_tes4_header(104)[..20]] {
        assert!(
            matches!(Archive::from_slice(bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
}

#[test]
fn rejects_unsupported_tes4_version() {
    assert!(matches!(
        Archive::from_slice(&tiny_tes4_header(42)),
        Err(Error::InvalidVersion(42))
    ));
}

#[test]
fn rejects_bad_tes4_header_size() {
    let mut bytes = tiny_tes4_header(104);
    bytes[8..12].copy_from_slice(&204_u32.to_le_bytes());
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::InvalidHeaderSize(204))
    ));
}

#[test]
fn rejects_tes4_xbox_archive_flag_at_parse_time() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 12, 3 | (1 << 6));

    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::NotImplemented("TES4 Xbox archive layout"))
    ));
}

#[test]
fn rejects_tes4_xmem_compression_flag_at_parse_time() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 12, 3 | (1 << 2) | (1 << 9));

    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::NotImplemented("TES4 XMem compression"))
    ));
}

#[test]
fn parses_synthetic_tes4_index() {
    let archive = Archive::from_slice(&tiny_tes4_index()).unwrap();
    let entry = &archive.entries()[0];
    assert_eq!(entry.path().unwrap(), "data\\file.txt");
    assert_eq!(entry.folder().unwrap(), "data");
    assert_eq!(entry.name().unwrap(), "file.txt");
    assert_eq!(entry.file().stored_size, 7);
    assert_eq!(entry.file().data_offset, 83);
    assert!(archive.get("DATA/file.TXT").is_some());
    assert!(archive.get("/DATA//file.TXT").is_some());
}

#[test]
fn parses_hash_only_tes4_index_for_hash_lookup() {
    let archive = Archive::from_slice(&tiny_hash_only_tes4_index()).unwrap();
    let entry = &archive.entries()[0];

    assert_eq!(entry.path(), None);
    assert_eq!(entry.folder(), None);
    assert_eq!(entry.name(), None);
    assert_eq!(entry.folder_hash(), hash_directory(b"data").0);
    assert_eq!(entry.file_hash(), hash_file(b"file.txt").0);
    assert!(archive.get("data/file.txt").is_some());
    assert!(
        archive
            .get_by_hash(hash_directory(b"data").0, hash_file(b"file.txt").0)
            .is_some()
    );
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn hash_only_tes4_extract_to_reports_missing_paths() {
    let archive = Archive::from_slice(&tiny_hash_only_tes4_index()).unwrap();
    let out = output_dir("tes4-hash-only");

    assert!(matches!(
        archive.extract_to(&out),
        Err(Error::ArchivePathsUnavailable)
    ));
    assert!(!out.exists());
}

#[test]
fn recovers_tes4_paths_from_embedded_file_names() {
    let archive = Archive::from_slice(&tiny_embedded_name_tes4_index()).unwrap();
    let entry = &archive.entries()[0];

    assert_eq!(entry.path().unwrap(), "data\\file.txt");
    assert_eq!(entry.folder().unwrap(), "data");
    assert_eq!(entry.name().unwrap(), "file.txt");
    assert_eq!(archive.read_entry(entry).unwrap(), b"payload");
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn extracts_hash_only_tes4_archive_with_path_dictionary() {
    let archive = Archive::from_slice(&tiny_hash_only_tes4_index()).unwrap();
    let out = output_dir("tes4-hash-dictionary");

    assert_eq!(
        archive
            .extract_to_with_paths(
                &out,
                [b"missing.txt".as_slice(), b"data/file.txt".as_slice()]
            )
            .unwrap(),
        7
    );
    assert_eq!(
        std::fs::read(out.join("data").join("file.txt")).unwrap(),
        b"payload"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn hash_only_tes4_path_dictionary_rejects_unsafe_matched_paths() {
    for unsafe_path in [
        b"../file.txt".as_slice(),
        b"data/bad:name.txt".as_slice(),
        b"data/bad\0name.txt".as_slice(),
    ] {
        let separator = unsafe_path
            .iter()
            .rposition(|byte| matches!(*byte, b'/' | b'\\'))
            .unwrap();
        let archive = Archive::from_slice(&tiny_hash_only_tes4_index_with_hashes(
            &unsafe_path[..separator],
            &unsafe_path[separator + 1..],
            b"payload",
        ))
        .unwrap();
        let out = output_dir("tes4-unsafe-hash-dictionary");

        assert!(
            matches!(archive.extract_to_with_paths(&out, [unsafe_path]), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
        );
        assert!(!out.join("file.txt").exists());
        assert!(!out.join("data").join("bad:name.txt").exists());
        assert!(!out.join("data").join("bad\0name.txt").exists());
        let _ = std::fs::remove_dir_all(out);
    }
}

#[test]
fn hash_only_tes4_path_dictionary_hashes_nested_basename() {
    let archive = Archive::from_slice(&tiny_hash_only_tes4_index_with_hashes(
        b"meshes/foo",
        b"bar.nif",
        b"nested payload",
    ))
    .unwrap();
    let out = output_dir("tes4-nested-hash-dictionary");

    assert_eq!(
        archive
            .extract_to_with_paths(&out, [b"meshes/foo/bar.nif".as_slice()])
            .unwrap(),
        14
    );
    assert_eq!(
        std::fs::read(out.join("meshes").join("foo").join("bar.nif")).unwrap(),
        b"nested payload"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn tes4_lookup_uses_openmw_style_path_normalization() {
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_names_and_payload(
        104,
        0,
        b"\\Data//Meshes",
        b"Foo.NIF",
        b"payload",
    ))
    .unwrap();

    assert_eq!(
        archive.entries()[0].path().unwrap(),
        "\\Data//Meshes\\Foo.NIF"
    );
    assert!(archive.contains("data/meshes/foo.nif"));
    assert!(archive.contains("/DATA\\\\MESHES//FOO.NIF"));
    assert_eq!(
        archive.read_file("data//meshes/foo.nif").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn file_data_offset_ignores_secondary_archive_flag() {
    let mut bytes = tiny_tes4_index();
    let offset = u32::from_le_bytes(bytes[70..74].try_into().unwrap());
    write_u32(&mut bytes, 70, offset | (1 << 31));

    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.entries()[0].file().data_offset, offset);
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn rejects_v105_folder_record_offset_outside_archive() {
    let mut bytes = tiny_tes4_index_with_version_and_payload(105, 0, b"payload");
    write_u32(&mut bytes, 56, 1);

    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn extracts_tes4_archive_to_directory() {
    let archive = Archive::from_slice(&tiny_tes4_index()).unwrap();
    let out = output_dir("tes4");

    assert_eq!(archive.extract_to(&out).unwrap(), 7);
    assert_eq!(
        std::fs::read(out.join("data").join("file.txt")).unwrap(),
        b"payload"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn extracts_tes4_archive_to_decoded_filesystem_paths() {
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_names_and_payload(
        104,
        0,
        b"data",
        b"Mar\xeda.txt",
        b"payload",
    ))
    .unwrap();
    let out = output_dir("tes4-encoding");

    assert_eq!(
        archive
            .extract_to_with_encoding(&out, FilenameEncoding::Windows1252)
            .unwrap(),
        7
    );
    assert_eq!(
        std::fs::read(out.join("data").join("María.txt")).unwrap(),
        b"payload"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn writes_tes4_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();
    builder.add_bytes("textures/bar.dds", b"texture").unwrap();

    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v104);
    assert_eq!(archive.len(), 2);
    assert_eq!(
        archive.read_file("meshes/foo.nif").unwrap().unwrap(),
        b"mesh"
    );
    assert_eq!(
        archive.read_file("textures/bar.dds").unwrap().unwrap(),
        b"texture"
    );
    assert!(
        archive
            .info()
            .archive_flags
            .contains(ArchiveFlags::DIRECTORY_STRINGS)
    );
    assert!(
        archive
            .info()
            .archive_flags
            .contains(ArchiveFlags::FILE_STRINGS)
    );
}

#[test]
fn tes4_extract_entry_to_path_creates_parent_directories() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();
    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();
    let out = output_dir("tes4-entry-create-parent");
    let file_path = out.join("missing/parents/foo.nif");

    assert_eq!(
        archive
            .extract_entry_to_path(&archive.entries()[0], &file_path)
            .unwrap(),
        4
    );
    assert_eq!(std::fs::read(&file_path).unwrap(), b"mesh");
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn writes_hash_only_tes4_archive_from_paths() {
    let mut builder = Builder::new();
    builder.set_name_mode(NameMode::HashOnly);
    builder.add_bytes("meshes/foo/bar.nif", b"mesh").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let info = archive.info();

    assert!(!info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS));
    assert!(!info.archive_flags.contains(ArchiveFlags::FILE_STRINGS));
    assert_eq!(info.folder_names_len, 0);
    assert_eq!(info.file_names_len, 0);
    assert_eq!(archive.entries()[0].path(), None);
    assert_eq!(
        archive.read_file("meshes/foo/bar.nif").unwrap().unwrap(),
        b"mesh"
    );
    assert!(
        archive
            .get_by_hash(hash_directory(b"meshes\\foo").0, hash_file(b"bar.nif").0)
            .is_some()
    );
}

#[test]
fn extracts_hash_only_tes4_writer_output_with_path_dictionary() {
    let mut builder = Builder::new();
    builder.set_name_mode(NameMode::HashOnly);
    builder.add_bytes("meshes/foo/bar.nif", b"mesh").unwrap();
    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let out = output_dir("tes4-writer-hash-only");

    assert!(matches!(
        archive.extract_to(&out),
        Err(Error::ArchivePathsUnavailable)
    ));
    assert_eq!(
        archive
            .extract_to_with_paths(&out, [b"meshes/foo/bar.nif".as_slice()])
            .unwrap(),
        4
    );
    assert_eq!(
        std::fs::read(out.join("meshes").join("foo").join("bar.nif")).unwrap(),
        b"mesh"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn writes_embedded_name_tes4_archive_without_string_tables() {
    let mut builder = Builder::new();
    builder.set_name_mode(NameMode::Embedded);
    builder.add_bytes("meshes/foo/bar.nif", b"mesh").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let info = archive.info();

    assert!(!info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS));
    assert!(!info.archive_flags.contains(ArchiveFlags::FILE_STRINGS));
    assert!(
        info.archive_flags
            .contains(ArchiveFlags::EMBEDDED_FILE_NAMES)
    );
    assert_eq!(archive.entries()[0].path().unwrap(), "meshes\\foo\\bar.nif");
    assert_eq!(
        archive.read_file("meshes/foo/bar.nif").unwrap().unwrap(),
        b"mesh"
    );
}

#[test]
fn writes_embedded_name_tes4_archive_with_string_tables() {
    let mut builder = Builder::new();
    builder.set_name_mode(NameMode::StringsAndEmbedded);
    builder.add_bytes("meshes/foo/bar.nif", b"mesh").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let info = archive.info();

    assert!(info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS));
    assert!(info.archive_flags.contains(ArchiveFlags::FILE_STRINGS));
    assert!(
        info.archive_flags
            .contains(ArchiveFlags::EMBEDDED_FILE_NAMES)
    );
    assert_eq!(archive.entries()[0].path().unwrap(), "meshes\\foo\\bar.nif");
    assert_eq!(
        archive.read_file("meshes/foo/bar.nif").unwrap().unwrap(),
        b"mesh"
    );
}

#[test]
fn string_tables_take_precedence_over_disagreeing_embedded_names() {
    let mut builder = Builder::new();
    builder.set_name_mode(NameMode::StringsAndEmbedded);
    builder.add_bytes("meshes/foo/bar.nif", b"mesh").unwrap();
    let mut bytes = builder.to_vec().unwrap();

    let original = b"meshes\\foo\\bar.nif";
    let replacement = b"others\\foo\\bar.nif";
    let start = bytes
        .windows(original.len())
        .rposition(|window| window == original)
        .unwrap();
    bytes[start..start + original.len()].copy_from_slice(replacement);

    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.entries()[0].path().unwrap(), "meshes\\foo\\bar.nif");
    assert_eq!(
        archive.read_file("meshes/foo/bar.nif").unwrap().unwrap(),
        b"mesh"
    );
    assert!(archive.read_file("others/foo/bar.nif").unwrap().is_none());
}

#[test]
fn writes_compressed_embedded_name_tes4_archive() {
    let mut builder = Builder::new();
    builder.set_compressed(true);
    builder.set_name_mode(NameMode::Embedded);
    builder
        .add_bytes("meshes/foo/bar.nif", b"mesh mesh mesh mesh")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert!(
        archive
            .info()
            .archive_flags
            .contains(ArchiveFlags::EMBEDDED_FILE_NAMES)
    );
    assert_eq!(
        archive.read_file("meshes/foo/bar.nif").unwrap().unwrap(),
        b"mesh mesh mesh mesh"
    );
}

#[test]
fn rejects_embedded_name_writer_for_v103() {
    let mut builder = Builder::new();
    builder.set_version(ArchiveVersion::v103);
    builder.set_name_mode(NameMode::Embedded);
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    assert!(matches!(
        builder.to_vec(),
        Err(Error::NotImplemented(
            "TES4 embedded file names require version 104 or 105"
        ))
    ));
}

#[test]
fn writes_tes4_v103_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.set_version(ArchiveVersion::v103);
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v103);
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn writes_tes4_v105_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.set_version(ArchiveVersion::v105);
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v105);
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
    assert!(
        !archive
            .info()
            .archive_flags
            .contains(ArchiveFlags::COMPRESSED)
    );
}

#[test]
fn tes4_game_profiles_select_pc_archive_versions() {
    for (profile, constructor, version) in [
        (
            GameProfile::Oblivion,
            Builder::oblivion as fn() -> Builder,
            ArchiveVersion::v103,
        ),
        (
            GameProfile::Fallout3,
            Builder::fallout3 as fn() -> Builder,
            ArchiveVersion::v104,
        ),
        (
            GameProfile::FalloutNewVegas,
            Builder::fallout_new_vegas as fn() -> Builder,
            ArchiveVersion::v104,
        ),
        (
            GameProfile::SkyrimLe,
            Builder::skyrim_le as fn() -> Builder,
            ArchiveVersion::v104,
        ),
        (
            GameProfile::SkyrimSe,
            Builder::skyrim_se as fn() -> Builder,
            ArchiveVersion::v105,
        ),
    ] {
        let mut from_profile = Builder::with_profile(profile);
        let mut from_constructor = constructor();
        from_profile.add_bytes("data/file.txt", b"payload").unwrap();
        from_constructor
            .add_bytes("data/file.txt", b"payload")
            .unwrap();

        let profile_archive = Archive::from_slice(&from_profile.to_vec().unwrap()).unwrap();
        let constructor_archive = Archive::from_slice(&from_constructor.to_vec().unwrap()).unwrap();

        assert_eq!(from_profile.version(), version);
        assert_eq!(from_constructor.version(), version);
        assert_eq!(profile_archive.info().version, version);
        assert_eq!(constructor_archive.info().version, version);
        assert!(
            !profile_archive
                .info()
                .archive_flags
                .contains(ArchiveFlags::COMPRESSED)
        );
        assert!(
            profile_archive
                .info()
                .archive_flags
                .contains(ArchiveFlags::DIRECTORY_STRINGS)
        );
        assert!(
            profile_archive
                .info()
                .archive_flags
                .contains(ArchiveFlags::FILE_STRINGS)
        );
    }
}

#[test]
fn tes4_game_profile_keeps_policy_overrides_explicit() {
    let mut builder = Builder::with_profile(GameProfile::SkyrimSe);
    builder.set_compressed(true);
    builder.set_name_mode(NameMode::Embedded);
    builder
        .add_bytes("data/file.txt", b"payload payload")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let info = archive.info();

    assert_eq!(info.version, ArchiveVersion::v105);
    assert!(info.archive_flags.contains(ArchiveFlags::COMPRESSED));
    assert!(
        info.archive_flags
            .contains(ArchiveFlags::EMBEDDED_FILE_NAMES)
    );
    assert!(!info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS));
    assert!(!info.archive_flags.contains(ArchiveFlags::FILE_STRINGS));
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload payload"
    );
}

#[test]
fn tes4_set_profile_only_changes_version() {
    let mut builder = Builder::new();
    builder.set_compressed(true);
    builder.set_name_mode(NameMode::HashOnly);
    builder.set_profile(GameProfile::Oblivion);

    assert_eq!(builder.version(), ArchiveVersion::v103);
    assert!(builder.compressed());
    assert_eq!(builder.name_mode(), NameMode::HashOnly);
}

#[test]
fn writes_compressed_tes4_v104_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.set_compressed(true);
    builder
        .add_bytes("data/file.txt", b"payload payload payload")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert!(
        archive
            .info()
            .archive_flags
            .contains(ArchiveFlags::COMPRESSED)
    );
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload payload payload"
    );
}

#[test]
fn writes_compressed_tes4_v105_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.set_version(ArchiveVersion::v105);
    builder.set_compressed(true);
    builder
        .add_bytes("data/file.txt", b"payload payload payload")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v105);
    assert!(
        archive
            .info()
            .archive_flags
            .contains(ArchiveFlags::COMPRESSED)
    );
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload payload payload"
    );
}

#[test]
fn tes4_writer_can_toggle_per_file_compression() {
    let mut builder = Builder::new();
    builder.set_compressed(true);
    builder
        .add_bytes_with_compression(
            "compressed.txt",
            b"compressed payload",
            CompressionOverride::Inherit,
        )
        .unwrap();
    builder
        .add_bytes_with_compression("plain.txt", b"plain payload", CompressionOverride::Store)
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    let compressed = archive.get("compressed.txt").unwrap().file();
    let plain = archive.get("plain.txt").unwrap().file();
    assert!(!compressed.compression_toggled);
    assert!(plain.compression_toggled);
    assert_eq!(
        archive.read_file("plain.txt").unwrap().unwrap(),
        b"plain payload"
    );
    assert_eq!(
        archive.read_file("compressed.txt").unwrap().unwrap(),
        b"compressed payload"
    );
}

#[cfg(unix)]
#[test]
fn tes4_writer_add_dir_follows_file_symlinks_at_relative_path() {
    let root = output_dir("tes4-add-dir-symlink");
    let external = output_dir("tes4-add-dir-external");
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
fn tes4_writer_encodes_legacy_text_paths() {
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

#[test]
fn writes_tes4_root_folder_archive() {
    let mut builder = Builder::new();
    builder.add_bytes("file.txt", b"hello").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.entries()[0].folder().unwrap(), "");
    assert_eq!(archive.entries()[0].name().unwrap(), "file.txt");
    assert_eq!(archive.read_file("file.txt").unwrap().unwrap(), b"hello");
}

#[test]
fn tes4_writer_output_is_deterministic() {
    let mut first = Builder::new();
    first.add_bytes("b.txt", b"b").unwrap();
    first.add_bytes("a.txt", b"a").unwrap();

    let mut second = Builder::new();
    second.add_bytes("a.txt", b"a").unwrap();
    second.add_bytes("b.txt", b"b").unwrap();

    assert_eq!(first.to_vec().unwrap(), second.to_vec().unwrap());
}

#[test]
fn tes4_writer_rejects_duplicate_normalized_paths() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();

    assert!(matches!(
        builder.add_bytes("meshes\\foo.nif", b"other"),
        Err(Error::DuplicatePath)
    ));
}

#[test]
fn tes4_writer_rejects_unsafe_paths() {
    for path in ["", ".", "../evil.txt", "bad:name.txt", "bad\0name.txt"] {
        let mut builder = Builder::new();
        assert!(matches!(
            builder.add_bytes(path.as_bytes(), b"payload"),
            Err(Error::InvalidArchivePath)
        ));
    }
}

#[test]
fn tes4_extract_to_rejects_parent_directory_paths() {
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_names_and_payload(
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_names_and_payload(
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
        matches!(Archive::from_slice(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_truncated_tes4_file_name_block() {
    let mut bytes = tiny_tes4_index();
    bytes.truncate(64);
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds | Error::Io(_))
    ));
}

#[test]
fn rejects_file_name_table_that_bleeds_into_payload() {
    let mut bytes = tiny_tes4_index();
    bytes[82] = b'X';
    assert!(
        matches!(Archive::from_slice(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_folder_file_count_that_disagrees_with_header() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 20, 2);
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_folder_name_without_trailing_nul() {
    let mut bytes = tiny_tes4_index();
    bytes[57] = b'X';
    assert!(
        matches!(Archive::from_slice(&bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn rejects_folder_record_offset_before_folder_block() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 48, 40);
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_file_payload_that_overlaps_index() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 70, 36);
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn file_size_checked_flag_is_not_part_of_stored_size() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 66, 7 | (1 << 31));
    let archive = Archive::from_slice(&bytes).unwrap();
    let record = archive.entries()[0].file();
    assert_eq!(record.stored_size, 7);
    assert!(record.checked);
}

#[test]
fn entry_path_preserves_archive_spelling_while_lookup_normalizes() {
    let mut bytes = tiny_tes4_index();
    bytes[53..58].copy_from_slice(b"Data\0");
    bytes[74..83].copy_from_slice(b"File.TXT\0");
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(archive.entries()[0].path().unwrap(), "Data\\File.TXT");
    assert!(archive.get("data/file.txt").is_some());
}

#[test]
fn rejects_xmem_compressed_files_at_parse_time() {
    let mut bytes = tiny_tes4_index();
    write_u32(&mut bytes, 12, 3 | (1 << 2) | (1 << 9));

    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::NotImplemented("TES4 XMem compression"))
    ));
}

#[test]
fn embedded_name_skip_requires_compressed_size_prefix_after_name() {
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(
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
    write_u32(&mut bytes, 70, 0x0000_ffff);
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn compression_toggle_disables_archive_default_compression() {
    let mut bytes = tiny_tes4_index_with_payload(1 << 2, b"payload");
    write_u32(&mut bytes, 66, 7 | (1 << 30));
    let archive = Archive::from_slice(&bytes).unwrap();
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
    let archive = Archive::from_slice(&bytes).unwrap();
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    assert!(matches!(
        archive.read_file("data/file.txt"),
        Err(Error::TrailingCompressedData)
    ));
}

#[test]
fn zlib_writer_extraction_does_not_write_extra_probe_byte() {
    let compressed = zlib_compress(b"payload larger than declared");
    let mut payload = Vec::new();
    push_u32(&mut payload, 7);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.extract_file("data/file.txt", &mut out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn zlib_writer_extraction_rejects_trailing_data_before_writing() {
    let compressed = zlib_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    payload.push(0);
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.extract_file("data/file.txt", &mut out),
        Err(Error::TrailingCompressedData)
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn failed_decompression_does_not_leave_partial_output() {
    let compressed = zlib_compress(b"short");
    let mut payload = Vec::new();
    push_u32(&mut payload, 99);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    let mut out = b"prefix".to_vec();
    assert!(
        archive
            .read_entry_into(&archive.entries()[0], &mut out)
            .is_err()
    );
    assert_eq!(out, b"prefix");
}

#[test]
fn failed_extract_to_keeps_existing_tes4_file_and_removes_temp() {
    let compressed = zlib_compress(b"payload larger than declared");
    let mut payload = Vec::new();
    push_u32(&mut payload, 7);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    let out = output_dir("tes4-atomic-failure");
    let file_path = out.join("data").join("file.txt");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"old contents").unwrap();

    assert!(matches!(
        archive.extract_to(&out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(std::fs::read(&file_path).unwrap(), b"old contents");
    assert_no_temp_extract_files(file_path.parent().unwrap());
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn failed_extract_entry_to_path_keeps_existing_tes4_file_and_removes_temp() {
    let compressed = zlib_compress(b"payload larger than declared");
    let mut payload = Vec::new();
    push_u32(&mut payload, 7);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_payload(1 << 2, &payload)).unwrap();
    let out = output_dir("tes4-entry-atomic-failure");
    std::fs::create_dir_all(&out).unwrap();
    let file_path = out.join("file.txt");
    std::fs::write(&file_path, b"old contents").unwrap();

    assert!(matches!(
        archive.extract_entry_to_path(&archive.entries()[0], &file_path),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(std::fs::read(&file_path).unwrap(), b"old contents");
    assert_no_temp_extract_files(&out);
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn extracts_synthetic_lz4_frame_tes4_file() {
    let compressed = lz4_frame_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
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
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
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

#[test]
fn lz4_frame_writer_extraction_does_not_write_extra_probe_byte() {
    let compressed = lz4_frame_compress(b"payload larger than declared");
    let mut payload = Vec::new();
    push_u32(&mut payload, 7);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.extract_file("data/file.txt", &mut out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn lz4_frame_writer_extraction_rejects_trailing_data_before_writing() {
    let compressed = lz4_frame_compress(b"compressed payload");
    let mut payload = Vec::new();
    push_u32(&mut payload, 18);
    payload.extend_from_slice(&compressed);
    payload.push(0);
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.extract_file("data/file.txt", &mut out),
        Err(Error::TrailingCompressedData)
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn failed_lz4_extract_to_keeps_existing_tes4_file_and_removes_temp() {
    let compressed = lz4_frame_compress(b"payload larger than declared");
    let mut payload = Vec::new();
    push_u32(&mut payload, 7);
    payload.extend_from_slice(&compressed);
    let archive = Archive::from_slice(&tiny_tes4_index_with_version_and_payload(
        105,
        1 << 2,
        &payload,
    ))
    .unwrap();
    let out = output_dir("tes4-lz4-atomic-failure");
    let file_path = out.join("data").join("file.txt");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"old contents").unwrap();

    assert!(matches!(
        archive.extract_to(&out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(std::fs::read(&file_path).unwrap(), b"old contents");
    assert_no_temp_extract_files(file_path.parent().unwrap());
    std::fs::remove_dir_all(out).unwrap();
}
