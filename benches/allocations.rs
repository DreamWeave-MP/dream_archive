use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder, bsa::NormalizedPath};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static CURRENT_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

const LOOKUP_FILE_COUNT: usize = 4_096;
const LOOKUP_ITERATIONS: usize = 100;
const LARGE_PAYLOAD_SIZE: usize = 64 * 1024 * 1024;

struct CountingAllocator;

// SAFETY: This allocator delegates all allocation operations to `System` and
// only updates atomic counters after successful allocation/reallocation or
// before deallocation. It does not change pointer ownership or layout contracts.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Delegates to `System` with the caller-provided layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_dealloc(layout.size());
        DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: Delegates to `System` with the same pointer/layout contract
        // required by `GlobalAlloc::dealloc`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Delegates to `System` with the caller-provided pointer,
        // original layout, and requested new size.
        let new_ptr = unsafe { System.realloc(ptr, old_layout, new_size) };
        if !new_ptr.is_null() {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            let old_size = old_layout.size();
            if new_size > old_size {
                record_alloc(new_size - old_size);
            } else {
                record_dealloc(old_size - new_size);
            }
        }
        new_ptr
    }
}

fn record_alloc(size: usize) {
    ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
    let current = CURRENT_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    let mut peak = PEAK_BYTES.load(Ordering::Relaxed);
    while current > peak {
        match PEAK_BYTES.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(value) => peak = value,
        }
    }
}

fn record_dealloc(size: usize) {
    DEALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
    CURRENT_BYTES.fetch_sub(size, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
struct Snapshot {
    alloc_calls: usize,
    dealloc_calls: usize,
    realloc_calls: usize,
    allocated_bytes: usize,
    deallocated_bytes: usize,
    current_bytes: usize,
    peak_bytes: usize,
}

fn snapshot() -> Snapshot {
    Snapshot {
        alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
        dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
        realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        current_bytes: CURRENT_BYTES.load(Ordering::Relaxed),
        peak_bytes: PEAK_BYTES.load(Ordering::Relaxed),
    }
}

fn reset_peak() {
    PEAK_BYTES.store(CURRENT_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn report_delta(name: &str, before: Snapshot, after: Snapshot, operations: usize) {
    let alloc_calls = after.alloc_calls - before.alloc_calls;
    let dealloc_calls = after.dealloc_calls - before.dealloc_calls;
    let realloc_calls = after.realloc_calls - before.realloc_calls;
    let alloc_bytes = after.allocated_bytes - before.allocated_bytes;
    let dealloc_bytes = after.deallocated_bytes - before.deallocated_bytes;
    let peak_live_bytes = after.peak_bytes.saturating_sub(before.current_bytes);
    println!(
        "{name:<34} ops {operations:>9} alloc_calls {alloc_calls:>9} dealloc_calls {dealloc_calls:>9} realloc_calls {realloc_calls:>6} alloc_bytes {alloc_bytes:>10} dealloc_bytes {dealloc_bytes:>10} peak_live+ {peak_live_bytes:>10} allocs/op {:>6.2}",
        f64::from(u32::try_from(alloc_calls).expect("too many allocation calls"))
            / f64::from(u32::try_from(operations).expect("too many benchmark operations")),
    );
}

fn build_lookup_paths() -> Vec<String> {
    (0..LOOKUP_FILE_COUNT)
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

fn bench_lookup_allocations(name: &str, paths: &[String], mut contains: impl FnMut(&[u8]) -> bool) {
    reset_peak();
    let before = snapshot();
    let mut hits = 0usize;
    for _ in 0..LOOKUP_ITERATIONS {
        for path in paths {
            hits += usize::from(black_box(contains(black_box(path.as_bytes()))));
        }
    }
    let after = snapshot();
    black_box(hits);
    report_delta(name, before, after, LOOKUP_ITERATIONS * paths.len());
}

fn bench_normalized_lookup_allocations(
    name: &str,
    paths: &[NormalizedPath],
    mut contains: impl FnMut(&NormalizedPath) -> bool,
) {
    reset_peak();
    let before = snapshot();
    let mut hits = 0usize;
    for _ in 0..LOOKUP_ITERATIONS {
        for path in paths {
            hits += usize::from(black_box(contains(black_box(path))));
        }
    }
    let after = snapshot();
    black_box(hits);
    report_delta(name, before, after, LOOKUP_ITERATIONS * paths.len());
}

fn large_payload() -> Vec<u8> {
    (0..LARGE_PAYLOAD_SIZE)
        .map(|offset| u8::try_from((offset / 4096) % 251).unwrap())
        .collect()
}

fn bench_one_extraction(name: &str, mut extract: impl FnMut() -> u64) {
    reset_peak();
    let before = snapshot();
    let bytes = black_box(extract());
    let after = snapshot();
    black_box(bytes);
    report_delta(name, before, after, 1);
}

fn output_dir(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target/bench-extract");
    path.push(format!("{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn run_lookup_benchmarks() {
    let paths = build_lookup_paths();
    let mixed_paths = paths
        .iter()
        .map(|path| mixed_case_path(path))
        .collect::<Vec<_>>();
    let misses = paths
        .iter()
        .map(|path| format!("missing/{path}"))
        .collect::<Vec<_>>();
    let normalized_paths = paths.iter().map(NormalizedPath::new).collect::<Vec<_>>();
    let normalized_mixed_paths = mixed_paths
        .iter()
        .map(NormalizedPath::new)
        .collect::<Vec<_>>();
    let normalized_misses = misses.iter().map(NormalizedPath::new).collect::<Vec<_>>();

    let mut tes3_builder = Tes3BsaBuilder::new();
    let mut tes4_builder = Tes4BsaBuilder::new();
    for path in &paths {
        tes3_builder.add_bytes(path, b"payload").unwrap();
        tes4_builder.add_bytes(path, b"payload").unwrap();
    }
    let tes3 = dream_archive::bsa::tes3::Archive::from_vec(tes3_builder.to_vec().unwrap()).unwrap();
    let tes4 = dream_archive::bsa::tes4::Archive::from_vec(tes4_builder.to_vec().unwrap()).unwrap();

    println!("lookup allocation counts: {LOOKUP_FILE_COUNT} files, {LOOKUP_ITERATIONS} passes");
    bench_lookup_allocations("tes3 normalized hit", &paths, |path| tes3.contains(path));
    bench_lookup_allocations("tes3 mixed hit", &mixed_paths, |path| tes3.contains(path));
    bench_lookup_allocations("tes3 miss", &misses, |path| tes3.contains(path));
    bench_normalized_lookup_allocations("tes3 pre-normalized hit", &normalized_paths, |path| {
        tes3.contains_normalized(path)
    });
    bench_normalized_lookup_allocations(
        "tes3 pre-normalized mixed hit",
        &normalized_mixed_paths,
        |path| tes3.contains_normalized(path),
    );
    bench_normalized_lookup_allocations("tes3 pre-normalized miss", &normalized_misses, |path| {
        tes3.contains_normalized(path)
    });
    bench_lookup_allocations("tes4 normalized hit", &paths, |path| tes4.contains(path));
    bench_lookup_allocations("tes4 mixed hit", &mixed_paths, |path| tes4.contains(path));
    bench_lookup_allocations("tes4 miss", &misses, |path| tes4.contains(path));
    bench_normalized_lookup_allocations("tes4 pre-normalized hit", &normalized_paths, |path| {
        tes4.contains_normalized(path)
    });
    bench_normalized_lookup_allocations(
        "tes4 pre-normalized mixed hit",
        &normalized_mixed_paths,
        |path| tes4.contains_normalized(path),
    );
    bench_normalized_lookup_allocations("tes4 pre-normalized miss", &normalized_misses, |path| {
        tes4.contains_normalized(path)
    });
}

fn run_extraction_benchmarks() {
    let payload = large_payload();
    let mut ba2_builder = Ba2Builder::new();
    ba2_builder.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::Zip));
    ba2_builder
        .add_bytes("textures/large.bin", &payload)
        .unwrap();
    let mut tes4_compressed_builder = Tes4BsaBuilder::new();
    tes4_compressed_builder.set_compressed(true);
    tes4_compressed_builder
        .add_bytes("textures/large.bin", &payload)
        .unwrap();
    let ba2 = dream_archive::ba2::Archive::from_vec(ba2_builder.to_vec().unwrap()).unwrap();
    let tes4_compressed =
        dream_archive::bsa::tes4::Archive::from_vec(tes4_compressed_builder.to_vec().unwrap())
            .unwrap();

    println!("large compressed extraction allocation counts: {LARGE_PAYLOAD_SIZE} bytes/file");
    bench_one_extraction("ba2 read_entry", || {
        u64::try_from(ba2.read_entry(&ba2.entries()[0]).unwrap().len()).unwrap()
    });
    bench_one_extraction("ba2 extract_entry sink", || {
        ba2.extract_entry(&ba2.entries()[0], std::io::sink())
            .unwrap()
    });
    bench_one_extraction("ba2 extract_entry_to_path", || {
        let out = output_dir("ba2-entry-allocations");
        std::fs::create_dir_all(&out).unwrap();
        let path = out.join("large.bin");
        let written = ba2.extract_entry_to_path(&ba2.entries()[0], &path).unwrap();
        std::fs::remove_dir_all(out).unwrap();
        written
    });
    bench_one_extraction("ba2 extract_to", || {
        let out = output_dir("ba2-allocations");
        let written = ba2.extract_to(&out).unwrap();
        std::fs::remove_dir_all(out).unwrap();
        written
    });
    bench_one_extraction("tes4 read_entry", || {
        u64::try_from(
            tes4_compressed
                .read_entry(&tes4_compressed.entries()[0])
                .unwrap()
                .len(),
        )
        .unwrap()
    });
    bench_one_extraction("tes4 extract_entry sink", || {
        tes4_compressed
            .extract_entry(&tes4_compressed.entries()[0], std::io::sink())
            .unwrap()
    });
    bench_one_extraction("tes4 extract_entry_to_path", || {
        let out = output_dir("tes4-entry-allocations");
        std::fs::create_dir_all(&out).unwrap();
        let path = out.join("large.bin");
        let written = tes4_compressed
            .extract_entry_to_path(&tes4_compressed.entries()[0], &path)
            .unwrap();
        std::fs::remove_dir_all(out).unwrap();
        written
    });
    bench_one_extraction("tes4 extract_to", || {
        let out = output_dir("tes4-allocations");
        let written = tes4_compressed.extract_to(&out).unwrap();
        std::fs::remove_dir_all(out).unwrap();
        written
    });
}

fn main() {
    run_lookup_benchmarks();
    run_extraction_benchmarks();
}
