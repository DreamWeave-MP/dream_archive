--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Where an archive's bytes come from. An archive opened from bytes keeps them in one buffer; one
-- opened from a path keeps a `@dream/fs` reader over the host's map of the file, reads its index
-- once, and reads each payload when it is asked for, straight into the buffer that wants it.
local types = require './types'
local util = require './util'
local fs = require '@dream/fs'

type Store = types.Store
type Reader = types.Reader

local fail = util.fail

local function fromBuffer(data: buffer): Store
  return { size = buffer.len(data), data = data, headLength = 0, position = 0 }
end

local function fromBytes(data: buffer | string): Store
  if type(data) == 'string' then return fromBuffer(buffer.fromstring(data)) end

  -- The caller keeps its buffer and may write it; the archive reads a copy, as the crate does.
  local source = data :: buffer
  local copy = buffer.create(buffer.len(source))
  buffer.copy(copy, 0, source, 0)
  return fromBuffer(copy)
end

local function fromPath(path: string): Store
  local result, message = fs.open(path)
  local reader = util.hostResult(result, message, path)
  return { size = reader:size(), reader = reader, path = path, headLength = 0, position = 0 }
end

--- Moves the reader to `offset` unless a read already left it there, as reads in archive order do.
local function seek(store: Store, reader: Reader, offset: number)
  if store.position ~= offset then reader:seek(offset) end

  -- Unknown until the read that follows completes; a read that throws leaves it so.
  store.position = -1
end

--- Copies `length` bytes at `offset` into `target` at `targetOffset`; the range is in bounds.
local function copyInto(
  store: Store,
  target: buffer,
  targetOffset: number,
  offset: number,
  length: number
)
  local data = store.data
  if data then
    buffer.copy(target, targetOffset, data, offset, length)
    return
  end

  local reader = store.reader :: Reader
  seek(store, reader, offset)
  local count, message = reader:readInto(target, targetOffset, length)
  if util.hostResult(count, message, store.path :: string) ~= length then
    fail(util.Messages.endOfFile)
  end

  store.position = offset + length
end

--- A new buffer of `length` bytes at `offset`; the range is in bounds.
local function read(store: Store, offset: number, length: number): buffer
  local data = store.data
  if data then
    local out = buffer.create(length)
    buffer.copy(out, 0, data, offset, length)
    return out
  end

  local reader = store.reader :: Reader
  seek(store, reader, offset)
  local data, message = reader:read(length)
  local out = util.hostResult(data, message, store.path :: string)
  if buffer.len(out) ~= length then fail(util.Messages.endOfFile) end

  store.position = offset + length
  return out
end

--- Where string reads of a file land before they become strings: one buffer reused, so a read
--- makes one string and no buffer. Larger reads get a buffer of their own.
local scratch = buffer.create(65536)
local ScratchLimit = 1048576

--- The `length` bytes at `offset` as a string; the range is in bounds.
local function readString(store: Store, offset: number, length: number): string
  local data = store.data
  if data then return buffer.readstring(data, offset, length) end

  if length > ScratchLimit then return buffer.tostring(read(store, offset, length)) end

  if length > buffer.len(scratch) then
    scratch = buffer.create(math.max(length, 2 * buffer.len(scratch)))
  end

  copyInto(store, scratch, 0, offset, length)
  return buffer.readstring(scratch, 0, length)
end

--- The little-endian u32 at `offset`; the four bytes are in bounds.
local function u32(store: Store, offset: number): number
  local data = store.data
  if data then return buffer.readu32(data, offset) end

  return buffer.readu32(read(store, offset, 4), 0)
end

--- A buffer holding the archive's first `length` bytes, at offset 0: the archive itself when it
--- is in memory, a read of the file otherwise, kept for the next call that needs no more.
local function head(store: Store, length: number): buffer
  local data = store.data
  if data then return data end

  local cached = store.head
  if cached and store.headLength >= length then return cached end

  local bytes = read(store, 0, length)
  store.head, store.headLength = bytes, length
  return bytes
end

--- `length` bytes at `offset` and where they start in the buffer returned: the archive's own
--- buffer at `offset` when in memory, a fresh read at 0 otherwise.
local function region(store: Store, offset: number, length: number): (buffer, number)
  local data = store.data
  if data then return data, offset end

  return read(store, offset, length), 0
end

return table.freeze {
  fromBytes = fromBytes,
  fromBuffer = fromBuffer,
  fromPath = fromPath,
  copyInto = copyInto,
  read = read,
  readString = readString,
  u32 = u32,
  head = head,
  region = region,
}
