# dream_archive

Read, extract and write Bethesda's BSA and BA2 archives in pure Rust, from Morrowind to
Starfield.

One type opens all three generations: Morrowind's BSA, the TES4 BSA of Oblivion through Skyrim
SE, and the BA2 of Fallout 4 and Starfield. The same calls list, look up, read, stream and extract
any of them, and a builder writes each family it reads. Archive paths stay bytes, because the
formats never say which code page their names are in; the crate converts only when you name one.
With the `luau` feature, it is also an [l3i](https://github.com/DreamWeave-MP/l3i) extension
providing the Luau module `@dream/archive`.

**Documentation, including the full Rust and Luau API reference:
<https://dreamweave-mp.github.io/dream_archive/>**

## Install

```sh
cargo add dream_archive
```

Rust 1.88. The default features read and write every family; `parallel` adds parallel
extraction, and `luau` the Luau module.

## Formats

| Family | Games | Read and extract | Write |
|---|---|---|---|
| TES3 BSA | Morrowind | yes | yes |
| TES4 BSA, versions 103 to 105 | Oblivion to Skyrim SE | yes, including hash-only archives | yes |
| BA2 GNRL | Fallout 4, Fallout 76, Starfield | yes | yes |
| BA2 DX10 | Fallout 4, Fallout 76, Starfield | yes, as DDS files | from DDS files or texture metadata |
| BA2 GNMF | console textures | metadata only | no |

Console layouts and XMem compression are refused with an error.

## Usage

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("Morrowind.bsa")?;
    for entry in archive.entries().take(3) {
        if let Some(path) = entry.path() {
            println!("{path}");
        }
    }

    // Any spelling of the path: case and slash direction do not matter.
    let texture = archive.read_file_required("Textures/Tx_Wood.DDS")?;
    println!("{} bytes", texture.len());
    Ok(())
}
```

```rust
use dream_archive::Tes3BsaBuilder;

fn main() -> dream_archive::bsa::Result<()> {
    let mut builder = Tes3BsaBuilder::new();
    builder.add_dir("MyMod")?;
    builder.write_path("MyMod.bsa")?;
    Ok(())
}
```

```luau
local dreamArchive = require("@dream/archive")
local archive = dreamArchive.openPath("Morrowind.bsa")
local entry = archive:get("textures/tx_wood.dds")
print(entry.size, entry.hashHex)
```

For a command line, see [dream_archivetool](https://DreamWeave-MP.github.io/dream_archivetool/),
which is built on this crate.

## Where to read next

- [Start here](https://dreamweave-mp.github.io/dream_archive/docs/start-here/): open, read,
  extract, build
- [Formats](https://dreamweave-mp.github.io/dream_archive/docs/formats/): versions, names,
  hashes, compression, and what is refused
- [Archive paths](https://dreamweave-mp.github.io/dream_archive/docs/paths/) and
  [hashes](https://dreamweave-mp.github.io/dream_archive/docs/hashes/): lookups, legacy code
  pages, and hash-only archives
- [Building archives](https://dreamweave-mp.github.io/dream_archive/docs/building/) and
  [BA2 textures](https://dreamweave-mp.github.io/dream_archive/docs/textures/)
- [Embedding Luau](https://dreamweave-mp.github.io/dream_archive/docs/luau-hosts/): l3i setup, and
  moving from the 0.2 `mlua` binding
- [Rust API](https://dreamweave-mp.github.io/dream_archive/docs/api/) and
  [Luau API](https://dreamweave-mp.github.io/dream_archive/docs/luau/)
- [Changelog](https://dreamweave-mp.github.io/dream_archive/home/changelog/)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -W clippy::pedantic -D warnings
cargo test --all-features
```

The `luau` feature needs l3i's toolchain, clang, lld and cross-language thin LTO, which
`.cargo/config.toml` sets; the test-only `luau-analysis` feature adds l3i's analysis frontend for
the typed tests, so `cargo test --features luau` builds without it. The site in `content/` is a
[DreamWeave Mod Template](https://github.com/DreamWeave-MP/DreamWeave-Mod-Template) site; preview
it with `zola serve`.

## License

GPL-3.0-only.

## Support

Has dream_archive been useful to you? Consider
[amplifying the signal](https://ko-fi.com/magicaldave) through ko-fi.
