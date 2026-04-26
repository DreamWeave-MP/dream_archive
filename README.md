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
| BA2 DX10 | yes | yes | yes | DDS headers are reconstructed from BA2 texture metadata. Writer accepts supported DDS files or explicit texture metadata plus raw payload bytes. |
| BA2 GNMF | metadata | no | no | Sony GNM (`.gnf`) texture archives. Explicitly out of scope for now; metadata is parsed only so callers can identify them instead of getting mystery meat. |
| TES3 BSA | yes | yes | yes | Morrowind-era archives. |
| TES4 BSA | yes | yes | yes | Oblivion/Fallout/Skyrim PC archives, including hash-only and embedded-name layouts. |
| Console/Xbox layouts | no | no | no | Out of scope. |
| XMem compression | no | no | no | Out of scope. |

“Write support” above means the writer semantics for that specific row are
implemented. It does **not** mean “every BA2 payload type is writable.” Words
mean things. Annoying, but useful.

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

Hash-only TES4 archives do not contain enough text to name every file. Use the
TES4-specific `extract_to_with_paths` API with a path dictionary for those.

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
builder.add_dds_file("textures/example.dds", "example.dds")?;
builder.write_path("Textures.ba2")?;
# Ok(())
# }
```

The DDS path parses supported DDS headers, maps them to BA2 texture metadata,
validates the payload size, strips the DDS header, and stores the raw texture
payload. It does not transcode formats or generate mips. If you already have BA2
texture metadata, `add_texture_bytes` still accepts the raw bytes after the DDS
header directly.

DX10/DDS support here is archive plumbing, not a texture processing library. It
does not decode pixels, preserve vendor/private DDS fields, handle texture
arrays/volumes, or aim for DirectXTex parity. A broader DDS crate belongs with a
renderer; this crate only does the pieces BA2 writing and extraction need today.

## Archive paths are not filesystem paths

Archive paths are virtual filesystem names stored inside the archive. They are
byte strings, not necessarily Unicode OS paths.

- Normal ASCII mod paths can be passed as `&str`.
- Legacy localized BSA paths should be encoded explicitly.
- The crate does not guess encodings, because guessing corrupts mods and then
  everyone has a bad afternoon.

Example for a legacy Windows code page path:

```rust,no_run
use dream_archive::bsa::{FilenameEncoding, encode_filename};

# fn main() -> dream_archive::bsa::Result<()> {
let encoded = encode_filename("textures/zażółć.dds", FilenameEncoding::Windows1250)?;
# let _ = encoded;
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

## Compatibility policy

The target is real PC archive compatibility. Console archive layouts and XMem
compression are not implemented. If a future PC archive proves a currently
unsupported path is real, useful, and not just format archaeology with a hat, it
can be added with tests.
