--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- The one archive type every `open*` returns, over any format's backend: the methods the native
-- `dream.archive.Archive` has, with its arguments, results, and messages. Entries are plain
-- tables, one per index for the archive's life, so two reads of an entry are the same table.
local encoding = require './encoding'
local extract = require './extract'
local types = require './types'
local util = require './util'

type Archive = types.Archive
type Backend = types.Backend
type Entry = types.Entry
type Entries = types.Entries

local fail = util.fail

type State = {
  backend: Backend,
  memo: { [string]: number | false },
  memoSize: number,
  label: string,
  cache: { [number]: Entry },
  list: Entries?,
  meta: { [string]: unknown },
}

local states = (setmetatable({}, { __mode = 'k' }) :: unknown) :: { [Archive]: State }

--- Every entry metatable, so an entry of another archive is told apart from a stranger.
local entryMetas = (setmetatable({}, { __mode = 'k' }) :: unknown) :: { [unknown]: boolean }

local function stateOf(archive: unknown, method: string): State
  local state = states[archive :: Archive]
  if not state then
    util.receiver(`dream.archive.Archive.{method}`, 'dream.archive.Archive', archive)
  end

  return state
end

local function wrongFormat(state: State, method: string, expected: string): never
  return fail(`{method} needs a {expected} archive; this archive is {state.backend.format}`)
end

--- What a column holds for a field that was computed and is nil.
local None = table.freeze {}

local Columns = {
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

local function compute(backend: Backend, index: number, key: string): unknown
  if key == 'path' then return backend.path(index) end

  if key == 'name' then return backend.name(index) end

  if key == 'folder' then return backend.folder(index) end

  if key == 'size' then return (backend.size(index)) end

  if key == 'storedSize' then return backend.storedSize(index) end

  if key == 'offset' then return backend.offset(index) end

  if key == 'compressed' then return backend.compressed(index) end

  if key == 'chunks' then return backend.chunks(index) end

  if key == 'hash' then return backend.hash(index) end

  if key == 'hashHex' then
    local hash = backend.hash(index)
    return if hash then util.hex64(hash) else nil
  end

  if key == 'folderHash' then return backend.folderHash(index) end

  if key == 'folderHashHex' then
    local hash = backend.folderHash(index)
    return if hash then util.hex64(hash) else nil
  end

  local hash = backend.ba2Hash(index)
  if not hash then return nil end

  if key == 'directoryHash' then return hash.directory end

  return if key == 'fileHash' then hash.file else hash.extension
end

local function entryMeta(backend: Backend): { [string]: unknown }
  local columns: { [string]: { [number]: unknown } } = {}
  for _, name in Columns do
    columns[name] = {}
  end

  local meta: { [string]: unknown } = {}
  -- Entries are frozen tables holding their `index`, `id`, and `format`; every other field is
  -- computed here on first read and kept, one column per field.
  meta.__index = function(entry: { [string]: unknown }, key: string): unknown
    local index = rawget(entry, 'index') :: number
    local column = columns[key]
    if not column then return nil end

    local value = column[index]
    if value == nil then
      value = compute(backend, index, key)
      column[index] = if value == nil then None else value
    end

    return if value == None then nil else value
  end

  meta.__tostring = function(entry: { [string]: unknown }): string
    local index = rawget(entry, 'index') :: number
    local path = backend.path(index)
    return `dream.archive.Entry({index}, {if path then util.lossy(path) else '<hash only>'})`
  end
  entryMetas[meta] = true
  return meta
end

local function entryOf(state: State, index: number): Entry
  local cached = state.cache[index]
  if cached then return cached end

  local made = (table.freeze(setmetatable(state.backend.row(index), state.meta)) :: unknown) :: Entry
  state.cache[index] = made
  return made
end

--- A 1-based index that names an entry, or the binding's bounds error.
local function handle(state: State, value: unknown, method: string): number
  local index = value :: number
  if type(value) ~= 'number' or index ~= math.floor(index) then
    index = util.checkIndex(`dream.archive.Archive.{method}`, value, 1)
  end

  if index < 1 or index > state.backend.count then fail(util.Messages.entryIndex) end

  return index
end

--- The most paths an archive remembers the answer for before it forgets them all.
local MemoLimit = 65536

--- The entry a path names. Answers are remembered by the path as asked, so asking again is one
--- table lookup.
local function lookup(state: State, path: unknown, method: string): number?
  if type(path) ~= 'string' then util.checkString(`dream.archive.Archive.{method}`, path, 1) end

  local memo = state.memo
  local known = memo[path :: string]
  if known ~= nil then return known or nil end

  local found = state.backend.lookup(path :: string)
  if state.memoSize >= MemoLimit then
    table.clear(memo)
    state.memoSize = 0
  end

  memo[path :: string] = found or false
  state.memoSize += 1
  return found
end

local function required(state: State, path: unknown, method: string): number
  local index = lookup(state, path, method)
  if not index then util.fileNotFound(path :: string) end

  return index :: number
end

--- The entry `target` names: an entry of this archive, or a path.
local function target(state: State, value: unknown): number
  if type(value) == 'string' then
    local index = lookup(state, value, 'readInto')
    if not index then util.fileNotFound(value) end

    return index :: number
  end

  if type(value) == 'table' then
    local index = rawget(value :: { [string]: unknown }, 'index')
    if type(index) == 'number' and state.cache[index] == value then return index end

    if entryMetas[getmetatable(value :: {})] then
      fail 'the entry handle belongs to a different archive'
    end
  end

  return util.stackValue(2, `expected an entry handle or an archive path, got {typeof(value)}`)
end

local EntriesMeta = table.freeze {
  __index = table.freeze {
    toTable = function(entries: Entries): { Entry }
      return table.move(entries, 1, #entries, 1, table.create(#entries) :: { Entry })
    end,
  },
}

local Methods = {}

function Methods.format(self: Archive): types.Format return stateOf(self, 'format').backend.format end

function Methods.len(self: Archive): number return stateOf(self, 'len').backend.count end

function Methods.isEmpty(self: Archive): boolean return stateOf(self, 'isEmpty').backend.count == 0 end

function Methods.archiveSize(self: Archive): number
  return stateOf(self, 'archiveSize').backend.store.size
end

function Methods.entries(self: Archive): Entries
  local state = stateOf(self, 'entries')
  local list = state.list
  if list then return list end

  local count = state.backend.count
  local made = table.create(count) :: { Entry }
  for index = 1, count do
    made[index] = entryOf(state, index)
  end

  local frozen = (table.freeze(setmetatable(made, EntriesMeta)) :: unknown) :: Entries
  state.list = frozen
  return frozen
end

function Methods.entry(self: Archive, index: number): Entry?
  local state = stateOf(self, 'entry')
  local position = util.checkIndex('dream.archive.Archive.entry', index, 1)
  if position < 1 or position > state.backend.count then return nil end

  return entryOf(state, position)
end

function Methods.contains(self: Archive, path: string): boolean
  return lookup(stateOf(self, 'contains'), path, 'contains') ~= nil
end

function Methods.get(self: Archive, path: string): Entry?
  local state = stateOf(self, 'get')
  local index = lookup(state, path, 'get')
  return if index then entryOf(state, index) else nil
end

function Methods.containsHash(
  self: Archive,
  first: unknown,
  second: unknown?,
  third: unknown?
): boolean
  return stateOf(self, 'containsHash').backend.hashLookup(first, second, third) ~= nil
end

function Methods.getByHash(self: Archive, first: unknown, second: unknown?, third: unknown?): Entry?
  local state = stateOf(self, 'getByHash')
  local index = state.backend.hashLookup(first, second, third)
  return if index then entryOf(state, index) else nil
end

function Methods.readFile(self: Archive, path: string): string?
  local state = stateOf(self, 'readFile')
  local index = lookup(state, path, 'readFile')
  return if index then state.backend.decodeString(index) else nil
end

function Methods.readFileRequired(self: Archive, path: string): string
  local state = stateOf(self, 'readFileRequired')
  return state.backend.decodeString(required(state, path, 'readFileRequired'))
end

function Methods.extractFile(self: Archive, path: string): string?
  local state = stateOf(self, 'extractFile')
  local index = lookup(state, path, 'extractFile')
  return if index then state.backend.decodeString(index) else nil
end

function Methods.extractFileRequired(self: Archive, path: string): string
  local state = stateOf(self, 'extractFileRequired')
  return state.backend.decodeString(required(state, path, 'extractFileRequired'))
end

function Methods.readEntry(self: Archive, index: number): string
  local state = stateOf(self, 'readEntry')
  return state.backend.decodeString(handle(state, index, 'readEntry'))
end

function Methods.extractEntry(self: Archive, index: number): string
  local state = stateOf(self, 'extractEntry')
  return state.backend.decodeString(handle(state, index, 'extractEntry'))
end

function Methods.extractEntryToPath(self: Archive, index: number, path: string): number
  local state = stateOf(self, 'extractEntryToPath')
  local position = handle(state, index, 'extractEntryToPath')
  local output = util.checkHostPath('dream.archive.Archive.extractEntryToPath', path, 2)
  local backend = state.backend
  return extract.writeAtomically(output, function(): buffer return backend.decode(position) end)
end

function Methods.readInto(
  self: Archive,
  entry: Entry | string,
  into: buffer,
  offset: number?
): number
  local state = stateOf(self, 'readInto')
  if type(into) ~= 'buffer' then
    util.argument(
      'dream.archive.Archive.readInto',
      2,
      'buffer',
      `expected buffer, got {typeof(into)}`
    )
  end

  local at = if offset == nil
    then 0
    else util.checkInteger('dream.archive.Archive.readInto', offset, 3, 'usize', 0, 2 ^ 53)
  local index = target(state, entry)
  local length = buffer.len(into)
  if at > length then fail 'buffer access out of bounds' end

  local backend = state.backend
  local size, problem = backend.size(index)
  if not size then fail(problem :: string) end

  local available = length - at
  if (size :: number) > available then
    fail(`readInto: the {size}-byte entry does not fit in the {available} bytes left in the buffer`)
  end

  return backend.decodeInto(index, into, at)
end

--- Every entry's output path below `root`, checked before anything is written.
local function outputPaths(state: State, root: string): { string }
  local backend = state.backend
  local paths = table.create(backend.count) :: { string }
  for index = 1, backend.count do
    local path = backend.path(index)
    if not path then
      if backend.format == 'bsaTes4' then fail(util.Messages.pathsUnavailable) end

      path = ''
    end

    paths[index] = extract.outputPath(root, path :: string)
  end

  return paths
end

--- Writes every entry to its path. Every folder is made before the first payload is decoded, as
--- the crate's parallel extraction makes them, so a failure leaves the same folders behind.
local function writeAll(state: State, paths: { string }): number
  local backend = state.backend
  local folders = { last = nil :: string? }
  for _, path in paths do
    extract.ensureParent(path, folders)
  end

  local written = 0
  for index, path in paths do
    written += extract.writeAtomically(path, function(): buffer return backend.decode(index) end)
  end

  return written
end

function Methods.extractTo(self: Archive, root: string): number
  local state = stateOf(self, 'extractTo')
  local folder = util.checkHostPath('dream.archive.Archive.extractTo', root, 1)
  return writeAll(state, outputPaths(state, folder))
end

function Methods.extractToWithEncoding(self: Archive, root: string, name: string): number
  local state = stateOf(self, 'extractToWithEncoding')
  local folder = util.checkHostPath('dream.archive.Archive.extractToWithEncoding', root, 1)
  local chosen =
    encoding.parse(util.checkString('dream.archive.Archive.extractToWithEncoding', name, 2))
  local backend = state.backend
  if backend.format == 'ba2' then wrongFormat(state, 'extractToWithEncoding', 'BSA') end

  local function decode(component: string): string return encoding.decode(component, chosen) end

  local folders = { last = nil :: string? }
  local written = 0
  for index = 1, backend.count do
    local stored = backend.path(index)
    if not stored then fail(util.Messages.pathsUnavailable) end

    local path = extract.outputPath(folder, stored :: string, decode)
    extract.ensureParent(path, folders)
    written += extract.writeAtomically(path, function(): buffer return backend.decode(index) end)
  end

  return written
end

function Methods.extractToWithPaths(self: Archive, root: string, paths: { string }): number
  local state = stateOf(self, 'extractToWithPaths')
  local folder = util.checkHostPath('dream.archive.Archive.extractToWithPaths', root, 1)
  if type(paths) ~= 'table' then
    util.argument(
      'dream.archive.Archive.extractToWithPaths',
      2,
      'table',
      `expected table, got {typeof(paths)}`
    )
  end

  local backend = state.backend
  if backend.format ~= 'bsaTes4' then wrongFormat(state, 'extractToWithPaths', 'TES4 BSA') end

  local folders = { last = nil :: string? }
  local extracted: { [number]: boolean } = {}
  local written = 0
  for _, candidate in ipairs(paths) do
    if type(candidate) ~= 'string' then continue end

    local index = backend.lookup(candidate)
    if not index or extracted[index] then continue end

    extracted[index] = true
    local path = extract.outputPath(folder, candidate)
    extract.ensureParent(path, folders)
    local chosen = index :: number
    written += extract.writeAtomically(path, function(): buffer return backend.decode(chosen) end)
  end

  return written
end

function Methods.info(self: Archive): types.BA2Info
  local state = stateOf(self, 'info')
  local info = state.backend.info
  if not info then wrongFormat(state, 'info', 'BA2') end

  return (info :: () -> types.BA2Info)()
end

local ArchiveMeta = table.freeze {
  __index = table.freeze(Methods),
  __tostring = function(archive: Archive): string
    local state = states[archive]
    return `dream.archive.Archive({state.backend.format}, {state.backend.count} entries, {state.label})`
  end,
}

local function new(backend: Backend, path: string?): Archive
  local archive = (table.freeze(setmetatable({}, ArchiveMeta)) :: unknown) :: Archive
  states[archive] = {
    backend = backend,
    label = if path then path else '<memory>',
    cache = {},
    memo = {},
    memoSize = 0,
    meta = entryMeta(backend),
  }
  return archive
end

--- The backend of an archive this module made, or nil for anything else.
local function backendOf(archive: unknown): Backend?
  local state = states[archive :: Archive]
  return if state then state.backend else nil
end

--- What an archive this module made reports itself as: the path it was opened from, or
--- `<memory>`; nil for anything else.
local function labelOf(archive: unknown): string?
  local state = states[archive :: Archive]
  return if state then state.label else nil
end

--- Whether `value` is an entry of an archive this module made.
local function isEntry(value: unknown): boolean
  return type(value) == 'table' and entryMetas[getmetatable(value :: {})] == true
end

--- A builder's `(archive, id)` arguments in the binding's order: both are converted, then the
--- archive's family is checked, then the id is taken as a 1-based position.
local function entryArgument(
  what: string,
  from: unknown,
  id: unknown,
  format: types.Format,
  family: string
): (Backend, number)
  local backend = backendOf(from)
  if not backend then util.archiveArgument(what, from) end

  local index = util.checkIndex(what, id, 3)
  local chosen = backend :: Backend
  if chosen.format ~= format then
    fail(`addArchiveEntry needs a {family} archive; this archive is {chosen.format}`)
  end

  if index < 1 then fail(util.Messages.entryIndex) end

  return chosen, index
end

return table.freeze {
  new = new,
  backendOf = backendOf,
  labelOf = labelOf,
  isEntry = isEntry,
  entryArgument = entryArgument,
}
