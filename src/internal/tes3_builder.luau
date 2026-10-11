--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Builds TES3 (Morrowind) BSAs: every member stored, ordered by hash and then path, so the same
-- members make the same bytes.
local archive = require './archive'
local encoding = require './encoding'
local sources = require './sources'
local tes3 = require './tes3'
local types = require './types'
local util = require './util'

type Source = types.Source

local fail, Messages = util.fail, util.Messages
local writeu32 = buffer.writeu32

type Member = { path: string, low: number, high: number, source: Source }

--- A stored path, or the crate's error for one that can't be stored.
local function normalizeStoredPath(path: string): string
  return util.storedPath(path) or fail(Messages.bsaInvalidPath)
end

local function new()
  local members: { Member } = {}
  local paths: { [string]: boolean } = {}

  local function addSource(path: string, source: Source)
    local stored = normalizeStoredPath(path)
    if paths[stored] then fail(Messages.bsaDuplicate) end

    local low, high = tes3.hashNormalized(stored)
    paths[stored] = true
    table.insert(members, { path = stored, low = low, high = high, source = source })
  end

  --- The members by hash, then path: the stored low word is the hash's high half.
  local function sorted(): { Member }
    local function key(member: Member): number return member.low * 0x200000 + member.high // 0x800 end

    local function before(left: Member, right: Member): boolean
      if left.high ~= right.high then return left.high < right.high end

      return left.path < right.path
    end

    return util.sortByKey(members, key, before)
  end

  --- The header, records, name offsets, names, and hashes, from the members' declared sizes.
  local function index(list: { Member }): buffer
    local count = #list
    local namesLength = 0
    for _, member in list do
      namesLength += #member.path + 1
    end

    local hashOffset = 12 * count + namesLength
    if hashOffset > 0xffffffff then fail(Messages.integral) end

    local head = buffer.create(12 + hashOffset + 8 * count)
    writeu32(head, 0, 0x100)
    writeu32(head, 4, hashOffset)
    writeu32(head, 8, count)
    local dataOffset = 0
    for position, member in list do
      local length = member.source.length
      if length > 0xffffffff then fail(Messages.integral) end

      local record = 12 + 8 * (position - 1)
      writeu32(head, record, length)
      writeu32(head, record + 4, dataOffset)
      dataOffset += length
      if dataOffset > 0xffffffff then fail(Messages.outOfBounds) end
    end

    local nameOffset = 0
    local names = 12 + 12 * count
    for position, member in list do
      writeu32(head, 12 + 8 * count + 4 * (position - 1), nameOffset)
      buffer.writestring(head, names + nameOffset, member.path)
      nameOffset += #member.path + 1
    end

    local hashes = names + namesLength
    for position, member in list do
      writeu32(head, hashes + 8 * (position - 1), member.low)
      writeu32(head, hashes + 8 * (position - 1) + 4, member.high)
    end

    return head
  end

  local function write(sink: sources.Sink?): buffer?
    local list = sorted()
    local head = index(list)
    local target = sink
    if not target then
      local total = buffer.len(head)
      for _, member in list do
        total += member.source.length
      end

      target = sources.memorySink(total)
    end

    sources.add(target :: sources.Sink, head)
    for _, member in list do
      sources.add(target :: sources.Sink, sources.load(member.source))
    end

    return sources.finish(target :: sources.Sink)
  end

  local builder = {}

  function builder.len(_self: unknown): number return #members end

  function builder.isEmpty(_self: unknown): boolean return #members == 0 end

  function builder.addBytes(_self: unknown, path: string, data: buffer | string)
    local stored = util.checkString('dream.archive.Tes3Builder.addBytes', path, 1)
    local payload = util.checkBytes('dream.archive.Tes3Builder.addBytes', data, 2)
    addSource(stored, sources.fromBytes(payload))
  end

  function builder.addEncodedPath(_self: unknown, path: string, name: string, data: buffer | string)
    local what = 'dream.archive.Tes3Builder.addEncodedPath'
    local text = util.checkHostPath(what, path, 1)
    local chosen = encoding.parse(util.checkString(what, name, 2))
    local payload = util.checkBytes(what, data, 3)
    addSource(encoding.encode(text, chosen), sources.fromBytes(payload))
  end

  function builder.addFile(_self: unknown, archivePath: string, source: string)
    local what = 'dream.archive.Tes3Builder.addFile'
    local stored = util.checkString(what, archivePath, 1)
    local file = sources.fromFile(util.checkHostPath(what, source, 2))
    addSource(stored, file)
  end

  function builder.addArchiveEntry(_self: unknown, archivePath: string, from: unknown, id: number)
    local what = 'dream.archive.Tes3Builder.addArchiveEntry'
    local stored = util.checkString(what, archivePath, 1)
    local chosen, index = archive.entryArgument(what, from, id, 'bsaTes3', 'TES3 BSA')
    if index > chosen.count then fail(Messages.outOfBounds) end

    local length = chosen.size(index) :: number
    addSource(stored, sources.fromArchive(chosen, index, length))
  end

  function builder.addDir(_self: unknown, root: string)
    for _, file in sources.collect(util.checkHostPath('dream.archive.Tes3Builder.addDir', root, 1)) do
      addSource(file.archivePath, { kind = 'file', path = file.path, length = file.size })
    end
  end

  function builder.writePath(_self: unknown, path: string)
    write(sources.fileSink(util.checkHostPath('dream.archive.Tes3Builder.writePath', path, 1)))
  end

  function builder.toBuffer(_self: unknown): buffer return write() :: buffer end

  function builder.toBytes(_self: unknown): string return buffer.tostring(write() :: buffer) end

  builder.toString = builder.toBytes

  return table.freeze(builder)
end

return table.freeze { new = new, normalizeStoredPath = normalizeStoredPath }
