# dream_archive

Pure-Rust Bethesda archive tooling for common PC archive layouts. It reads,
lists, extracts, and builds the archive families used by Morrowind, Oblivion,
Fallout, Skyrim, Fallout 4, and Starfield-era games, subject to the format rows
below rather than wishful thinking.

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
- `lua`: enables the `mlua` bindings and pulls in `ba2`, `bsa`, `mlua`'s
  `luajit52` support, and vendored LuaJIT sources.

## Lua bindings

Enable it in `Cargo.toml`:

```toml
dream_archive = { version = "0.1", features = ["lua"] }
```

The `lua` feature exposes a byte-first Lua API through `dream_archive::lua` for
Rust embedders. It does not install a standalone `require("dream_archive")` C Lua
module by itself; register the `mlua` table in your application. Lua strings are
archive path bytes and payload bytes. Filesystem arguments are the exception:
all `open_path`, `detect_path`, `write_path`, `extract_to*`,
`extract_entry_to_path`, `add_dir`, and source paths such as `add_file` /
`add_dds_file` are converted as UTF-8 host paths. Archive paths passed to
`read_file`, `add_bytes`, `add_file`'s first argument, hash helpers, and
`normalize_path` remain raw archive path bytes. `bsa.encode_filename()` and
builder `add_encoded_path()` are the opposite boundary: they take UTF-8 text and
produce or insert legacy-encoded archive filename bytes. If your Unix filesystem
path is not valid UTF-8, use the Rust API directly.
Annoying, but less dishonest than pretending all paths are the same kind of
string.

```rust,no_run
fn main() -> mlua::Result<()> {
let lua = mlua::Lua::new();
let module = dream_archive::lua::create_module(&lua)?;
lua.globals().set("dream_archive", module)?;

lua.load(r#"
    local builder = dream_archive.ba2.Builder.new()
    builder:set_compression(dream_archive.ba2.compression.zip)
    builder:add_bytes("meshes/example.nif", "payload")

    -- to_bytes()/to_string() return raw archive bytes as a Lua string.
    local archive = dream_archive.open_bytes(builder:to_bytes())
    assert(archive:format() == "ba2")
    assert(archive:read_file_required("meshes/example.nif") == "payload")
"#).exec()?;
Ok(())
}
```

The module exposes:

- top-level detection/open/read/extract facade: `open_path`, `open_bytes`,
  `detect_path`, `guess_format`, archive `entries`, `read_file*`,
  `extract_file*`, `read_entry`, `extract_entry`, `extract_entry_to_path`, and
  `extract_to`;
- BA2-specific archive inspection, hashes, GNRL builder, and DX10 builder;
- BSA encoding/decoding helpers and path normalization;
- TES3/TES4 archive inspection, hashes, builders, TES4 profiles, name modes,
  compression policy, and archive type bits.

Common calls look like this:

| Lua call | Result |
| --- | --- |
| `dream_archive.open_bytes(bytes)` | generic BA2/TES3/TES4 archive |
| `dream_archive.guess_format(bytes)` | `"ba2"`, `"bsa-tes3"`, `"bsa-tes4"`, or `nil` |
| `archive:entries()` | array-style table of copied entry metadata |
| `archive:read_file(path)` | payload bytes or `nil` |
| `archive:read_file_required(path)` | payload bytes or error |
| `archive:extract_file(path)` | extracted payload bytes or `nil` |
| `archive:extract_file_required(path)` | extracted payload bytes or error |
| `archive:extract_to(target)` | writes files for side effect; errors throw |
| `archive:read_entry(1)` | reads the first entry; entry indices are 1-based |
| `builder:add_bytes(path, bytes)` | adds archive path bytes and payload bytes |
| `builder:to_bytes()` | raw archive bytes |

`entries()` returns a fully materialized Lua table, copying entry metadata into
Lua values. That is convenient for scripts and not a streaming iterator.
`read_file*`, `extract_file*`, and `extract_entry` also materialize the complete
payload as a Lua string; for large archives that means Rust buffering plus a Lua
string copy. `open_bytes(bytes)` copies the Lua archive string into Rust-owned
storage, and builder `to_bytes()` / `to_string()` build a Rust archive buffer and
then copy it into Lua. For large archives, prefer `open_path()` and
`write_path()`. Likewise TES4 `extract_to_with_paths()` copies the supplied
contiguous Lua sequence (`1..n`, no gaps) of candidate archive path byte strings
before it starts matching hash-only entries. It is not a key/value dictionary;
non-sequence keys are ignored.

```lua
local hash_only = dream_archive.bsa.tes4.open_path("HashOnly.bsa")
hash_only:extract_to_with_paths("out", {
    "meshes/foo.nif",
    "textures/foo.dds",
})
```

TES4 hash-only entries cannot expose names they do not have. Their Lua entry
tables use `nil` for `path`, `folder`, and `name`; use `folder_hash.hex` and
`file_hash.hex` when you need exact identity.

Use `dream_archive.open_*` when you want one generic reader for BA2/TES3/TES4.
Use `dream_archive.ba2.open_*`, `dream_archive.bsa.tes3.open_*`, or
`dream_archive.bsa.tes4.open_*` when you need format-specific metadata or helper
APIs such as BA2 `info()` or TES4 `extract_to_with_paths()`.

BA2 archive compression accepts `nil`, `"none"`/`"store"`, `"zip"`, or `"lz4"`.
Per-file compression overrides accept `nil`, `"inherit"`, `"store"`, or
`"compress"`. `set_zlib_level(level)` accepts `0..=9` and rejects other values at
the setter call, not later during `to_bytes()`.

DX10 BA2 texture building needs either a DDS source (`add_dds_bytes` /
`add_dds_file`) or a raw texture payload plus the BA2 texture metadata. The bytes
passed to `add_texture_bytes()` are raw texture payload bytes, not a complete DDS
file:

```lua
local dx10 = dream_archive.ba2.Dx10Builder.new()
dx10:add_texture_bytes("textures/foo.dds", {
    height = 1,
    width = 1,
    mip_count = 1,
    format = 61,
    flags = 0,
    tile_mode = 0,
}, raw_texture_payload)
```

TES4 archive type bits are numeric flags. Combine them with addition or your
LuaJIT bit library of choice:

```lua
builder:set_archive_types(
    dream_archive.bsa.tes4.archive_types.meshes
  + dream_archive.bsa.tes4.archive_types.textures
)
```

Legacy BSA filenames must still be encoded explicitly from Lua:

```lua
local path = dream_archive.bsa.encode_filename(
    "textures/zażółć.dds",
    dream_archive.bsa.encoding.windows1250
)
local bytes = archive:read_file_required(path)
```

Optional read/extract APIs return `nil` when the archive does not contain the
path. `detect_path` and `guess_format` return `nil` for unknown or unsupported
headers. The `*_required` variants raise an error instead:

```lua
local maybe = archive:read_file("meshes/foo.nif")
if maybe == nil then
    -- absent from the archive
end

local ok, err = pcall(function()
    archive:extract_file_required("missing.nif")
end)
if not ok then
    print(err)
end
```

`normalize_path` returns normalized archive path bytes, not a Unicode-cleaned OS
path. It normalizes to forward slashes; stored entry names and hash helper
`normalized` fields may display backslashes depending on the archive family.
TES3/TES4 64-bit hashes are exposed as exact component fields plus a fixed width
hexadecimal string; they are not exposed as Lua numbers because LuaJIT numbers
are not a safe exact carrier for arbitrary `u64` values.

```lua
local ba2 = dream_archive.ba2.hash_file("Meshes/Foo.NIF")
-- ba2.directory, ba2.file, ba2.extension, ba2.normalized

local tes3 = dream_archive.bsa.tes3.hash_file("Meshes/Foo.NIF")
-- tes3.lo, tes3.hi, tes3.hex, tes3.normalized

local tes4 = dream_archive.bsa.tes4.hash_file("Meshes/Foo.NIF")
-- tes4.last, tes4.last2, tes4.length, tes4.first, tes4.crc, tes4.hex
-- no numeric u64 field; use hex for exact identity
```

To support Lua's `require`, preload the module yourself:

```rust,no_run
fn main() -> mlua::Result<()> {
let lua = mlua::Lua::new();
let package: mlua::Table = lua.globals().get("package")?;
let preload: mlua::Table = package.get("preload")?;
preload.set(
    "dream_archive",
    lua.create_function(|lua, ()| dream_archive::lua::create_module(lua))?,
)?;

// Lua side:
// local dream_archive = require("dream_archive")
Ok(())
}
```

## Compatibility policy

The target is real PC archive compatibility. Console archive layouts and XMem
compression are not implemented. If a future PC archive proves a currently
unsupported path is real, useful, and not just format archaeology with a hat, it
can be added with tests.

## Support

Has `dream-archive` been useful to you?

If so, please consider [amplifying the signal](https://ko-fi.com/magicaldave) through my ko-fi.

Thank you for using `dream-archive`.
