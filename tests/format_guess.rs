use dream_archive::FileFormat;

#[test]
fn ba2_guess_respects_feature_gate() {
    let mut bytes = &b"BTDXpayload"[..];
    let expected = if cfg!(feature = "ba2") {
        Some(FileFormat::BA2)
    } else {
        None
    };
    assert_eq!(dream_archive::guess_format(&mut bytes).unwrap(), expected);
}

#[test]
fn bsa_guess_respects_feature_gates() {
    let mut tes3 = &b"\0\x01\0\0payload"[..];
    let expected_tes3 = if cfg!(feature = "bsa-tes3") {
        Some(FileFormat::BSA(dream_archive::BsaFormat::TES3))
    } else {
        None
    };
    assert_eq!(
        dream_archive::guess_format(&mut tes3).unwrap(),
        expected_tes3
    );

    let mut tes4 = &b"BSA\0payload"[..];
    let expected_tes4 = if cfg!(feature = "bsa-tes4") {
        Some(FileFormat::BSA(dream_archive::BsaFormat::TES4))
    } else {
        None
    };
    assert_eq!(
        dream_archive::guess_format(&mut tes4).unwrap(),
        expected_tes4
    );
}

#[cfg(feature = "ba2")]
#[test]
fn top_level_archive_reads_ba2_from_vec() {
    let mut builder = dream_archive::Ba2Builder::new();
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    let archive = dream_archive::Archive::from_vec(builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.format(), FileFormat::BA2);
    assert_eq!(archive.len(), 1);
    assert_eq!(
        archive.read_file_required("DATA/FILE.TXT").unwrap(),
        b"payload"
    );
}

#[cfg(feature = "bsa-tes3")]
#[test]
fn top_level_archive_reads_tes3_bsa_from_vec() {
    let mut builder = dream_archive::Tes3BsaBuilder::new();
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    let archive = dream_archive::Archive::from_vec(builder.to_vec().unwrap()).unwrap();

    assert_eq!(
        archive.format(),
        FileFormat::BSA(dream_archive::BsaFormat::TES3)
    );
    assert_eq!(archive.len(), 1);
    assert_eq!(
        archive.read_file_required("DATA/FILE.TXT").unwrap(),
        b"payload"
    );
}

#[cfg(feature = "bsa-tes4")]
#[test]
fn top_level_archive_reads_tes4_bsa_from_vec() {
    let mut builder = dream_archive::Tes4BsaBuilder::new();
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    let archive = dream_archive::Archive::from_vec(builder.to_vec().unwrap()).unwrap();

    assert_eq!(
        archive.format(),
        FileFormat::BSA(dream_archive::BsaFormat::TES4)
    );
    assert_eq!(archive.len(), 1);
    assert_eq!(
        archive.read_file_required("DATA/FILE.TXT").unwrap(),
        b"payload"
    );
}

#[cfg(feature = "ba2")]
#[test]
fn top_level_required_read_reports_missing_member() {
    let mut builder = dream_archive::Ba2Builder::new();
    builder.add_bytes("data/file.txt", b"payload").unwrap();
    let archive = dream_archive::Archive::from_vec(builder.to_vec().unwrap()).unwrap();

    assert!(matches!(
        archive.read_file_required("missing.txt"),
        Err(dream_archive::Error::FileNotFound(path)) if path.as_slice() == b"missing.txt"
    ));
}
