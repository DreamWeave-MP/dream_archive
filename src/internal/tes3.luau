--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- TES3 (Morrowind) BSA: uncompressed payloads, a name for every entry, and a 64-bit hash of each
-- name. The index is checked when the archive opens, exactly as the crate checks it; names,
-- lookups, and hashes are read from it when first asked for.
local Store = require './store'
local bytes = require '@dream/bytes'
local names = require './names'
local types = require './types'
local util = require './util'

type Backend = types.Backend
type Store = types.Store

local fail, Messages = util.fail, util.Messages
local readu32, readu8 = buffer.readu32, buffer.readu8

local Version = 0x100

--- The two u32 halves of the hash of a normalized path, as stored: low word first.
local function hashNormalized(path: string): (number, number)
  local length = #path
  local midpoint = length // 2
  local low = 0
  for index = 1, midpoint do
    low = bit32.bxor(low, bit32.lshift(string.byte(path, index), ((index - 1) % 4) * 8))
  end

  local high = 0
  for index = midpoint + 1, length do
    local rotation = bit32.lshift(string.byte(path, index), ((index - midpoint - 1) % 4) * 8)
    high = bit32.rrotate(bit32.bxor(high, rotation), bit32.band(rotation, 31))
  end

  return low, high
end

--- The hash's 64-bit value: the stored low word is its high half, so hashes order as they sort.
local function numeric(low: number, high: number): integer return util.int64(high, low) end

local function hashFile(path: string)
  local normalized = util.normalizeHashPath(path)
  local low, high = hashNormalized(normalized)
  local hash = numeric(low, high)
  return { lo = low, hi = high, hex = util.hex64(hash), hash = hash, normalized = normalized }
end

local function open(store: Store): Backend
  local size = store.size
  if size < 4 then fail(Messages.endOfFile) end

  local header = Store.head(store, math.min(size, 12))
  local version = readu32(header, 0)
  if version ~= Version then util.invalidVersion(version) end

  if size < 12 then fail(Messages.endOfFile) end

  local hashOffset, count = readu32(header, 4), readu32(header, 8)
  local offsetsStart = 12 + 8 * count
  local namesStart = offsetsStart + 4 * count
  local hashesStart = 12 + hashOffset
  local dataStart = hashesStart + 8 * count
  if namesStart > hashesStart or dataStart > size then fail(Messages.outOfBounds) end

  local index = Store.head(store, dataStart)
  local namesLength = hashesStart - namesStart
  -- A name is read up to its NUL, so one that starts after the blob's last NUL runs off the end.
  local lastNul = -1
  for position = hashesStart - 1, namesStart, -1 do
    if readu8(index, position) == 0 then
      lastNul = position - namesStart
      break
    end
  end

  for entry = 0, count - 1 do
    local relative = readu32(index, offsetsStart + 4 * entry)
    if relative > namesLength then fail(Messages.outOfBounds) end

    if relative > lastNul then fail(Messages.endOfFile) end
  end

  for entry = 0, count - 1 do
    local record = 12 + 8 * entry
    if dataStart + readu32(index, record + 4) + readu32(index, record) > size then
      fail(Messages.outOfBounds)
    end
  end

  local function nameStart(entry: number): number
    return namesStart + readu32(index, offsetsStart + 4 * (entry - 1))
  end

  --- Each name's length, found once by searching for its NUL.
  local lengths: { number } = {}

  local function nameLength(entry: number, start: number): number
    local known = lengths[entry]
    if known then return known end

    local length = (bytes.find(index, '\0', start) :: number) - start
    lengths[entry] = length
    return length
  end

  local function path(entry: number): string
    local start = nameStart(entry)
    return buffer.readstring(index, start, nameLength(entry, start))
  end

  local function payload(entry: number): (number, number)
    local record = 12 + 8 * (entry - 1)
    return dataStart + readu32(index, record + 4), readu32(index, record)
  end

  local function buildNames(): names.Index
    local built = names.new()
    local plain = names.regionCanonical(index, namesStart, namesLength)
    for entry = 1, count do
      local start = nameStart(entry)
      built.add(index, start, nameLength(entry, start), entry, plain and readu8(index, start) ~= 92)
    end

    return built
  end

  local nameIndex: names.Index? = nil

  local function lookup(raw: string): number?
    local built = nameIndex or buildNames()
    nameIndex = built
    return built.find(raw)
  end

  local hashes: { [integer]: number }? = nil

  local function hashLookup(first: unknown, second: unknown, third: unknown): number?
    if second ~= nil or third ~= nil then
      fail 'a TES3 hash lookup takes one integer: the entry hash'
    end

    if typeof(first) ~= 'integer' then
      util.stackValue(2, `expected integer, got {typeof(first)}`)
    end

    local map = hashes
    if not map then
      local built: { [integer]: number } = {}
      for entry = count - 1, 0, -1 do
        local at = hashesStart + 8 * entry
        built[numeric(readu32(index, at), readu32(index, at + 4))] = entry + 1
      end

      hashes, map = built, built
    end

    return (map :: { [integer]: number })[first :: integer]
  end

  return {
    format = 'bsaTes3',
    count = count,
    store = store,
    lookup = lookup,
    hashLookup = hashLookup,
    path = path,
    name = function(entry: number): string? return util.lastComponent(path(entry)) end,
    folder = function(entry: number): string? return util.parentFolder(path(entry)) end,
    size = function(entry: number): (number?, string?)
      return readu32(index, 12 + 8 * (entry - 1)), nil
    end,
    storedSize = function(entry: number): number return readu32(index, 12 + 8 * (entry - 1)) end,
    offset = function(entry: number): number? return readu32(index, 16 + 8 * (entry - 1)) end,
    compressed = function(): boolean return false end,
    chunks = function(): number? return nil end,
    hash = function(entry: number): integer?
      local at = hashesStart + 8 * (entry - 1)
      return numeric(readu32(index, at), readu32(index, at + 4))
    end,
    folderHash = function(): integer? return nil end,
    ba2Hash = function(): { directory: number, file: number, extension: number }? return nil end,
    decode = function(entry: number): buffer
      local start, length = payload(entry)
      return Store.read(store, start, length)
    end,

    decodeString = function(entry: number): string
      local start, length = payload(entry)
      return Store.readString(store, start, length)
    end,

    decodeInto = function(entry: number, target: buffer, at: number): number
      local start, length = payload(entry)
      Store.copyInto(store, target, at, start, length)
      return length
    end,

    row = function(entry: number): { [string]: unknown }
      local record = 12 + 8 * (entry - 1)
      local size = readu32(index, record)
      local at = hashesStart + 8 * (entry - 1)
      return {
        index = entry,
        id = entry,
        format = 'bsaTes3',
        path = path(entry),
        size = size,
        storedSize = size,
        offset = readu32(index, record + 4),
        compressed = false,
        hash = numeric(readu32(index, at), readu32(index, at + 4)),
      }
    end,
  }
end

return table.freeze {
  open = open,
  hashFile = hashFile,
  hashNormalized = hashNormalized,
  numeric = numeric,
}
