--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- BA2 (Fallout 4, Fallout 76, Starfield): general files (GNRL) in chunks, zlib or LZ4 block
-- compressed; textures (DX10) whose DDS header is rebuilt from the record on extraction; and
-- console textures (GNMF), whose metadata is read and whose payloads are refused.
local Store = require './store'
local bytes = require '@dream/bytes'
local dds = require './dds'
local names = require './names'
local types = require './types'
local util = require './util'

type Backend = types.Backend
type Store = types.Store
type Chunk = types.Chunk
type TextureHeader = types.TextureHeader

local fail, Messages = util.fail, util.Messages
local readu32, readu16, readu8 = buffer.readu32, buffer.readu16, buffer.readu8

local Magic = 0x58445442
local Gnrl, Dx10, Gnmf = 0x4c524e47, 0x30315844, 0x464d4e47
local Sentinel = 0xbaadf00d
local FormatNames: { [number]: string } = { [Gnrl] = 'gnrl', [Dx10] = 'dx10', [Gnmf] = 'gnmf' }
local HeaderSizes: { [number]: number } = { [Gnrl] = 0x10, [Dx10] = 0x18, [Gnmf] = 0x30 }
local Versions: { [number]: boolean } =
  { [1] = true, [2] = true, [3] = true, [7] = true, [8] = true }

--- The CRC-32 the BA2 hash uses: IEEE, but from 0 with no final inversion, skipping bytes past
--- 127. From `bytes.crc32`, whose seed continues a finished CRC: seeding it with all ones starts
--- the register at 0, and inverting the result undoes the finish.
local function crc(text: string): number
  if text == '' then return 0 end

  local clean = text
  if string.find(clean, '[\128-\255]') then clean = (string.gsub(clean, '[\128-\255]', '')) end

  return bit32.bnot(bytes.crc32(clean, 0xffffffff))
end

--- The BA2 normalization: `/` is `\`, ASCII is lowercased, separators come off both ends, and an
--- empty or overlong path is `.`.
local function normalize(path: string): string return util.normalizeHashPath(path) end

--- The three hash fields of a normalized path.
local function hashNormalized(path: string): (number, number, number)
  local separator = string.find(path, '\\[^\\]*$')
  local parent = if separator then string.sub(path, 1, separator - 1) else ''
  local first = if separator then separator + 1 else 1
  local dot = string.find(path, '%.[^%.]*$')
  local last = if dot then dot - 1 else #path
  local stem = if last >= first then string.sub(path, first, last) else ''
  local extension = if dot then string.sub(path, dot + 1, dot + 4) else ''
  local packed = 0
  for index = #extension, 1, -1 do
    packed = packed * 256 + string.byte(extension, index)
  end

  return crc(parent), crc(stem), packed
end

local function hashFile(path: string)
  local normalized = normalize(path)
  local directory, file, extension = hashNormalized(normalized)
  return { directory = directory, file = file, extension = extension, normalized = normalized }
end

--- A u64 field as a number: exact below 2^53, which every real offset is.
local function u64(data: buffer, offset: number): number
  return readu32(data, offset) + readu32(data, offset + 4) * 0x100000000
end

--- A hash field as the binding reads one: an exact u32, from a number or an integer.
local function exact(value: unknown, stack: number): number
  local number: number
  if type(value) == 'number' then
    number = value
  elseif typeof(value) == 'integer' then
    number = integer.tonumber(value :: integer)
  else
    return util.stackValue(stack, `expected integer, got {typeof(value)}`)
  end

  if number ~= math.floor(number) or number < 0 or number > 0xffffffff then
    util.stackValue(stack, `number {number} is not an exact u32`)
  end

  return number
end

local function open(store: Store): Backend
  local size = store.size
  local loaded = math.min(size, 65536)
  local index = Store.head(store, loaded)

  local function need(length: number)
    if length > size then fail(Messages.endOfFile) end

    if length > loaded then
      loaded = math.min(size, math.max(length, loaded * 2))
      index = Store.head(store, loaded)
    end
  end

  need(4)
  local magic = readu32(index, 0)
  if magic ~= Magic then util.invalidMagic(magic) end

  need(8)
  local version = readu32(index, 4)
  if not Versions[version] then util.invalidVersion(version) end

  need(12)
  local format = readu32(index, 8)
  local formatName = FormatNames[format]
  if not formatName then fail(`invalid format read from archive header: {util.hex32(format)}`) end

  need(24)
  local count = readu32(index, 12)
  local stringTable = u64(index, 16)
  local position = 24
  if version == 2 or version == 3 then
    need(32)
    position = 32
  end

  local compression = 'zip'
  if version == 3 then
    need(36)
    if readu32(index, 32) == 3 then compression = 'lz4' end

    position = 36
  end

  local headerSize = HeaderSizes[format]
  local chunked = format ~= Gnrl
  -- A record is at least 16 bytes, so a count the archive cannot hold reserves no more than it can.
  local records: { number } = table.create(math.min(count, size // 16)) :: { number }
  -- The records of a well-formed GNRL archive, read at once.
  if not chunked then need(math.min(size, position + 36 * count)) end

  -- Which mips a texture's chunks covered, marked with a number new for each texture.
  local marks: { [number]: number } = {}
  local stamp = 0
  for entry = 1, count do
    records[entry] = position
    need(position + 16)
    local chunkCount = readu8(index, position + 13)
    local fileHeaderSize = readu16(index, position + 14)
    if fileHeaderSize ~= headerSize then fail(`invalid BA2 file header size: {fileHeaderSize}`) end

    position += 16
    local textureAt = position
    if format == Dx10 then
      need(position + 8)
      position += 8
    elseif format == Gnmf then
      need(position + 32)
      position += 32
    end

    local chunkSize = if chunked then 24 else 20
    local firstChunk = position
    for _ = 1, chunkCount do
      need(position + chunkSize)
      local sentinel = readu32(index, position + chunkSize - 4)
      if sentinel ~= Sentinel then fail(`invalid chunk sentinel: {util.hex32(sentinel)}`) end

      local packed, unpacked = readu32(index, position + 8), readu32(index, position + 12)
      if u64(index, position) + (if packed == 0 then unpacked else packed) > size then
        fail(Messages.outOfBounds)
      end

      position += chunkSize
    end

    if format == Dx10 then
      local height, width = readu16(index, textureAt), readu16(index, textureAt + 2)
      local mipCount, textureFormat = readu8(index, textureAt + 4), readu8(index, textureAt + 5)
      local flags = readu8(index, textureAt + 6)
      dds.check(height, width, mipCount, textureFormat)
      stamp += 1
      for chunk = 0, chunkCount - 1 do
        local at = firstChunk + 24 * chunk
        local first, last = readu16(index, at + 16), readu16(index, at + 18)
        if first > last or last >= mipCount then util.dds 'invalid BA2 texture mip range' end

        for mip = first, last do
          if marks[mip] == stamp then util.dds 'duplicate BA2 texture mip range' end

          marks[mip] = stamp
        end

        local expected = dds.mipBytes(height, width, flags, textureFormat, first, last)
        if readu32(index, at + 12) ~= expected then
          util.dds 'DDS payload size does not match metadata'
        end
      end

      for mip = 0, mipCount - 1 do
        if marks[mip] ~= stamp then util.dds 'missing BA2 texture mip range' end
      end
    end
  end

  -- The string table: a u16 length and that many bytes per entry, in record order.
  local names_: buffer? = nil
  local namesBase = 0
  local nameStarts: { number } = {}
  if stringTable ~= 0 then
    if stringTable > size then fail(Messages.outOfBounds) end

    local table_, base = Store.region(store, stringTable, size - stringTable)
    names_, namesBase = table_, base
    nameStarts = table.create(count) :: { number }
    local cursor = base
    local limit = base + size - stringTable
    for entry = 1, count do
      if cursor + 2 > limit then fail(Messages.endOfFile) end

      local length = readu16(table_, cursor)
      if cursor + 2 + length > limit then fail(Messages.endOfFile) end

      nameStarts[entry] = cursor
      cursor += 2 + length
    end
  end

  local function nameOf(entry: number): string?
    local region = names_
    if not region then return nil end

    local start = nameStarts[entry]
    local length = readu16(region, start)
    if length == 0 then return nil end

    return buffer.readstring(region, start + 2, length)
  end

  local function chunkCountOf(entry: number): number return readu8(index, records[entry] + 13) end

  local function chunkAt(entry: number, chunk: number): number
    return records[entry]
      + (if format == Dx10 then 24 elseif format == Gnmf then 48 else 16)
      + (if chunked then 24 else 20) * chunk
  end

  local function textureOf(entry: number): TextureHeader?
    if format ~= Dx10 then return nil end

    local at = records[entry] + 16
    return {
      height = readu16(index, at),
      width = readu16(index, at + 2),
      mipCount = readu8(index, at + 4),
      format = readu8(index, at + 5),
      flags = readu8(index, at + 6),
      tileMode = readu8(index, at + 7),
    }
  end

  local function chunksSize(entry: number): number
    local total = 0
    for chunk = 0, chunkCountOf(entry) - 1 do
      total += readu32(index, chunkAt(entry, chunk) + 12)
    end

    return total
  end

  local function decodedSize(entry: number): (number?, string?)
    if format == Gnmf then
      return nil, 'support for this feature is not implemented: BA2 GNMF extraction'
    end

    local total = chunksSize(entry)
    if format == Dx10 then
      total += dds.headerLength(readu8(index, records[entry] + 21))
    end

    return total, nil
  end

  --- One chunk's decoded bytes.
  local function chunkBytes(entry: number, chunk: number): buffer
    local at = chunkAt(entry, chunk)
    local offset, packed, unpacked = u64(index, at), readu32(index, at + 8), readu32(index, at + 12)
    if packed == 0 then return Store.read(store, offset, unpacked) end

    local compressed = Store.read(store, offset, packed)
    if compression == 'lz4' then return util.lz4BlockExact(compressed, unpacked) end

    return util.inflateExact(compressed, unpacked)
  end

  --- Writes an entry's decoded bytes into `target` at `at`, uncompressed chunks straight from the
  --- archive; returns the count.
  local function decodeInto(entry: number, target: buffer, at: number): number
    if format == Gnmf then util.notImplemented 'BA2 GNMF extraction' end

    local cursor = at
    if format == Dx10 then
      local header = dds.headerOf(textureOf(entry) :: TextureHeader)
      buffer.copy(target, cursor, header)
      cursor += buffer.len(header)
    end

    for chunk = 0, chunkCountOf(entry) - 1 do
      local position = chunkAt(entry, chunk)
      local packed, unpacked = readu32(index, position + 8), readu32(index, position + 12)
      if packed == 0 then
        Store.copyInto(store, target, cursor, u64(index, position), unpacked)
      else
        buffer.copy(target, cursor, chunkBytes(entry, chunk))
      end

      cursor += unpacked
    end

    return cursor - at
  end

  local function decode(entry: number): buffer
    if format == Gnmf then util.notImplemented 'BA2 GNMF extraction' end

    if format == Gnrl and chunkCountOf(entry) == 1 then return chunkBytes(entry, 0) end

    local total = decodedSize(entry) :: number
    local out = buffer.create(total)
    decodeInto(entry, out, 0)
    return out
  end

  local nameIndex: names.Index? = nil

  local function buildNames(): names.Index
    local built = names.new()
    local region = names_
    if not region then return built end

    local plain = names.regionCanonical(region, namesBase, size - stringTable)
    for entry = 1, count do
      local start = nameStarts[entry]
      local length = readu16(region, start)
      if length > 0 then
        built.add(region, start + 2, length, entry, plain and readu8(region, start + 2) ~= 92)
      end
    end

    return built
  end

  local hashes: { [number]: { [integer]: number } }? = nil

  local function hashIndex(): { [number]: { [integer]: number } }
    local map = hashes
    if map then return map end

    local built: { [number]: { [integer]: number } } = {}
    for entry = count, 1, -1 do
      local at = records[entry]
      local directory = readu32(index, at + 8)
      local files = built[directory]
      if not files then
        files = {}
        built[directory] = files
      end

      files[util.int64(readu32(index, at), readu32(index, at + 4))] = entry
    end

    hashes = built
    return built
  end

  local function find(directory: number, file: number, extension: number): number?
    local files = hashIndex()[directory]
    return if files then files[util.int64(file, extension)] else nil
  end

  local function lookup(raw: string): number?
    local built = nameIndex or buildNames()
    nameIndex = built

    if not built.isEmpty() then return built.find(raw) end

    local key = util.lookupKey(raw)
    if not util.namesAFile(key) then return nil end

    local directory, file, extension = hashNormalized(normalize(key))
    return find(directory, file, extension)
  end

  local missing = 'a BA2 hash lookup takes three numbers: the directory, file, and extension hashes'

  local function hashLookup(first: unknown, second: unknown, third: unknown): number?
    if second == nil or third == nil then fail(missing) end

    local directory, file, extension = exact(first, 2), exact(second, 3), exact(third, 4)
    return find(directory, file, extension)
  end

  local function chunkList(entry: number): { Chunk }
    local list: { Chunk } = table.create(chunkCountOf(entry)) :: { Chunk }
    for chunk = 0, chunkCountOf(entry) - 1 do
      local at = chunkAt(entry, chunk)
      list[chunk + 1] = {
        offset = u64(index, at),
        packedSize = readu32(index, at + 8),
        size = readu32(index, at + 12),
        firstMip = if chunked then readu16(index, at + 16) else 0,
        lastMip = if chunked then readu16(index, at + 18) else 0,
      }
    end

    return list
  end

  return {
    format = 'ba2',
    count = count,
    store = store,
    lookup = lookup,
    hashLookup = hashLookup,
    path = nameOf,
    name = function(entry: number): string?
      local name = nameOf(entry)
      return if name then util.lastComponent(name) else nil
    end,

    folder = function(entry: number): string?
      local name = nameOf(entry)
      return if name then util.parentFolder(name) else nil
    end,
    size = decodedSize,
    storedSize = function(entry: number): number
      local total = 0
      for chunk = 0, chunkCountOf(entry) - 1 do
        local at = chunkAt(entry, chunk)
        local packed = readu32(index, at + 8)
        total += if packed == 0 then readu32(index, at + 12) else packed
      end

      return total
    end,

    offset = function(entry: number): number?
      if chunkCountOf(entry) == 0 then return nil end

      return u64(index, chunkAt(entry, 0))
    end,

    compressed = function(entry: number): boolean
      for chunk = 0, chunkCountOf(entry) - 1 do
        if readu32(index, chunkAt(entry, chunk) + 8) ~= 0 then return true end
      end

      return false
    end,
    chunks = chunkCountOf,
    hash = function(): integer? return nil end,
    folderHash = function(): integer? return nil end,
    ba2Hash = function(entry: number): { directory: number, file: number, extension: number }?
      local at = records[entry]
      return {
        directory = readu32(index, at + 8),
        file = readu32(index, at),
        extension = readu32(index, at + 4),
      }
    end,
    decode = decode,
    decodeString = function(entry: number): string
      if format == Gnrl and chunkCountOf(entry) == 1 then
        local at = chunkAt(entry, 0)
        if readu32(index, at + 8) == 0 then
          return Store.readString(store, u64(index, at), readu32(index, at + 12))
        end
      end

      return buffer.tostring(decode(entry))
    end,
    decodeInto = decodeInto,

    row = function(entry: number): { [string]: unknown }
      local at = records[entry]
      local chunkCount = chunkCountOf(entry)
      local stored, unpacked, compressed = 0, 0, false
      for chunk = 0, chunkCount - 1 do
        local position = chunkAt(entry, chunk)
        local packed, size = readu32(index, position + 8), readu32(index, position + 12)
        stored += if packed == 0 then size else packed
        unpacked += size
        compressed = compressed or packed ~= 0
      end

      local row: { [string]: unknown } = {
        index = entry,
        id = entry,
        format = 'ba2',
        path = nameOf(entry),
        storedSize = stored,
        offset = if chunkCount > 0 then u64(index, chunkAt(entry, 0)) else nil,
        compressed = compressed,
        chunks = chunkCount,
        directoryHash = readu32(index, at + 8),
        fileHash = readu32(index, at),
        extensionHash = readu32(index, at + 4),
      }

      if format == Gnrl then
        row.size = unpacked
      elseif format == Dx10 then
        row.size = unpacked + dds.headerLength(readu8(index, at + 21))
      end

      return row
    end,

    info = function(): types.BA2Info
      return {
        format = formatName,
        version = version,
        compression = compression,
        strings = stringTable ~= 0,
      }
    end,

    ba2 = {
      format = formatName,
      version = version,
      compression = compression,
      texture = textureOf,
      chunkList = chunkList,
    },
  }
end

return table.freeze {
  open = open,
  hashFile = hashFile,
  hashNormalized = hashNormalized,
  normalize = normalize,
}
