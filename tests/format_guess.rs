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
