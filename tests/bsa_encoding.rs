use dream_archive::bsa::{FilenameEncoding, decode_filename_lossy, encode_filename};

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

#[test]
fn encodes_utf8_without_copying() {
    let encoded = encode_filename("meshes/foo.nif", FilenameEncoding::Utf8).unwrap();
    assert!(matches!(encoded, std::borrow::Cow::Borrowed(_)));
    assert_eq!(encoded.as_ref(), b"meshes/foo.nif");
}

#[test]
fn encodes_windows_code_pages_losslessly() {
    assert_eq!(
        encode_filename("zażółć", FilenameEncoding::Windows1250)
            .unwrap()
            .as_ref(),
        b"za\xbf\xf3\xb3\xe6"
    );
    assert_eq!(
        encode_filename("привет", FilenameEncoding::Windows1251)
            .unwrap()
            .as_ref(),
        [0xef, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2]
    );
    assert_eq!(
        encode_filename("María_F.fuz", FilenameEncoding::Windows1252)
            .unwrap()
            .as_ref(),
        b"Mar\xeda_F.fuz"
    );
}

#[test]
fn encodes_cp437_losslessly() {
    assert_eq!(
        encode_filename("éöü", FilenameEncoding::Cp437)
            .unwrap()
            .as_ref(),
        b"\x82\x94\x81"
    );
    assert_eq!(
        encode_filename("box│─", FilenameEncoding::Cp437)
            .unwrap()
            .as_ref(),
        b"box\xb3\xc4"
    );
}

#[test]
fn cp437_extended_bytes_roundtrip() {
    for byte in 0x80..=0xff {
        let bytes = [byte];
        let decoded = decode_filename_lossy(&bytes, FilenameEncoding::Cp437);
        let encoded = encode_filename(&decoded, FilenameEncoding::Cp437).unwrap();
        assert_eq!(encoded.as_ref(), &[byte]);
    }
}

#[test]
fn rejects_unrepresentable_text_without_replacement() {
    let error = encode_filename("你好", FilenameEncoding::Windows1251).unwrap_err();
    assert_eq!(error.encoding(), FilenameEncoding::Windows1251);
    let error = encode_filename("✓", FilenameEncoding::Cp437).unwrap_err();
    assert_eq!(error.encoding(), FilenameEncoding::Cp437);
}
