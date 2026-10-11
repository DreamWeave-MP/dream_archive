--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- DDS headers for BA2 DX10 textures: the header a texture record reconstructs on extraction, the
-- payload sizes its mips must have, and the DDS files the DX10 builder takes apart. Tightly packed
-- 2D textures and cubemaps only; arrays, volumes, and pixel conversion are out of scope.
local types = require './types'
local util = require './util'

type TextureHeader = types.TextureHeader

local dds = util.dds
local band, bor = bit32.band, bit32.bor
local readu32 = buffer.readu32

local Ddsd = table.freeze {
  caps = 0x1,
  height = 0x2,
  width = 0x4,
  pitch = 0x8,
  pixelFormat = 0x1000,
  mipmapCount = 0x20000,
  linearSize = 0x80000,
  depth = 0x800000,
}
local Caps = table.freeze { complex = 0x8, texture = 0x1000, mipmap = 0x400000 }
local CubemapCaps2 = 0xfe00
local Cubemap, Volume = 0x200, 0x200000
local Pf = table.freeze {
  alphaPixels = 0x1,
  alpha = 0x2,
  fourCc = 0x4,
  rgb = 0x40,
  luminance = 0x20000,
}
local Texture2d, MiscTextureCube = 3, 4

local function fourcc(text: string): number return buffer.readu32(buffer.fromstring(text), 0) end

local Dx10, Dxt1, Dxt3, Dxt5 = fourcc 'DX10', fourcc 'DXT1', fourcc 'DXT3', fourcc 'DXT5'
local Bc4u, Bc4s, Bc5u, Bc5s = fourcc 'BC4U', fourcc 'BC4S', fourcc 'BC5U', fourcc 'BC5S'

--- Bytes per 4x4 block for block formats, bytes per pixel (negated) for linear ones.
local Layouts: { [number]: number } = {
  [71] = 8,
  [72] = 8,
  [80] = 8,
  [81] = 8,
  [74] = 16,
  [75] = 16,
  [77] = 16,
  [78] = 16,
  [83] = 16,
  [84] = 16,
  [95] = 16,
  [96] = 16,
  [98] = 16,
  [99] = 16,
  [28] = -4,
  [29] = -4,
  [30] = -4,
  [32] = -4,
  [87] = -4,
  [91] = -4,
  [88] = -4,
  [93] = -4,
  [49] = -2,
  [50] = -2,
  [52] = -2,
  [85] = -2,
  [86] = -2,
  [61] = -1,
  [62] = -1,
  [63] = -1,
  [64] = -1,
  [65] = -1,
}

type Fields = {
  flags: number,
  pitch: number,
  pfFlags: number,
  fourcc: number,
  rgbBits: number,
  rMask: number,
  gMask: number,
  bMask: number,
  aMask: number,
  caps: number,
  caps2: number,
  dxgi: number,
  misc: number,
}

--- The legacy FourCC block formats: their code and block size.
local LegacyBlocks: { [number]: { fourcc: number, block: number } } = {
  [71] = { fourcc = Dxt1, block = 8 },
  [74] = { fourcc = Dxt3, block = 16 },
  [77] = { fourcc = Dxt5, block = 16 },
  [81] = { fourcc = Bc4s, block = 8 },
  [80] = { fourcc = Bc4u, block = 8 },
  [84] = { fourcc = Bc5s, block = 16 },
  [83] = { fourcc = Bc5u, block = 16 },
}

--- The block formats only a DX10 header can name.
local Dx10Blocks: { [number]: number } =
  { [72] = 8, [75] = 16, [78] = 16, [95] = 16, [96] = 16, [98] = 16, [99] = 16 }

--- The pitched formats only a DX10 header can name, by bytes per pixel.
local Dx10Pitched: { [number]: number } = {
  [91] = 4,
  [93] = 4,
  [32] = 4,
  [30] = 4,
  [29] = 4,
  [52] = 2,
  [50] = 2,
  [64] = 1,
  [63] = 1,
  [62] = 1,
}

--- The plain formats: bytes per pixel, pixel-format flags, bit count, and the four masks.
local Plain: { [number]: { number } } = {
  [28] = { 4, bor(Pf.rgb, Pf.alphaPixels), 32, 0xff, 0xff00, 0xff0000, 0xff000000 },
  [87] = { 4, bor(Pf.rgb, Pf.alphaPixels), 32, 0xff0000, 0xff00, 0xff, 0xff000000 },
  [88] = { 4, Pf.rgb, 32, 0xff0000, 0xff00, 0xff, 0 },
  [85] = { 2, Pf.rgb, 16, 0xf800, 0x7e0, 0x1f, 0 },
  [86] = { 2, bor(Pf.rgb, Pf.alphaPixels), 16, 0x7c00, 0x3e0, 0x1f, 0x8000 },
  [49] = { 2, bor(Pf.luminance, Pf.alphaPixels), 16, 0xff, 0, 0, 0xff00 },
  [65] = { 1, Pf.alpha, 8, 0, 0, 0, 0xff },
  [61] = { 1, Pf.luminance, 8, 0xff, 0, 0, 0 },
}

local function blockLinearSize(width: number, height: number, block: number): number
  return math.max((width + 3) // 4, 1) * math.max((height + 3) // 4, 1) * block
end

--- The OpenMW-compatible DDS header a BA2 DX10 record stands for: 128 bytes, or 148 with the
--- DX10 extension.
local function header(
  height: number,
  width: number,
  mipCount: number,
  format: number,
  flags: number
): buffer
  if width == 0 or height == 0 then dds 'zero-sized texture' end

  if mipCount == 0 then dds 'zero mip count' end

  local fields: Fields = {
    flags = bor(Ddsd.caps, Ddsd.pixelFormat, Ddsd.width, Ddsd.height, Ddsd.mipmapCount),
    pitch = 0,
    pfFlags = 0,
    fourcc = 0,
    rgbBits = 0,
    rMask = 0,
    gMask = 0,
    bMask = 0,
    aMask = 0,
    caps = Caps.texture,
    caps2 = 0,
    dxgi = 0,
    misc = 0,
  }

  if mipCount > 1 then fields.caps = bor(fields.caps, Caps.mipmap, Caps.complex) end

  if band(flags, 1) ~= 0 then
    fields.caps = bor(fields.caps, Caps.complex)
    fields.caps2 = CubemapCaps2
    fields.misc = MiscTextureCube
  end

  local legacy = LegacyBlocks[format]
  local dx10Block = Dx10Blocks[format]
  local pitched = Dx10Pitched[format]
  local plain = Plain[format]
  if legacy then
    fields.flags = bor(fields.flags, Ddsd.linearSize)
    fields.pfFlags = Pf.fourCc
    fields.fourcc = legacy.fourcc
    fields.pitch = blockLinearSize(width, height, legacy.block)
  elseif dx10Block then
    fields.flags = bor(fields.flags, Ddsd.linearSize)
    fields.pfFlags = Pf.fourCc
    fields.fourcc = Dx10
    fields.dxgi = format
    fields.pitch = blockLinearSize(width, height, dx10Block)
  elseif pitched then
    fields.flags = bor(fields.flags, Ddsd.pitch)
    fields.pfFlags = Pf.fourCc
    fields.fourcc = Dx10
    fields.dxgi = format
    fields.pitch = width * pitched
  elseif plain then
    fields.flags = bor(fields.flags, Ddsd.pitch)
    fields.pitch = width * plain[1]
    fields.pfFlags = plain[2]
    fields.rgbBits = plain[3]
    fields.rMask, fields.gMask, fields.bMask, fields.aMask = plain[4], plain[5], plain[6], plain[7]
  else
    dds 'unsupported DXGI format'
  end

  local extended = fields.fourcc == Dx10
  local out = buffer.create(if extended then 148 else 128)
  buffer.writestring(out, 0, 'DDS ')
  buffer.writeu32(out, 4, 124)
  buffer.writeu32(out, 8, fields.flags)
  buffer.writeu32(out, 12, height)
  buffer.writeu32(out, 16, width)
  buffer.writeu32(out, 20, fields.pitch % 0x100000000)
  buffer.writeu32(out, 28, mipCount)
  buffer.writeu32(out, 76, 32)
  buffer.writeu32(out, 80, fields.pfFlags)
  buffer.writeu32(out, 84, fields.fourcc)
  buffer.writeu32(out, 88, fields.rgbBits)
  buffer.writeu32(out, 92, fields.rMask)
  buffer.writeu32(out, 96, fields.gMask)
  buffer.writeu32(out, 100, fields.bMask)
  buffer.writeu32(out, 104, fields.aMask)
  buffer.writeu32(out, 108, fields.caps)
  buffer.writeu32(out, 112, fields.caps2)
  if extended then
    buffer.writeu32(out, 128, fields.dxgi)
    buffer.writeu32(out, 132, Texture2d)
    buffer.writeu32(out, 136, fields.misc)
    buffer.writeu32(out, 140, 1)
  end

  return out
end

--- What `header` checks, without writing one: an archive's texture records are checked once, when
--- it opens.
local function check(height: number, width: number, mipCount: number, format: number)
  if width == 0 or height == 0 then dds 'zero-sized texture' end

  if mipCount == 0 then dds 'zero mip count' end

  if not Layouts[format] then dds 'unsupported DXGI format' end
end

--- How long `header` is for a format it takes: 148 bytes with the DX10 extension, 128 without.
local function headerLength(format: number): number
  return if Dx10Blocks[format] or Dx10Pitched[format] then 148 else 128
end

local function headerOf(texture: TextureHeader): buffer
  return header(texture.height, texture.width, texture.mipCount, texture.format, texture.flags)
end

--- The payload bytes mips `first` to `last` of a texture take, every face of a cubemap included.
local function mipBytes(
  height: number,
  width: number,
  flags: number,
  format: number,
  first: number,
  last: number
): number
  local layout = Layouts[format]
  if not layout then dds 'unsupported DXGI format' end

  local faces = if band(flags, 1) ~= 0 then 6 else 1
  local size = 0
  for mip = first, last do
    -- The crate shifts a u32 by the mip number; a release build masks the shift to five bits.
    local shift = mip % 32
    local mipWidth = math.max(bit32.rshift(width, shift), 1)
    local mipHeight = math.max(bit32.rshift(height, shift), 1)
    local face = if layout > 0
      then ((mipWidth + 3) // 4) * ((mipHeight + 3) // 4) * layout
      else mipWidth * mipHeight * -layout
    size += face * faces
  end

  return size
end

local function mipPayloadSize(texture: TextureHeader, first: number, last: number): number
  return mipBytes(texture.height, texture.width, texture.flags, texture.format, first, last)
end

local function payloadSize(texture: TextureHeader): number
  if texture.mipCount == 0 then dds 'zero mip count' end

  return mipPayloadSize(texture, 0, texture.mipCount - 1)
end

local function validatePayloadSize(texture: TextureHeader, actual: number)
  if actual ~= payloadSize(texture) then dds 'DDS payload size does not match metadata' end
end

local function readAt(bytes: buffer, offset: number): number
  if offset + 4 > buffer.len(bytes) then dds 'truncated DDS header' end

  return readu32(bytes, offset)
end

--- The DXGI format a plain pixel format means.
local function plainFormat(
  flags: number,
  bits: number,
  r: number,
  g: number,
  b: number,
  a: number
): number
  local rgbAlpha = bor(Pf.rgb, Pf.alphaPixels)
  if bits == 32 and band(flags, rgbAlpha) == rgbAlpha then
    if r == 0xff and g == 0xff00 and b == 0xff0000 and a == 0xff000000 then return 28 end

    if r == 0xff0000 and g == 0xff00 and b == 0xff and a == 0xff000000 then return 87 end
  end

  if
    bits == 32
    and band(flags, Pf.rgb) ~= 0
    and r == 0xff0000
    and g == 0xff00
    and b == 0xff
    and a == 0
  then
    return 88
  end

  if
    bits == 16
    and band(flags, Pf.rgb) ~= 0
    and r == 0xf800
    and g == 0x7e0
    and b == 0x1f
    and a == 0
  then
    return 85
  end

  if
    bits == 16
    and band(flags, rgbAlpha) == rgbAlpha
    and r == 0x7c00
    and g == 0x3e0
    and b == 0x1f
    and a == 0x8000
  then
    return 86
  end

  local luminanceAlpha = bor(Pf.luminance, Pf.alphaPixels)
  if bits == 16 and r == 0xff and a == 0xff00 and band(flags, luminanceAlpha) == luminanceAlpha then
    return 49
  end

  if bits == 8 and a == 0xff and band(flags, Pf.alpha) ~= 0 then return 65 end

  if bits == 8 and r == 0xff and band(flags, Pf.luminance) ~= 0 then return 61 end

  return dds 'unsupported DDS pixel format'
end

local LegacyFormats: { [number]: number } = {
  [Dxt1] = 71,
  [Dxt3] = 74,
  [Dxt5] = 77,
  [Bc4u] = 80,
  [Bc4s] = 81,
  [Bc5u] = 83,
  [Bc5s] = 84,
}

--- A DDS file taken apart for a DX10 BA2: its texture metadata and where its payload starts.
local function parse(bytes: buffer): (TextureHeader, number)
  local length = buffer.len(bytes)
  if length < 128 then dds 'truncated DDS header' end

  if buffer.readstring(bytes, 0, 4) ~= 'DDS ' then dds 'invalid DDS magic' end

  if readu32(bytes, 4) ~= 124 then dds 'invalid DDS header size' end

  if readu32(bytes, 76) ~= 32 then dds 'invalid DDS pixel format size' end

  local flags, caps2 = readu32(bytes, 8), readu32(bytes, 112)
  if band(flags, Ddsd.depth) ~= 0 or band(caps2, Volume) ~= 0 then
    dds 'unsupported DDS volume texture'
  end

  local pfFlags, pfFourcc = readu32(bytes, 80), readu32(bytes, 84)
  local payloadOffset, arraySize = 128, 1
  if pfFourcc == Dx10 then
    if length < 148 then dds 'truncated DDS DX10 header' end

    payloadOffset, arraySize = 148, readu32(bytes, 140)
  end

  local height, width = readu32(bytes, 12), readu32(bytes, 16)
  local mipCount = math.max(readu32(bytes, 28), 1)
  local format: number
  if band(pfFlags, Pf.fourCc) ~= 0 then
    if pfFourcc == Dx10 then
      local dxgi = readAt(bytes, 128)
      if readAt(bytes, 132) ~= Texture2d then dds 'unsupported DDS resource dimension' end

      if dxgi > 255 or not Layouts[dxgi] then dds 'unsupported DXGI format' end

      format = dxgi
    else
      local legacy = LegacyFormats[pfFourcc]
      if not legacy then dds 'unsupported DDS FourCC' end

      format = legacy
    end
  else
    format = plainFormat(
      pfFlags,
      readu32(bytes, 88),
      readu32(bytes, 92),
      readu32(bytes, 96),
      readu32(bytes, 100),
      readu32(bytes, 104)
    )
  end

  local cubemap = if payloadOffset == 148
    then band(readAt(bytes, 136), MiscTextureCube) ~= 0
    else band(caps2, Cubemap) ~= 0
  if height > 0xffff then dds 'texture height exceeds BA2 field' end

  if width > 0xffff then dds 'texture width exceeds BA2 field' end

  if mipCount > 0xff then dds 'mip count exceeds BA2 field' end

  local texture: TextureHeader = {
    height = height,
    width = width,
    mipCount = mipCount,
    format = format,
    flags = if cubemap then 1 else 0,
    tileMode = 0,
  }
  headerOf(texture)
  local payloadLength = length - payloadOffset
  -- A DX10 header's array size counts textures, and cubes for a cubemap; versions of the crate
  -- before 1.0.0 wrote a cubemap's face count, 6, there.
  local earlierCube = arraySize == 6 and cubemap and payloadLength == payloadSize(texture)
  if arraySize ~= 1 and not earlierCube then dds 'unsupported DDS array size' end

  validatePayloadSize(texture, payloadLength)
  return texture, payloadOffset
end

return table.freeze {
  header = header,
  headerOf = headerOf,
  check = check,
  headerLength = headerLength,
  mipBytes = mipBytes,
  mipPayloadSize = mipPayloadSize,
  payloadSize = payloadSize,
  validatePayloadSize = validatePayloadSize,
  parse = parse,
}
