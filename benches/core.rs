//! The Rust core's hot paths: path lookups and payload decoding per archive family.

#![allow(clippy::missing_panics_doc, clippy::semicolon_if_nothing_returned)]

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dream_archive::bsa::tes4::NameMode;
use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder};

const ENTRY_COUNT: usize = 4096;
const BIG_PAYLOAD: usize = 64 * 1024;

fn entry_path(index: usize) -> String {
    format!("meshes/folder{:03}/file{index:05}.nif", index % 128)
}

fn mixed_case(path: &str) -> String {
    path.bytes()
        .enumerate()
        .map(|(index, byte)| match byte {
            b'/' if index % 2 == 0 => '\\',
            b'a'..=b'z' if index % 3 == 0 => byte.to_ascii_uppercase() as char,
            _ => byte as char,
        })
        .collect()
}

fn lookups(c: &mut Criterion) {
    let paths: Vec<String> = (0..ENTRY_COUNT).map(entry_path).collect();
    let mixed: Vec<String> = paths.iter().map(|path| mixed_case(path)).collect();
    let misses: Vec<String> = paths.iter().map(|path| format!("missing/{path}")).collect();
    let mut ba2 = Ba2Builder::new();
    let mut tes3 = Tes3BsaBuilder::new();
    let mut tes4 = Tes4BsaBuilder::new();
    let mut tes4_hash_only = Tes4BsaBuilder::new();
    tes4_hash_only.set_name_mode(NameMode::HashOnly);
    for path in &paths {
        ba2.add_bytes(path, b"payload").unwrap();
        tes3.add_bytes(path, b"payload").unwrap();
        tes4.add_bytes(path, b"payload").unwrap();
        tes4_hash_only.add_bytes(path, b"payload").unwrap();
    }
    let ba2 = dream_archive::ba2::Archive::from_vec(ba2.to_vec().unwrap()).unwrap();
    let tes3 = dream_archive::bsa::tes3::Archive::from_vec(tes3.to_vec().unwrap()).unwrap();
    let tes4 = dream_archive::bsa::tes4::Archive::from_vec(tes4.to_vec().unwrap()).unwrap();
    let tes4_hash_only =
        dream_archive::bsa::tes4::Archive::from_vec(tes4_hash_only.to_vec().unwrap()).unwrap();

    let mut group = c.benchmark_group("contains");
    group.throughput(Throughput::Elements(ENTRY_COUNT as u64));
    let cases: [(&str, &[String]); 3] = [("hit", &paths), ("mixed", &mixed), ("miss", &misses)];
    for (case, paths) in cases {
        group.bench_function(format!("ba2/{case}"), |b| {
            b.iter(|| {
                paths
                    .iter()
                    .filter(|path| ba2.contains(black_box(path)))
                    .count()
            })
        });
        group.bench_function(format!("tes3/{case}"), |b| {
            b.iter(|| {
                paths
                    .iter()
                    .filter(|path| tes3.contains(black_box(path)))
                    .count()
            })
        });
        group.bench_function(format!("tes4/{case}"), |b| {
            b.iter(|| {
                paths
                    .iter()
                    .filter(|path| tes4.contains(black_box(path)))
                    .count()
            })
        });
        group.bench_function(format!("tes4_hash_only/{case}"), |b| {
            b.iter(|| {
                paths
                    .iter()
                    .filter(|path| tes4_hash_only.contains(black_box(path)))
                    .count()
            })
        });
    }
    group.finish();

    // One hot path repeated: what a script loop pays per call without cache misses.
    let mut group = c.benchmark_group("contains_one");
    let hit = entry_path(7);
    let miss = "missing/nothing.nif".to_owned();
    for (case, path) in [("hit", &hit), ("miss", &miss)] {
        group.bench_function(format!("ba2/{case}"), |b| {
            b.iter(|| ba2.contains(black_box(path)))
        });
        group.bench_function(format!("tes3/{case}"), |b| {
            b.iter(|| tes3.contains(black_box(path)))
        });
        group.bench_function(format!("tes4/{case}"), |b| {
            b.iter(|| tes4.contains(black_box(path)))
        });
    }
    group.finish();
}

fn decode(c: &mut Criterion) {
    let big: Vec<u8> = (0..BIG_PAYLOAD)
        .map(|i| u8::try_from((i * 7 + i / 13) % 251).unwrap())
        .collect();
    let small = b"tiny payload!!!".to_vec();
    let mut tes3 = Tes3BsaBuilder::new();
    let mut tes4 = Tes4BsaBuilder::new();
    let mut tes4_zlib = Tes4BsaBuilder::new();
    tes4_zlib.set_compressed(true);
    let mut tes4_lz4 = Tes4BsaBuilder::skyrim_se();
    tes4_lz4.set_compressed(true);
    let mut ba2 = Ba2Builder::new();
    let mut ba2_zlib = Ba2Builder::new();
    ba2_zlib.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::Zip));
    let mut ba2_lz4 = Ba2Builder::new();
    ba2_lz4.set_version(dream_archive::ba2::ArchiveVersion::v3);
    ba2_lz4.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::LZ4));
    for (path, payload) in [("big.bin", &big), ("small.bin", &small)] {
        tes3.add_bytes(path, payload).unwrap();
        tes4.add_bytes(path, payload).unwrap();
        tes4_zlib.add_bytes(path, payload).unwrap();
        tes4_lz4.add_bytes(path, payload).unwrap();
        ba2.add_bytes(path, payload).unwrap();
        ba2_zlib.add_bytes(path, payload).unwrap();
        ba2_lz4.add_bytes(path, payload).unwrap();
    }
    let archives = [
        (
            "tes3",
            dream_archive::Archive::from_vec(tes3.to_vec().unwrap()).unwrap(),
        ),
        (
            "tes4",
            dream_archive::Archive::from_vec(tes4.to_vec().unwrap()).unwrap(),
        ),
        (
            "tes4_zlib",
            dream_archive::Archive::from_vec(tes4_zlib.to_vec().unwrap()).unwrap(),
        ),
        (
            "tes4_lz4",
            dream_archive::Archive::from_vec(tes4_lz4.to_vec().unwrap()).unwrap(),
        ),
        (
            "ba2",
            dream_archive::Archive::from_vec(ba2.to_vec().unwrap()).unwrap(),
        ),
        (
            "ba2_zlib",
            dream_archive::Archive::from_vec(ba2_zlib.to_vec().unwrap()).unwrap(),
        ),
        (
            "ba2_lz4",
            dream_archive::Archive::from_vec(ba2_lz4.to_vec().unwrap()).unwrap(),
        ),
    ];
    let mut group = c.benchmark_group("read_file");
    for (name, archive) in &archives {
        let mut out = Vec::with_capacity(BIG_PAYLOAD);
        group.throughput(Throughput::Bytes(BIG_PAYLOAD as u64));
        group.bench_function(format!("{name}/64k"), |b| {
            b.iter(|| {
                out.clear();
                archive
                    .extract_file(black_box("big.bin"), &mut out)
                    .unwrap()
            })
        });
        group.throughput(Throughput::Bytes(small.len() as u64));
        group.bench_function(format!("{name}/small"), |b| {
            b.iter(|| {
                out.clear();
                archive
                    .extract_file(black_box("small.bin"), &mut out)
                    .unwrap()
            })
        });
    }
    group.finish();
}

fn configure() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
        .sample_size(20)
}

criterion_group! { name = benches; config = configure(); targets = lookups, decode }
criterion_main!(benches);
