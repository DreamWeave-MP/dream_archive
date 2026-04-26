use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder};
use std::{hint::black_box, time::Instant};

const FILE_COUNT: usize = 4_096;

fn iterations() -> usize {
    std::env::var("DREAM_ARCHIVE_LOOKUP_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200)
}

fn build_paths() -> Vec<String> {
    (0..FILE_COUNT)
        .map(|index| format!("meshes/folder{:03}/file{:05}.nif", index % 128, index))
        .collect()
}

fn mixed_case_path(path: &str) -> String {
    path.bytes()
        .enumerate()
        .map(|(index, byte)| match byte {
            b'/' if index % 2 == 0 => '\\',
            b'a'..=b'z' if index % 3 == 0 => byte.to_ascii_uppercase() as char,
            _ => byte as char,
        })
        .collect()
}

fn bench_case(
    name: &str,
    iterations: usize,
    paths: &[String],
    mut contains: impl FnMut(&[u8]) -> bool,
) {
    let lookups = iterations
        .checked_mul(paths.len())
        .expect("lookup benchmark iteration count overflowed");
    let lookups_f64 = f64::from(u32::try_from(lookups).expect("too many benchmark lookups"));
    let start = Instant::now();
    let mut hits = 0usize;
    for _ in 0..iterations {
        for path in paths {
            hits += usize::from(black_box(contains(black_box(path.as_bytes()))));
        }
    }
    let elapsed = start.elapsed();
    let nanos_per_lookup = elapsed.as_secs_f64() * 1_000_000_000.0 / lookups_f64;
    println!(
        "{name:<28} {lookups:>10} lookups {hits:>10} hits {:>10.3} ms {:>10.1} ns/lookup",
        elapsed.as_secs_f64() * 1000.0,
        nanos_per_lookup
    );
}

fn main() {
    let paths = build_paths();
    let mixed_paths = paths
        .iter()
        .map(|path| mixed_case_path(path))
        .collect::<Vec<_>>();
    let misses = paths
        .iter()
        .map(|path| format!("missing/{path}"))
        .collect::<Vec<_>>();
    let runs = iterations();

    let mut ba2_builder = Ba2Builder::new();
    let mut tes3_builder = Tes3BsaBuilder::new();
    let mut tes4_builder = Tes4BsaBuilder::new();
    for path in &paths {
        ba2_builder.add_bytes(path, b"payload").unwrap();
        tes3_builder.add_bytes(path, b"payload").unwrap();
        tes4_builder.add_bytes(path, b"payload").unwrap();
    }

    let ba2 = dream_archive::ba2::Archive::from_vec(ba2_builder.to_vec().unwrap()).unwrap();
    let tes3 = dream_archive::bsa::tes3::Archive::from_vec(tes3_builder.to_vec().unwrap()).unwrap();
    let tes4 = dream_archive::bsa::tes4::Archive::from_vec(tes4_builder.to_vec().unwrap()).unwrap();

    println!("files: {FILE_COUNT}, iterations per case: {runs}");
    bench_case("ba2 normalized hit", runs, &paths, |path| {
        ba2.contains(path)
    });
    bench_case("ba2 mixed hit", runs, &mixed_paths, |path| {
        ba2.contains(path)
    });
    bench_case("ba2 miss", runs, &misses, |path| ba2.contains(path));
    bench_case("tes3 normalized hit", runs, &paths, |path| {
        tes3.contains(path)
    });
    bench_case("tes3 mixed hit", runs, &mixed_paths, |path| {
        tes3.contains(path)
    });
    bench_case("tes3 miss", runs, &misses, |path| tes3.contains(path));
    bench_case("tes4 normalized hit", runs, &paths, |path| {
        tes4.contains(path)
    });
    bench_case("tes4 mixed hit", runs, &mixed_paths, |path| {
        tes4.contains(path)
    });
    bench_case("tes4 miss", runs, &misses, |path| tes4.contains(path));
}
