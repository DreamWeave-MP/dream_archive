--!strict
--!native
-- What the archive suites share: a seeded generator, writers for archives the builders can't make
-- (hash-only, stringless, multi-chunk, console, malformed), and one observation of an archive,
-- written against the API `@dream-archive` shares with the crate's native binding, which the
-- differential suite's goldens were recorded from.
local bytes = require '@dream/bytes'

export type Random = (bound: number?) -> number

--- A xorshift32 generator: `random(n)` is in `0..n-1`, `random()` a u32.
local function generator(seed: number): Random
  local state = if seed == 0 then 0x9e3779b9 else seed % 0x100000000
  return function(bound: number?): number
    state = bit32.bxor(state, bit32.lshift(state, 13))
    state = bit32.bxor(state, bit32.rshift(state, 17))
    state = bit32.bxor(state, bit32.lshift(state, 5))
    return if bound then state % bound else state
  end
end

export type Writer = {
  u8: (value: number) -> (),
  u16: (value: number) -> (),
  u32: (value: number) -> (),
  u64: (value: number) -> (),
  raw: (value: string) -> (),
  at: () -> number,
  set32: (position: number, value: number) -> (),
  done: () -> string,
}

local function writer(): Writer
  local parts: { string } = {}
  local length = 0
  local patches: { { number } } = {}
  local function raw(value: string)
    table.insert(parts, value)
    length += #value
  end

  return {
    u8 = function(value: number) raw(string.pack('<I1', value % 256)) end,
    u16 = function(value: number) raw(string.pack('<I2', value % 0x10000)) end,
    u32 = function(value: number) raw(string.pack('<I4', value % 0x100000000)) end,
    u64 = function(value: number)
      raw(string.pack('<I4I4', value % 0x100000000, value // 0x100000000))
    end,
    raw = raw,
    at = function(): number return length end,
    set32 = function(position: number, value: number) table.insert(patches, { position, value }) end,
    done = function(): string
      local out = buffer.fromstring(table.concat(parts))
      for _, patch in patches do
        buffer.writeu32(out, patch[1], patch[2])
      end

      return buffer.tostring(out)
    end,
  }
end

local function zlib(data: string): string return buffer.tostring(bytes.deflate(data)) end

local function u64Of(value: integer): (number, number)
  local scratch = buffer.create(8)
  buffer.writeinteger(scratch, 0, value)
  return buffer.readu32(scratch, 0), buffer.readu32(scratch, 4)
end

export type FrameOptions = {
  blockSize: number?,
  blockChecksums: boolean?,
  contentChecksum: boolean?,
  contentSize: boolean?,
  linked: boolean?,
  stored: boolean?,
}

--- An LZ4 frame written by hand, with the descriptor flags lz4_flex's encoder never sets: block
--- and content checksums, the content size, linked blocks, stored blocks, and small blocks.
local function lz4Frame(data: string, options: FrameOptions): string
  local code = options.blockSize or 4
  local blockSize = ({ 0x10000, 0x40000, 0x100000, 0x400000 })[code - 3]
  local flags = 0x40
  if not options.linked then
    flags += 0x20
  end

  if options.blockChecksums then
    flags += 0x10
  end

  if options.contentSize then
    flags += 0x08
  end

  if options.contentChecksum then
    flags += 0x04
  end

  local descriptor = string.char(flags, code * 16)
  if options.contentSize then
    descriptor ..= string.pack('<I4I4', #data, 0)
  end

  local header = bit32.band(bit32.rshift(bytes.xxh32(descriptor), 8), 0xff)
  local parts = { '\4\34\77\24', descriptor, string.char(header) }
  for start = 1, math.max(#data, 1), blockSize do
    local chunk = string.sub(data, start, start + blockSize - 1)
    if chunk == '' then break end

    local body, size = chunk, #chunk + 0x80000000
    if not options.stored then
      body = buffer.tostring(bytes.lz4Compress(chunk))
      size = #body
    end

    table.insert(parts, string.pack('<I4', size))
    table.insert(parts, body)
    if options.blockChecksums then table.insert(parts, string.pack('<I4', bytes.xxh32(body))) end
  end

  table.insert(parts, string.pack('<I4', 0))
  if options.contentChecksum then table.insert(parts, string.pack('<I4', bytes.xxh32(data))) end

  return table.concat(parts)
end

export type Tes3File = { name: string, data: string }

--- A TES3 BSA of `files` in the given order, hashed by `hashFile`.
local function tes3(files: { Tes3File }, hashFile: (string) -> { lo: number, hi: number }): string
  local w = writer()
  local namesLength = 0
  for _, file in files do
    namesLength += #file.name + 1
  end

  w.u32(0x100)
  w.u32(12 * #files + namesLength)
  w.u32(#files)
  local offset = 0
  for _, file in files do
    w.u32(#file.data)
    w.u32(offset)
    offset += #file.data
  end

  local nameOffset = 0
  for _, file in files do
    w.u32(nameOffset)
    nameOffset += #file.name + 1
  end

  for _, file in files do
    w.raw(file.name .. '\0')
  end

  for _, file in files do
    local hash = hashFile(file.name)
    w.u32(hash.lo)
    w.u32(hash.hi)
  end

  for _, file in files do
    w.raw(file.data)
  end

  return w.done()
end

export type Tes4File = {
  name: string,
  data: string,
  compress: string?,
  frame: FrameOptions?,
  toggled: boolean?,
  checked: boolean?,
  embedded: string?,
}
export type Tes4Folder = { name: string, files: { Tes4File } }
export type Tes4Spec = {
  version: number,
  flags: number,
  archiveTypes: number?,
  folders: { Tes4Folder },
  padding: number?,
  hashDirectory: (string) -> { hash: integer },
  hashFile: (string) -> { hash: integer },
}

--- A TES4 BSA laid out by hand: folder records, folder blocks, file names, payloads, with the
--- name tables, embedded names, and compression the flags and files ask for.
local function tes4(spec: Tes4Spec): string
  local version, flags = spec.version, spec.flags
  local directoryStrings = bit32.band(flags, 1) ~= 0
  local fileStrings = bit32.band(flags, 2) ~= 0
  local embedded = bit32.band(flags, 0x100) ~= 0 and version ~= 103
  local folderNamesLength, fileNamesLength, fileCount = 0, spec.padding or 0, 0
  for _, folder in spec.folders do
    folderNamesLength += #folder.name + 1
    for _, file in folder.files do
      fileNamesLength += #file.name + 1
      fileCount += 1
    end
  end

  if not fileStrings then fileNamesLength = spec.padding or 0 end

  local recordSize = if version == 105 then 24 else 16
  local w = writer()
  w.raw 'BSA\0'
  w.u32(version)
  w.u32(0x24)
  w.u32(flags)
  w.u32(#spec.folders)
  w.u32(fileCount)
  w.u32(if directoryStrings then folderNamesLength else 0)
  w.u32(fileNamesLength)
  w.u16(spec.archiveTypes or 0x100)
  w.u16(0)
  local blockOffset = 0x24 + recordSize * #spec.folders
  for _, folder in spec.folders do
    local low, high = u64Of(spec.hashDirectory(folder.name).hash)
    w.u32(low)
    w.u32(high)
    w.u32(#folder.files)
    if version == 105 then
      w.u32(0)
      w.u64(blockOffset)
    else
      w.u32(blockOffset)
    end

    blockOffset += (if directoryStrings then #folder.name + 2 else 0) + 16 * #folder.files
  end

  local payloads: { string } = {}
  local sizeAt: { number } = {}
  local offsetAt: { number } = {}
  for _, folder in spec.folders do
    if directoryStrings then
      w.u8(#folder.name + 1)
      w.raw(folder.name .. '\0')
    end

    for _, file in folder.files do
      local low, high = u64Of(spec.hashFile(file.name).hash)
      w.u32(low)
      w.u32(high)
      local stored = file.data
      if file.compress then
        local body = if file.frame
          then lz4Frame(file.data, file.frame)
          elseif file.compress == 'lz4' then buffer.tostring(bytes.lz4FrameCompress(file.data))
          else zlib(file.data)
        stored = string.pack('<I4', #file.data) .. body
      end

      if embedded then
        local name = file.embedded or `{folder.name}\\{file.name}`
        stored = string.char(#name) .. name .. stored
      end

      table.insert(payloads, stored)
      local size = #stored
      if file.toggled then
        size += 0x40000000
      end

      if file.checked then
        size += 0x80000000
      end

      table.insert(sizeAt, w.at())
      w.u32(size)
      table.insert(offsetAt, w.at())
      w.u32(0)
    end
  end

  if fileStrings then
    for _, folder in spec.folders do
      for _, file in folder.files do
        w.raw(file.name .. '\0')
      end
    end
  end

  w.raw(string.rep('\0', spec.padding or 0))
  for index, payload in payloads do
    w.set32(offsetAt[index], w.at())
    w.raw(payload)
  end

  return w.done()
end

export type Ba2Chunk = { data: string, compress: string?, first: number?, last: number? }
export type Ba2Entry = {
  name: string,
  hash: { directory: number, file: number, extension: number }?,
  texture: {
    height: number,
    width: number,
    mipCount: number,
    format: number,
    flags: number,
    tileMode: number,
  }?,
  gnmf: { number }?,
  chunks: { Ba2Chunk },
}

export type Ba2Spec = {
  version: number,
  format: string,
  compressionCode: number?,
  strings: boolean,
  entries: { Ba2Entry },
  hashFile: (string) -> { directory: number, file: number, extension: number },
}

--- A BA2 laid out by hand: records, chunks (stored, zlib, or LZ4 blocks), and a string table
--- when asked for.
local function ba2(spec: Ba2Spec): string
  local w = writer()
  w.raw 'BTDX'
  w.u32(spec.version)
  w.raw(spec.format)
  w.u32(#spec.entries)
  local tableAt = w.at()
  w.u64(0)
  if spec.version == 2 or spec.version == 3 then w.u64(1) end

  if spec.version == 3 then w.u32(spec.compressionCode or 0) end

  local chunked = spec.format ~= 'GNRL'
  local offsets: { { number } } = {}
  local stored: { { string } } = {}
  for index, entry in spec.entries do
    local hash = entry.hash or spec.hashFile(entry.name)
    w.u32(hash.file)
    w.u32(hash.extension)
    w.u32(hash.directory)
    w.u8(0)
    w.u8(#entry.chunks)
    w.u16(if spec.format == 'GNRL' then 0x10 elseif spec.format == 'DX10' then 0x18 else 0x30)
    if entry.texture then
      local texture = entry.texture
      w.u16(texture.height)
      w.u16(texture.width)
      w.u8(texture.mipCount)
      w.u8(texture.format)
      w.u8(texture.flags)
      w.u8(texture.tileMode)
    elseif spec.format == 'GNMF' then
      for slot = 1, 8 do
        w.u32(if entry.gnmf then entry.gnmf[slot] or 0 else slot)
      end
    end

    offsets[index], stored[index] = {}, {}
    for _, chunk in entry.chunks do
      local data = chunk.data
      local packed = 0
      if chunk.compress == 'zip' then
        data = zlib(chunk.data)
        packed = #data
      elseif chunk.compress == 'lz4' then
        data = buffer.tostring(bytes.lz4Compress(chunk.data))
        packed = #data
      end

      table.insert(offsets[index], w.at())
      table.insert(stored[index], data)
      w.u64(0)
      w.u32(packed)
      w.u32(#chunk.data)
      if chunked then
        w.u16(chunk.first or 0)
        w.u16(chunk.last or 0)
      end

      w.u32(0xbaadf00d)
    end
  end

  for index in spec.entries do
    for chunk, data in stored[index] do
      w.set32(offsets[index][chunk], w.at())
      w.raw(data)
    end
  end

  if spec.strings then
    w.set32(tableAt, w.at())
    for _, entry in spec.entries do
      w.u16(#entry.name)
      w.raw(entry.name)
    end
  end

  return w.done()
end

local CodecDetails = {
  'zlib decompression failed: ',
  'lz4 decompression failed: ',
  'LZ4 frame operation failed: ',
}

--- An error message as the comparison sees it: the codec libraries word their details
--- differently, and the crate's failure to reserve a table its header claims is `<capacity>`.
local function errorText(message: unknown): string
  local text = tostring(message)
  for _, prefix in CodecDetails do
    if string.sub(text, 1, #prefix) == prefix then return `{prefix}<detail>` end
  end

  if text == 'archive table or payload requests more memory than can be allocated' then
    return '<capacity>'
  end

  -- Past the declared size the crate decodes one more byte through flate2's state machine and
  -- the port through miniz_oxide's one-shot decoder; a stream corrupt just there can fail as
  -- corrupt in one and as one byte too long in the other.
  local expected, actual = string.match(
    text,
    '^buffer failed to decompress to the expected size: expected (%d+) bytes, got (%d+) bytes$'
  )
  if expected and tonumber(actual) == (tonumber(expected) :: number) + 1 then
    return 'zlib decompression failed: <detail>'
  end

  return text
end

local function digest(data: string): string
  return string.format('%016x', (bytes.xxh3(data) :: unknown) :: number)
end

local function show(value: unknown): string
  if typeof(value) == 'integer' then return string.format('%016x', (value :: unknown) :: number) end

  if type(value) == 'string' then return string.format('%q', value) end

  return tostring(value)
end

local EntryFields = {
  'index',
  'id',
  'format',
  'path',
  'name',
  'folder',
  'size',
  'storedSize',
  'offset',
  'compressed',
  'chunks',
  'hash',
  'hashHex',
  'folderHash',
  'folderHashHex',
  'directoryHash',
  'fileHash',
  'extensionHash',
}

--- Everything an archive says about itself and its entries through the API, one line each:
--- every field, every read (as length and digest, or the error), `readInto` at an offset, the
--- lookups by path variants and by hash, and the format-specific errors.
local function observe(module: any, data: string, label: string?): string
  local lines: { string } = {}
  local function line(text: string) table.insert(lines, text) end

  local opened, archive = pcall(module.openBytes, data)
  if not opened then return `open: {errorText(archive)}` end

  line(
    `format {archive:format()} len {archive:len()} size {archive:archiveSize()} empty {archive:isEmpty()}`
  )
  local infoOk, info = pcall(archive.info, archive)
  if infoOk then
    line(`info {info.format} {info.version} {info.compression} {info.strings}`)
  else
    line(`info: {errorText(info)}`)
  end

  local text = tostring(archive)
  line(if label then text else (string.gsub(text, ', [^,]*%)$', ')')))
  local entries = archive:entries()
  line(`entries {#entries}`)
  for index, entry in entries do
    local fields = {}
    for _, field in EntryFields do
      table.insert(fields, `{field}={show(entry[field])}`)
    end

    line(table.concat(fields, ' '))
    line(`tostring {tostring(entry)}`)
    local read, result = pcall(archive.readEntry, archive, index)
    line(if read then `read {#result} {digest(result)}` else `read: {errorText(result)}`)
    local size = entry.size
    local room = if type(size) == 'number' and size < 0x1000000 then size else 0
    local target = buffer.create(room + 3)
    local into, count = pcall(archive.readInto, archive, entry, target, 3)
    if into then
      line(`readInto {count} {digest(buffer.readstring(target, 3, count))}`)
    else
      line(`readInto: {errorText(count)}`)
    end

    local path = entry.path
    if type(path) == 'string' then
      for _, variant in
        {
          path,
          string.upper(path),
          (string.gsub(path, '\\', '/')),
          `//{path}`,
          `{path}/`,
        }

      do
        local found = archive:get(variant)
        line(`get {show(variant)} {if found then found.index else 'nil'}`)
      end
    end

    local byHash: any = nil
    if entry.directoryHash then
      byHash = archive:getByHash(entry.directoryHash, entry.fileHash, entry.extensionHash)
    elseif entry.folderHash then
      byHash = archive:getByHash(entry.folderHash, entry.hash)
    elseif entry.hash then
      byHash = archive:getByHash(entry.hash)
    end

    line(`byHash {if byHash then byHash.index else 'nil'}`)
  end

  for _, query in { 'missing.txt', '', 'a/', 'misc/example.txt', 'license.txt', 'textures/foo.dds' } do
    local found = archive:get(query)
    line(`lookup {show(query)} {if found then found.index else 'nil'}`)
  end

  local bad, problem = pcall(archive.readFileRequired, archive, 'definitely/missing.bin')
  line(`required {bad} {errorText(problem)}`)
  return table.concat(lines, '\n')
end

--- Bytes per 4x4 block (positive) or per pixel (negative) for each DXGI format DX10 BA2s take.
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
  [88] = -4,
  [91] = -4,
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

local Formats: { number } = {}
for format in Layouts do
  table.insert(Formats, format)
end

table.sort(Formats)

export type Texture = {
  height: number,
  width: number,
  mipCount: number,
  format: number,
  flags: number,
  tileMode: number,
}

--- The payload bytes mips `first` to `last` of a texture take.
local function mipBytes(texture: Texture, first: number, last: number): number
  local layout = Layouts[texture.format] or 4
  local faces = if bit32.band(texture.flags, 1) ~= 0 then 6 else 1
  local size = 0
  for mip = first, last do
    local width = math.max(bit32.rshift(texture.width, mip), 1)
    local height = math.max(bit32.rshift(texture.height, mip), 1)
    local face = if layout > 0
      then ((width + 3) // 4) * ((height + 3) // 4) * layout
      else width * height * -layout
    size += face * faces
  end

  return size
end

local LegacyCodes: { [number]: string } = {
  [71] = 'DXT1',
  [74] = 'DXT3',
  [77] = 'DXT5',
  [80] = 'BC4U',
  [81] = 'BC4S',
  [83] = 'BC5U',
  [84] = 'BC5S',
}

--- The plain pixel formats: flags, bit count, and the four masks.
local PlainFormats: { [number]: { number } } = {
  [28] = { 0x41, 32, 0xff, 0xff00, 0xff0000, 0xff000000 },
  [87] = { 0x41, 32, 0xff0000, 0xff00, 0xff, 0xff000000 },
  [88] = { 0x40, 32, 0xff0000, 0xff00, 0xff, 0 },
  [85] = { 0x40, 16, 0xf800, 0x7e0, 0x1f, 0 },
  [86] = { 0x41, 16, 0x7c00, 0x3e0, 0x1f, 0x8000 },
  [49] = { 0x20001, 16, 0xff, 0, 0, 0xff00 },
  [65] = { 0x2, 8, 0, 0, 0, 0xff },
  [61] = { 0x20000, 8, 0xff, 0, 0, 0 },
}

--- A DDS file for `texture` with `payload`: a legacy FourCC header, a plain pixel format, or
--- the DX10 extension (`style` picks; `arraySize` and `volume` make the ones the builder refuses).
local function ddsFile(
  texture: Texture,
  payload: string,
  style: string,
  arraySize: number?,
  volume: boolean?
): string
  local header = buffer.create(148)
  buffer.writestring(header, 0, 'DDS ')
  buffer.writeu32(header, 4, 124)
  buffer.writeu32(header, 8, 0x1007 + (if volume then 0x800000 else 0))
  buffer.writeu32(header, 12, texture.height)
  buffer.writeu32(header, 16, texture.width)
  buffer.writeu32(header, 24, 1)
  buffer.writeu32(header, 28, texture.mipCount)
  buffer.writeu32(header, 76, 32)
  local cube = bit32.band(texture.flags, 1) ~= 0
  buffer.writeu32(header, 112, if cube then 0xfe00 else 0)
  local length = 128
  if style == 'legacy' and LegacyCodes[texture.format] then
    buffer.writeu32(header, 80, 4)
    buffer.writestring(header, 84, LegacyCodes[texture.format])
  elseif style == 'plain' and PlainFormats[texture.format] then
    local plain = PlainFormats[texture.format]
    buffer.writeu32(header, 80, plain[1])
    for index = 2, 6 do
      buffer.writeu32(header, 84 + 4 * (index - 1), plain[index])
    end
  else
    buffer.writeu32(header, 80, 4)
    buffer.writestring(header, 84, 'DX10')
    buffer.writeu32(header, 128, texture.format)
    buffer.writeu32(header, 132, 3)
    buffer.writeu32(header, 136, if cube then 4 else 0)
    buffer.writeu32(header, 140, arraySize or 1)
    length = 148
  end

  return buffer.readstring(header, 0, length) .. payload
end

--- Where this file is, from its chunk name: the folder the suites and fixtures are in.
local function here(): string
  local source = debug.info(1, 's')
  local path = if string.sub(source, 1, 1) == '@' or string.sub(source, 1, 1) == '='
    then string.sub(source, 2)
    else source
  return string.match(path, '^(.*)/[^/]*$') or '.'
end

--- What a runner sets before it requires a suite, for the cases the harness's globals set.
local settings: { [string]: unknown } = {}

return table.freeze {
  settings = settings,
  generator = generator,
  writer = writer,
  tes3 = tes3,
  tes4 = tes4,
  ba2 = ba2,
  lz4Frame = lz4Frame,
  zlib = zlib,
  errorText = errorText,
  digest = digest,
  observe = observe,
  here = here,
  Formats = Formats,
  mipBytes = mipBytes,
  ddsFile = ddsFile,
}
