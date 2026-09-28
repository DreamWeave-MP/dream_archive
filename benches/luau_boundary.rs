//! The Luau boundary: what a script pays to list, look up, and read archive members.
//!
//! The scripts are frozen at the pre-migration baseline so the numbers stay comparable across
//! the mlua-to-l3i migration; only the harness that runs them changes.

#![allow(clippy::missing_panics_doc, clippy::semicolon_if_nothing_returned)]

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use dream_archive::{Ba2Builder, Tes3BsaBuilder, Tes4BsaBuilder};

const ENTRY_COUNT: usize = 4096;
const BIG_PAYLOAD: usize = 64 * 1024;

/// One script per scenario: `archive` is the opened archive under test (4096 small members),
/// `path` a present member path, `missing` an absent one.
const SCRIPTS: &[(&str, &str)] = &[
    (
        "entries_indexed",
        "local es = archive:entries() local n = 0 \
         for i = 1, #es do if es[i].index == i then n += 1 end end return n",
    ),
    (
        "entries_for",
        "local n = 0 for _, e in archive:entries() do if e.path ~= nil then n += 1 end end return n",
    ),
    (
        "contains_hit",
        "local n = 0 for i = 1, 1000 do if archive:contains(path) then n += 1 end end return n",
    ),
    (
        "contains_miss",
        "local n = 0 for i = 1, 1000 do if archive:contains(missing) then n += 1 end end return n",
    ),
    (
        "read_entry_small",
        "local n = 0 for i = 1, 1000 do n += #archive:readEntry(1) end return n",
    ),
];

/// Scenarios on the one-member archive whose `big` member is 64 KiB; `buf` is a 64 KiB buffer.
const BIG_SCRIPTS: &[(&str, &str)] = &[(
    "read_file_64k",
    "local n = 0 for i = 1, 16 do n += #archive:readFile(big) end return n",
)];

/// Scenarios that only exist after the migration (a sequence view of entry handles, hash
/// lookups, and reads straight into a script buffer); the harness skips those it cannot run.
const POST_MIGRATION_SCRIPTS: &[(&str, &str)] = &[
    (
        "contains_hash",
        "local n = 0 for i = 1, 1000 do if archive:containsHash(hash) then n += 1 end end return n",
    ),
    (
        "entries_hash",
        "local es = archive:entries() local n = 0 \
         for i = 1, #es do if archive:containsHash(es[i].hash) then n += 1 end end return n",
    ),
];

const POST_MIGRATION_BIG_SCRIPTS: &[(&str, &str)] = &[(
    "read_into_64k",
    "local n = 0 for i = 1, 16 do n += archive:readInto(big, buf, 0) end return n",
)];

fn entry_path(index: usize) -> String {
    format!("meshes/folder{:03}/file{index:05}.nif", index % 128)
}

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
    /// The module function that opens `bytes` as this fixture's format.
    opener: &'static str,
    /// Whether this is the one-member 64 KiB archive rather than the 4096-member one.
    big: bool,
}

fn fixtures() -> Vec<Fixture> {
    let big = vec![0x5au8; BIG_PAYLOAD];
    let mut fixtures = Vec::new();
    for (name, opener, compressed) in [
        ("tes3", "dreamArchive.bsa.tes3.openBytes", false),
        ("tes4", "dreamArchive.bsa.tes4.openBytes", false),
        ("tes4_zlib", "dreamArchive.bsa.tes4.openBytes", true),
        ("ba2", "dreamArchive.ba2.openBytes", false),
    ] {
        let build = |entries: &[(String, Vec<u8>)]| -> Vec<u8> {
            match name {
                "tes3" => {
                    let mut builder = Tes3BsaBuilder::new();
                    for (path, payload) in entries {
                        builder.add_bytes(path, payload).unwrap();
                    }
                    builder.to_vec().unwrap()
                }
                "ba2" => {
                    let mut builder = Ba2Builder::new();
                    for (path, payload) in entries {
                        builder.add_bytes(path, payload).unwrap();
                    }
                    builder.to_vec().unwrap()
                }
                _ => {
                    let mut builder = Tes4BsaBuilder::new();
                    builder.set_compressed(compressed);
                    for (path, payload) in entries {
                        builder.add_bytes(path, payload).unwrap();
                    }
                    builder.to_vec().unwrap()
                }
            }
        };
        let small: Vec<(String, Vec<u8>)> = (0..ENTRY_COUNT)
            .map(|index| {
                (
                    entry_path(index),
                    vec![u8::try_from(index % 251).unwrap(); 16],
                )
            })
            .collect();
        fixtures.push(Fixture {
            name,
            bytes: build(&small),
            opener,
            big: false,
        });
        fixtures.push(Fixture {
            name: match name {
                "tes3" => "tes3_big",
                "tes4" => "tes4_big",
                "tes4_zlib" => "tes4_zlib_big",
                _ => "ba2_big",
            },
            bytes: build(&[("big/payload.bin".to_owned(), big.clone())]),
            opener,
            big: true,
        });
    }
    fixtures
}

/// The chunk that binds the scenario's locals and returns the scenario as a function.
fn chunk(opener: &str, body: &str) -> String {
    format!(
        "local archive = {opener}(archiveBytes) \
         local path, missing, big = {path:?}, 'missing/nothing.nif', 'big/payload.bin' \
         local buf = buffer.create({BIG_PAYLOAD}) \
         local hash = archive.containsHash and archive:entries()[1].hash \
         return function() {body} end",
        path = entry_path(7)
    )
}

fn luau_boundary(c: &mut Criterion) {
    let lua = mlua::Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dreamArchive", module).unwrap();
    for fixture in fixtures() {
        lua.globals()
            .set("archiveBytes", lua.create_string(&fixture.bytes).unwrap())
            .unwrap();
        let mut group = c.benchmark_group(fixture.name);
        let (scripts, post) = if fixture.big {
            (BIG_SCRIPTS, POST_MIGRATION_BIG_SCRIPTS)
        } else {
            (SCRIPTS, POST_MIGRATION_SCRIPTS)
        };
        for (name, body) in scripts {
            let function: mlua::Function = lua.load(chunk(fixture.opener, body)).eval().unwrap();
            group.bench_function(*name, |b| b.iter(|| function.call::<f64>(()).unwrap()));
        }
        for (name, body) in post {
            let function: mlua::Function = lua.load(chunk(fixture.opener, body)).eval().unwrap();
            if function.call::<f64>(()).is_err() {
                continue;
            }
            group.bench_function(*name, |b| b.iter(|| function.call::<f64>(()).unwrap()));
        }
        group.finish();
    }
}

fn configure() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(20)
}

criterion_group! { name = benches; config = configure(); targets = luau_boundary }
criterion_main!(benches);
