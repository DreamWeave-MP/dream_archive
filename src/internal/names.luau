--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- The path index every family looks names up in. `dream_path` matches a path with `\` and `/` as
-- one separator, ASCII case ignored, and leading and repeated separators dropped; here a path is
-- rewritten into a scratch buffer with `\` for every run of separators and none in front, and
-- its bytes are interned, case-insensitively, by `@dream/intern`. Building the index makes no
-- strings, and neither does a lookup: the archive's own name bytes are interned where they lie,
-- and a query is rewritten into the scratch buffer and found there.
local bytes = require '@dream/bytes'
local intern = require '@dream/intern'

local Scratch = buffer.create(1024)

--- Writes the canonical form of `length` bytes of `source` at `offset` into the scratch buffer,
--- growing it as needed; returns the scratch buffer and the canonical length.
local function canonical(source: buffer, offset: number, length: number): (buffer, number)
  if length > buffer.len(Scratch) then
    Scratch = buffer.create(math.max(length, 2 * buffer.len(Scratch)))
  end

  local out = Scratch
  local written = 0
  local separator = true
  for position = offset, offset + length - 1 do
    local byte = buffer.readu8(source, position)
    if byte == 47 or byte == 92 then
      if separator then continue end

      separator = true
      buffer.writeu8(out, written, 92)
    else
      separator = false
      buffer.writeu8(out, written, byte)
    end

    written += 1
  end

  return out, written
end

--- Whether `length` bytes of `source` at `offset` are already canonical: no `/`, and no `\` at
--- the start or after another.
local function isCanonical(source: buffer, offset: number, length: number): boolean
  local separator = true
  for position = offset, offset + length - 1 do
    local byte = buffer.readu8(source, position)
    if byte == 47 then return false end

    if byte == 92 then
      if separator then return false end

      separator = true
    else
      separator = false
    end
  end

  return true
end

export type Index = {
  add: (source: buffer, offset: number, length: number, entry: number, canonical: boolean?) -> (),
  find: (path: string) -> number?,
  isEmpty: () -> boolean,
}

local QueryBuffer = buffer.create(1024)

--- An empty index; `add` names entries, the first entry a name is added for keeping it. A caller
--- that knows a name is canonical already says so, and the bytes aren't looked at here.
local function new(): Index
  local pool = intern.new 'ascii-nocase'
  local entries: { number } = {}
  local count = 0

  local function add(source: buffer, offset: number, length: number, entry: number, known: boolean?)
    local token
    if known or isCanonical(source, offset, length) then
      token = pool:intern(source, offset, length)
    else
      local out, written = canonical(source, offset, length)
      token = pool:intern(out, 0, written)
    end

    if not entries[token] then
      entries[token] = entry
      count += 1
    end
  end

  local function find(path: string): number?
    local token
    if
      string.byte(path, 1) ~= 92
      and not string.find(path, '/', 1, true)
      and not string.find(path, '\\\\', 1, true)
    then
      token = pool:find(path)
    else
      local length = #path
      if length > buffer.len(QueryBuffer) then
        QueryBuffer = buffer.create(math.max(length, 2 * buffer.len(QueryBuffer)))
      end

      buffer.writestring(QueryBuffer, 0, path)
      local out, written = canonical(QueryBuffer, 0, length)
      token = pool:find(out, 0, written)
    end

    return if token then entries[token] else nil
  end

  return { add = add, find = find, isEmpty = function(): boolean return count == 0 end }
end

--- Whether every name in `length` bytes of `source` at `offset` is canonical once its own leading
--- byte is: the region has no `/` and no two `\` in a row. One search each, in Rust.
local function regionCanonical(source: buffer, offset: number, length: number): boolean
  local slash = bytes.find(source, '/', offset)
  if slash and slash < offset + length then return false end

  local double = bytes.find(source, '\\\\', offset)
  return not double or double >= offset + length
end

return table.freeze { new = new, canonical = canonical, regionCanonical = regionCanonical }
