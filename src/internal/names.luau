--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- The path index every family looks names up in. `dream_path` matches a path with `\` and `/` as
-- one separator, ASCII case ignored, and leading and repeated separators dropped; those are the
-- rules of the `@dream/intern` pool the names are kept in, which applies them as it hashes and
-- compares. Building the index makes no strings, and neither does a lookup: the archive's own
-- name bytes are interned where they lie, and a query is found as it was asked.
local intern = require '@dream/intern'

local Rules: dream_intern_Rules = {
  nocase = true,
  replace = { ['/'] = '\\' },
  collapse = '\\',
  trimStart = '\\',
}

export type Index = {
  add: (source: buffer, offset: number, length: number, entry: number) -> (),
  find: (path: string) -> number?,
  isEmpty: () -> boolean,
}

--- An empty index; `add` names entries, the first entry a name is added for keeping it.
local function new(): Index
  -- Not annotated as a `dream_intern_Pool`, which would compile `find` to native code: l3i 2.0.1
  -- leaves that code reading a slot table the pool has replaced, once a name it already holds is
  -- interned while it is at its load limit. The binding's own `find` reads the pool as it is.
  local pool = intern.new(Rules)
  local tokenOf = pool:interner()
  local entries: { number } = {}
  local count = 0

  local function add(source: buffer, offset: number, length: number, entry: number)
    local token = tokenOf(source, offset, length)
    if not entries[token] then
      entries[token] = entry
      count += 1
    end
  end

  local function find(path: string): number?
    local token = pool:find(path)
    return if token then entries[token] else nil
  end

  return { add = add, find = find, isEmpty = function(): boolean return count == 0 end }
end

return table.freeze { new = new }
