use dream_archive::bsa::{tes3, tes4};

#[test]
fn tes3_hash_matches_reference_vectors() {
    let hash = |path: &[u8]| tes3::hash_file(path).0.numeric();
    assert_eq!(
        hash(b"meshes/c/artifact_bloodring_01.nif"),
        0x1c3c_1149_920d_5f0c
    );
    assert_eq!(
        hash(b"meshes/x/ex_stronghold_pylon00.nif"),
        0x2025_0749_accc_d202
    );
    assert_eq!(
        hash(b"meshes/r/xsteam_centurions.kf"),
        0x6e5c_0f31_2507_2ea6
    );
    assert_eq!(
        hash(b"textures/tx_rock_cave_mu_01.dds"),
        0x5806_0c2f_a3d8_f759
    );
    assert_eq!(
        hash(b"meshes/f/furn_ashl_chime_02.nif"),
        0x7c3b_2f3a_bffc_8611
    );
    assert_eq!(hash(b"textures/tx_rope_woven.dds"), 0x5865_632f_0c05_2c64);
    assert_eq!(hash(b"icons/a/tx_templar_skirt.dds"), 0x4651_2a0b_60ed_a673);
    assert_eq!(hash(b"icons/m/misc_prongs00.dds"), 0x5171_5677_bba8_37d3);
    assert_eq!(
        hash(b"meshes/i/in_c_stair_plain_tall_02.nif"),
        0x2a32_4956_bf89_b1c9
    );
    assert_eq!(hash(b"meshes/r/xkwama worker.nif"), 0x6d44_6e35_2c3f_5a1e);
}

#[test]
fn tes3_hash_returns_archive_normalized_path() {
    let (forward, normalized) = tes3::hash_file(b"/FOO/bar/");
    let (backward, _) = tes3::hash_file(b"foo\\BAR");
    assert_eq!(forward, backward);
    assert_eq!(normalized, b"foo\\bar");
}

#[test]
fn tes4_directory_hash_matches_reference_vectors() {
    let hash = |path: &[u8]| tes4::hash_directory(path).0.numeric();
    assert_eq!(
        hash(b"textures/armor/amuletsandrings/elder council"),
        0x04bc_422c_742c_696c
    );
    assert_eq!(
        hash(b"sound/voice/skyrim.esm/maleuniquedbguardian"),
        0x5940_85ac_732b_616e
    );
    assert_eq!(
        hash(b"textures/architecture/windhelm"),
        0xc1d9_7ebe_741e_6c6d
    );
}

#[test]
fn tes4_file_hash_matches_reference_vectors() {
    let hash = |path: &[u8]| tes4::hash_file(path).0.numeric();
    assert_eq!(
        hash(b"darkbrotherhood__0007469a_1.fuz"),
        0x011f_11b0_641b_5f31
    );
    assert_eq!(hash(b"elder_council_amulet_n.dds"), 0xdc53_1e2f_6516_dfee);
    assert_eq!(
        hash(b"testtoddquest_testtoddhappy_00027fa2_1.mp3"),
        0xde03_01ee_7426_5f31
    );
    assert_eq!(hash(b"Mar\xEDa_F.fuz"), 0x690e_0782_6d07_5f66);
}

#[test]
fn tes4_hashes_preserve_bethesda_tool_oddities() {
    let empty = tes4::hash_directory(b"");
    let current = tes4::hash_directory(b".");
    assert_eq!(empty, current);

    let gitignore = tes4::hash_file(b".gitignore").0;
    let gitmodules = tes4::hash_file(b".gitmodules").0;
    assert_eq!(gitignore, gitmodules);
    assert_eq!(gitignore.numeric(), 0);

    let with_parent = tes4::hash_file(b"users/john/test.txt").0;
    let file_only = tes4::hash_file(b"test.txt").0;
    assert_eq!(with_parent, file_only);
}
