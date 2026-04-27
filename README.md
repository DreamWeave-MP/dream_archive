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

# fn main() -> dream_archive::Result<()> {
let archive = Archive::open_path("Data/SomeArchive.bsa")?;

for entry in archive.entries() {
    if let Some(path) = entry.path() {
        println!("{path}");
    }
}
# Ok(())
# }
```

Read a required file:

```rust,no_run
# use dream_archive::Archive;
# fn main() -> dream_archive::Result<()> {
# let archive = Archive::open_path("Data/SomeArchive.bsa")?;
let bytes = archive.read_file_required("meshes/foo.nif")?;
println!("{} bytes", bytes.len());
# Ok(())
# }
```

Extract everything with paths stored in the archive:

```rust,no_run
# use dream_archive::Archive;
# fn main() -> dream_archive::Result<()> {
let archive = Archive::open_path("Data/SomeArchive.ba2")?;
archive.extract_to("out")?;
# Ok(())
# }
```

Some archive layouts do not contain enough text to name every file. Hash-only
TES4 archives need the TES4-specific `extract_to_with_paths` API with a path
dictionary. BA2 archives without string tables still have hashes, but
`Entry::path()` returns `None` through the top-level facade because no filename
text exists to recover. No path does not mean empty filename.

```rust,no_run
use dream_archive::bsa::tes4::Archive;

# fn main() -> dream_archive::bsa::Result<()> {
let archive = Archive::open_path("HashOnly.bsa")?;
archive.extract_to_with_paths("out", [
    b"meshes/foo.nif".as_slice(),
    b"textures/foo.dds".as_slice(),
])?;
# Ok(())
# }
```

## Building archives

Build a TES3/Morrowind BSA from a directory:

```rust,no_run
use dream_archive::Tes3BsaBuilder;

# fn main() -> dream_archive::bsa::Result<()> {
let mut builder = Tes3BsaBuilder::new();
builder.add_dir("Data")?;
builder.write_path("MyMod.bsa")?;
# Ok(())
# }
```

Build a TES4-family BSA using a PC game profile:

```rust,no_run
use dream_archive::{
    Tes4BsaBuilder,
    bsa::tes4::{ArchiveTypes, NameMode},
};

# fn main() -> dream_archive::bsa::Result<()> {
let mut builder = Tes4BsaBuilder::skyrim_le();
builder.set_archive_types(ArchiveTypes::MISC);
builder.set_name_mode(NameMode::Strings);
builder.add_dir("Data")?;
builder.write_path("MyMod.bsa")?;
# Ok(())
# }
```

Build a BA2 GNRL archive:

```rust,no_run
use dream_archive::{Ba2Builder, ba2::Ba2CompressionFormat};

# fn main() -> dream_archive::ba2::Result<()> {
let mut builder = Ba2Builder::new();
builder.set_compression(Some(Ba2CompressionFormat::Zip));
builder.add_dir("Data")?;
builder.write_path("MyMod.ba2")?;
# Ok(())
# }
```

Build a BA2 DX10 texture archive from DDS files:

```rust,no_run
use dream_archive::{Ba2Dx10Builder, ba2::Ba2CompressionFormat};

# fn main() -> dream_archive::ba2::Result<()> {
let mut builder = Ba2Dx10Builder::new();
builder.set_compression(Some(Ba2CompressionFormat::Zip));
builder.add_dds_file("textures/example.dds", "source/example.dds")?;
builder.write_path("Textures.ba2")?;
# Ok(())
# }
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
use dream_archive::bsa::{FilenameEncoding, encode_filename};

# fn main() -> dream_archive::bsa::Result<()> {
let encoded = encode_filename("textures/zażółć.dds", FilenameEncoding::Windows1250)?;
# let archive = dream_archive::Archive::open_path("Data/SomeArchive.bsa")?;
let bytes = archive.read_file_required(encoded.as_ref())?;
# let _ = bytes;
# Ok(())
# }
```

For GNMF BA2 archives, inspect metadata rather than attempting extraction:

```rust,no_run
# fn main() -> dream_archive::ba2::Result<()> {
use dream_archive::ba2::{Archive, PayloadFormat};

let archive = Archive::open_path("Textures.ba2")?;
if archive.info().format == PayloadFormat::GNMF {
    eprintln!("GNMF metadata is readable; extraction is unsupported");
    return Ok(());
}
# Ok(())
# }
```

## Feature flags

Default features enable BA2 and both BSA families.

- `ba2`: BA2 reader/extractor plus GNRL and DX10 writers.
- `bsa-tes3`: TES3 BSA support.
- `bsa-tes4`: TES4-family BSA support.
- `bsa`: both BSA families.
- `parallel`: parallel extraction with Rayon.
- `lua`: enables the `mlua` bindings and pulls in `ba2`, `bsa`, LuaJIT 5.2
  compatibility, and vendored LuaJIT sources.

## Lua bindings

The `lua` feature exposes a byte-first Lua API through `dream_archive::lua`.
Lua strings are archive path bytes and payload bytes; filesystem APIs are the
only methods that interpret strings as host paths. This mirrors the Rust API
instead of quietly converting old BSA paths through whatever Unicode guess was
nearest. That would be convenient right up until it corrupts a mod.

```rust,no_run
# fn main() -> mlua::Result<()> {
let lua = mlua::Lua::new();
let module = dream_archive::lua::create_module(&lua)?;
lua.globals().set("dream_archive", module)?;

lua.load(r#"
    local builder = dream_archive.ba2.Builder.new()
    builder:set_compression("zip")
    builder:add_bytes("meshes/example.nif", "payload")

    local archive = dream_archive.open_bytes(builder:to_string())
    assert(archive:format() == "ba2")
    assert(archive:read_file_required("meshes/example.nif") == "payload")
"#).exec()?;
# Ok(())
# }
```

The module exposes:

- top-level detection/open/read/extract facade: `open_path`, `open_bytes`,
  `detect_path`, `guess_format`, archive `entries`, `read_file*`, and
  `extract_to`;
- BA2-specific archive inspection, hashes, GNRL builder, and DX10 builder;
- BSA encoding/decoding helpers and path normalization;
- TES3/TES4 archive inspection, hashes, builders, TES4 profiles, name modes,
  compression policy, and archive type bits.

## Compatibility policy

The target is real PC archive compatibility. Console archive layouts and XMem
compression are not implemented. If a future PC archive proves a currently
unsupported path is real, useful, and not just format archaeology with a hat, it
can be added with tests.
