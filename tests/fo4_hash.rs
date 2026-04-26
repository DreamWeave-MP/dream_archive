mod common;

use bstr::ByteSlice as _;
use dream_archive::fo4::{Archive, hash_file};

#[test]
fn hashes_match_known_ba2_values() {
    let hash = hash_file(
        b"Textures\\CreationClub\\BGSFO4001\\AnimObjects\\PipBoy\\PipBoy02(Black)_d.DDS".as_bstr(),
    )
    .0;
    assert_eq!(hash.file, 0x69E1_E82C);
    assert_eq!(hash.extension, 0x0073_6464);
    assert_eq!(hash.directory, 0x2315_7A84);

    let hash = hash_file(
        b"Materials/CreationClub/BGSFO4003/AnimObjects/PipBoy/PipBoyLabels01(Camo01).BGSM"
            .as_bstr(),
    )
    .0;
    assert_eq!(hash.file, 0x0785_843B);
    assert_eq!(hash.extension, 0x6D73_6762);
    assert_eq!(hash.directory, 0x8183_74CC);
}

#[test]
fn hash_normalization_edges_are_stable() {
    let (hash_a, normalized_a) = hash_file(b"/Textures/Foo/Bar.DDS\\".as_bstr());
    let (hash_b, normalized_b) = hash_file(b"textures\\foo\\bar.dds".as_bstr());
    assert_eq!(hash_a, hash_b);
    assert_eq!(normalized_a.as_bstr(), b"textures\\foo\\bar.dds".as_bstr());
    assert_eq!(normalized_b.as_bstr(), b"textures\\foo\\bar.dds".as_bstr());

    let (hash_non_ascii, normalized_non_ascii) =
        hash_file(b"Sound/Voice/Fallout4.esm/RobotMrHandy/Mar\xEDa_M.fuz".as_bstr());
    let (hash_ascii_removed, _) =
        hash_file(b"Sound/Voice/Fallout4.esm/RobotMrHandy/Mara_M.fuz".as_bstr());
    assert_eq!(hash_non_ascii, hash_ascii_removed);
    assert_eq!(
        normalized_non_ascii.as_bstr(),
        b"sound\\voice\\fallout4.esm\\robotmrhandy\\mar\xEDa_m.fuz".as_bstr()
    );

    let (_, normalized_empty) = hash_file(b"".as_bstr());
    assert_eq!(normalized_empty.as_bstr(), b".".as_bstr());
}

#[test]
fn path_lookup_uses_normalized_hashes() {
    let archive =
        Archive::open_path(common::fixture("bsa-rs/data/fo4_next_gen_test/gnrl_v8.ba2")).unwrap();
    assert!(archive.contains("/LICENSE.TXT\\"));
    assert!(archive.contains("samplea.png"));
    assert!(archive.contains("SampleA.PNG"));
}
