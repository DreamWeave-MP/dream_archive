#![cfg(feature = "bsa-tes4")]

use dream_archive::bsa::{Archive, ArchiveFlags, ArchiveTypes, ArchiveVersion, Error};
use std::path::PathBuf;

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[test]
fn parses_tes4_header_metadata() {
    let archive =
        Archive::open_path(fixture("tests/fixtures/bsa/tes4/valid/test_104.bsa")).unwrap();
    let info = archive.info();

    assert_eq!(info.version, ArchiveVersion::v104);
    assert_eq!(info.folder_record_offset, 0x24);
    assert_eq!(info.folder_count, 1);
    assert_eq!(info.file_count, 2);
    assert_eq!(info.folder_names_len, 2);
    assert_eq!(info.file_names_len, 24);
    assert!(info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS));
    assert!(info.archive_flags.contains(ArchiveFlags::FILE_STRINGS));
    assert!(info.archive_flags.contains(ArchiveFlags::COMPRESSED));
    assert!(
        info.archive_flags
            .contains(ArchiveFlags::EMBEDDED_FILE_NAMES)
    );
    assert!(info.archive_types.contains(ArchiveTypes::MISC));
    assert_eq!(archive.len(), 2);
    assert!(!archive.is_empty());
    assert_eq!(archive.archive_size(), 50_866);

    let paths: Vec<_> = archive
        .entries()
        .iter()
        .map(|entry| entry.path().to_string())
        .collect();
    assert_eq!(paths, ["preview.png", "license.txt"]);
    let preview = archive.get("PREVIEW.PNG").unwrap();
    assert_eq!(preview.name(), "preview.png");
    assert_eq!(preview.folder(), "");
    assert!(preview.file().is_compressed(info.archive_flags));
    assert_eq!(preview.file().data_offset, 111);
}

#[test]
fn invalid_tes4_headers_match_expected_errors() {
    let root = fixture("tests/fixtures/bsa/tes4/invalid");
    assert!(matches!(
        Archive::open_path(root.join("invalid_magic.bsa")),
        Err(Error::InvalidMagic(_))
    ));
    assert!(matches!(
        Archive::open_path(root.join("invalid_version.bsa")),
        Err(Error::InvalidVersion(42))
    ));
    assert!(matches!(
        Archive::open_path(root.join("invalid_size.bsa")),
        Err(Error::InvalidHeaderSize(204))
    ));
}
