--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Builds TES4-family PC BSAs (versions 103, 104, 105): folders and files ordered by hash, names
-- in string tables, embedded, both, or neither, and each member stored or compressed by the
-- archive's default or its own override.
local archive = require './archive'
local bytes = require '@dream/bytes'
local encoding = require './encoding'
local sources = require './sources'
local tes4 = require './tes4'
local types = require './types'
local util = require './util'

type Source = types.Source

local fail, Messages = util.fail, util.Messages
local writeu32, writeu8 = buffer.writeu32, buffer.writeu8

type Override = 'inherit' | 'store' | 'compress'
type NameMode = 'strings' | 'hashOnly' | 'embedded' | 'stringsAndEmbedded'

type Member = {
  folder: string,
  name: string,
  folderHash: tes4.Hash,
  fileHash: tes4.Hash,
  folderValue: integer,
  fileValue: integer,
  compression: Override,
  source: Source,
}

local Profiles: { [string]: number } = {
  oblivion = 103,
  fallout3 = 104,
  falloutNewVegas = 104,
  skyrimLe = 104,
  skyrimSe = 105,
}

local NameModes: { [string]: NameMode } = {
  strings = 'strings',
  hashOnly = 'hashOnly',
  embedded = 'embedded',
  stringsAndEmbedded = 'stringsAndEmbedded',
}

local function override(value: unknown): Override
  if value == nil or value == 'inherit' then return 'inherit' end

  if value == 'store' or value == 'compress' then return value :: Override end

  return fail(`unknown compression override: {value}`)
end

local function zlibLevel(what: string, value: unknown): number
  local level = util.checkInteger(what, value, 1, 'u32', 0, 0xffffffff)
  if level > 9 then fail(`zlib compression level must be 0..=9, got {level}`) end

  return level
end

--- A stored path as its folder and name: the stored path's last component is the name, and the
--- rest, joined with `\`, the folder.
local function normalizeStoredPath(path: string): (string, string)
  local stored = util.storedPath(path) or fail(Messages.bsaInvalidPath)
  local at = string.find(stored, '\\[^\\]*$')
  if not at then return '', stored end

  return string.sub(stored, 1, at - 1), string.sub(stored, at + 1)
end

--- A hash's 64-bit value as two numbers that order the same way: the CRC, then the low half.
local function order(hash: tes4.Hash): (number, number)
  return hash.crc, ((hash.first * 256 + hash.length) * 256 + hash.last2) * 256 + hash.last
end

--- A number that orders hashes as their 64-bit values do, except within one of 2^11 of each
--- other: the CRC and the top 21 bits of the low half.
local function orderKey(hash: tes4.Hash): number
  local high, low = order(hash)
  return high * 0x200000 + low // 0x800
end

local function before(left: tes4.Hash, right: tes4.Hash): boolean?
  local leftHigh, leftLow = order(left)
  local rightHigh, rightLow = order(right)
  if leftHigh ~= rightHigh then return leftHigh < rightHigh end

  if leftLow ~= rightLow then return leftLow < rightLow end

  return nil
end

local function new()
  local version = 104
  local archiveTypes = 0x100
  local compressed = false
  local nameMode: NameMode = 'strings'
  local level = 6
  local members: { Member } = {}
  local paths: { [string]: boolean } = {}

  local function addSource(folder: string, name: string, compression: Override, source: Source)
    local key = `{folder}\0{name}`
    if paths[key] then fail(Messages.bsaDuplicate) end

    local folderHash, fileHash = tes4.hashDirectory(folder), tes4.hashFile(name)
    paths[key] = true
    table.insert(members, {
      folder = folder,
      name = name,
      folderHash = folderHash,
      fileHash = fileHash,
      folderValue = tes4.numeric(folderHash),
      fileValue = tes4.numeric(fileHash),
      compression = compression,
      source = source,
    })
  end

  local function isCompressed(member: Member): boolean
    if member.compression == 'inherit' then return compressed end

    return member.compression == 'compress'
  end

  local function embeddedName(member: Member): string
    if member.folder == '' then return member.name end

    return `{member.folder}\\{member.name}`
  end

  --- A member's stored body, and the decoded length that goes before it when it's compressed.
  local function stored(member: Member): (buffer | string, number?)
    local data = sources.load(member.source)
    if not isCompressed(member) then return data, nil end

    local length = sources.length(data)
    if length > 0xffffffff then fail(Messages.integral) end

    local body = if version == 105
      then bytes.lz4FrameCompress(data)
      else bytes.deflate(data, { format = 'zlib', level = level })
    return body, length
  end

  local function write(sink: sources.Sink?): buffer?
    local strings = nameMode == 'strings' or nameMode == 'stringsAndEmbedded'
    local embedded = nameMode == 'embedded' or nameMode == 'stringsAndEmbedded'
    if embedded and version == 103 then
      util.notImplemented 'TES4 embedded file names require version 104 or 105'
    end

    -- The crate orders by folder hash, folder, file hash, and name: folders as a whole, then the
    -- files in each.
    local byFolder: { [string]: { Member } } = {}
    local folderNames: { string } = {}
    for _, member in members do
      local group = byFolder[member.folder]
      if group then
        table.insert(group, member)
      else
        byFolder[member.folder] = { member }
        table.insert(folderNames, member.folder)
      end
    end

    local function folderKey(folder: string): number return orderKey(byFolder[folder][1].folderHash) end

    local function folderBefore(left: string, right: string): boolean
      local folder = before(byFolder[left][1].folderHash, byFolder[right][1].folderHash)
      if folder ~= nil then return folder end

      return left < right
    end

    local function fileKey(member: Member): number return orderKey(member.fileHash) end

    local function fileBefore(left: Member, right: Member): boolean
      local file = before(left.fileHash, right.fileHash)
      if file ~= nil then return file end

      return left.name < right.name
    end

    local list: { Member } = table.create(#members) :: { Member }
    for _, folder in util.sortByKey(folderNames, folderKey, folderBefore) do
      for _, member in util.sortByKey(byFolder[folder], fileKey, fileBefore) do
        table.insert(list, member)
      end
    end

    local payloads: { buffer | string } = table.create(#list) :: { buffer | string }
    local prefixes: { number | false } = table.create(#list) :: { number | false }
    for position, member in list do
      local body, length = stored(member)
      payloads[position], prefixes[position] = body, length or false
    end

    -- Folders are runs of members with the same folder.
    local folderStarts: { number } = {}
    for position, member in list do
      if position == 1 or list[position - 1].folder ~= member.folder then
        table.insert(folderStarts, position)
      end
    end

    local folderCount, fileCount = #folderStarts, #list
    local folderNamesLength, fileNamesLength = 0, 0
    if strings then
      for _, start in folderStarts do
        folderNamesLength += #list[start].folder + 1
      end

      for _, member in list do
        fileNamesLength += #member.name + 1
      end
    end

    local recordSize = if version == 105 then 24 else 16
    local dataOffset = 0x24
      + recordSize * folderCount
      + (if strings then folderCount + folderNamesLength else 0)
      + 16 * fileCount
      + fileNamesLength
    local total = dataOffset
    for position, member in list do
      total += sources.length(payloads[position]) + (if prefixes[position] then 4 else 0)
      if embedded then
        total += 1 + #embeddedName(member)
      end
    end

    local out = buffer.create(total)
    writeu32(out, 0, 0x00415342)
    writeu32(out, 4, version)
    writeu32(out, 8, 0x24)
    local flags = 0
    if strings then
      flags += 0x3
    end

    if embedded then
      flags += tes4.Flags.embeddedNames
    end

    if compressed then
      flags += tes4.Flags.compressed
    end

    writeu32(out, 12, flags)
    writeu32(out, 16, folderCount)
    writeu32(out, 20, fileCount)
    writeu32(out, 24, folderNamesLength)
    writeu32(out, 28, fileNamesLength)
    buffer.writeu16(out, 32, archiveTypes)

    local blockOffset = 0x24 + recordSize * folderCount
    for folder, start in folderStarts do
      local finish = (folderStarts[folder + 1] or fileCount + 1) - 1
      local record = 0x24 + recordSize * (folder - 1)
      buffer.writeinteger(out, record, list[start].folderValue)
      writeu32(out, record + 8, finish - start + 1)
      if version == 105 then
        writeu32(out, record + 16, blockOffset % 0x100000000)
        writeu32(out, record + 20, blockOffset // 0x100000000)
      else
        if blockOffset > 0xffffffff then fail(Messages.integral) end

        writeu32(out, record + 12, blockOffset)
      end

      blockOffset += (if strings then #list[start].folder + 2 else 0) + 16 * (finish - start + 1)
    end

    if dataOffset > 0xffffffff then fail(Messages.integral) end

    local cursor = 0x24 + recordSize * folderCount
    local payloadOffset = dataOffset
    for folder, start in folderStarts do
      local finish = (folderStarts[folder + 1] or fileCount + 1) - 1
      if strings then
        local name = list[start].folder
        if #name + 1 > 255 then fail(Messages.integral) end

        writeu8(out, cursor, #name + 1)
        buffer.writestring(out, cursor + 1, name)
        cursor += #name + 2
      end

      for position = start, finish do
        local member = list[position]
        buffer.writeinteger(out, cursor, member.fileValue)
        local length = sources.length(payloads[position])
          + (if prefixes[position] then 4 else 0)
          + (if embedded then #embeddedName(member) + 1 else 0)
        if length >= 0x40000000 then fail(Messages.outOfBounds) end

        local size = length
        if isCompressed(member) ~= compressed then
          size += 0x40000000
        end

        writeu32(out, cursor + 8, size)
        writeu32(out, cursor + 12, payloadOffset)
        payloadOffset += length
        if payloadOffset > 0xffffffff then fail(Messages.outOfBounds) end

        cursor += 16
      end
    end

    if strings then
      for _, member in list do
        buffer.writestring(out, cursor, member.name)
        cursor += #member.name + 1
      end
    end

    for position, member in list do
      if embedded then
        local name = embeddedName(member)
        if #name > 255 then fail(Messages.integral) end

        writeu8(out, cursor, #name)
        buffer.writestring(out, cursor + 1, name)
        cursor += 1 + #name
      end

      local prefix = prefixes[position]
      if prefix then
        writeu32(out, cursor, prefix)
        cursor += 4
      end

      local body = payloads[position]
      if type(body) == 'string' then
        buffer.writestring(out, cursor, body)
      else
        buffer.copy(out, cursor, body :: buffer)
      end

      cursor += sources.length(body)
    end

    if not sink then return out end

    sources.add(sink, out)
    return sources.finish(sink)
  end

  local builder = {}

  function builder.len(_self: unknown): number return #members end

  function builder.isEmpty(_self: unknown): boolean return #members == 0 end

  function builder.setVersion(_self: unknown, value: number)
    local chosen =
      util.checkInteger('dream.archive.Tes4Builder.setVersion', value, 1, 'u32', 0, 0xffffffff)
    if chosen ~= 103 and chosen ~= 104 and chosen ~= 105 then
      fail(`unsupported TES4 BSA version: {chosen}`)
    end

    version = chosen
  end

  function builder.setProfile(_self: unknown, profile: string)
    local name = util.checkString('dream.archive.Tes4Builder.setProfile', profile, 1)
    local chosen = Profiles[name]
    if not chosen then fail(`unknown TES4 profile: {name}`) end

    version = chosen
  end

  function builder.setArchiveTypes(_self: unknown, bits: number)
    archiveTypes =
      util.checkInteger('dream.archive.Tes4Builder.setArchiveTypes', bits, 1, 'u16', 0, 0xffff)
  end

  function builder.setCompressed(_self: unknown, value: boolean)
    if type(value) ~= 'boolean' then
      util.argument(
        'dream.archive.Tes4Builder.setCompressed',
        1,
        'boolean',
        `expected boolean, got {typeof(value)}`
      )
    end

    compressed = value
  end

  function builder.setNameMode(_self: unknown, mode: string)
    local name = util.checkString('dream.archive.Tes4Builder.setNameMode', mode, 1)
    local chosen = NameModes[name]
    if not chosen then fail(`unknown TES4 name mode: {name}`) end

    nameMode = chosen
  end

  function builder.setZlibLevel(_self: unknown, value: number)
    level = zlibLevel('dream.archive.Tes4Builder.setZlibLevel', value)
  end

  local function addBytes(path: unknown, data: unknown, compression: Override, what: string)
    local folder, name = normalizeStoredPath(util.checkString(what, path, 1))
    if paths[`{folder}\0{name}`] then fail(Messages.bsaDuplicate) end

    addSource(folder, name, compression, sources.fromBytes(util.checkBytes(what, data, 2)))
  end

  function builder.addBytes(_self: unknown, path: string, data: buffer | string)
    addBytes(path, data, 'inherit', 'dream.archive.Tes4Builder.addBytes')
  end

  function builder.addBytesWithCompression(
    _self: unknown,
    path: string,
    data: buffer | string,
    compression: string?
  )
    local what = 'dream.archive.Tes4Builder.addBytesWithCompression'
    util.checkString(what, path, 1)
    util.checkBytes(what, data, 2)
    addBytes(path, data, override(compression), what)
  end

  function builder.addEncodedPath(_self: unknown, path: string, name: string, data: buffer | string)
    local what = 'dream.archive.Tes4Builder.addEncodedPath'
    local text = util.checkHostPath(what, path, 1)
    local chosen = encoding.parse(util.checkString(what, name, 2))
    util.checkBytes(what, data, 3)
    addBytes(encoding.encode(text, chosen), data, 'inherit', what)
  end

  function builder.addFile(_self: unknown, archivePath: string, source: string)
    local what = 'dream.archive.Tes4Builder.addFile'
    local stored = util.checkString(what, archivePath, 1)
    local file = sources.fromFile(util.checkHostPath(what, source, 2))
    local folder, name = normalizeStoredPath(stored)
    addSource(folder, name, 'inherit', file)
  end

  local function addArchiveEntry(
    path: unknown,
    from: unknown,
    id: unknown,
    compression: unknown,
    what: string
  )
    local stored = util.checkString(what, path, 1)
    if compression ~= nil then util.checkString(what, compression, 4) end

    local chosen, index = archive.entryArgument(what, from, id, 'bsaTes4', 'TES4 BSA')
    local mode = override(compression)
    if index > chosen.count then fail(Messages.outOfBounds) end

    local length, problem = chosen.size(index)
    if not length then fail(problem :: string) end

    local folder, name = normalizeStoredPath(stored)
    addSource(folder, name, mode, sources.fromArchive(chosen, index, length :: number))
  end

  function builder.addArchiveEntry(
    _self: unknown,
    archivePath: string,
    from: unknown,
    id: number
  )
    addArchiveEntry(archivePath, from, id, nil, 'dream.archive.Tes4Builder.addArchiveEntry')
  end

  function builder.addArchiveEntryWithCompression(
    _self: unknown,
    archivePath: string,
    from: unknown,
    id: number,
    compression: string?
  )
    addArchiveEntry(
      archivePath,
      from,
      id,
      compression,
      'dream.archive.Tes4Builder.addArchiveEntryWithCompression'
    )
  end

  function builder.addDir(_self: unknown, root: string)
    for _, file in sources.collect(util.checkHostPath('dream.archive.Tes4Builder.addDir', root, 1)) do
      local folder, name = normalizeStoredPath(file.archivePath)
      addSource(folder, name, 'inherit', { kind = 'file', path = file.path, length = file.size })
    end
  end

  function builder.writePath(_self: unknown, path: string)
    write(sources.fileSink(util.checkHostPath('dream.archive.Tes4Builder.writePath', path, 1)))
  end

  function builder.toBuffer(_self: unknown): buffer return write() :: buffer end

  function builder.toBytes(_self: unknown): string return buffer.tostring(write() :: buffer) end

  builder.toString = builder.toBytes

  return table.freeze(builder)
end

return table.freeze { new = new, override = override, zlibLevel = zlibLevel }
