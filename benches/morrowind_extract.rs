#![cfg(feature = "bsa-tes3")]

use dream_archive::bsa::tes3::Archive;
use std::{env, fs, path::PathBuf, time::Instant};

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn iterations() -> usize {
    env::var("DREAM_ARCHIVE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10)
}

fn main() {
    let archive_path = root("Morrowind.bsa");
    if !archive_path.exists() {
        eprintln!(
            "skipping local Morrowind.bsa benchmark: {} does not exist",
            archive_path.display()
        );
        return;
    }

    let archive = Archive::open_path(&archive_path).expect("failed to open Morrowind.bsa");
    let output_root = root("target/morrowind-extract-bench");
    let runs = iterations();
    let mut samples = Vec::with_capacity(runs);
    let mut bytes = 0u64;

    for run in 1..=runs {
        if output_root.exists() {
            fs::remove_dir_all(&output_root).expect("failed to clean benchmark output");
        }

        let start = Instant::now();
        bytes = archive
            .extract_to(&output_root)
            .expect("failed to extract Morrowind.bsa");
        let elapsed = start.elapsed();
        samples.push(elapsed.as_secs_f64() * 1000.0);
        println!(
            "run {run:>3}/{runs}: extracted {} files ({bytes} bytes) in {:.3} ms",
            archive.len(),
            samples[samples.len() - 1]
        );
    }

    let sample_count = u32::try_from(samples.len()).expect("too many benchmark samples");
    let total = samples.iter().sum::<f64>();
    let mean = total / f64::from(sample_count);
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    let median = if sorted.len() % 2 == 0 {
        f64::midpoint(sorted[sorted.len() / 2 - 1], sorted[sorted.len() / 2])
    } else {
        sorted[sorted.len() / 2]
    };
    let variance = samples
        .iter()
        .map(|sample| {
            let diff = sample - mean;
            diff * diff
        })
        .sum::<f64>()
        / f64::from(sample_count);

    println!();
    println!("archive: {}", archive_path.display());
    println!("files:   {}", archive.len());
    println!("bytes:   {bytes}");
    println!("runs:    {}", samples.len());
    println!("mean:    {mean:.3} ms");
    println!("median:  {median:.3} ms");
    println!("min:     {:.3} ms", sorted[0]);
    println!("max:     {:.3} ms", sorted[sorted.len() - 1]);
    println!("stdev:   {:.3} ms", variance.sqrt());
}
