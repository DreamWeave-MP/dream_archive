use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder};
use std::{hint::black_box, path::PathBuf, time::Instant};

const FILE_COUNT: usize = 1_024;
const PAYLOAD_SIZE: usize = 512;
const LARGE_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;

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

fn large_payload() -> Vec<u8> {
    (0..LARGE_PAYLOAD_SIZE)
        .map(|offset| u8::try_from((offset / 4096) % 251).unwrap())
        .collect()
}

fn output_dir(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target/bench-extract");
    path.push(format!("{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
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

fn bench_writer_case<Entry>(
    name: &str,
    iterations: usize,
    entry: &Entry,
    mut extract_entry: impl FnMut(&Entry, &mut std::io::Sink) -> u64,
) {
    let start = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..iterations {
        let mut sink = std::io::sink();
        bytes += usize::try_from(black_box(extract_entry(black_box(entry), &mut sink))).unwrap();
    }
    print_throughput(name, bytes, start.elapsed());
}

fn bench_extract_to_case(
    name: &str,
    iterations: usize,
    mut extract_to: impl FnMut(&PathBuf) -> u64,
) {
    let start = Instant::now();
    let mut bytes = 0usize;
    for iteration in 0..iterations {
        let out = output_dir(&format!("{name}-{iteration}"));
        bytes += usize::try_from(black_box(extract_to(black_box(&out)))).unwrap();
        std::fs::remove_dir_all(out).unwrap();
    }
    print_throughput(name, bytes, start.elapsed());
}

fn bench_extract_entry_to_path_case<Entry>(
    name: &str,
    iterations: usize,
    entry: &Entry,
    mut extract_entry_to_path: impl FnMut(&Entry, &PathBuf) -> u64,
) {
    let start = Instant::now();
    let mut bytes = 0usize;
    for iteration in 0..iterations {
        let out = output_dir(&format!("{name}-{iteration}")).join("large.bin");
        bytes += usize::try_from(black_box(extract_entry_to_path(
            black_box(entry),
            black_box(&out),
        )))
        .unwrap();
        std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
    }
    print_throughput(name, bytes, start.elapsed());
}

fn print_throughput(name: &str, bytes: usize, elapsed: std::time::Duration) {
    let mib =
        f64::from(u32::try_from(bytes).expect("too many benchmark bytes")) / (1024.0 * 1024.0);
    println!(
        "{name:<32} {bytes:>12} bytes {:>10.3} ms {:>10.1} MiB/s",
        elapsed.as_secs_f64() * 1000.0,
        mib / elapsed.as_secs_f64()
    );
}

fn build_large_compressed_archives() -> (
    dream_archive::ba2::Archive,
    dream_archive::ba2::Archive,
    dream_archive::bsa::tes4::Archive,
) {
    let payload = large_payload();
    let mut ba2_builder = Ba2Builder::new();
    ba2_builder.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::Zip));
    ba2_builder
        .add_bytes("textures/large.bin", &payload)
        .unwrap();
    let mut ba2_lz4_builder = Ba2Builder::new();
    ba2_lz4_builder.set_version(dream_archive::ba2::ArchiveVersion::v3);
    ba2_lz4_builder.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::LZ4));
    ba2_lz4_builder
        .add_bytes("textures/large.bin", &payload)
        .unwrap();
    let mut tes4_builder = Tes4BsaBuilder::new();
    tes4_builder.set_compressed(true);
    tes4_builder
        .add_bytes("textures/large.bin", &payload)
        .unwrap();

    (
        dream_archive::ba2::Archive::from_vec(ba2_builder.to_vec().unwrap()).unwrap(),
        dream_archive::ba2::Archive::from_vec(ba2_lz4_builder.to_vec().unwrap()).unwrap(),
        dream_archive::bsa::tes4::Archive::from_vec(tes4_builder.to_vec().unwrap()).unwrap(),
    )
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

    let large_runs = std::env::var("DREAM_ARCHIVE_LARGE_EXTRACT_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let (large_ba2, large_ba2_lz4, large_tes4) = build_large_compressed_archives();
    println!(
        "large compressed bytes/file: {LARGE_PAYLOAD_SIZE}, iterations per case: {large_runs}"
    );
    bench_case(
        "ba2 large read_entry",
        large_runs,
        large_ba2.entries(),
        |entry| large_ba2.read_entry(entry).unwrap(),
    );
    bench_writer_case(
        "ba2 large extract_entry sink",
        large_runs,
        &large_ba2.entries()[0],
        |entry, sink| large_ba2.extract_entry(entry, sink).unwrap(),
    );
    bench_extract_to_case("ba2 large extract_to", large_runs, |out| {
        large_ba2.extract_to(out).unwrap()
    });
    bench_case(
        "ba2 lz4 large read_entry",
        large_runs,
        large_ba2_lz4.entries(),
        |entry| large_ba2_lz4.read_entry(entry).unwrap(),
    );
    bench_writer_case(
        "ba2 lz4 large extract_entry sink",
        large_runs,
        &large_ba2_lz4.entries()[0],
        |entry, sink| large_ba2_lz4.extract_entry(entry, sink).unwrap(),
    );
    bench_extract_entry_to_path_case(
        "ba2 lz4 large extract_entry_to_path",
        large_runs,
        &large_ba2_lz4.entries()[0],
        |entry, out| large_ba2_lz4.extract_entry_to_path(entry, out).unwrap(),
    );
    bench_case(
        "tes4 large read_entry",
        large_runs,
        large_tes4.entries(),
        |entry| large_tes4.read_entry(entry).unwrap(),
    );
    bench_writer_case(
        "tes4 large extract_entry sink",
        large_runs,
        &large_tes4.entries()[0],
        |entry, sink| large_tes4.extract_entry(entry, sink).unwrap(),
    );
    bench_extract_to_case("tes4 large extract_to", large_runs, |out| {
        large_tes4.extract_to(out).unwrap()
    });
}
