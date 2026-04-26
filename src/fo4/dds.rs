use super::{Error, Result, fourcc};

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

const DDPF_ALPHAPIXELS: u32 = 0x0000_0001;
const DDPF_ALPHA: u32 = 0x0000_0002;
const DDPF_FOURCC: u32 = 0x0000_0004;
const DDPF_RGB: u32 = 0x0000_0040;
const DDPF_LUMINANCE: u32 = 0x0002_0000;

const DDS_DIMENSION_TEXTURE2D: u32 = 3;
const DDS_RESOURCE_MISC_TEXTURECUBE: u32 = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DdsHeader {
    pub height: u16,
    pub width: u16,
    pub mip_count: u8,
    pub format: u8,
    pub flags: u8,
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

    let mut fields = base_fields(header);
    apply_format(&mut fields, header.format, width, height);
    emit_header(out, header, width, height, &fields);
    Ok(())
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

fn apply_format(fields: &mut HeaderFields, format: u8, width: u32, height: u32) {
    if apply_legacy_block_format(fields, format, width, height) {
        return;
    }
    if apply_dx10_block_format(fields, format, width, height) {
        return;
    }
    if apply_dx10_pitch_format(fields, format, width) {
        return;
    }
    apply_plain_format(fields, format, width);
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
        dxgi::BC1_UNORM => {
            set_fourcc_block(fields, fourcc(*b"DXT1"), width * height / 2);
        }
        dxgi::BC2_UNORM => {
            set_fourcc_block(fields, fourcc(*b"DXT3"), width * height);
        }
        dxgi::BC3_UNORM => {
            set_fourcc_block(fields, fourcc(*b"DXT5"), width * height);
        }
        dxgi::BC4_SNORM => {
            set_fourcc_block(fields, fourcc(*b"BC4S"), width * height / 2);
        }
        dxgi::BC4_UNORM => {
            set_fourcc_block(fields, fourcc(*b"BC4U"), width * height / 2);
        }
        dxgi::BC5_SNORM => {
            set_fourcc_block(fields, fourcc(*b"BC5S"), width * height);
        }
        dxgi::BC5_UNORM => {
            set_fourcc_block(fields, fourcc(*b"BC5U"), width * height);
        }
        _ => return false,
    }
    true
}

fn apply_dx10_block_format(fields: &mut HeaderFields, format: u8, width: u32, height: u32) -> bool {
    match format {
        dxgi::BC1_UNORM_SRGB => {
            set_dx10_block(fields, format, width * height / 2);
        }
        dxgi::BC2_UNORM_SRGB
        | dxgi::BC3_UNORM_SRGB
        | dxgi::BC6H_UF16
        | dxgi::BC6H_SF16
        | dxgi::BC7_UNORM
        | dxgi::BC7_UNORM_SRGB => {
            set_dx10_block(fields, format, width * height);
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
        | dxgi::R8G8B8A8_UNORM_SRGB => {
            set_dx10_pitch(fields, format, width * 4);
        }
        dxgi::R8G8_SINT | dxgi::R8G8_UINT => {
            set_dx10_pitch(fields, format, width * 2);
        }
        dxgi::R8_SINT | dxgi::R8_SNORM | dxgi::R8_UINT => {
            set_dx10_pitch(fields, format, width);
        }
        _ => return false,
    }
    true
}

fn apply_plain_format(fields: &mut HeaderFields, format: u8, width: u32) {
    match format {
        dxgi::R8G8B8A8_UNORM => {
            set_rgb(
                fields,
                width * 4,
                32,
                0x0000_00ff,
                0x0000_ff00,
                0x00ff_0000,
                0xff00_0000,
            );
        }
        dxgi::B8G8R8A8_UNORM => {
            set_rgb(
                fields,
                width * 4,
                32,
                0x00ff_0000,
                0x0000_ff00,
                0x0000_00ff,
                0xff00_0000,
            );
        }
        dxgi::B8G8R8X8_UNORM => {
            set_rgbx(fields, width * 4, 32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff);
        }
        dxgi::B5G6R5_UNORM => {
            set_rgbx(fields, width * 2, 16, 0x0000_f800, 0x0000_07e0, 0x0000_001f);
        }
        dxgi::B5G5R5A1_UNORM => {
            set_rgb(
                fields,
                width * 2,
                16,
                0x0000_7c00,
                0x0000_03e0,
                0x0000_001f,
                0x0000_8000,
            );
        }
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
        _ => {}
    }
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
    push_u32(out, u32::from(fields.fourcc == fourcc(*b"DX10")));
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
