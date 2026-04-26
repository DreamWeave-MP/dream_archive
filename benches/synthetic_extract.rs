use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder};
use std::{hint::black_box, time::Instant};

const FILE_COUNT: usize = 1_024;
const PAYLOAD_SIZE: usize = 512;

fn iterations() -> usize {
    std::env::var("DREAM_ARCHIVE_EXTRACT_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50)
}

fn payload(index: usize) -> Vec<u8> {
    (0..PAYLOAD_SIZE)
        .map(|offset| u8::try_from((index + offset) % 251).unwrap())
        .collect()
}

fn build_archives() -> (
    dream_archive::ba2::Archive,
    dream_archive::bsa::tes3::Archive,
    dream_archive::bsa::tes4::Archive,
    dream_archive::bsa::tes4::Archive,
) {
    let mut ba2_builder = Ba2Builder::new();
    ba2_builder.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::Zip));
    let mut tes3_builder = Tes3BsaBuilder::new();
    let mut tes4_builder = Tes4BsaBuilder::new();
    let mut tes4_compressed_builder = Tes4BsaBuilder::new();
    tes4_compressed_builder.set_compressed(true);

    for index in 0..FILE_COUNT {
        let path = format!("meshes/folder{:03}/file{:05}.nif", index % 64, index);
        let payload = payload(index);
        ba2_builder.add_bytes(&path, &payload).unwrap();
        tes3_builder.add_bytes(&path, &payload).unwrap();
        tes4_builder.add_bytes(&path, &payload).unwrap();
        tes4_compressed_builder.add_bytes(&path, &payload).unwrap();
    }

    (
        dream_archive::ba2::Archive::from_vec(ba2_builder.to_vec().unwrap()).unwrap(),
        dream_archive::bsa::tes3::Archive::from_vec(tes3_builder.to_vec().unwrap()).unwrap(),
        dream_archive::bsa::tes4::Archive::from_vec(tes4_builder.to_vec().unwrap()).unwrap(),
        dream_archive::bsa::tes4::Archive::from_vec(tes4_compressed_builder.to_vec().unwrap())
            .unwrap(),
    )
}

fn bench_case<Entry>(
    name: &str,
    iterations: usize,
    entries: &[Entry],
    mut read_entry: impl FnMut(&Entry) -> Vec<u8>,
) {
    let start = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..iterations {
        for entry in entries {
            bytes += black_box(read_entry(black_box(entry))).len();
        }
    }
    let elapsed = start.elapsed();
    let mib =
        f64::from(u32::try_from(bytes).expect("too many benchmark bytes")) / (1024.0 * 1024.0);
    println!(
        "{name:<24} {bytes:>12} bytes {:>10.3} ms {:>10.1} MiB/s",
        elapsed.as_secs_f64() * 1000.0,
        mib / elapsed.as_secs_f64()
    );
}

fn main() {
    let runs = iterations();
    let (ba2, tes3, tes4, tes4_compressed) = build_archives();

    println!(
        "files: {FILE_COUNT}, payload bytes/file: {PAYLOAD_SIZE}, iterations per case: {runs}"
    );
    bench_case("ba2 gnrl zlib", runs, ba2.entries(), |entry| {
        ba2.read_entry(entry).unwrap()
    });
    bench_case("tes3 stored", runs, tes3.entries(), |entry| {
        tes3.read_entry(entry).unwrap()
    });
    bench_case("tes4 stored", runs, tes4.entries(), |entry| {
        tes4.read_entry(entry).unwrap()
    });
    bench_case("tes4 zlib", runs, tes4_compressed.entries(), |entry| {
        tes4_compressed.read_entry(entry).unwrap()
    });
}
