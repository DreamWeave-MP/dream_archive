--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- TES4-family PC BSA (Oblivion to Skyrim SE): versions 103 and 104 with zlib, 105 with LZ4
-- frames; names from string tables, from embedded payload names, both, or neither. The index is
-- checked when the archive opens, in the crate's order; names and lookups are made when asked.
local Store = require './store'
local names = require './names'
local types = require './types'
local util = require './util'

type Backend = types.Backend
type Store = types.Store

local fail, Messages = util.fail, util.Messages
local readu32, readu16, readu8 = buffer.readu32, buffer.readu16, buffer.readu8
local band = bit32.band

local Magic = 0x00415342
local HeaderSize = 0x24
local Flags = table.freeze {
  directoryStrings = 0x1,
  fileStrings = 0x2,
  compressed = 0x4,
  xboxArchive = 0x40,
  embeddedNames = 0x100,
  xboxCompressed = 0x200,
}

--- The crate's checksum of a hash's middle bytes: `crc = byte + crc * 0x1003f`, wrapping.
local function crc(text: string, first: number, last: number): number
  local value = 0
  for index = first, last do
    value = (string.byte(text, index) + value * 0x1003f) % 0x100000000
  end

  return value
end

export type Hash = { last: number, last2: number, length: number, first: number, crc: number }

local Scratch = buffer.create(8)

--- The on-disk 64-bit value: `last`, `last2`, `length`, `first`, then `crc`, little-endian.
local function numeric(hash: Hash): integer
  buffer.writeu8(Scratch, 0, hash.last)
  buffer.writeu8(Scratch, 1, hash.last2)
  buffer.writeu8(Scratch, 2, hash.length)
  buffer.writeu8(Scratch, 3, hash.first)
  buffer.writeu32(Scratch, 4, hash.crc)
  return buffer.readinteger(Scratch, 0)
end

local function hashNormalizedDirectory(path: string): Hash
  local length = #path
  local hash = { last = 0, last2 = 0, length = length % 256, first = 0, crc = 0 }
  if length >= 3 then hash.last2 = string.byte(path, length - 1) end

  if length >= 1 then
    hash.last = string.byte(path, length)
    hash.first = string.byte(path, 1)
  end

  if hash.length > 3 then hash.crc = crc(path, 2, length - 2) end

  return hash
end

--- The extensions Bethesda's hash knows, by their first four bytes.
local Extensions: { [string]: number } =
  { ['.nif'] = 1, ['.kf'] = 2, ['.dds'] = 3, ['.wav'] = 4, ['.adp'] = 5 }

local function hashNormalizedFile(path: string): Hash
  local dot = string.find(path, '%.[^%.]*$')
  local stem = if dot then string.sub(path, 1, dot - 1) else path
  local extension = if dot then string.sub(path, dot) else ''
  if stem == '' or #stem >= 260 or #extension >= 16 then
    return { last = 0, last2 = 0, length = 0, first = 0, crc = 0 }
  end

  local hash = hashNormalizedDirectory(stem)
  hash.crc = (hash.crc + crc(extension, 1, #extension)) % 0x100000000
  -- The crate compares the extension's first four bytes, padded with NUL, to each known one.
  local four = string.sub(extension, 1, 4)
  local fourEnd = string.find(four, '\0', 1, true)
  if fourEnd then four = string.sub(four, 1, fourEnd - 1) end

  local known = if extension == '' then 0 else Extensions[four]
  if known then
    hash.first = (hash.first + 32 * band(known, 0xfc)) % 256
    hash.last = (hash.last + band(known, 0xfe) * 64) % 256
    hash.last2 = (hash.last2 + known * 128) % 256
  end

  return hash
end

local function hashDirectory(path: string): Hash
  return hashNormalizedDirectory(util.normalizeHashPath(path))
end

local function hashFile(path: string): Hash
  local normalized = util.normalizeHashPath(path)
  local at = string.find(normalized, '\\[^\\]*$')
  if at then normalized = string.sub(normalized, at + 1) end

  return hashNormalizedFile(normalized)
end

--- The table the binding returns for a hash.
local function described(hash: Hash)
  local value = numeric(hash)
  return {
    last = hash.last,
    last2 = hash.last2,
    length = hash.length,
    first = hash.first,
    crc = hash.crc,
    hex = util.hex64(value),
    hash = value,
  }
end

--- The folder and file hashes of a lookup-normalized path.
local function pathHash(key: string): (integer, integer)
  local at = string.find(key, '/[^/]*$')
  local folder = if at then string.sub(key, 1, at - 1) else ''
  local name = if at then string.sub(key, at + 1) else key
  return numeric(hashDirectory(folder)), numeric(hashFile(name))
end

--- The stored path of a folder and a name, joined as the crate joins them.
local function join(folder: string, name: string): string
  if folder == '' or name == '' then return folder .. name end

  return `{folder}\\{name}`
end

local function open(store: Store): Backend
  local size = store.size
  local loaded = math.min(size, 4096)
  local index = Store.head(store, loaded)

  --- Makes the index buffer hold at least `length` bytes; the caller checked `size`.
  local function ensure(length: number)
    if length <= loaded then return end

    loaded = math.min(size, math.max(length, loaded * 2))
    index = Store.head(store, loaded)
  end

  local function need(length: number)
    if length > size then fail(Messages.endOfFile) end

    ensure(length)
  end

  need(4)
  local magic = readu32(index, 0)
  if magic ~= Magic then util.invalidMagic(magic) end

  need(8)
  local version = readu32(index, 4)
  if version ~= 103 and version ~= 104 and version ~= 105 then util.invalidVersion(version) end

  need(12)
  local folderRecordOffset = readu32(index, 8)
  if folderRecordOffset ~= HeaderSize then
    fail(`invalid size read from archive header: {folderRecordOffset}`)
  end

  need(16)
  local flags = readu32(index, 12)
  if band(flags, Flags.xboxArchive) ~= 0 then util.notImplemented 'TES4 Xbox archive layout' end

  -- Oblivion's version 103 archives set the XMem bit on PC too; it only means XMem from 104.
  if
    version ~= 103
    and band(flags, Flags.compressed) ~= 0
    and band(flags, Flags.xboxCompressed) ~= 0
  then
    util.notImplemented 'TES4 XMem compression'
  end

  need(36)
  local folderCount, fileCount = readu32(index, 16), readu32(index, 20)
  local fileNamesLength = readu32(index, 28)
  local directoryStrings = band(flags, Flags.directoryStrings) ~= 0
  local fileStrings = band(flags, Flags.fileStrings) ~= 0
  local archiveCompressed = band(flags, Flags.compressed) ~= 0
  local embedded = version ~= 103 and band(flags, Flags.embeddedNames) ~= 0
  local usesXmem = version ~= 103 and band(flags, Flags.xboxCompressed) ~= 0
  local recordSize = if version == 105 then 24 else 16

  -- The whole index of a well-formed archive, read at once.
  ensure(
    math.min(
      size,
      HeaderSize
        + recordSize * folderCount
        + (if directoryStrings then folderCount + readu32(index, 24) else 0)
        + 16 * fileCount
        + fileNamesLength
    )
  )

  local parsedFileCount = 0
  for folder = 0, folderCount - 1 do
    local record = HeaderSize + recordSize * folder
    need(record + recordSize)
    parsedFileCount += readu32(index, record + 8)
    if parsedFileCount > 0xffffffff then fail(Messages.outOfBounds) end
  end

  if parsedFileCount ~= fileCount then fail(Messages.outOfBounds) end

  local folderNames: { string } = table.create(folderCount) :: { string }
  local folderRecords: { number } = table.create(folderCount) :: { number }
  -- A file record is 16 bytes, so a count the archive cannot hold reserves no more than it can.
  local entryFolder: { number } = table.create(math.min(fileCount, size // 16)) :: { number }
  local entryRecord: { number } = table.create(math.min(fileCount, size // 16)) :: { number }
  local position = HeaderSize + recordSize * folderCount
  local entry = 0
  for folder = 1, folderCount do
    local record = HeaderSize + recordSize * (folder - 1)
    folderRecords[folder] = record
    local filesAt = if version == 105
      then readu32(index, record + 16) + readu32(index, record + 20) * 0x100000000
      else readu32(index, record + 12)
    if filesAt < position or filesAt > size then fail(Messages.outOfBounds) end

    if directoryStrings then
      need(position + 1)
      local length = readu8(index, position)
      need(position + 1 + length)
      if length == 0 or readu8(index, position + length) ~= 0 then fail(Messages.endOfFile) end

      local name = buffer.readstring(index, position + 1, length - 1)
      folderNames[folder] = if name == '.' then '' else name
      position += 1 + length
    end

    local files = readu32(index, record + 8)
    need(position + 16 * files)
    for _ = 1, files do
      entry += 1
      entryFolder[entry] = folder
      entryRecord[entry] = position
      position += 16
    end
  end

  local namesStart = position
  local namesEnd = namesStart + fileNamesLength
  if namesEnd > size then fail(Messages.outOfBounds) end

  ensure(namesEnd)

  local nameStarts: { number } = if fileStrings then table.create(fileCount) :: { number } else {}
  local nameLengths: { number } = if fileStrings then table.create(fileCount) :: { number } else {}
  local embeddedFolders: { string } = if embedded then table.create(fileCount) :: { string } else {}
  local embeddedNames: { string } = if embedded then table.create(fileCount) :: { string } else {}
  local embeddedSkips: { number } = if embedded then table.create(fileCount) :: { number } else {}
  local cursor = namesStart
  for current = 1, fileCount do
    local record = entryRecord[current]
    if embedded then
      local start = band(readu32(index, record + 12), 0x7fffffff)
      local stored = band(readu32(index, record + 8), 0x3fffffff)
      if start + stored > size or stored == 0 then fail(Messages.outOfBounds) end

      local head = Store.read(store, start, math.min(stored, 256))
      local length = readu8(head, 0)
      if 1 + length > stored then fail(Messages.outOfBounds) end

      local text = buffer.readstring(head, 1, length)
      embeddedSkips[current] = 1 + length
      local at = string.find(text, '[/\\][^/\\]*$')
      embeddedFolders[current] = if at then string.sub(text, 1, at - 1) else ''
      embeddedNames[current] = if at then string.sub(text, at + 1) else text
    end

    if fileStrings then
      local finish = cursor
      while finish < namesEnd and readu8(index, finish) ~= 0 do
        finish += 1
      end

      if finish >= namesEnd then fail(Messages.endOfFile) end

      nameStarts[current] = cursor
      nameLengths[current] = finish - cursor
      cursor = finish + 1
    end
  end

  -- Fallout 3's voice archives pad the name block with NULs after the last name.
  for padding = cursor, namesEnd - 1 do
    if readu8(index, padding) ~= 0 then fail(Messages.outOfBounds) end
  end

  for current = 1, fileCount do
    local record = entryRecord[current]
    local start = band(readu32(index, record + 12), 0x7fffffff)
    local stored = band(readu32(index, record + 8), 0x3fffffff)
    if start < namesEnd or start + stored > size then fail(Messages.outOfBounds) end
  end

  local function folderOf(current: number): string?
    if directoryStrings then return folderNames[entryFolder[current]] end

    return if embedded then embeddedFolders[current] else nil
  end

  local function nameOf(current: number): string?
    if fileStrings then
      return buffer.readstring(index, nameStarts[current], nameLengths[current])
    end

    return if embedded then embeddedNames[current] else nil
  end

  local function path(current: number): string?
    local folder, name = folderOf(current), nameOf(current)
    if not folder or not name then return nil end

    return join(folder, name)
  end

  local function isCompressed(current: number): boolean
    local toggled = band(readu32(index, entryRecord[current] + 8), 0x40000000) ~= 0
    return archiveCompressed ~= toggled
  end

  --- Where an entry's payload starts and how long it is, past its embedded name.
  local function payload(current: number): (number, number)
    local record = entryRecord[current]
    local start = band(readu32(index, record + 12), 0x7fffffff)
    local length = band(readu32(index, record + 8), 0x3fffffff)
    if embedded then
      local skip = embeddedSkips[current]
      return start + skip, length - skip
    end

    return start, length
  end

  local function decodedSize(current: number): (number?, string?)
    local start, length = payload(current)
    if not isCompressed(current) then return length, nil end

    if length < 4 then return nil, Messages.outOfBounds end

    return Store.u32(store, start), nil
  end

  local function decode(current: number): buffer
    local start, length = payload(current)
    if not isCompressed(current) then return Store.read(store, start, length) end

    if usesXmem then util.notImplemented 'TES4 XMem compression' end

    if length < 4 then fail(Messages.outOfBounds) end

    local expected = Store.u32(store, start)
    local compressed = Store.read(store, start + 4, length - 4)
    if version == 105 then
      if length < 8 or readu32(compressed, 0) ~= 0x184d2204 then fail(Messages.invalidLz4Frame) end

      return util.lz4FrameExact(compressed, expected)
    end

    return util.inflateExact(compressed, expected)
  end

  --- The paths of the entries that have one, joined into a scratch buffer as the crate joins
  --- them, without making strings.
  local function buildNames(): names.Index
    local built = names.new()
    local joined = buffer.create(1024)
    -- With the name block and every folder free of `/`, of `\\` pairs, and of separators at
    -- either end, a joined path is canonical as it stands.
    local plain = fileStrings
      and directoryStrings
      and names.regionCanonical(index, namesStart, fileNamesLength)
    if plain then
      for _, folder in folderNames do
        if
          string.find(folder, '/', 1, true)
          or string.find(folder, '\\\\', 1, true)
          or string.byte(folder, 1) == 92
          or string.byte(folder, -1) == 92
        then
          plain = false
          break
        end
      end
    end

    for current = 1, fileCount do
      local folder = folderOf(current)
      if not folder then continue end

      local nameLength = if fileStrings
        then nameLengths[current]
        elseif embedded then #embeddedNames[current]
        else nil
      if not nameLength then continue end

      local length = #folder + 1 + nameLength
      if length > buffer.len(joined) then joined = buffer.create(2 * length) end

      buffer.writestring(joined, 0, folder)
      local at = #folder
      if at > 0 and nameLength > 0 then
        buffer.writeu8(joined, at, 92)
        at += 1
      end

      if fileStrings then
        buffer.copy(joined, at, index, nameStarts[current], nameLength)
      else
        buffer.writestring(joined, at, embeddedNames[current])
      end

      built.add(
        joined,
        0,
        at + nameLength,
        current,
        plain and (nameLength == 0 or readu8(index, nameStarts[current]) ~= 92)
      )
    end

    return built
  end

  local hashes: { [integer]: { [integer]: number } }? = nil

  local function hashIndex(): { [integer]: { [integer]: number } }
    local map = hashes
    if map then return map end

    local built: { [integer]: { [integer]: number } } = {}
    for current = fileCount, 1, -1 do
      local folder = buffer.readinteger(index, folderRecords[entryFolder[current]])
      local files = built[folder]
      if not files then
        files = {}
        built[folder] = files
      end

      files[buffer.readinteger(index, entryRecord[current])] = current
    end

    hashes = built
    return built
  end

  local nameIndex: names.Index? = nil

  local function lookup(raw: string): number?
    local built = nameIndex or buildNames()
    nameIndex = built

    if not built.isEmpty() then return built.find(raw) end

    local key = util.lookupKey(raw)
    if not util.namesAFile(key) then return nil end

    local folder, file = pathHash(key)
    local files = hashIndex()[folder]
    return if files then files[file] else nil
  end

  local message = 'a TES4 hash lookup takes two integers: the folder hash and the file hash'

  local function hashLookup(first: unknown, second: unknown, third: unknown): number?
    if third ~= nil or second == nil then fail(message) end

    if typeof(first) ~= 'integer' then
      util.stackValue(2, `expected integer, got {typeof(first)}`)
    end

    if typeof(second) ~= 'integer' then
      util.stackValue(3, `expected integer, got {typeof(second)}`)
    end

    local files = hashIndex()[first :: integer]
    return if files then files[second :: integer] else nil
  end

  return {
    format = 'bsaTes4',
    count = fileCount,
    store = store,
    lookup = lookup,
    hashLookup = hashLookup,
    path = path,
    name = nameOf,
    folder = folderOf,
    size = decodedSize,
    storedSize = function(current: number): number
      return band(readu32(index, entryRecord[current] + 8), 0x3fffffff)
    end,

    offset = function(current: number): number?
      return band(readu32(index, entryRecord[current] + 12), 0x7fffffff)
    end,
    compressed = isCompressed,
    chunks = function(): number? return nil end,
    hash = function(current: number): integer?
      return buffer.readinteger(index, entryRecord[current])
    end,

    folderHash = function(current: number): integer?
      return buffer.readinteger(index, folderRecords[entryFolder[current]])
    end,
    ba2Hash = function(): { directory: number, file: number, extension: number }? return nil end,
    decode = decode,
    decodeString = function(current: number): string
      if isCompressed(current) then return buffer.tostring(decode(current)) end

      local start, length = payload(current)
      return Store.readString(store, start, length)
    end,

    row = function(current: number): { [string]: unknown }
      local record = entryRecord[current]
      local row: { [string]: unknown } = {
        index = current,
        id = current,
        format = 'bsaTes4',
        path = path(current),
        storedSize = band(readu32(index, record + 8), 0x3fffffff),
        offset = band(readu32(index, record + 12), 0x7fffffff),
        compressed = isCompressed(current),
        hash = buffer.readinteger(index, record),
        folderHash = buffer.readinteger(index, folderRecords[entryFolder[current]]),
      }

      if not row.compressed then
        local _, length = payload(current)
        row.size = length
      end

      return row
    end,

    decodeInto = function(current: number, target: buffer, at: number): number
      if isCompressed(current) then
        local decoded = decode(current)
        buffer.copy(target, at, decoded)
        return buffer.len(decoded)
      end

      local start, length = payload(current)
      Store.copyInto(store, target, at, start, length)
      return length
    end,

    tes4 = { version = version, flags = flags, archiveTypes = readu16(index, 32) },
  }
end

return table.freeze {
  open = open,
  hashDirectory = hashDirectory,
  hashFile = hashFile,
  described = described,
  numeric = numeric,
  Flags = Flags,
}
