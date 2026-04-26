use crate::ba2::{Error, Result, TextureHeader, fourcc};

const DDSD_CAPS: u32 = 0x0000_0001;
const DDSD_HEIGHT: u32 = 0x0000_0002;
const DDSD_WIDTH: u32 = 0x0000_0004;
const DDSD_PITCH: u32 = 0x0000_0008;
const DDSD_PIXELFORMAT: u32 = 0x0000_1000;
const DDSD_MIPMAPCOUNT: u32 = 0x0002_0000;
const DDSD_LINEARSIZE: u32 = 0x0008_0000;

const DDSCAPS_COMPLEX: u32 = 0x0000_0008;
const DDSCAPS_TEXTURE: u32 = 0x0000_1000;
const DDSCAPS_MIPMAP: u32 = 0x0040_0000;

const DDSCAPS2_CUBEMAP: u32 = 0x0000_0200;
const DDSCAPS2_POSITIVEX: u32 = 0x0000_0400;
const DDSCAPS2_NEGATIVEX: u32 = 0x0000_0800;
const DDSCAPS2_POSITIVEY: u32 = 0x0000_1000;
const DDSCAPS2_NEGATIVEY: u32 = 0x0000_2000;
const DDSCAPS2_POSITIVEZ: u32 = 0x0000_4000;
const DDSCAPS2_NEGATIVEZ: u32 = 0x0000_8000;
const DDSCAPS2_VOLUME: u32 = 0x0020_0000;

const DDPF_ALPHAPIXELS: u32 = 0x0000_0001;
const DDPF_ALPHA: u32 = 0x0000_0002;
const DDPF_FOURCC: u32 = 0x0000_0004;
const DDPF_RGB: u32 = 0x0000_0040;
const DDPF_LUMINANCE: u32 = 0x0002_0000;

const DDS_DIMENSION_TEXTURE2D: u32 = 3;
const DDS_RESOURCE_MISC_TEXTURECUBE: u32 = 4;

pub(crate) const MAX_HEADER_SIZE: usize = 148;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DdsHeader {
    pub height: u16,
    pub width: u16,
    pub mip_count: u8,
    pub format: u8,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DdsTexture<'a> {
    pub header: TextureHeader,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy)]
struct ParsedHeader {
    height: u32,
    width: u32,
    mip_count: u32,
    pixel_format: PixelFormat,
    caps2: u32,
    payload_offset: usize,
}

#[derive(Clone, Copy)]
struct PixelFormat {
    flags: u32,
    fourcc: u32,
    rgb_bits: u32,
    r_mask: u32,
    g_mask: u32,
    b_mask: u32,
    a_mask: u32,
}

#[derive(Clone, Copy)]
enum TextureLayout {
    Block { bytes_per_block: usize },
    Linear { bytes_per_pixel: usize },
}

#[allow(non_upper_case_globals)]
mod dxgi {
    pub const BC1_UNORM: u8 = 71;
    pub const BC1_UNORM_SRGB: u8 = 72;
    pub const BC2_UNORM: u8 = 74;
    pub const BC2_UNORM_SRGB: u8 = 75;
    pub const BC3_UNORM: u8 = 77;
    pub const BC3_UNORM_SRGB: u8 = 78;
    pub const BC4_UNORM: u8 = 80;
    pub const BC4_SNORM: u8 = 81;
    pub const BC5_UNORM: u8 = 83;
    pub const BC5_SNORM: u8 = 84;
    pub const B5G6R5_UNORM: u8 = 85;
    pub const B5G5R5A1_UNORM: u8 = 86;
    pub const B8G8R8A8_UNORM: u8 = 87;
    pub const B8G8R8X8_UNORM: u8 = 88;
    pub const R8G8B8A8_UNORM: u8 = 28;
    pub const R8G8B8A8_UNORM_SRGB: u8 = 29;
    pub const R8G8B8A8_UINT: u8 = 30;
    pub const R8G8B8A8_SINT: u8 = 32;
    pub const R8G8_UNORM: u8 = 49;
    pub const R8G8_UINT: u8 = 50;
    pub const R8G8_SINT: u8 = 52;
    pub const R8_UNORM: u8 = 61;
    pub const R8_UINT: u8 = 62;
    pub const R8_SNORM: u8 = 63;
    pub const R8_SINT: u8 = 64;
    pub const A8_UNORM: u8 = 65;
    pub const B8G8R8A8_UNORM_SRGB: u8 = 91;
    pub const B8G8R8X8_UNORM_SRGB: u8 = 93;
    pub const BC6H_UF16: u8 = 95;
    pub const BC6H_SF16: u8 = 96;
    pub const BC7_UNORM: u8 = 98;
    pub const BC7_UNORM_SRGB: u8 = 99;
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[derive(Default)]
struct HeaderFields {
    flags: u32,
    pitch_or_linear: u32,
    pf_flags: u32,
    fourcc: u32,
    rgb_bits: u32,
    r_mask: u32,
    g_mask: u32,
    b_mask: u32,
    a_mask: u32,
    caps: u32,
    caps2: u32,
    dxgi_format: u32,
    misc_flags: u32,
    array_size: u32,
}

/// Append an OpenMW-compatible DDS header for a BA2 DX10 texture record.
pub(crate) fn write_dds_header(out: &mut Vec<u8>, header: DdsHeader) -> Result<()> {
    let width = u32::from(header.width);
    let height = u32::from(header.height);
    if width == 0 || height == 0 {
        return Err(Error::Dds("zero-sized texture"));
    }
    if header.mip_count == 0 {
        return Err(Error::Dds("zero mip count"));
    }

    let mut fields = base_fields(header);
    apply_format(&mut fields, header.format, width, height)?;
    emit_header(out, header, width, height, &fields);
    Ok(())
}

pub(crate) fn validate_texture_header(header: TextureHeader) -> Result<()> {
    let mut out = Vec::new();
    write_dds_header(
        &mut out,
        DdsHeader {
            height: header.height,
            width: header.width,
            mip_count: header.mip_count,
            format: header.format,
            flags: header.flags,
        },
    )
}

pub(crate) fn validate_texture_payload_size(header: TextureHeader, actual: usize) -> Result<()> {
    validate_payload_size(header, actual)
}

pub(crate) fn validate_texture_mip_payload_size(
    header: TextureHeader,
    first_mip: u16,
    last_mip: u16,
    actual: usize,
) -> Result<()> {
    let expected = texture_mip_payload_size(header, first_mip, last_mip)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Dds("DDS payload size does not match metadata"))
    }
}

pub(crate) fn parse_dds_for_dx10(bytes: &[u8]) -> Result<DdsTexture<'_>> {
    let parsed = parse_header(bytes)?;
    let format = parse_format(bytes, parsed)?;
    let flags = u8::from(parsed.is_cubemap(bytes)?);
    let header = TextureHeader {
        height: parsed
            .height
            .try_into()
            .map_err(|_| Error::Dds("texture height exceeds BA2 field"))?,
        width: parsed
            .width
            .try_into()
            .map_err(|_| Error::Dds("texture width exceeds BA2 field"))?,
        mip_count: parsed
            .mip_count
            .try_into()
            .map_err(|_| Error::Dds("mip count exceeds BA2 field"))?,
        format,
        flags,
        tile_mode: 0,
    };
    validate_texture_header(header)?;

    let payload = bytes
        .get(parsed.payload_offset..)
        .ok_or(Error::Dds("truncated DDS payload"))?;
    validate_payload_size(header, payload.len())?;
    Ok(DdsTexture { header, payload })
}

fn parse_header(bytes: &[u8]) -> Result<ParsedHeader> {
    if bytes.len() < 128 {
        return Err(Error::Dds("truncated DDS header"));
    }
    if &bytes[0..4] != b"DDS " {
        return Err(Error::Dds("invalid DDS magic"));
    }
    if read_u32_at(bytes, 4)? != 124 {
        return Err(Error::Dds("invalid DDS header size"));
    }
    if read_u32_at(bytes, 76)? != 32 {
        return Err(Error::Dds("invalid DDS pixel format size"));
    }
    let depth = read_u32_at(bytes, 24)?;
    let caps2 = read_u32_at(bytes, 112)?;
    if depth != 0 || caps2 & DDSCAPS2_VOLUME != 0 {
        return Err(Error::Dds("unsupported DDS volume texture"));
    }

    let pixel_format = PixelFormat {
        flags: read_u32_at(bytes, 80)?,
        fourcc: read_u32_at(bytes, 84)?,
        rgb_bits: read_u32_at(bytes, 88)?,
        r_mask: read_u32_at(bytes, 92)?,
        g_mask: read_u32_at(bytes, 96)?,
        b_mask: read_u32_at(bytes, 100)?,
        a_mask: read_u32_at(bytes, 104)?,
    };
    let payload_offset = if pixel_format.fourcc == fourcc(*b"DX10") {
        if bytes.len() < 148 {
            return Err(Error::Dds("truncated DDS DX10 header"));
        }
        148
    } else {
        128
    };
    Ok(ParsedHeader {
        height: read_u32_at(bytes, 12)?,
        width: read_u32_at(bytes, 16)?,
        mip_count: read_u32_at(bytes, 28)?.max(1),
        pixel_format,
        caps2,
        payload_offset,
    })
}

fn parse_format(bytes: &[u8], parsed: ParsedHeader) -> Result<u8> {
    let pixel_format = parsed.pixel_format;
    if pixel_format.flags & DDPF_FOURCC != 0 {
        return parse_fourcc_format(bytes, parsed.payload_offset, pixel_format.fourcc);
    }
    parse_plain_format(pixel_format)
}

fn parse_fourcc_format(bytes: &[u8], payload_offset: usize, value: u32) -> Result<u8> {
    match value {
        value if value == fourcc(*b"DXT1") => Ok(dxgi::BC1_UNORM),
        value if value == fourcc(*b"DXT3") => Ok(dxgi::BC2_UNORM),
        value if value == fourcc(*b"DXT5") => Ok(dxgi::BC3_UNORM),
        value if value == fourcc(*b"BC4U") => Ok(dxgi::BC4_UNORM),
        value if value == fourcc(*b"BC4S") => Ok(dxgi::BC4_SNORM),
        value if value == fourcc(*b"BC5U") => Ok(dxgi::BC5_UNORM),
        value if value == fourcc(*b"BC5S") => Ok(dxgi::BC5_SNORM),
        value if value == fourcc(*b"DX10") => parse_dx10_format(bytes, payload_offset),
        _ => Err(Error::Dds("unsupported DDS FourCC")),
    }
}

fn parse_dx10_format(bytes: &[u8], payload_offset: usize) -> Result<u8> {
    debug_assert_eq!(payload_offset, 148);
    let format = read_u32_at(bytes, 128)?;
    let dimension = read_u32_at(bytes, 132)?;
    let array_size = read_u32_at(bytes, 140)?;
    if dimension != DDS_DIMENSION_TEXTURE2D {
        return Err(Error::Dds("unsupported DDS resource dimension"));
    }
    let misc_flags = read_u32_at(bytes, 136)?;
    let cube = misc_flags & DDS_RESOURCE_MISC_TEXTURECUBE != 0;
    if (cube && array_size != 6) || (!cube && array_size != 1) {
        return Err(Error::Dds("unsupported DDS array size"));
    }
    let format = u8::try_from(format).map_err(|_| Error::Dds("unsupported DXGI format"))?;
    let _ = texture_layout(format)?;
    Ok(format)
}

impl ParsedHeader {
    fn is_cubemap(self, bytes: &[u8]) -> Result<bool> {
        if self.payload_offset == 148 {
            Ok(read_u32_at(bytes, 136)? & DDS_RESOURCE_MISC_TEXTURECUBE != 0)
        } else {
            Ok(self.caps2 & DDSCAPS2_CUBEMAP != 0)
        }
    }
}

fn parse_plain_format(pixel_format: PixelFormat) -> Result<u8> {
    match pixel_format {
        PixelFormat {
            flags,
            rgb_bits: 32,
            r_mask: 0x0000_00ff,
            g_mask: 0x0000_ff00,
            b_mask: 0x00ff_0000,
            a_mask: 0xff00_0000,
            ..
        } if flags & (DDPF_RGB | DDPF_ALPHAPIXELS) == (DDPF_RGB | DDPF_ALPHAPIXELS) => {
            Ok(dxgi::R8G8B8A8_UNORM)
        }
        PixelFormat {
            flags,
            rgb_bits: 32,
            r_mask: 0x00ff_0000,
            g_mask: 0x0000_ff00,
            b_mask: 0x0000_00ff,
            a_mask: 0xff00_0000,
            ..
        } if flags & (DDPF_RGB | DDPF_ALPHAPIXELS) == (DDPF_RGB | DDPF_ALPHAPIXELS) => {
            Ok(dxgi::B8G8R8A8_UNORM)
        }
        PixelFormat {
            flags,
            rgb_bits: 32,
            r_mask: 0x00ff_0000,
            g_mask: 0x0000_ff00,
            b_mask: 0x0000_00ff,
            a_mask: 0,
            ..
        } if flags & DDPF_RGB != 0 => Ok(dxgi::B8G8R8X8_UNORM),
        PixelFormat {
            flags,
            rgb_bits: 16,
            r_mask: 0x0000_f800,
            g_mask: 0x0000_07e0,
            b_mask: 0x0000_001f,
            a_mask: 0,
            ..
        } if flags & DDPF_RGB != 0 => Ok(dxgi::B5G6R5_UNORM),
        PixelFormat {
            flags,
            rgb_bits: 16,
            r_mask: 0x0000_7c00,
            g_mask: 0x0000_03e0,
            b_mask: 0x0000_001f,
            a_mask: 0x0000_8000,
            ..
        } if flags & (DDPF_RGB | DDPF_ALPHAPIXELS) == (DDPF_RGB | DDPF_ALPHAPIXELS) => {
            Ok(dxgi::B5G5R5A1_UNORM)
        }
        PixelFormat {
            flags,
            rgb_bits: 16,
            r_mask: 0x0000_00ff,
            a_mask: 0x0000_ff00,
            ..
        } if flags & (DDPF_LUMINANCE | DDPF_ALPHAPIXELS) == (DDPF_LUMINANCE | DDPF_ALPHAPIXELS) => {
            Ok(dxgi::R8G8_UNORM)
        }
        PixelFormat {
            flags,
            rgb_bits: 8,
            a_mask: 0x0000_00ff,
            ..
        } if flags & DDPF_ALPHA != 0 => Ok(dxgi::A8_UNORM),
        PixelFormat {
            flags,
            rgb_bits: 8,
            r_mask: 0x0000_00ff,
            ..
        } if flags & DDPF_LUMINANCE != 0 => Ok(dxgi::R8_UNORM),
        _ => Err(Error::Dds("unsupported DDS pixel format")),
    }
}

fn validate_payload_size(header: TextureHeader, actual: usize) -> Result<()> {
    let expected = texture_payload_size(header)?;
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Dds("DDS payload size does not match metadata"))
    }
}

fn texture_payload_size(header: TextureHeader) -> Result<usize> {
    if header.mip_count == 0 {
        return Err(Error::Dds("zero mip count"));
    }
    texture_mip_payload_size(header, 0, u16::from(header.mip_count.saturating_sub(1)))
}

fn texture_mip_payload_size(header: TextureHeader, first_mip: u16, last_mip: u16) -> Result<usize> {
    let layout = texture_layout(header.format)?;
    let faces = if header.flags & 1 != 0 {
        6usize
    } else {
        1usize
    };
    let mut size = 0usize;
    for mip in first_mip..=last_mip {
        let width: usize = (u32::from(header.width) >> u32::from(mip))
            .max(1)
            .try_into()?;
        let height: usize = (u32::from(header.height) >> u32::from(mip))
            .max(1)
            .try_into()?;
        let face_size = match layout {
            TextureLayout::Block { bytes_per_block } => width
                .div_ceil(4)
                .checked_mul(height.div_ceil(4))
                .and_then(|blocks| blocks.checked_mul(bytes_per_block)),
            TextureLayout::Linear { bytes_per_pixel } => width
                .checked_mul(height)
                .and_then(|pixels| pixels.checked_mul(bytes_per_pixel)),
        }
        .ok_or(Error::OutOfBounds)?;
        size = size
            .checked_add(face_size.checked_mul(faces).ok_or(Error::OutOfBounds)?)
            .ok_or(Error::OutOfBounds)?;
    }
    Ok(size)
}

fn texture_layout(format: u8) -> Result<TextureLayout> {
    match format {
        dxgi::BC1_UNORM | dxgi::BC1_UNORM_SRGB | dxgi::BC4_UNORM | dxgi::BC4_SNORM => {
            Ok(TextureLayout::Block { bytes_per_block: 8 })
        }
        dxgi::BC2_UNORM
        | dxgi::BC2_UNORM_SRGB
        | dxgi::BC3_UNORM
        | dxgi::BC3_UNORM_SRGB
        | dxgi::BC5_UNORM
        | dxgi::BC5_SNORM
        | dxgi::BC6H_UF16
        | dxgi::BC6H_SF16
        | dxgi::BC7_UNORM
        | dxgi::BC7_UNORM_SRGB => Ok(TextureLayout::Block {
            bytes_per_block: 16,
        }),
        dxgi::R8G8B8A8_UNORM
        | dxgi::R8G8B8A8_UNORM_SRGB
        | dxgi::R8G8B8A8_UINT
        | dxgi::R8G8B8A8_SINT
        | dxgi::B8G8R8A8_UNORM
        | dxgi::B8G8R8A8_UNORM_SRGB
        | dxgi::B8G8R8X8_UNORM
        | dxgi::B8G8R8X8_UNORM_SRGB => Ok(TextureLayout::Linear { bytes_per_pixel: 4 }),
        dxgi::R8G8_UNORM
        | dxgi::R8G8_UINT
        | dxgi::R8G8_SINT
        | dxgi::B5G6R5_UNORM
        | dxgi::B5G5R5A1_UNORM => Ok(TextureLayout::Linear { bytes_per_pixel: 2 }),
        dxgi::R8_UNORM | dxgi::R8_UINT | dxgi::R8_SNORM | dxgi::R8_SINT | dxgi::A8_UNORM => {
            Ok(TextureLayout::Linear { bytes_per_pixel: 1 })
        }
        _ => Err(Error::Dds("unsupported DXGI format")),
    }
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(Error::Dds("truncated DDS header"))?;
    Ok(u32::from_le_bytes(
        value.try_into().expect("slice length checked"),
    ))
}

fn base_fields(header: DdsHeader) -> HeaderFields {
    let mut fields = HeaderFields {
        flags: DDSD_CAPS | DDSD_PIXELFORMAT | DDSD_WIDTH | DDSD_HEIGHT | DDSD_MIPMAPCOUNT,
        caps: DDSCAPS_TEXTURE,
        array_size: 1,
        ..HeaderFields::default()
    };
    if header.mip_count > 1 {
        fields.caps |= DDSCAPS_MIPMAP | DDSCAPS_COMPLEX;
    }
    if header.flags & 1 != 0 {
        fields.caps |= DDSCAPS_COMPLEX;
        fields.caps2 = DDSCAPS2_CUBEMAP
            | DDSCAPS2_POSITIVEX
            | DDSCAPS2_NEGATIVEX
            | DDSCAPS2_POSITIVEY
            | DDSCAPS2_NEGATIVEY
            | DDSCAPS2_POSITIVEZ
            | DDSCAPS2_NEGATIVEZ;
        fields.misc_flags = DDS_RESOURCE_MISC_TEXTURECUBE;
        fields.array_size = 6;
    }
    fields
}

fn apply_format(fields: &mut HeaderFields, format: u8, width: u32, height: u32) -> Result<()> {
    if apply_legacy_block_format(fields, format, width, height)
        || apply_dx10_block_format(fields, format, width, height)
        || apply_dx10_pitch_format(fields, format, width)
        || apply_plain_format(fields, format, width)
    {
        Ok(())
    } else {
        Err(Error::Dds("unsupported DXGI format"))
    }
}

fn block_linear_size(width: u32, height: u32, block_bytes: u32) -> u32 {
    width.div_ceil(4).max(1) * height.div_ceil(4).max(1) * block_bytes
}

fn set_fourcc_block(fields: &mut HeaderFields, fourcc_value: u32, pitch_or_linear: u32) {
    fields.flags |= DDSD_LINEARSIZE;
    fields.pf_flags = DDPF_FOURCC;
    fields.fourcc = fourcc_value;
    fields.pitch_or_linear = pitch_or_linear;
}

fn set_dx10_block(fields: &mut HeaderFields, format: u8, pitch_or_linear: u32) {
    set_fourcc_block(fields, fourcc(*b"DX10"), pitch_or_linear);
    fields.dxgi_format = u32::from(format);
}

fn set_dx10_pitch(fields: &mut HeaderFields, format: u8, pitch_or_linear: u32) {
    fields.flags |= DDSD_PITCH;
    fields.pf_flags = DDPF_FOURCC;
    fields.fourcc = fourcc(*b"DX10");
    fields.dxgi_format = u32::from(format);
    fields.pitch_or_linear = pitch_or_linear;
}

fn apply_legacy_block_format(
    fields: &mut HeaderFields,
    format: u8,
    width: u32,
    height: u32,
) -> bool {
    match format {
        dxgi::BC1_UNORM => set_fourcc_block(
            fields,
            fourcc(*b"DXT1"),
            block_linear_size(width, height, 8),
        ),
        dxgi::BC2_UNORM => set_fourcc_block(
            fields,
            fourcc(*b"DXT3"),
            block_linear_size(width, height, 16),
        ),
        dxgi::BC3_UNORM => set_fourcc_block(
            fields,
            fourcc(*b"DXT5"),
            block_linear_size(width, height, 16),
        ),
        dxgi::BC4_SNORM => set_fourcc_block(
            fields,
            fourcc(*b"BC4S"),
            block_linear_size(width, height, 8),
        ),
        dxgi::BC4_UNORM => set_fourcc_block(
            fields,
            fourcc(*b"BC4U"),
            block_linear_size(width, height, 8),
        ),
        dxgi::BC5_SNORM => set_fourcc_block(
            fields,
            fourcc(*b"BC5S"),
            block_linear_size(width, height, 16),
        ),
        dxgi::BC5_UNORM => set_fourcc_block(
            fields,
            fourcc(*b"BC5U"),
            block_linear_size(width, height, 16),
        ),
        _ => return false,
    }
    true
}

fn apply_dx10_block_format(fields: &mut HeaderFields, format: u8, width: u32, height: u32) -> bool {
    match format {
        dxgi::BC1_UNORM_SRGB => set_dx10_block(fields, format, block_linear_size(width, height, 8)),
        dxgi::BC2_UNORM_SRGB
        | dxgi::BC3_UNORM_SRGB
        | dxgi::BC6H_UF16
        | dxgi::BC6H_SF16
        | dxgi::BC7_UNORM
        | dxgi::BC7_UNORM_SRGB => {
            set_dx10_block(fields, format, block_linear_size(width, height, 16));
        }
        _ => return false,
    }
    true
}

fn apply_dx10_pitch_format(fields: &mut HeaderFields, format: u8, width: u32) -> bool {
    match format {
        dxgi::B8G8R8A8_UNORM_SRGB
        | dxgi::B8G8R8X8_UNORM_SRGB
        | dxgi::R8G8B8A8_SINT
        | dxgi::R8G8B8A8_UINT
        | dxgi::R8G8B8A8_UNORM_SRGB => set_dx10_pitch(fields, format, width * 4),
        dxgi::R8G8_SINT | dxgi::R8G8_UINT => set_dx10_pitch(fields, format, width * 2),
        dxgi::R8_SINT | dxgi::R8_SNORM | dxgi::R8_UINT => set_dx10_pitch(fields, format, width),
        _ => return false,
    }
    true
}

fn apply_plain_format(fields: &mut HeaderFields, format: u8, width: u32) -> bool {
    match format {
        dxgi::R8G8B8A8_UNORM => set_rgb(
            fields,
            width * 4,
            32,
            0x0000_00ff,
            0x0000_ff00,
            0x00ff_0000,
            0xff00_0000,
        ),
        dxgi::B8G8R8A8_UNORM => set_rgb(
            fields,
            width * 4,
            32,
            0x00ff_0000,
            0x0000_ff00,
            0x0000_00ff,
            0xff00_0000,
        ),
        dxgi::B8G8R8X8_UNORM => {
            set_rgbx(fields, width * 4, 32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff);
        }
        dxgi::B5G6R5_UNORM => {
            set_rgbx(fields, width * 2, 16, 0x0000_f800, 0x0000_07e0, 0x0000_001f);
        }
        dxgi::B5G5R5A1_UNORM => set_rgb(
            fields,
            width * 2,
            16,
            0x0000_7c00,
            0x0000_03e0,
            0x0000_001f,
            0x0000_8000,
        ),
        dxgi::R8G8_UNORM => {
            fields.flags |= DDSD_PITCH;
            fields.pf_flags = DDPF_LUMINANCE | DDPF_ALPHAPIXELS;
            fields.rgb_bits = 16;
            fields.r_mask = 0x0000_00ff;
            fields.a_mask = 0x0000_ff00;
            fields.pitch_or_linear = width * 2;
        }
        dxgi::A8_UNORM => {
            fields.flags |= DDSD_PITCH;
            fields.pf_flags = DDPF_ALPHA;
            fields.rgb_bits = 8;
            fields.a_mask = 0x0000_00ff;
            fields.pitch_or_linear = width;
        }
        dxgi::R8_UNORM => {
            fields.flags |= DDSD_PITCH;
            fields.pf_flags = DDPF_LUMINANCE;
            fields.rgb_bits = 8;
            fields.r_mask = 0x0000_00ff;
            fields.pitch_or_linear = width;
        }
        _ => return false,
    }
    true
}

fn set_rgb(fields: &mut HeaderFields, pitch: u32, bits: u32, r: u32, g: u32, b: u32, a: u32) {
    set_rgbx(fields, pitch, bits, r, g, b);
    fields.pf_flags |= DDPF_ALPHAPIXELS;
    fields.a_mask = a;
}

fn set_rgbx(fields: &mut HeaderFields, pitch: u32, bits: u32, r: u32, g: u32, b: u32) {
    fields.flags |= DDSD_PITCH;
    fields.pf_flags = DDPF_RGB;
    fields.rgb_bits = bits;
    fields.r_mask = r;
    fields.g_mask = g;
    fields.b_mask = b;
    fields.pitch_or_linear = pitch;
}

fn emit_header(
    out: &mut Vec<u8>,
    header: DdsHeader,
    width: u32,
    height: u32,
    fields: &HeaderFields,
) {
    out.extend_from_slice(b"DDS ");
    push_u32(out, 124);
    push_u32(out, fields.flags);
    push_u32(out, height);
    push_u32(out, width);
    push_u32(out, fields.pitch_or_linear);
    push_u32(out, 0);
    push_u32(out, u32::from(header.mip_count));
    for _ in 0..11 {
        push_u32(out, 0);
    }
    push_u32(out, 32);
    push_u32(out, fields.pf_flags);
    push_u32(out, fields.fourcc);
    push_u32(out, fields.rgb_bits);
    push_u32(out, fields.r_mask);
    push_u32(out, fields.g_mask);
    push_u32(out, fields.b_mask);
    push_u32(out, fields.a_mask);
    push_u32(out, fields.caps);
    push_u32(out, fields.caps2);
    push_u32(out, 0);
    push_u32(out, 0);
    push_u32(out, 0);
    if fields.fourcc == fourcc(*b"DX10") {
        push_u32(out, fields.dxgi_format);
        push_u32(out, DDS_DIMENSION_TEXTURE2D);
        push_u32(out, fields.misc_flags);
        push_u32(out, fields.array_size);
        push_u32(out, 0);
    }
}
