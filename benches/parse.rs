use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder};
use std::{hint::black_box, time::Instant};

const FILE_COUNT: usize = 10_000;

fn iterations() -> usize {
    std::env::var("DREAM_ARCHIVE_PARSE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100)
}

fn build_archives() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut ba2 = Ba2Builder::new();
    let mut tes3 = Tes3BsaBuilder::new();
    let mut tes4 = Tes4BsaBuilder::new();
    let mut tes4_hash_only = Tes4BsaBuilder::new();
    tes4_hash_only.set_name_mode(dream_archive::bsa::tes4::NameMode::HashOnly);

    for index in 0..FILE_COUNT {
        let path = format!("meshes/folder{:03}/file{index:05}.nif", index % 128);
        let payload = [u8::try_from(index % 251).unwrap(); 16];
        ba2.add_bytes(&path, payload).unwrap();
        tes3.add_bytes(&path, payload).unwrap();
        tes4.add_bytes(&path, payload).unwrap();
        tes4_hash_only.add_bytes(&path, payload).unwrap();
    }

    (
        ba2.to_vec().unwrap(),
        tes3.to_vec().unwrap(),
        tes4.to_vec().unwrap(),
        tes4_hash_only.to_vec().unwrap(),
    )
}

fn bench_case(name: &str, iterations: usize, bytes: &[u8], mut parse: impl FnMut(&[u8]) -> usize) {
    let start = Instant::now();
    let mut entries = 0usize;
    for _ in 0..iterations {
        entries += black_box(parse(black_box(bytes)));
    }
    let elapsed = start.elapsed();
    println!(
        "{name:<24} {iterations:>8} parses {entries:>10} entries {:>10.3} ms {:>10.1} parses/s",
        elapsed.as_secs_f64() * 1000.0,
        f64::from(u32::try_from(iterations).expect("too many benchmark iterations"))
            / elapsed.as_secs_f64()
    );
}

fn main() {
    let runs = iterations();
    let (ba2, tes3, tes4, tes4_hash_only) = build_archives();
    println!("files: {FILE_COUNT}, iterations per case: {runs}");
    bench_case("ba2 parse", runs, &ba2, |bytes| {
        dream_archive::ba2::Archive::from_slice(bytes)
            .unwrap()
            .len()
    });
    bench_case("tes3 parse", runs, &tes3, |bytes| {
        dream_archive::bsa::tes3::Archive::from_slice(bytes)
            .unwrap()
            .len()
    });
    bench_case("tes4 strings parse", runs, &tes4, |bytes| {
        dream_archive::bsa::tes4::Archive::from_slice(bytes)
            .unwrap()
            .len()
    });
    bench_case("tes4 hash-only parse", runs, &tes4_hash_only, |bytes| {
        dream_archive::bsa::tes4::Archive::from_slice(bytes)
            .unwrap()
            .len()
    });
}
