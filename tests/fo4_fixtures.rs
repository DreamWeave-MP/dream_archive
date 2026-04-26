mod common;

use dream_archive::{
    FileFormat,
    fo4::{Archive, Error, Format, Version},
};
use std::{fs, io::Read as _};
use walkdir::WalkDir;

#[test]
fn invalid_headers_match_expected_errors() {
    let root = common::fixture("bsa-rs/data/fo4_invalid_test");
    assert!(matches!(
        Archive::open_path(root.join("invalid_magic.ba2")),
        Err(Error::InvalidMagic(_))
    ));
    assert!(matches!(
        Archive::open_path(root.join("invalid_format.ba2")),
        Err(Error::InvalidFormat(_))
    ));
    assert!(matches!(
        Archive::open_path(root.join("invalid_version.ba2")),
        Err(Error::InvalidVersion(0x101))
    ));
    assert!(matches!(
        Archive::open_path(root.join("invalid_sentinel.ba2")),
        Err(Error::InvalidChunkSentinel(0xDEAD_BEEF))
    ));
    assert!(matches!(
        Archive::open_path(root.join("invalid_size.ba2")),
        Err(Error::InvalidChunkSize(0xCCCC))
    ));
}

#[test]
fn missing_string_tables_are_accepted() {
    let root = common::fixture("bsa-rs/data/fo4_missing_string_table_test");
    let archive = Archive::open_path(root.join("in.ba2")).unwrap();
    assert_eq!(archive.options().format, Format::GNRL);
    let entry = archive.get("misc/example.txt").unwrap();
    assert!(entry.name().is_empty());
    assert_eq!(
        archive.read_entry(entry).unwrap(),
        fs::read(root.join("data/misc/example.txt")).unwrap()
    );
}

#[test]
fn lists_names_for_vfs_indexing() {
    let root = common::fixture("bsa-rs/data/fo4_next_gen_test");
    let archive = Archive::open_path(root.join("gnrl_v8.ba2")).unwrap();
    let names: Vec<_> = archive
        .entries()
        .iter()
        .map(|entry| entry.name().to_string())
        .collect();
    assert_eq!(names, ["License.txt", "SampleA.png"]);
    assert!(archive.contains("license.txt"));
    assert!(archive.contains("SampleA.png"));
}

#[test]
fn reads_compressed_general_archives() {
    let root = common::fixture("bsa-rs/data/fo4_compression_test");
    for archive_name in ["normal.ba2", "xbox.ba2"] {
        let archive = Archive::open_path(root.join(archive_name)).unwrap();
        assert_eq!(archive.options().format, Format::GNRL);
        for item in WalkDir::new(root.join("data"))
            .into_iter()
            .filter_map(Result::ok)
        {
            if !item.file_type().is_file() {
                continue;
            }
            let rel = item.path().strip_prefix(root.join("data")).unwrap();
            let rel = rel.to_string_lossy().replace('/', "\\");
            assert_eq!(
                archive.read_file(rel.as_bytes()).unwrap().unwrap(),
                fs::read(item.path()).unwrap()
            );
        }
    }
}

#[test]
fn reconstructs_dx10_dds() {
    let root = common::fixture("bsa-rs/data/fo4_dds_test");
    let archive = Archive::open_path(root.join("in.ba2")).unwrap();
    assert_eq!(archive.options().format, Format::DX10);
    let data = archive
        .read_file("Fence006_1K_Roughness.dds")
        .unwrap()
        .unwrap();
    let expected = fs::read(root.join("Fence006_1K_Roughness.dds")).unwrap();
    assert_eq!(data.len(), expected.len());
    assert_eq!(&data[148..], &expected[148..]);
    assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(data[128..132].try_into().unwrap()), 98);
    assert_eq!(u32::from_le_bytes(data[132..136].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(data[68..72].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(data[144..148].try_into().unwrap()), 0);
}

#[test]
fn reconstructs_cubemap_dds() {
    let root = common::fixture("bsa-rs/data/fo4_cubemap_test");
    let archive = Archive::open_path(root.join("in.ba2")).unwrap();
    let data = archive.read_file("blacksky_e.dds").unwrap().unwrap();
    assert_eq!(data, fs::read(root.join("blacksky_e.dds")).unwrap());
}

#[test]
fn next_gen_versions_are_accepted() {
    let root = common::fixture("bsa-rs/data/fo4_next_gen_test");
    for (path, format, version) in [
        ("gnrl_v7.ba2", Format::GNRL, Version::v7),
        ("gnrl_v8.ba2", Format::GNRL, Version::v8),
        ("dx10_v7.ba2", Format::DX10, Version::v7),
        ("dx10_v8.ba2", Format::DX10, Version::v8),
    ] {
        let archive = Archive::open_path(root.join(path)).unwrap();
        assert_eq!(archive.options().format, format);
        assert_eq!(archive.options().version, version);
    }
}

#[test]
fn next_gen_dx10_extracts_bsa_rs_compatible_dds() {
    let root = common::fixture("bsa-rs/data/fo4_next_gen_test");
    let expected = fs::read(root.join("dx10/Fence006_1K_Roughness.dds")).unwrap();
    for archive_name in ["dx10_v7.ba2", "dx10_v8.ba2"] {
        let archive = Archive::open_path(root.join(archive_name)).unwrap();
        let data = archive
            .read_file("Fence006_1K_Roughness.dds")
            .unwrap()
            .unwrap();
        assert_eq!(data.len(), expected.len());
        assert_eq!(&data[148..], &expected[148..]);
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(data[68..72].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(data[144..148].try_into().unwrap()), 0);
    }
}

#[test]
fn guessed_format_is_btdx() {
    let mut file =
        fs::File::open(common::fixture("bsa-rs/data/common_guess_test/fo4.ba2")).unwrap();
    assert_eq!(
        dream_archive::guess_format(&mut file).unwrap(),
        Some(FileFormat::FO4)
    );
    let mut rest = Vec::new();
    file.read_to_end(&mut rest).unwrap();
    assert!(!rest.is_empty());
}

#[test]
fn guess_format_is_weak_and_consumes_magic() {
    let mut bytes = &b"BTDX this is not a real archive"[..];
    assert_eq!(
        dream_archive::guess_format(&mut bytes).unwrap(),
        Some(FileFormat::FO4)
    );
    assert_eq!(bytes, b" this is not a real archive");
}
