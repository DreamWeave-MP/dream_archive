--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Builds BA2s: general archives (GNRL), one chunk per member, and texture archives (DX10) from
-- DDS files, from texture metadata and payloads, or from another archive's texture records copied
-- chunk for chunk. Members are ordered by hash and then path.
local Store = require './store'
local archive = require './archive'
local ba2 = require './ba2'
local bytes = require '@dream/bytes'
local dds = require './dds'
local sources = require './sources'
local tes4Builder = require './tes4_builder'
local types = require './types'
local util = require './util'

type Source = types.Source
type Backend = types.Backend
type TextureHeader = types.TextureHeader
type Chunk = types.Chunk

local fail, Messages = util.fail, util.Messages
local writeu32, writeu16, writeu8 = buffer.writeu32, buffer.writeu16, buffer.writeu8

local Versions: { [number]: boolean } =
  { [1] = true, [2] = true, [3] = true, [7] = true, [8] = true }
local Sentinel = 0xbaadf00d

local function headerSize(version: number): number
  if version == 2 then return 32 end

  if version == 3 then return 36 end

  return 24
end

local function parseVersion(what: string, value: unknown): number
  local version = util.checkInteger(what, value, 1, 'u32', 0, 0xffffffff)
  if not Versions[version] then fail(`unsupported BA2 version: {version}`) end

  return version
end

local function parseCompression(value: unknown): string?
  if value == nil or value == 'none' or value == 'store' then return nil end

  if value == 'zip' or value == 'lz4' then return value :: string end

  return fail(`unknown BA2 compression: {value}`)
end

--- A stored path: TES3's rules, and shorter than 260 bytes.
local function normalizeStoredPath(path: string): string
  local stored = util.storedPath(path)
  if not stored or #stored >= 260 then fail(Messages.ba2InvalidPath) end

  return stored :: string
end

type Member = {
  name: string,
  directory: number,
  file: number,
  extension: number,
  compression: 'inherit' | 'store' | 'compress',
  source: Source,
  texture: TextureHeader?,
}

--- The members by their hash's file, extension, and directory fields, then path.
local function sortMembers(members: { Member }): { Member }
  local function key(member: Member): number
    return member.file * 0x200000 + member.extension // 0x800
  end

  local function before(left: Member, right: Member): boolean
    if left.extension ~= right.extension then return left.extension < right.extension end

    if left.directory ~= right.directory then return left.directory < right.directory end

    return left.name < right.name
  end

  return util.sortByKey(members, key, before)
end

--- The common header: magic, version, payload format, count, string table offset, and the
--- fields versions 2 and 3 add.
local function writeHeader(
  out: buffer,
  version: number,
  format: string,
  count: number,
  stringTable: number,
  lz4: boolean
)
  buffer.writestring(out, 0, 'BTDX')
  writeu32(out, 4, version)
  buffer.writestring(out, 8, format)
  writeu32(out, 12, count)
  writeu32(out, 16, stringTable % 0x100000000)
  writeu32(out, 20, stringTable // 0x100000000)
  if version == 2 or version == 3 then writeu32(out, 24, 1) end

  if version == 3 then writeu32(out, 32, if lz4 then 3 else 0) end
end

local function stringTable(list: { Member }): buffer
  local length = 0
  for _, member in list do
    length += 2 + #member.name
  end

  local out = buffer.create(length)
  local cursor = 0
  for _, member in list do
    writeu16(out, cursor, #member.name)
    buffer.writestring(out, cursor + 2, member.name)
    cursor += 2 + #member.name
  end

  return out
end

local function isCompressed(member: Member, default: string?): boolean
  if member.compression == 'inherit' then return default ~= nil end

  return member.compression == 'compress'
end

local function compress(data: buffer | string, default: string?, level: number): buffer
  if default == 'lz4' then return bytes.lz4Compress(data) end

  return bytes.deflate(data, { format = 'zlib', level = level })
end

local function new()
  local version = 1
  local compression: string? = nil
  local level = 6
  local members: { Member } = {}
  local names: { [string]: boolean } = {}

  local function addSource(name: string, mode: 'inherit' | 'store' | 'compress', source: Source)
    if names[name] then fail(Messages.ba2Duplicate) end

    local directory, file, extension = ba2.hashNormalized(name)
    names[name] = true
    table.insert(members, {
      name = name,
      directory = directory,
      file = file,
      extension = extension,
      compression = mode,
      source = source,
    })
  end

  local function write(sink: sources.Sink?): buffer?
    if compression == 'lz4' and version ~= 3 then
      for _, member in members do
        if isCompressed(member, compression) then
          util.notImplemented 'BA2 LZ4 writer requires version 3'
        end
      end
    end

    local list = sortMembers(members)
    local count = #list
    local recordsAt = headerSize(version)
    local head = buffer.create(recordsAt + 36 * count)
    writeHeader(head, version, 'GNRL', count, 0, compression == 'lz4')
    local anyCompressed = false
    for position, member in list do
      local record = recordsAt + 36 * (position - 1)
      writeu32(head, record, member.file)
      writeu32(head, record + 4, member.extension)
      writeu32(head, record + 8, member.directory)
      writeu8(head, record + 13, 1)
      writeu16(head, record + 14, 0x10)
      if member.source.length > 0xffffffff then fail(Messages.integral) end

      writeu32(head, record + 28, member.source.length)
      writeu32(head, record + 32, Sentinel)
      anyCompressed = anyCompressed or isCompressed(member, compression)
    end

    local names = stringTable(list)
    local target = sink
    if not target then
      local total: number? = nil
      if not anyCompressed then
        total = buffer.len(head) + buffer.len(names)
        for _, member in list do
          total = (total :: number) + member.source.length
        end
      end

      target = sources.memorySink(total)
    end

    local output = target :: sources.Sink
    sources.add(output, head)
    for position, member in list do
      local record = recordsAt + 36 * (position - 1)
      local offset = output.length
      local data = sources.load(member.source)
      local packed = 0
      if isCompressed(member, compression) then
        local compressed = compress(data, compression, level)
        packed = buffer.len(compressed)
        data = compressed
      end

      writeu32(head, record + 16, offset % 0x100000000)
      writeu32(head, record + 20, offset // 0x100000000)
      writeu32(head, record + 24, packed)
      sources.add(output, data)
    end

    local tableAt = output.length
    writeu32(head, 16, tableAt % 0x100000000)
    writeu32(head, 20, tableAt // 0x100000000)
    sources.add(output, names)
    return sources.finish(output, head)
  end

  local builder = {}

  function builder.len(_self: unknown): number return #members end

  function builder.isEmpty(_self: unknown): boolean return #members == 0 end

  function builder.setVersion(_self: unknown, value: number)
    version = parseVersion('dream.archive.Ba2Builder.setVersion', value)
  end

  function builder.setCompression(_self: unknown, value: string?)
    if value ~= nil then util.checkString('dream.archive.Ba2Builder.setCompression', value, 1) end

    compression = parseCompression(value)
  end

  function builder.setZlibLevel(_self: unknown, value: number)
    level = tes4Builder.zlibLevel('dream.archive.Ba2Builder.setZlibLevel', value)
  end

  local function addBytes(
    path: unknown,
    data: unknown,
    mode: 'inherit' | 'store' | 'compress',
    what: string
  )
    local name = normalizeStoredPath(util.checkString(what, path, 1))
    if names[name] then fail(Messages.ba2Duplicate) end

    addSource(name, mode, sources.fromBytes(util.checkBytes(what, data, 2)))
  end

  function builder.addBytes(_self: unknown, path: string, data: buffer | string)
    addBytes(path, data, 'inherit', 'dream.archive.Ba2Builder.addBytes')
  end

  function builder.addBytesWithCompression(
    _self: unknown,
    path: string,
    data: buffer | string,
    mode: string?
  )
    local what = 'dream.archive.Ba2Builder.addBytesWithCompression'
    util.checkString(what, path, 1)
    util.checkBytes(what, data, 2)
    addBytes(path, data, tes4Builder.override(mode), what)
  end

  function builder.addFile(_self: unknown, archivePath: string, source: string)
    local what = 'dream.archive.Ba2Builder.addFile'
    local stored = util.checkString(what, archivePath, 1)
    local file = sources.fromFile(util.checkHostPath(what, source, 2))
    addSource(normalizeStoredPath(stored), 'inherit', file)
  end

  local function addArchiveEntry(
    path: unknown,
    from: unknown,
    id: unknown,
    mode: unknown,
    what: string
  )
    local stored = util.checkString(what, path, 1)
    if mode ~= nil then util.checkString(what, mode, 4) end

    local backend, index = archive.entryArgument(what, from, id, 'ba2', 'BA2')
    local chosen = tes4Builder.override(mode)
    if index > backend.count then fail(Messages.outOfBounds) end

    local length, problem = backend.size(index)
    if not length then fail(problem :: string) end

    addSource(
      normalizeStoredPath(stored),
      chosen,
      sources.fromArchive(backend, index, length :: number)
    )
  end

  function builder.addArchiveEntry(
    _self: unknown,
    archivePath: string,
    from: unknown,
    id: number
  )
    addArchiveEntry(archivePath, from, id, nil, 'dream.archive.Ba2Builder.addArchiveEntry')
  end

  function builder.addArchiveEntryWithCompression(
    _self: unknown,
    archivePath: string,
    from: unknown,
    id: number,
    mode: string?
  )
    addArchiveEntry(
      archivePath,
      from,
      id,
      mode,
      'dream.archive.Ba2Builder.addArchiveEntryWithCompression'
    )
  end

  function builder.addDir(_self: unknown, root: string)
    for _, file in sources.collect(util.checkHostPath('dream.archive.Ba2Builder.addDir', root, 1)) do
      addSource(
        normalizeStoredPath(file.archivePath),
        'inherit',
        { kind = 'file', path = file.path, length = file.size }
      )
    end
  end

  function builder.writePath(_self: unknown, path: string)
    write(sources.fileSink(util.checkHostPath('dream.archive.Ba2Builder.writePath', path, 1)))
  end

  function builder.toBuffer(_self: unknown): buffer return write() :: buffer end

  function builder.toBytes(_self: unknown): string return buffer.tostring(write() :: buffer) end

  builder.toString = builder.toBytes

  return table.freeze(builder)
end

local TextureFields = { 'flags', 'format', 'height', 'mipCount', 'tileMode', 'width' }
local TextureLimits: { [string]: { number | string } } = {
  height = { 0xffff, 'u16' },
  width = { 0xffff, 'u16' },
  mipCount = { 0xff, 'u8' },
  format = { 0xff, 'u8' },
  flags = { 0xff, 'u8' },
  tileMode = { 0xff, 'u8' },
}

--- The `header` table `addTextureBytes` takes, read as the binding's options reader reads it.
local function readTextureHeader(value: unknown): TextureHeader
  local what = 'Dx10Builder:addTextureBytes'
  if type(value) ~= 'table' then fail(`{what}: options must be a table, got {typeof(value)}`) end

  local options = value :: { [unknown]: unknown }
  for key in options do
    if type(key) ~= 'string' then fail(`{what}: option keys must be strings, got {typeof(key)}`) end
  end

  local read: { [string]: number } = {}
  for _, field in { 'height', 'width', 'mipCount', 'format', 'flags', 'tileMode' } do
    local given = options[field]
    if given == nil then fail(`{what}: missing required option '{field}'`) end

    local limit = TextureLimits[field]
    local number: number
    if type(given) == 'number' then
      number = given
    elseif typeof(given) == 'integer' then
      number = integer.tonumber(given :: integer)
    else
      return fail(`{what}.{field}: expected integer, got {typeof(given)}`)
    end

    if number ~= math.floor(number) or number < 0 or number > (limit[1] :: number) then
      fail(`{what}.{field}: number {number} is not an exact {limit[2]}`)
    end

    read[field] = number
  end

  for key in options do
    if not TextureLimits[key :: string] then
      fail(
        `{what}: unknown option '{tostring(key)}'; known options are {table.concat(
          TextureFields,
          ', '
        )}`
      )
    end
  end

  return {
    height = read.height,
    width = read.width,
    mipCount = read.mipCount,
    format = read.format,
    flags = read.flags,
    tileMode = read.tileMode,
  }
end

local function validateTexture(texture: TextureHeader)
  if texture.mipCount == 0 then util.dds 'zero mip count' end

  dds.headerOf(texture)
end

type PreparedChunk = {
  stored: (buffer | string)?,
  chunk: Chunk?,
  packed: number,
  size: number,
  first: number,
  last: number,
}

local function newDx10()
  local version = 1
  local compression: string? = nil
  local level = 6
  local members: { Member } = {}
  local names: { [string]: boolean } = {}

  local function addSource(name: string, texture: TextureHeader, source: Source)
    if names[name] then fail(Messages.ba2Duplicate) end

    local directory, file, extension = ba2.hashNormalized(name)
    names[name] = true
    table.insert(members, {
      name = name,
      directory = directory,
      file = file,
      extension = extension,
      compression = 'inherit' :: 'inherit',
      source = source,
      texture = texture,
    })
  end

  local function addTexture(path: string, texture: TextureHeader, data: buffer | string)
    validateTexture(texture)
    local length = if type(data) == 'string' then #data else buffer.len(data :: buffer)
    dds.validatePayloadSize(texture, length)
    local name = normalizeStoredPath(path)
    if names[name] then fail(Messages.ba2Duplicate) end

    addSource(name, texture, sources.fromBytes(data))
  end

  local function addDds(path: string, data: buffer)
    local texture, payloadAt = dds.parse(data)
    local payload = buffer.create(buffer.len(data) - payloadAt)
    buffer.copy(payload, 0, data, payloadAt)
    addTexture(path, texture, payload)
  end

  --- Each member's chunks, compressed or copied, as the crate prepares them before writing.
  local function prepare(list: { Member }): { { PreparedChunk } }
    if compression == 'lz4' and version ~= 3 then
      for _, member in list do
        if isCompressed(member, compression) then
          util.notImplemented 'BA2 LZ4 writer requires version 3'
        end
      end
    end

    local prepared = table.create(#list) :: { { PreparedChunk } }
    for position, member in list do
      local source = member.source
      local texture = member.texture :: TextureHeader
      if source.kind == 'archive' then
        local backend = source.backend :: Backend
        local facts = backend.ba2 :: types.BA2Facts
        if facts.format ~= 'dx10' then
          util.notImplemented 'BA2 DX10 builder can only preserve DX10 archives'
        end

        if facts.version ~= version then
          util.notImplemented 'BA2 DX10 archive entry preservation requires matching archive versions'
        end

        if facts.compression ~= (compression or 'zip') then
          util.notImplemented 'BA2 DX10 archive entry preservation requires matching compression formats'
        end

        local chunks: { PreparedChunk } = {}
        for _, chunk in facts.chunkList(source.index :: number) do
          table.insert(chunks, {
            chunk = chunk,
            packed = chunk.packedSize,
            size = chunk.size,
            first = chunk.firstMip,
            last = chunk.lastMip,
          })
        end

        prepared[position] = chunks
        continue
      end

      local data = sources.load(source)
      local packed = 0
      local stored: buffer | string = data
      if isCompressed(member, compression) then
        local compressed = compress(data, compression, level)
        packed = buffer.len(compressed)
        stored = compressed
      end

      prepared[position] = {
        {
          stored = stored,
          packed = packed,
          size = sources.length(data),
          first = 0,
          last = texture.mipCount - 1,
        },
      }
    end

    return prepared
  end

  local function storedLength(chunk: PreparedChunk): number
    if chunk.stored then return sources.length(chunk.stored :: buffer | string) end

    local raw = chunk.chunk :: Chunk
    return if raw.packedSize == 0 then raw.size else raw.packedSize
  end

  local function write(sink: sources.Sink?): buffer?
    local list = sortMembers(members)
    local prepared = prepare(list)
    local recordsAt = headerSize(version)
    local payloadAt = recordsAt
    local tableAt = 0
    for _, chunks in prepared do
      payloadAt += 24 + 24 * #chunks
    end

    tableAt = payloadAt
    for _, chunks in prepared do
      for _, chunk in chunks do
        tableAt += storedLength(chunk)
      end
    end

    local names = stringTable(list)
    local head = buffer.create(payloadAt)
    writeHeader(head, version, 'DX10', #list, tableAt, compression == 'lz4')
    local cursor = recordsAt
    local offset = payloadAt
    for position, member in list do
      local texture = member.texture :: TextureHeader
      local chunks = prepared[position]
      writeu32(head, cursor, member.file)
      writeu32(head, cursor + 4, member.extension)
      writeu32(head, cursor + 8, member.directory)
      if #chunks > 255 then fail(Messages.integral) end

      writeu8(head, cursor + 13, #chunks)
      writeu16(head, cursor + 14, 0x18)
      writeu16(head, cursor + 16, texture.height)
      writeu16(head, cursor + 18, texture.width)
      writeu8(head, cursor + 20, texture.mipCount)
      writeu8(head, cursor + 21, texture.format)
      writeu8(head, cursor + 22, texture.flags)
      writeu8(head, cursor + 23, texture.tileMode)
      cursor += 24
      for _, chunk in chunks do
        writeu32(head, cursor, offset % 0x100000000)
        writeu32(head, cursor + 4, offset // 0x100000000)
        writeu32(head, cursor + 8, chunk.packed)
        if chunk.size > 0xffffffff then fail(Messages.integral) end

        writeu32(head, cursor + 12, chunk.size)
        writeu16(head, cursor + 16, chunk.first)
        writeu16(head, cursor + 18, chunk.last)
        writeu32(head, cursor + 20, Sentinel)
        offset += storedLength(chunk)
        cursor += 24
      end
    end

    local output = sink or sources.memorySink(tableAt + buffer.len(names))
    sources.add(output, head)
    for position, chunks in prepared do
      for _, chunk in chunks do
        local stored = chunk.stored
        if not stored then
          local raw = chunk.chunk :: Chunk
          local backend = list[position].source.backend :: Backend
          stored = Store.read(backend.store, raw.offset, storedLength(chunk))
        end

        sources.add(output, stored :: buffer | string)
      end
    end

    sources.add(output, names)
    return sources.finish(output)
  end

  local builder = {}

  function builder.len(_self: unknown): number return #members end

  function builder.isEmpty(_self: unknown): boolean return #members == 0 end

  function builder.setVersion(_self: unknown, value: number)
    version = parseVersion('dream.archive.Ba2Dx10Builder.setVersion', value)
  end

  function builder.setCompression(_self: unknown, value: string?)
    if value ~= nil then
      util.checkString('dream.archive.Ba2Dx10Builder.setCompression', value, 1)
    end

    compression = parseCompression(value)
  end

  function builder.setZlibLevel(_self: unknown, value: number)
    level = tes4Builder.zlibLevel('dream.archive.Ba2Dx10Builder.setZlibLevel', value)
  end

  function builder.addDdsBytes(_self: unknown, path: string, data: buffer | string)
    local what = 'dream.archive.Ba2Dx10Builder.addDdsBytes'
    local stored = util.checkString(what, path, 1)
    local payload = util.checkBytes(what, data, 2)
    addDds(
      stored,
      if type(payload) == 'string' then buffer.fromstring(payload) else payload :: buffer
    )
  end

  function builder.addTextureBytes(
    _self: unknown,
    path: string,
    header: TextureHeader,
    data: buffer | string
  )
    local what = 'dream.archive.Ba2Dx10Builder.addTextureBytes'
    local stored = util.checkString(what, path, 1)
    local payload = util.checkBytes(what, data, 3)
    addTexture(stored, readTextureHeader(header), payload)
  end

  function builder.addDdsFile(_self: unknown, archivePath: string, source: string)
    local what = 'dream.archive.Ba2Dx10Builder.addDdsFile'
    local stored = util.checkString(what, archivePath, 1)
    local data = util.readHostFile(util.checkHostPath(what, source, 2))
    addDds(stored, data)
  end

  function builder.addArchiveEntry(_self: unknown, archivePath: string, from: unknown, id: number)
    local what = 'dream.archive.Ba2Dx10Builder.addArchiveEntry'
    local stored = util.checkString(what, archivePath, 1)
    local backend, index = archive.entryArgument(what, from, id, 'ba2', 'BA2')
    local facts = backend.ba2 :: types.BA2Facts
    if facts.format ~= 'dx10' then
      util.notImplemented 'BA2 DX10 builder can only preserve DX10 archives'
    end

    if index > backend.count then fail(Messages.outOfBounds) end

    local texture = facts.texture(index)
    if not texture then util.notImplemented 'BA2 DX10 builder can only preserve DX10 entries' end

    addSource(
      normalizeStoredPath(stored),
      texture :: TextureHeader,
      { kind = 'archive', backend = backend, index = index, length = 0 }
    )
  end

  function builder.writePath(_self: unknown, path: string)
    write(sources.fileSink(util.checkHostPath('dream.archive.Ba2Dx10Builder.writePath', path, 1)))
  end

  function builder.toBuffer(_self: unknown): buffer return write() :: buffer end

  function builder.toBytes(_self: unknown): string return buffer.tostring(write() :: buffer) end

  builder.toString = builder.toBytes

  return table.freeze(builder)
end

return table.freeze { new = new, newDx10 = newDx10, normalizeStoredPath = normalizeStoredPath }
