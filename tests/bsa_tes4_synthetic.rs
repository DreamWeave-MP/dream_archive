#![cfg(feature = "bsa-tes4")]

use dream_archive::bsa::{Archive, ArchiveVersion, Error};

const MAGIC: u32 = u32::from_le_bytes(*b"BSA\0");
const HEADER_SIZE: u32 = 0x24;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn tiny_tes4_header(version: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, version);
    push_u32(&mut bytes, HEADER_SIZE);
    push_u32(&mut bytes, 3);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 2);
    push_u32(&mut bytes, 4);
    push_u32(&mut bytes, 16);
    push_u16(&mut bytes, 1 << 8);
    push_u16(&mut bytes, 0);
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
