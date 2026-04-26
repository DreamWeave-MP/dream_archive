use dream_archive::bsa::{FilenameEncoding, decode_filename_lossy};

#[test]
fn decodes_utf8_lossily() {
    assert_eq!(
        decode_filename_lossy(b"meshes/foo.nif", FilenameEncoding::Utf8),
        "meshes/foo.nif"
    );
    assert_eq!(
        decode_filename_lossy(b"bad\xff", FilenameEncoding::Utf8),
        "bad�"
    );
}

#[test]
fn decodes_windows_code_pages_for_display() {
    assert_eq!(
        decode_filename_lossy(b"za\xbf\xf3\xb3\xe6", FilenameEncoding::Windows1250),
        "zażółć"
    );
    assert_eq!(
        decode_filename_lossy(
            &[0xef, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2],
            FilenameEncoding::Windows1251
        ),
        "привет"
    );
    assert_eq!(
        decode_filename_lossy(b"Mar\xeda_F.fuz", FilenameEncoding::Windows1252),
        "María_F.fuz"
    );
}

#[test]
fn decodes_cp437_for_display() {
    assert_eq!(
        decode_filename_lossy(b"\x82\x94\x81", FilenameEncoding::Cp437),
        "éöü"
    );
    assert_eq!(
        decode_filename_lossy(b"box\xb3\xc4", FilenameEncoding::Cp437),
        "box│─"
    );
}
