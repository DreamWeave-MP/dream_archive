--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- What every format shares: the crate's error messages, argument checks, lookup normalization,
-- 64-bit hashes as Luau integers, the codecs with the crate's size and trailing-data rules, and
-- host file access with the operating system's messages the crate reported.
local bytes = require '@dream/bytes'
local fs = require '@dream/fs'

local function fail(message: string): never return error(message, 0) end

--- The crate's `{value:#010x}`.
local function hex32(value: number): string return string.format('0x%08x', value) end

--- Bytes as UTF-8 with every invalid sequence replaced, as Rust's lossy `Display` writes them.
local function lossy(text: string): string
  if utf8.len(text) then return text end

  return bytes.decode(text, 'utf-8')
end

local Messages = table.freeze {
  outOfBounds = 'archive offset or size is out of bounds',
  endOfFile = 'unexpected end of file',
  shortRead = 'failed to fill whole buffer',
  unknownFormat = 'unknown or disabled archive format',
  bsaInvalidPath = 'archive path can not be stored safely',
  ba2InvalidPath = 'invalid archive path',
  bsaDuplicate = 'duplicate normalized archive path',
  ba2Duplicate = 'duplicate archive path',
  trailing = 'compressed payload has trailing data',
  pathsUnavailable = 'archive does not contain filenames required for path-based extraction; use TES4 extract_to_with_paths with a path dictionary when applicable',
  unsafeOutput = 'archive path can not be extracted safely',
  noFileName = 'archive entry has no file name',
  outputNoFileName = 'output path has no file name',
  sourceChanged = 'deferred source file size changed before archive write',
  integral = 'archive integer field can not fit on this platform',
  capacity = 'archive table or payload requests more memory than can be allocated',
  invalidLz4Frame = 'expected TES4 v105 LZ4 frame data',
  entryIndex = 'entry index out of bounds',
}

local function invalidMagic(value: number): never
  return fail(`invalid magic read from archive header: {hex32(value)}`)
end

local function invalidVersion(value: number): never
  return fail(`invalid version read from archive header: {value}`)
end

local function notImplemented(feature: string): never
  return fail(`support for this feature is not implemented: {feature}`)
end

local function dds(message: string): never return fail(`invalid DDS/texture metadata: {message}`) end

local function sizeMismatch(expected: number, actual: number): never
  return fail(
    `buffer failed to decompress to the expected size: expected {expected} bytes, got {actual} bytes`
  )
end

local function fileNotFound(path: string): never
  return fail(`archive member not found: {lossy(path)}`)
end

--- The binding's message for an argument it couldn't convert. A method's arguments sit one stack
--- slot later than their position, after `self`; a method's name has its type's capital in it.
local function argument(what: string, position: number, expected: string, problem: string): never
  local stack = position + (if string.find(what, '%.%u') then 1 else 0)
  return fail(
    `{what}: bad argument #{position} (expected {expected}): Lua stack index {stack}: {problem}`
  )
end

--- The binding's message for a value read inside a call rather than converted at its boundary.
local function stackValue(stack: number, problem: string): never
  return fail(`Lua stack index {stack}: {problem}`)
end

--- The binding's message for an archive argument that isn't one.
local function archiveArgument(what: string, value: unknown): never
  return fail(
    `{what}: bad argument #2 (expected dream.archive.Archive): invalid argument #3 to '{what}' (dream.archive.Archive expected, got {typeof(
      value
    )})`
  )
end

--- The binding's message for a receiver that isn't one.
local function receiver(what: string, expected: string, value: unknown): never
  return fail(`invalid argument #1 to '{what}' ({expected} expected, got {typeof(value)})`)
end

local function checkString(what: string, value: unknown, position: number): string
  if type(value) == 'string' then return value end

  return argument(what, position, 'string', `expected string, got {typeof(value)}`)
end

--- A host path: a string the host can name, which the crate takes as UTF-8 only.
local function checkHostPath(what: string, value: unknown, position: number): string
  local path = checkString(what, value, position)
  if not utf8.len(path) then
    fail(`{what}: bad argument #{position} (expected string): Lua string is not valid UTF-8`)
  end

  return path
end

local function checkBytes(what: string, value: unknown, position: number): buffer | string
  if type(value) == 'string' or type(value) == 'buffer' then return value :: buffer | string end

  return argument(what, position, 'string or buffer', `expected buffer, got {typeof(value)}`)
end

--- An exact integer in `low..high`, as the binding's `Exact<T>` reads one; `kind` names the Rust
--- type the binding said.
local function checkInteger(
  what: string,
  value: unknown,
  position: number,
  kind: string,
  low: number,
  high: number
): number
  local number: number
  if type(value) == 'number' then
    number = value
  elseif typeof(value) == 'integer' then
    number = integer.tonumber(value :: integer)
  else
    return argument(what, position, 'integer', `expected integer, got {typeof(value)}`)
  end

  if number ~= math.floor(number) or number < low or number > high then
    return argument(what, position, 'integer', `number {number} is not an exact {kind}`)
  end

  return number
end

--- A 1-based entry position, as the binding reads one: any exact integer, bounds checked later.
local function checkIndex(what: string, value: unknown, position: number): number
  return checkInteger(what, value, position, 'i64', -2 ^ 63, 2 ^ 63)
end

local Scratch = buffer.create(8)

--- The 64-bit integer whose low and high halves are these two u32 values.
local function int64(low: number, high: number): integer
  buffer.writeu32(Scratch, 0, low)
  buffer.writeu32(Scratch, 4, high)
  return buffer.readinteger(Scratch, 0)
end

local function hex64(value: integer): string
  return string.format('%016x', (value :: unknown) :: number)
end

--- `dream_path`'s normalization, the one every archive matches a lookup path with: `\` is `/`,
--- ASCII is lowercased, and leading and repeated separators are dropped. A trailing one stays, so
--- a path naming a folder matches no file.
local function lookupKey(path: string): string
  local key = string.lower(path)
  if string.find(key, '\\', 1, true) then key = (string.gsub(key, '\\', '/')) end

  if string.byte(key, 1) == 47 or string.find(key, '//', 1, true) then
    key = (string.gsub((string.gsub(key, '^/+', '')), '//+', '/'))
  end

  return key
end

--- The BSA hash normalization, which isn't the lookup one: `/` is `\`, ASCII is lowercased,
--- separators come off both ends, and an empty or overlong path hashes as `.`.
local function normalizeHashPath(path: string): string
  local lowered = string.lower(path)
  if string.find(lowered, '/', 1, true) then lowered = (string.gsub(lowered, '/', '\\')) end

  if string.byte(lowered, 1) == 92 or string.byte(lowered, -1) == 92 then
    lowered = string.match(lowered, '^\\*(.-)\\*$') :: string
  end

  if lowered == '' or #lowered >= 260 then return '.' end

  return lowered
end

--- A path as builders store it: both separators split it, empty and `.` components go, ASCII is
--- lowercased, and the components are joined with `\`; nil when it has no component, or one that
--- is `..` or holds a NUL or a `:`. A path with none of those to undo takes only C calls.
local function storedPath(path: string): string?
  local lowered = string.lower(path)
  if string.find(lowered, '/', 1, true) then lowered = (string.gsub(lowered, '/', '\\')) end

  local first, last = string.byte(lowered, 1), string.byte(lowered, -1)
  if
    first
    and first ~= 92
    and first ~= 46
    and last ~= 92
    and not string.find(lowered, '\\\\', 1, true)
    and not string.find(lowered, '\\.', 1, true)
    and not string.find(lowered, ':', 1, true)
    and not string.find(lowered, '\0', 1, true)
  then
    return lowered
  end

  local parts = {}
  for component in string.gmatch(lowered, '[^\\]+') do
    if component == '.' then continue end

    if
      component == '..'
      or string.find(component, '\0', 1, true)
      or string.find(component, ':', 1, true)
    then
      return nil
    end

    table.insert(parts, component)
  end

  if #parts == 0 then return nil end

  return table.concat(parts, '\\')
end

--- `items` in order, by `key`, a number that orders them as their full order does except among
--- equal keys, which `before` orders. The keys sort with Luau's own comparison, so a full order
--- is only asked of the rare items that share a key.
local function sortByKey<T>(items: { T }, key: (T) -> number, before: (T, T) -> boolean): { T }
  local keys: { number } = table.create(#items) :: { number }
  local groups: { [number]: { T } } = {}
  for _, item in items do
    local value = key(item)
    local group = groups[value]
    if group then
      table.insert(group, item)
    else
      groups[value] = { item }
      table.insert(keys, value)
    end
  end

  table.sort(keys)
  local sorted: { T } = table.create(#items) :: { T }
  for _, value in keys do
    local group = groups[value]
    if #group > 1 then table.sort(group, before) end

    for _, item in group do
      table.insert(sorted, item)
    end
  end

  return sorted
end

--- Whether a normalized lookup path can name a file: an empty one, or one ending in a separator,
--- names a folder.
local function namesAFile(key: string): boolean
  local last = string.byte(key, -1)
  return last ~= nil and last ~= 47
end

--- The part of a stored path after its last separator.
local function lastComponent(path: string): string
  local at = string.find(path, '[/\\][^/\\]*$')
  if not at then return path end

  return string.sub(path, at + 1)
end

--- The part of a stored path before its last separator, `''` at the root.
local function parentFolder(path: string): string
  local at = string.find(path, '[/\\][^/\\]*$')
  if not at then return '' end

  return string.sub(path, 1, at - 1)
end

--- Archive errors retain the OS reason, without the native operation and host path prefix.
local function hostMessage(message: unknown, path: string?): string
  local text = tostring(message)
  if path then
    local prefix = string.match(text, '^dream%.fs%.[^:]+: ')
    local context = if prefix then prefix .. lossy(path) .. ': ' else nil
    if context and string.sub(text, 1, #context) == context then
      return string.sub(text, #context + 1)
    end
  end

  return text
end

local function hostResult<Value>(value: Value?, message: string?, path: string): Value
  if message then fail(hostMessage(message, path)) end

  return assert(value)
end

--- Traversal errors identify the failing descendant, not necessarily the walk's root.
local function walkMessage(message: string): string
  return string.match(message, '(IO error for operation on .*)$')
    or string.match(message, '(File system loop found: .*)$')
    or message
end

--- A zlib stream that inflates to exactly `expected` bytes and ends where the input does, with
--- the crate's messages. The crate fills the expected size and then decodes one byte more to tell
--- a longer stream from a finished one, so the cap is one byte past the size: a stream corrupt
--- just past it fails as corrupt there too, and a longer one is reported one byte long.
local function inflateExact(source: buffer, expected: number): buffer
  -- bytes.inflate throws for a corrupt stream and for one longer than maxSize.
  local ok, result = pcall(
    bytes.inflate,
    source,
    { format = 'zlib', maxSize = expected + 1 } :: { format: string?, maxSize: number? }
  )
  if not ok then
    local message = tostring(result)
    if string.find(message, 'exceeds maxSize', 1, true) then
      return sizeMismatch(expected, expected + 1)
    end

    local detail = if string.sub(message, 1, 15) == 'bytes.inflate: '
      then string.sub(message, 16)
      else message
    return fail(`zlib decompression failed: {detail}`)
  end

  local out = result :: buffer
  local actual = buffer.len(out)
  if actual ~= expected then return sizeMismatch(expected, actual) end

  -- The stream's Adler-32, checked by the decoder, is its last four bytes unless more follow.
  local length = buffer.len(source)
  if length < 4 or bit32.byteswap(buffer.readu32(source, length - 4)) ~= bytes.adler32(out) then
    return fail(Messages.trailing)
  end

  return out
end

--- An LZ4 block that decodes to exactly `expected` bytes (BA2).
local function lz4BlockExact(source: buffer, expected: number): buffer
  -- bytes.lz4Decompress throws for a malformed block or one that needs more room.
  local ok, result = pcall(bytes.lz4Decompress, source, expected, nil)
  if not ok then
    local message = tostring(result)
    local detail = if string.sub(message, 1, 21) == 'bytes.lz4Decompress: '
      then string.sub(message, 22)
      else message
    return fail(`lz4 decompression failed: {detail}`)
  end

  local out = result :: buffer
  if buffer.len(out) ~= expected then return sizeMismatch(expected, buffer.len(out)) end

  return out
end

local BlockSizes: { [number]: number } =
  { [4] = 0x10000, [5] = 0x40000, [6] = 0x100000, [7] = 0x400000 }

--- `length` bytes of `source` at `offset`, in a buffer of their own.
local function slice(source: buffer, offset: number, length: number): buffer
  local out = buffer.create(length)
  buffer.copy(out, 0, source, offset, length)
  return out
end

--- An LZ4 frame of independent blocks, read block by block with the decoded size known: what
--- lz4_flex's frame decoder accepts, checked as it checks it (header, block, and content
--- checksums, block sizes, content size), and nothing more. Anything else, or anything that
--- fails, is nil, and the frame decoder reads the frame again for its own answer.
local function lz4FrameBlocks(source: buffer, expected: number): buffer?
  local length = buffer.len(source)
  if length < 7 then return nil end

  local flags, descriptor = buffer.readu8(source, 4), buffer.readu8(source, 5)
  if
    bit32.band(flags, 0xc0) ~= 0x40
    or bit32.band(flags, 0x23) ~= 0x20
    or bit32.band(descriptor, 0x8f) ~= 0
  then
    return nil
  end

  local maxBlock = BlockSizes[bit32.rshift(descriptor, 4)]
  if not maxBlock then return nil end

  local position = 6
  local contentSize: number? = nil
  if bit32.band(flags, 0x08) ~= 0 then
    if length < 15 then return nil end

    contentSize = buffer.readu32(source, 6) + buffer.readu32(source, 10) * 0x100000000
    position = 14
  end

  local header =
    bit32.band(bit32.rshift(bytes.xxh32(buffer.readstring(source, 4, position - 4)), 8), 0xff)
  if buffer.readu8(source, position) ~= header then return nil end

  position += 1
  local blockChecksums = bit32.band(flags, 0x10) ~= 0
  local pieces: { buffer } = {}
  local produced = 0
  while true do
    if position + 4 > length then return nil end

    local size = buffer.readu32(source, position)
    position += 4
    if size == 0 then break end

    local stored = bit32.band(size, 0x80000000) ~= 0
    size = bit32.band(size, 0x7fffffff)
    if size > maxBlock or position + size > length then return nil end

    local block = slice(source, position, size)
    position += size
    if blockChecksums then
      if position + 4 > length or bytes.xxh32(block) ~= buffer.readu32(source, position) then
        return nil
      end

      position += 4
    end

    local decoded = block
    if not stored then
      -- bytes.lz4Decompress throws for a malformed block, or one longer than the room left.
      local ok, result =
        pcall(bytes.lz4Decompress, block, math.min(maxBlock, expected - produced), nil)
      if not ok then return nil end

      decoded = result :: buffer
    end

    produced += buffer.len(decoded)
    if produced > expected then return nil end

    table.insert(pieces, decoded)
  end

  if contentSize and contentSize ~= produced or produced ~= expected then return nil end

  local out = pieces[1]
  if #pieces ~= 1 then
    out = buffer.create(produced)
    local at = 0
    for _, piece in pieces do
      buffer.copy(out, at, piece)
      at += buffer.len(piece)
    end
  end

  if bit32.band(flags, 0x04) ~= 0 then
    if position + 4 > length or bytes.xxh32(out) ~= buffer.readu32(source, position) then
      return nil
    end

    position += 4
  end

  if position ~= length then return nil end

  return out
end

--- An LZ4 frame that decodes to exactly `expected` bytes (TES4 version 105).
local function lz4FrameExact(source: buffer, expected: number): buffer
  local fast = lz4FrameBlocks(source, expected)
  if fast then return fast end

  -- bytes.lz4FrameDecompress throws for a malformed frame and for one longer than maxSize.
  local ok, result =
    pcall(bytes.lz4FrameDecompress, source, { maxSize = expected } :: { maxSize: number? })
  if not ok then
    local message = tostring(result)
    if string.find(message, 'exceeds maxSize', 1, true) then
      return sizeMismatch(expected, expected + 1)
    end

    local detail = if string.sub(message, 1, 26) == 'bytes.lz4FrameDecompress: '
      then string.sub(message, 27)
      else message
    return fail(`LZ4 frame operation failed: {detail}`)
  end

  local out = result :: buffer
  if buffer.len(out) ~= expected then return sizeMismatch(expected, buffer.len(out)) end

  return out
end

--- A host file's bytes, with the host's message as the crate's.
local function readHostFile(path: string): buffer
  local data, message = fs.readFile(path)
  return hostResult(data, message, path)
end

--- A host file's size in bytes, following links, as `fs::metadata` reads it.
local function hostFileSize(path: string): number
  local info, message = fs.stat(path)
  if message then fail(hostMessage(message, path)) end
  if not info then return fail 'No such file or directory (os error 2)' end

  return info.size
end

--- Writes `data` to `path` directly, as `File::create` and a write do.
local function writeHostFile(path: string, data: buffer)
  local count, message = fs.writeFile(path, data)
  hostResult(count, message, path)
end

return table.freeze {
  fail = fail,
  hex32 = hex32,
  hex64 = hex64,
  lossy = lossy,
  Messages = Messages,
  invalidMagic = invalidMagic,
  invalidVersion = invalidVersion,
  notImplemented = notImplemented,
  dds = dds,
  sizeMismatch = sizeMismatch,
  fileNotFound = fileNotFound,
  argument = argument,
  stackValue = stackValue,
  receiver = receiver,
  archiveArgument = archiveArgument,
  checkString = checkString,
  checkHostPath = checkHostPath,
  checkBytes = checkBytes,
  checkInteger = checkInteger,
  checkIndex = checkIndex,
  int64 = int64,
  lookupKey = lookupKey,
  normalizeHashPath = normalizeHashPath,
  storedPath = storedPath,
  sortByKey = sortByKey,
  namesAFile = namesAFile,
  lastComponent = lastComponent,
  parentFolder = parentFolder,
  hostMessage = hostMessage,
  hostResult = hostResult,
  walkMessage = walkMessage,
  inflateExact = inflateExact,
  lz4BlockExact = lz4BlockExact,
  lz4FrameExact = lz4FrameExact,
  readHostFile = readHostFile,
  hostFileSize = hostFileSize,
  writeHostFile = writeHostFile,
}
