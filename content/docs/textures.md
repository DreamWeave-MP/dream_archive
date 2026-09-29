+++
title = "BA2 textures"
description = "DX10 archives: the texture header, the DDS files that come out, the DDS files that go in, the formats supported, and copying textures between archives."
weight = 70

[extra]
kind = "guide"
+++

A DX10 BA2 does not store DDS files. It stores each texture's dimensions, mip count and DXGI
format in its record, and the pixel data in chunks, one per range of mips, with the DDS header
left out. dream_archive puts the header back when a texture comes out, and takes it off when one
goes in. It is archive plumbing, not a texture library: it does not decode pixels, convert
formats or generate mips.

## The texture header

Each entry of a DX10 archive has `FileHeader::DX10(TextureHeader)` in `entry.file().header`:

| Field | Type | Meaning |
|---|---|---|
| `height`, `width` | `u16` | the top mip's size in pixels |
| `mip_count` | `u8` | how many mips, at least 1 |
| `format` | `u8` | the DXGI format number |
| `flags` | `u8` | bit 0 marks a cubemap, whose data holds six faces |
| `tile_mode` | `u8` | the high byte of the archive's flags field; kept, not interpreted |

The entry's chunks, `entry.file().chunks()`, each carry the inclusive range of mips they hold in
`mips`. When the archive opens, every texture is checked: its format must be one of the
[supported formats](#supported-formats), its mip ranges must cover every mip exactly once, and
each chunk's size must match its mips. An archive that fails any of these does not open, with a
`Dds` error naming the problem.

## Extracting textures

Reading or extracting a DX10 entry produces a complete DDS file: a header built from the
`TextureHeader`, followed by the chunks decoded in order. Formats the original DDS header can
describe get it (`DXT1`, `DXT5`, RGB masks and the like); sRGB, BC6H, BC7 and integer formats get
the extended `DX10` header, which for a cubemap gives an array size of 6. `extracted_len` counts
the header.

Only what the archive stores can be put back. A DDS file that went into a BA2 comes out with the
same pixel data, but fields the archive never kept, such as the reserved words some tools write
into the header, come out as zero.

## Building from DDS files

```rust
use dream_archive::Ba2Dx10Builder;
use dream_archive::ba2::{Archive, Ba2CompressionFormat, FileHeader};

fn main() -> dream_archive::ba2::Result<()> {
    let mut builder = Ba2Dx10Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder.add_dds_file("textures/fence_roughness.dds", "Fence006_1K_Roughness.dds")?;
    builder.write_path("Textures.ba2")?;

    let archive = Archive::open_path("Textures.ba2")?;
    let entry = archive.get_required("textures/fence_roughness.dds")?;
    if let FileHeader::DX10(texture) = entry.file().header {
        println!(
            "{}x{}, {} mips, DXGI format {}",
            texture.width, texture.height, texture.mip_count, texture.format
        );
    }
    Ok(())
}
```

```text
1024x1024, 11 mips, DXGI format 98
```

`add_dds_bytes` and `add_dds_file` parse the DDS header, turn it into a `TextureHeader`, check
that the data after it is exactly the size the header describes, and store the data as one chunk
holding every mip. `add_dds_file` reads the file when it is called, not when the archive is
written. A DDS file is refused, with a `Dds` error, when:

- it is not a DDS file: no `DDS ` magic, or a header or pixel-format size that is not 124 or 32;
- its depth field is anything but 0, which is read as a volume texture even when the header's
  flags do not say it is one. Some tools write 1 there for ordinary textures, and their files are
  refused;
- its extended `DX10` header describes anything but a 2D texture with an array size of 1, or a
  cubemap with an array size of 6. The DDS specification counts cubes, not faces, in that field,
  so a cubemap saved by a tool that follows it, with an array size of 1, is refused;
- its pixel format is not one of the supported formats;
- its width or height is above 65535, or it has more than 255 mips;
- the data after the header is not exactly as long as its dimensions, format, mips and faces
  require. Padding, extra data and missing mips are all refused.

The formats a DDS file can arrive in: the legacy FourCCs `DXT1`, `DXT3`, `DXT5`, `BC4U`, `BC4S`,
`BC5U` and `BC5S`; any supported format behind an extended `DX10` header; and legacy RGB, luminance
or alpha masks for the uncompressed formats marked in the table below. `DXT2`, `DXT4`, `ATI1` and
`ATI2` are not accepted; the texture has to be resaved with one of the others.

## Building from texture data

When the texture is already split into header and data, `add_texture_bytes` takes both. The data
is the pixel data only, without a DDS header, and must be exactly the size the header describes:

```rust
use dream_archive::Ba2Dx10Builder;
use dream_archive::ba2::{Archive, TextureHeader};

fn main() -> dream_archive::ba2::Result<()> {
    // A 4x4 BC1 texture is one 8-byte block.
    let header = TextureHeader { height: 4, width: 4, mip_count: 1, format: 71, flags: 0, tile_mode: 0 };
    let mut builder = Ba2Dx10Builder::new();
    builder.add_texture_bytes("textures/black.dds", header, [0u8; 8])?;

    let archive = Archive::from_vec(builder.to_vec()?)?;
    let dds = archive.read_file_required("textures/black.dds")?;
    assert_eq!(dds.len(), 128 + 8);
    Ok(())
}
```

`add_texture_file` does the same with the data in a file, read when the archive is written, and
`add_dir_with_texture_header` adds every file below a directory with one shared header, which is
only useful for test archives. All of them have `_with_compression` forms.

## Copying textures between archives

`Dx10Builder::add_archive_entry` copies a texture from an opened DX10 archive without decoding
it: its header, its chunk records with their mip ranges, and its stored chunk bytes, compressed or
not, go into the new archive unchanged. That only works when the stored bytes mean the same thing
in both archives, so writing fails with `NotImplemented` unless the source archive has the same
version as the builder and the same compression method: zlib, when the builder's compression is
`None` or `Zip`, or LZ4.

## Supported formats

| DXGI | Format | Extracted header | Accepted from legacy masks |
|---:|---|---|---|
| 71 | `BC1_UNORM` | `DXT1` | |
| 72 | `BC1_UNORM_SRGB` | `DX10` | |
| 74 | `BC2_UNORM` | `DXT3` | |
| 75 | `BC2_UNORM_SRGB` | `DX10` | |
| 77 | `BC3_UNORM` | `DXT5` | |
| 78 | `BC3_UNORM_SRGB` | `DX10` | |
| 80 | `BC4_UNORM` | `BC4U` | |
| 81 | `BC4_SNORM` | `BC4S` | |
| 83 | `BC5_UNORM` | `BC5U` | |
| 84 | `BC5_SNORM` | `BC5S` | |
| 95 | `BC6H_UF16` | `DX10` | |
| 96 | `BC6H_SF16` | `DX10` | |
| 98 | `BC7_UNORM` | `DX10` | |
| 99 | `BC7_UNORM_SRGB` | `DX10` | |
| 28 | `R8G8B8A8_UNORM` | RGB masks | yes |
| 29 | `R8G8B8A8_UNORM_SRGB` | `DX10` | |
| 30 | `R8G8B8A8_UINT` | `DX10` | |
| 32 | `R8G8B8A8_SINT` | `DX10` | |
| 87 | `B8G8R8A8_UNORM` | RGB masks | yes |
| 91 | `B8G8R8A8_UNORM_SRGB` | `DX10` | |
| 88 | `B8G8R8X8_UNORM` | RGB masks | yes |
| 93 | `B8G8R8X8_UNORM_SRGB` | `DX10` | |
| 85 | `B5G6R5_UNORM` | RGB masks | yes |
| 86 | `B5G5R5A1_UNORM` | RGB masks | yes |
| 49 | `R8G8_UNORM` | luminance and alpha masks | yes |
| 50 | `R8G8_UINT` | `DX10` | |
| 52 | `R8G8_SINT` | `DX10` | |
| 61 | `R8_UNORM` | luminance mask | yes |
| 62 | `R8_UINT` | `DX10` | |
| 63 | `R8_SNORM` | `DX10` | |
| 64 | `R8_SINT` | `DX10` | |
| 65 | `A8_UNORM` | alpha mask | yes |
