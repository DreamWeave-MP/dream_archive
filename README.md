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
- `lua`: enables the `mlua` Luau bindings, `ba2`, `bsa`, and the re-exported
  `dream_path`'s Luau companion helpers. It does not select an `mlua` runtime.
- `standalone-lua`: selects `mlua`'s Luau backend (built from source); intended
  for this crate's tests, examples, and documentation builds rather than normal
  downstream library use.

## Luau bindings

Enable it in `Cargo.toml`:

```toml
dream_archive = { version = "0.2", features = ["lua"] }
```

As of 0.2.0 the bindings target [Luau](https://luau.org) instead of LuaJIT, and
the whole Lua-facing surface follows the same conventions as DreamWeave's other
Luau APIs: functions, methods, table fields, and enum-like string values are
camelCase (`openPath`, `readFileRequired`, `folderHash`, `"bsaTes4"`,
`"skyrimSe"`), types are PascalCase (`Builder`, `Dx10Builder`), and numeric
constants are UPPER_SNAKE (`archiveTypes.MESHES`, `version.V8`). The
conventional global is `dreamArchive`, next to `dreamPath`. Scripts written
against 0.1's snake_case names need renaming; nothing else about the behaviour
changed.

Embedding applications must choose the `mlua` runtime centrally. If you just want
to run this crate's examples or tests without an application's feature graph, use
`standalone-lua` instead. Building this crate by itself with `lua` but no `mlua`
runtime selected is intentionally incomplete. Selecting runtimes in every
lower-level crate is how you get one build graph wearing several fake moustaches.

The `lua` feature exposes a byte-first Luau API through `dream_archive::lua` for
Rust embedders. It does not install a standalone `require("@dreamArchive")` C
module by itself; register the `mlua` table in your application. Use the
`dream_archive::dream_path` re-export for companion path helpers instead of
adding a separate `dream_path` dependency just to reach the same API. Lua strings
are archive path bytes and payload bytes. Filesystem arguments are the exception:
all `openPath`, `detectPath`, `writePath`, `extractTo*`,
`extractEntryToPath`, `addDir`, and source paths such as `addFile` /
`addDdsFile` are converted as UTF-8 host paths. Archive paths passed to
`readFile`, `addBytes`, `addFile`'s first argument, hash helpers, and
`normalizePath` remain raw archive path bytes. `bsa.encodeFilename()` and
builder `addEncodedPath()` are the opposite boundary: they take UTF-8 text and
produce or insert legacy-encoded archive filename bytes. If your Unix filesystem
path is not valid UTF-8, use the Rust API directly.
Annoying, but less dishonest than pretending all paths are the same kind of
string.

Luau builders follow the Rust deferred-source behavior: `addFile` records a host
path and reads it when `writePath` / `toBytes` runs. Format-specific archive
entry tables include both `index` and `id` fields; today they are the same
1-based number. Pass that `id` to `builder:addArchiveEntry(...)` to preserve an
entry from an already-open archive without round-tripping it through a Lua
string. BA2 and TES4 builders also expose
`addArchiveEntryWithCompression(path, archive, id, policy)`.

```rust,no_run
fn main() -> mlua::Result<()> {
let lua = mlua::Lua::new();
lua.globals().set(
    "dreamPath",
    dream_archive::dream_path::lua::create_module(&lua)?,
)?;
let module = dream_archive::lua::create_module(&lua)?;
lua.globals().set("dreamArchive", module)?;

lua.load(r#"
    local builder = dreamArchive.ba2.Builder.new()
    builder:setCompression(dreamArchive.ba2.compression.zip)
    builder:addBytes("meshes/example.nif", "payload")

    -- toBytes()/toString() return raw archive bytes as a Lua string.
    local archive = dreamArchive.openBytes(builder:toBytes())
    assert(archive:format() == "ba2")
    assert(archive:readFileRequired("meshes/example.nif") == "payload")
"#).exec()?;
Ok(())
}
```

The module exposes:

- top-level detection/open/read/extract facade: `openPath`, `openBytes`,
  `detectPath`, `guessFormat`, archive `entries`, `readFile*`,
  `extractFile*`, `readEntry`, `extractEntry`, `extractEntryToPath`, and
  `extractTo`;
- BA2-specific archive inspection, hashes, GNRL builder, and DX10 builder;
- BSA encoding/decoding helpers and path normalization;
- TES3/TES4 archive inspection, hashes, builders, TES4 profiles, name modes,
  compression policy, and archive type bits.

Register `dream_archive::dream_path` next to `dreamArchive` when scripts need
the shared virtual path helpers. The re-exported `dreamPath` handles path
normalization/helpers; `dreamArchive` handles open/list/read/extract/build
archive mechanics; `dream_archivetool` should remain the layer that decides
filesystem rewrite, diff, and verification policy.

Common calls look like this:

| Luau call | Result |
| --- | --- |
| `dreamArchive.openBytes(bytes)` | generic BA2/TES3/TES4 archive |
| `dreamArchive.guessFormat(bytes)` | `"ba2"`, `"bsaTes3"`, `"bsaTes4"`, or `nil` |
| `archive:entries()` | array-style table of copied entry metadata |
| `archive:readFile(path)` | payload bytes or `nil` |
| `archive:readFileRequired(path)` | payload bytes or error |
| `archive:extractFile(path)` | extracted payload bytes or `nil` |
| `archive:extractFileRequired(path)` | extracted payload bytes or error |
| `archive:extractTo(target)` | writes files for side effect; errors throw |
| `archive:readEntry(1)` | reads the first entry; entry indices are 1-based |
| `builder:addBytes(path, bytes)` | adds archive path bytes and payload bytes |
| `builder:toBytes()` | raw archive bytes |

`entries()` returns a fully materialized Lua table, copying entry metadata into
Lua values. That is convenient for scripts and not a streaming iterator.
`readFile*`, `extractFile*`, and `extractEntry` also materialize the complete
payload as a Lua string; for large archives that means Rust buffering plus a Lua
string copy. The Rust `open_file* -> Read` archive APIs are not exposed as Lua
userdata readers. Luau gets byte strings or filesystem extraction; if you need an
actual Rust `Read`, stay in Rust. `openBytes(bytes)` copies the Lua archive
string into Rust-owned storage, and builder `toBytes()` / `toString()` build a
Rust archive buffer and then copy it into Lua. For large archives, prefer
`openPath()` and `writePath()`. Likewise TES4 `extractToWithPaths()` copies
the supplied contiguous Lua sequence (`1..n`, no gaps) of candidate archive path
byte strings before it starts matching hash-only entries. It is not a key/value
dictionary; non-sequence keys are ignored.

```luau
local hashOnly = dreamArchive.bsa.tes4.openPath("HashOnly.bsa")
hashOnly:extractToWithPaths("out", {
    "meshes/foo.nif",
    "textures/foo.dds",
})
```

TES4 hash-only entries cannot expose names they do not have. Their Luau entry
tables use `nil` for `path`, `folder`, and `name`; use `folderHash.hex` and
`fileHash.hex` when you need exact identity.

Use `dreamArchive.open*` when you want one generic reader for BA2/TES3/TES4.
Use `dreamArchive.ba2.open*`, `dreamArchive.bsa.tes3.open*`, or
`dreamArchive.bsa.tes4.open*` when you need format-specific metadata or helper
APIs such as BA2 `info()` or TES4 `extractToWithPaths()`.

BA2 archive compression accepts `nil`, `"none"`/`"store"`, `"zip"`, or `"lz4"`.
Per-file compression overrides accept `nil`, `"inherit"`, `"store"`, or
`"compress"`. `setZlibLevel(level)` accepts `0..=9` and rejects other values at
the setter call, not later during `toBytes()`.

TES4 builders take a profile through `setProfile`: `"oblivion"`, `"fallout3"`,
`"falloutNewVegas"`, `"skyrimLe"`, or `"skyrimSe"`, and a name mode through
`setNameMode`: `"strings"`, `"hashOnly"`, `"embedded"`, or
`"stringsAndEmbedded"`. The `profile`, `nameMode`, `compression`, and `encoding`
tables map each of those values to itself, so either spelling works:
`builder:setProfile(dreamArchive.bsa.tes4.profile.skyrimSe)` or
`builder:setProfile("skyrimSe")`.

DX10 BA2 texture building needs either a DDS source (`addDdsBytes` /
`addDdsFile`) or a raw texture payload plus the BA2 texture metadata. The bytes
passed to `addTextureBytes()` are raw texture payload bytes, not a complete DDS
file:

```luau
local dx10 = dreamArchive.ba2.Dx10Builder.new()
dx10:addTextureBytes("textures/foo.dds", {
    height = 1,
    width = 1,
    mipCount = 1,
    format = 61,
    flags = 0,
    tileMode = 0,
}, rawTexturePayload)
```

TES4 archive type bits are numeric flags. Combine them with Luau's `bit32`
library (or addition, when each flag appears once):

```luau
builder:setArchiveTypes(bit32.bor(
    dreamArchive.bsa.tes4.archiveTypes.MESHES,
    dreamArchive.bsa.tes4.archiveTypes.TEXTURES
))
```

Legacy BSA filenames must still be encoded explicitly from Luau:

```luau
local path = dreamArchive.bsa.encodeFilename(
    "textures/zażółć.dds",
    dreamArchive.bsa.encoding.windows1250
)
local bytes = archive:readFileRequired(path)
```

Optional read/extract APIs return `nil` when the archive does not contain the
path. `detectPath` and `guessFormat` return `nil` for unknown or unsupported
headers. The `*Required` variants raise an error instead:

```luau
local maybe = archive:readFile("meshes/foo.nif")
if maybe == nil then
    -- absent from the archive
end

local ok, err = pcall(function()
    archive:extractFileRequired("missing.nif")
end)
if not ok then
    print(err)
end
```

`normalizePath` returns normalized archive path bytes, not a Unicode-cleaned OS
path. It normalizes to forward slashes; stored entry names and hash helper
`normalized` fields may display backslashes depending on the archive family.
TES3/TES4 64-bit hashes are exposed as exact component fields plus a fixed width
hexadecimal string; they are not exposed as Lua numbers because Luau numbers are
doubles, which are not a safe exact carrier for arbitrary `u64` values.

```luau
local ba2 = dreamArchive.ba2.hashFile("Meshes/Foo.NIF")
-- ba2.directory, ba2.file, ba2.extension, ba2.normalized

local tes3 = dreamArchive.bsa.tes3.hashFile("Meshes/Foo.NIF")
-- tes3.lo, tes3.hi, tes3.hex, tes3.normalized

local tes4 = dreamArchive.bsa.tes4.hashFile("Meshes/Foo.NIF")
-- tes4.last, tes4.last2, tes4.length, tes4.first, tes4.crc, tes4.hex
-- no numeric u64 field; use hex for exact identity
```

To support Luau's `require`, register the module yourself. Luau `require` looks
registered modules up by an `@`-prefixed name:

```rust,no_run
fn main() -> mlua::Result<()> {
let lua = mlua::Lua::new();
lua.register_module("@dreamArchive", dream_archive::lua::create_module(&lua)?)?;
lua.register_module(
    "@dreamPath",
    dream_archive::dream_path::lua::create_module(&lua)?,
)?;

// Luau side:
// local dreamArchive = require("@dreamArchive")
Ok(())
}
```

## Compatibility policy

The target is real PC archive compatibility. Console archive layouts and XMem
compression are not implemented. If a future PC archive proves a currently
unsupported path is real, useful, and not just format archaeology with a hat, it
can be added with tests.

## Support

Has `dream_archive` been useful to you?

If so, please consider [amplifying the signal](https://ko-fi.com/magicaldave) through my ko-fi.

Thank you for using `dream_archive`.
