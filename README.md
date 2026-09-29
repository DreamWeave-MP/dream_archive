# dream_archive

Pure-Rust Bethesda archive tooling for common PC archive layouts. It reads,
lists, opens file-like readers, extracts, and builds the archive families used by
Morrowind, Oblivion, Fallout, Skyrim, Fallout 4, and Starfield-era games,
subject to the format rows below rather than wishful thinking.

The crate keeps archive paths as bytes. That is intentional. Old Bethesda tools
and mods do not always agree on Unicode, code pages, or reality in general.
ASCII paths work as ordinary Rust strings; legacy localized BSA paths should use
an explicit encoding helper instead of a hopeful guess.

## Capability matrix

| Format | Read | Extract | Write | Notes |
| --- | --- | --- | --- | --- |
| BA2 GNRL | yes | yes | yes | General-file BA2 archives. |
| BA2 DX10 | yes | yes | yes | Extraction emits DDS files by reconstructing standard DDS headers from BA2 texture metadata. Private/vendor DDS header fields are not preserved. Writer accepts supported DDS files or explicit texture metadata plus raw payload bytes. |
| BA2 GNMF | metadata | no | no | Sony GNM (`.gnf`) texture archives. Explicitly out of scope for now; metadata is parsed only so callers can identify them instead of getting mystery meat. |
| TES3 BSA | yes | yes | yes | Morrowind-era archives. |
| TES4 BSA | yes | yes | yes | Oblivion/Fallout/Skyrim PC archives, including hash-only and embedded-name layouts. |
| Console/Xbox layouts | no | no | no | Out of scope. |
| XMem compression | no | no | no | Out of scope. |

“Write support” above means the writer semantics for that specific row are
implemented. It does **not** mean “every BA2 payload type is writable.” Words
mean things. Annoying, but useful.

BA2 parsing recognizes the supported PC BA2 versions used across Fallout
4/Fallout 76/Starfield-era archives, including ZIP and supported LZ4 layouts.
GNMF remains metadata-only.

GNMF is not a missing general-file feature. It is Sony GNM texture data with
console-style swizzle/unswizzle requirements. This crate does not currently
extract or write it. If you need GNMF support, bring real fixtures and a clear
compatibility target; otherwise we are not guessing our way into a PlayStation
texture pipeline.

## 30-second usage

Open any supported archive and list recoverable paths:

```rust,no_run
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
let archive = Archive::open_path("Data/SomeArchive.bsa")?;

for entry in archive.entries() {
    if let Some(path) = entry.path() {
        println!("{path}");
    }
}
Ok(())
}
```

Read a required file:

```rust,no_run
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
let archive = Archive::open_path("Data/SomeArchive.bsa")?;
let bytes = archive.read_file_required("meshes/foo.nif")?;
println!("{} bytes", bytes.len());
Ok(())
}
```

Open a required file as a `Read` implementation, for callers that want ordinary
file-like streaming instead of a `Vec<u8>`:

```rust,no_run
use std::io::Read as _;
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
let archive = Archive::open_path("Data/SomeArchive.bsa")?;
let mut reader = archive.open_file_required("meshes/foo.nif")?;

let mut bytes = Vec::new();
reader.read_to_end(&mut bytes)?;
println!("{} bytes", bytes.len());
Ok(())
}
```

Uncompressed archive entries read directly from archive storage. Compressed
entries may be decoded into an internal buffer before the reader is returned so
format errors remain format errors instead of appearing later as generic I/O
failures. A reader API is not a wizard. Annoying, but quite useful.

Extract everything with paths stored in the archive:

```rust,no_run
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
let archive = Archive::open_path("Data/SomeArchive.ba2")?;
archive.extract_to("out")?;
Ok(())
}
```

Some archive layouts do not contain enough text to name every file. Hash-only
TES4 archives need the TES4-specific `extract_to_with_paths` API with a path
dictionary. BA2 archives without string tables still have hashes, but
`Entry::path()` returns `None` through the top-level facade because no filename
text exists to recover. No path does not mean empty filename.

```rust,no_run
use dream_archive::bsa::tes4::Archive;

fn main() -> dream_archive::bsa::Result<()> {
let archive = Archive::open_path("HashOnly.bsa")?;
archive.extract_to_with_paths("out", [
    b"meshes/foo.nif".as_slice(),
    b"textures/foo.dds".as_slice(),
])?;
Ok(())
}
```

## Building archives

Build a TES3/Morrowind BSA from a directory:

```rust,no_run
use dream_archive::Tes3BsaBuilder;

fn main() -> dream_archive::bsa::Result<()> {
let mut builder = Tes3BsaBuilder::new();
builder.add_dir("Data")?;
builder.write_path("MyMod.bsa")?;
Ok(())
}
```

Build a TES4-family BSA using a PC game profile:

```rust,no_run
use dream_archive::{
    Tes4BsaBuilder,
    bsa::tes4::{ArchiveTypes, NameMode},
};

fn main() -> dream_archive::bsa::Result<()> {
let mut builder = Tes4BsaBuilder::skyrim_le();
builder.set_archive_types(ArchiveTypes::MISC);
builder.set_name_mode(NameMode::Strings);
builder.add_dir("Data")?;
builder.write_path("MyMod.bsa")?;
Ok(())
}
```

Build a BA2 GNRL archive:

```rust,no_run
use dream_archive::{Ba2Builder, ba2::Ba2CompressionFormat};

fn main() -> dream_archive::ba2::Result<()> {
let mut builder = Ba2Builder::new();
builder.set_compression(Some(Ba2CompressionFormat::Zip));
builder.add_dir("Data")?;
builder.write_path("MyMod.ba2")?;
Ok(())
}
```

Builder filesystem inputs are deferred. `add_file` and `add_dir` validate archive
paths, reject duplicates, and record source file metadata when called, but they
read payload bytes during `write_path`, `write_seek`, or `to_vec`. If a deferred
source file disappears, becomes unreadable, or changes size before writing, the
write fails rather than emitting an archive whose table lies about its payloads.

For filesystem output, prefer `write_path` or `write_seek`. `to_vec` necessarily
buffers the final archive because it returns a `Vec<u8>`; useful for tests and
small archives, not magic.

When rewriting an archive, unchanged entries can be attached from the parsed
source archive instead of first extracting them into a `Vec<u8>`:

```rust,no_run
use std::sync::Arc;
use dream_archive::bsa::tes3::{Archive, Builder};

fn main() -> dream_archive::bsa::Result<()> {
let source = Arc::new(Archive::open_path("Old.bsa")?);
let mut builder = Builder::new();

for (id, entry) in source.entries_with_ids() {
    if entry.path() != b"data/replaced.txt".as_slice() {
        builder.add_archive_entry(entry.path(), Arc::clone(&source), id)?;
    }
}

builder.add_file("data/replaced.txt", "loose/replaced.txt")?;
builder.write_path("New.bsa")?;
Ok(())
}
```

BA2 and TES4 builders expose the same `add_archive_entry` pattern, plus
`add_archive_entry_with_compression` where per-file compression policy matters.
The first implementation decodes existing entries and writes them according to
the destination builder policy. Raw compressed-byte copy is deliberately not
claimed here. That would be a different feature, not a vibe.

Build a BA2 DX10 texture archive from DDS files:

```rust,no_run
use dream_archive::{Ba2Dx10Builder, ba2::Ba2CompressionFormat};

fn main() -> dream_archive::ba2::Result<()> {
let mut builder = Ba2Dx10Builder::new();
builder.set_compression(Some(Ba2CompressionFormat::Zip));
builder.add_dds_file("textures/example.dds", "source/example.dds")?;
builder.write_path("Textures.ba2")?;
Ok(())
}
```

The first path is the archive path to store. The second path is the filesystem
DDS source to read.

The DDS path parses supported DDS headers, maps them to BA2 texture metadata,
validates the payload size, strips the DDS header, and stores the raw texture
payload. It does not transcode formats or generate mips. If you already have BA2
texture metadata, `add_texture_bytes` accepts raw texture payload bytes only —
not a complete DDS file with its header still attached.

DX10/DDS support here is archive plumbing, not a texture processing library. It
does not decode pixels, preserve vendor/private DDS fields, handle texture
arrays/volumes, transcode formats, repair payloads, or aim for DirectXTex parity.
A broader DDS crate belongs with a renderer; this crate only does the pieces BA2
writing and extraction need today.

Supported DDS input for BA2 DX10 writing is deliberately narrow: tightly packed
2D textures and cubemaps using the DXGI/FourCC/plain formats this crate can
reconstruct on extraction. The payload byte count must exactly match the DDS
dimensions, format, mip count, and cubemap face count. Texture arrays and 3D
volume textures are rejected.

## Archive paths are not filesystem paths

Archive paths are virtual filesystem names stored inside the archive. They are
byte strings, not necessarily Unicode OS paths.

- Normal ASCII mod paths can be passed as `&str`; this is fine only when the
  archive path bytes are actually UTF-8/ASCII.
- Legacy localized BSA paths should be encoded explicitly.
- The crate does not guess encodings, because guessing corrupts mods and then
  everyone has a bad afternoon.

Example for a legacy Windows code page path:

```rust,no_run
use dream_archive::{Archive, bsa::{FilenameEncoding, encode_filename}};

fn main() -> dream_archive::Result<()> {
let encoded = encode_filename("textures/zażółć.dds", FilenameEncoding::Windows1250)?;
let archive = Archive::open_path("Data/SomeArchive.bsa")?;
let bytes = archive.read_file_required(encoded.as_ref())?;
println!("{} bytes", bytes.len());
Ok(())
}
```

For GNMF BA2 archives, inspect metadata rather than attempting extraction:

```rust,no_run
use dream_archive::ba2::{Archive, PayloadFormat};

fn main() -> dream_archive::ba2::Result<()> {
let archive = Archive::open_path("Textures.ba2")?;
if archive.info().format == PayloadFormat::GNMF {
    eprintln!("GNMF metadata is readable; extraction is unsupported");
    return Ok(());
}
Ok(())
}
```

## Feature flags

Default features enable BA2 and both BSA families.

- `ba2`: BA2 reader/extractor plus GNRL and DX10 writers.
- `bsa-tes3`: TES3 BSA support.
- `bsa-tes4`: TES4-family BSA support.
- `bsa`: both BSA families.
- `parallel`: parallel extraction with Rayon.
- `luau`: the Luau bindings as an [l3i](https://github.com/DreamWeave-MP/l3i)
  extension (`dream_archive::luau`); enables `ba2` and `bsa`. `lua` is the old
  name of the same feature.

## Luau bindings

Enable it in `Cargo.toml`:

```toml
dream_archive = { version = "0.3", features = ["luau"] }
```

As of 0.3.0 the bindings are an [l3i](https://github.com/DreamWeave-MP/l3i)
extension instead of an `mlua` module table: extension `dream.archive`, module
`@dream/archive`, userdata type `dream.archive.Archive`. The crate never creates
a Luau VM. The host composes `dream_archive::luau::ArchiveExtension` into a
`RuntimePlan`, instantiates runtimes from it, and decides whether the module is
also a global (the conventional name is still `dreamArchive`). l3i's toolchain
policy (clang, lld, cross-language thin LTO) applies to anything that builds the
`luau` feature; copy l3i's `.cargo/config.toml` as this repository does.

```rust,no_run
use l3i::Runtime;
use l3i::extension::{RuntimePlan, RuntimePolicy};

fn main() -> l3i::Result<()> {
    let plan = RuntimePlan::builder()
        .policy(RuntimePolicy::new().compat_global("@dream/archive", "dreamArchive"))
        .extension(dream_archive::luau::ArchiveExtension)
        .finalize()?;
    let runtime = Runtime::from_plan(&plan)?;
    runtime.exec(r#"
        local dreamArchive = require("@dream/archive")
        local builder = dreamArchive.ba2.Builder.new()
        builder:setCompression(dreamArchive.ba2.compression.zip)
        builder:addBytes("meshes/example.nif", "payload")
        local archive = dreamArchive.openBytes(builder:toBytes())
        assert(archive:format() == "ba2")
        assert(archive:readFileRequired("meshes/example.nif") == "payload")
    "#)?;
    Ok(())
}
```

The plan renders `.d.luau` definitions for the whole surface
(`RuntimePlan::type_definitions`), and `plan.check_definitions()` (l3i feature
`analysis`) is the gate this crate's tests run: every member is typed in Luau's
own checker, and a strict script that does `require("@dream/archive")` type
checks against the module stubs.

### Module shape

`@dream/archive` has `openPath`, `openBytes`, `detectPath`, `guessFormat`,
`normalizePath`, and the nested modules `ba2` (`@dream/archive/ba2`: `openPath`,
`openBytes`, `hashFile`, `Builder.new`, `Dx10Builder.new`, `compression`,
`version`) and `bsa` (`@dream/archive/bsa`: `encodeFilename`,
`decodeFilenameLossy`, `normalizePath`, `encoding`, and `tes3` / `tes4` with
`openPath`, `openBytes`, hash helpers, `Builder.new`, and the `nameMode`,
`profile`, and `archiveTypes` constants). Every table is frozen, and
`dreamArchive.bsa.tes3 == require("@dream/archive/bsa/tes3")`.

Every `open*` returns the one `Archive` type; the format-specific members
(`info` for BA2, `extractToWithEncoding` for BSA, `extractToWithPaths` for TES4)
raise a clear error on the wrong format. Archive members:

| Luau call | Result |
| --- | --- |
| `archive:format()`, `archive:len()`, `archive:isEmpty()`, `archive:archiveSize()` | metadata |
| `archive:entries()` | a sequence view of entry handles (`#`, `[i]`, `for`, `:toTable()`) |
| `archive:entry(i)`, `archive:get(path)`, `archive:getByHash(...)` | one entry handle or `nil` |
| `archive:contains(path)`, `archive:containsHash(...)` | boolean |
| `archive:readInto(entry or path, buffer, offset?)` | decodes straight into the buffer, returns the byte count |
| `archive:readFile(path)` / `readFileRequired` / `extractFile` / `extractFileRequired` | payload bytes as a string (the slow path: two copies) |
| `archive:readEntry(i)`, `archive:extractEntry(i)` | payload bytes as a string, 1-based index |
| `archive:extractEntryToPath(i, path)`, `archive:extractTo(dir)` | bytes written |
| `archive:extractToWithEncoding(dir, encoding)` | BSA only |
| `archive:extractToWithPaths(dir, { paths })` | TES4 only: names hash-only entries from candidates |
| `archive:info()` | BA2 only: `{ format, version, compression, strings }` |

An entry handle reads its metadata from the shared archive index: `index` and
`id` (the 1-based position; direct fields), `format`, `path`, `name`, `folder`
(`nil` where the archive stores no name), `size` (decoded), `storedSize`,
`offset`, `compressed`, `chunks` (BA2), and the hashes. Two handles of one entry
compare equal.

### Hashes

TES3 and TES4 hashes are Luau integers carrying all 64 bits, compared with
`==` only: `entry.hash` (TES3: the entry hash; TES4: the file hash) and TES4
`entry.folderHash`, with `hashHex` / `folderHashHex` keeping the 16-digit
spelling. BA2 hashes are the three exact fields `directoryHash`, `fileHash`,
`extensionHash` (numbers). `containsHash` and `getByHash` take the same shape:
`(hash)` for TES3, `(folderHash, fileHash)` for TES4, `(directory, file,
extension)` for BA2. The hash helpers return tables whose `hash` field is the
integer next to the old fields:

```luau
local h = dreamArchive.bsa.tes3.hashFile("Meshes/Foo.NIF")
-- h.lo, h.hi, h.hex, h.hash (integer), h.normalized
assert(archive:containsHash(h.hash))

local t4 = dreamArchive.bsa.tes4.hashFile("foo.dds")
-- t4.last, t4.last2, t4.length, t4.first, t4.crc, t4.hex, t4.hash (integer)
local d = dreamArchive.bsa.tes4.hashDirectory("textures")
local e = tes4:getByHash(d.hash, t4.hash)
```

### Bytes

Archive paths are byte strings; host filesystem paths are UTF-8 strings.
Payload inputs (`openBytes`, `guessFormat`, builder `addBytes`,
`addBytesWithCompression`, `addEncodedPath`, `addDdsBytes`, `addTextureBytes`)
take a Luau `buffer` or a string. Builders return archive bytes with
`toBytes()` / `toString()` (a string) or `toBuffer()` (a buffer). `readInto`
is the fast read: one decode into the caller's buffer, no Lua string; it fails
before writing when the entry does not fit after `offset`, and a decode error
leaves the buffer partially written.

```luau
local buf = buffer.create(1024 * 1024)
for _, entry in archive:entries() do
    if entry.size <= buffer.len(buf) then
        local n = archive:readInto(entry, buf)
    end
end
```

### Builders

`ba2.Builder.new()`, `ba2.Dx10Builder.new()`, `bsa.tes3.Builder.new()`, and
`bsa.tes4.Builder.new()` keep their 0.2 methods. `addArchiveEntry(path,
archive, id)` (and `addArchiveEntryWithCompression`) preserve an entry from an
already-open archive of the same family, using `entry.id`; the source archive
is shared, not copied per call. `addTextureBytes` reads its header table
strictly: `height`, `width`, `mipCount`, `format`, `flags`, `tileMode` are
required exact integers and any other key is an error.

### Breaking changes from 0.2

- Rust: `dream_archive::lua` and the `mlua` types are gone; `dream_archive::luau`
  exports `ArchiveExtension`, `Archive` (`archive()`, `path()`, `entry(i)`),
  `Entry`, `Entries`, and the module and key constants. Hosts create no VM;
  downstream crates augment `dream.archive.Archive` through l3i's planner.
- `ba2.openBytes`, `bsa.tes3.openBytes`, and `bsa.tes4.openBytes` (and the
  `openPath` variants) return the same `Archive` type as the top-level `open*`,
  with `contains`, `archiveSize`, `info`, and the `extractToWith*` methods
  available on every archive of the right format.
- `archive:entries()` is a view of handles, not a table of tables. `#`, `[i]`,
  `for`, and `:toTable()` work; `table.insert`, `ipairs`, and `pairs` do not.
- Entry hashes are integers: `entry.hash` replaces `entry.hash.lo/hi/hex`
  (TES3) and `entry.fileHash` (TES4, now `entry.hash`); `entry.folderHash` is an
  integer; the hex strings moved to `entry.hashHex` and `entry.folderHashHex`;
  BA2's `entry.hash.directory/file/extension` are `entry.directoryHash`,
  `entry.fileHash`, `entry.extensionHash`. TES3 `entry.size` / `entry.offset`
  and TES4 `entry.storedSize` / `entry.dataOffset` are `size`, `storedSize`,
  and `offset` on every format (`offset` is the TES4 data offset).
- Integer arguments (indices, versions, levels, header fields) must be exact:
  `readEntry(1.5)` is an error instead of a rounded read.
- Non-UTF-8 host path strings fail with "not valid UTF-8" rather than
  "invalid utf-8".

### Performance

`benches/luau_boundary.rs` runs frozen scripts against archives of 4096 members
(and a 64 KiB member for the read scenarios); mean per script call, 0.2.3 with
mlua against 0.3.0 with l3i, same machine, same scripts:

| scenario | 0.2.3 (mlua) | 0.3.0 (l3i) |
| --- | --- | --- |
| TES3 `entries()` then `es[i].index` over 4096 | 6.15 ms | 0.73 ms |
| TES3 `for _, e in entries()` reading `e.path` | 16.8 ms | 1.10 ms |
| TES4 `entries()` indexed / `for` | 15.6 ms / 15.4 ms | 0.68 ms / 1.06 ms |
| BA2 `entries()` indexed / `for` | 5.3 ms / 5.3 ms | 0.67 ms / 1.07 ms |
| `contains(path)` hit, 1000 calls (TES3 / TES4) | 281 µs / 250 µs | 146 µs / 155 µs |
| `containsHash(h)` hit, 1000 calls (TES3) | n/a | 87 µs |
| `readEntry(1)` of 16 bytes, 1000 calls (TES3 / BA2) | 259 µs / 246 µs | 119 µs / 112 µs |
| `readFile` of 64 KiB, 16 calls (TES3 / TES4 zlib) | 260 µs / 490 µs | 217 µs / 414 µs |
| `readInto` of 64 KiB, 16 calls (TES3 / TES4 zlib) | n/a | 27 µs / 240 µs |

The Rust core changed too (`benches/core.rs`): path lookups normalize into a
stack buffer instead of allocating (TES3/TES4 `contains` over 4096 distinct
paths 1.03 ms / 1.43 ms to 0.39 ms; BA2 hashes in place), and zlib payloads
inflate from the slice through one per-thread inflater instead of a reader stack
(a 16-byte TES4 zlib member 13.9 µs to 2.6 µs, a 64 KiB one 68 µs to 18 µs;
BA2 zlib 5.4 µs to 2.9 µs and 28 µs to 20 µs).

## Compatibility policy

The target is real PC archive compatibility. Console archive layouts and XMem
compression are not implemented. If a future PC archive proves a currently
unsupported path is real, useful, and not just format archaeology with a hat, it
can be added with tests.

## Support

Has `dream_archive` been useful to you?

If so, please consider [amplifying the signal](https://ko-fi.com/magicaldave) through my ko-fi.

Thank you for using `dream_archive`.
