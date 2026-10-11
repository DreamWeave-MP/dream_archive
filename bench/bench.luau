--!strict
-- The archive port, timed from Luau: open and index, lookups by path and by hash, listing, reads
-- (stored, zlib, LZ4 blocks and frames, DDS headers rebuilt), extraction, and building. Run by
-- `jess run --capability filesystem.read --capability filesystem.write --package .
-- bench/bench.luau <scratch> [filter] [real archive]...`, on a release build.
local fs = require '@dream/fs'
local port = require '@dream-archive'

-- A build without the verified toolchain runs Luau slower than a release does, so a number from
-- one says nothing about what ships.
if not jess.bench.verified then
  error('archive bench: this jess was built without the verified toolchain: measure on a release build', 0)
end

--- The arguments: a folder to write into, a filter on the workload names (`''` for all), and
--- real archives to read in place.
local scratch: string, filter: string = ...
local realArchives: { string } = { select(3, ...) }
local only = if filter ~= '' then filter else nil

export type Row = { group: string, name: string, port: number, note: string? }

local rows: { Row } = {}

local function median(samples: { number }): number
  table.sort(samples)
  return samples[(#samples + 1) // 2]
end

--- Seconds per call of `body`: warmed up, then the median of seven batches of a size that
--- takes about a tenth of a second.
-- A full collection, run between batches, so one batch's garbage isn't the next one's collection.
local collect = jess.bench.collect

local function measure(body: () -> ()): number
  collect()
  body()
  local start = os.clock()
  body()
  local once = math.max(os.clock() - start, 1e-7)
  local iterations = math.max(1, math.floor(0.1 / once))
  local samples = {}
  for batch = 1, 7 do
    collect()
    start = os.clock()
    for _ = 1, iterations do
      body()
    end

    samples[batch] = (os.clock() - start) / iterations
  end

  return median(samples)
end

--- Times `make(port)`'s workload.
local function workload(
  group: string,
  name: string,
  make: (module: any) -> () -> unknown,
  note: string?
)
  if only and not string.find(`{group} {name}`, only, 1, true) then return end

  local body = make(port)
  table.insert(rows, { group = group, name = name, port = measure(body :: () -> ()), note = note })
end

local random = 0x2545f491
local function nextRandom(): number
  random = bit32.bxor(random, bit32.lshift(random, 13))
  random = bit32.bxor(random, bit32.rshift(random, 17))
  random = bit32.bxor(random, bit32.lshift(random, 5))
  return random
end

local Words =
  { 'meshes', 'textures', 'sound', 'icons', 'm', 'x', 'clutter', 'door', 'rock', 'Ex', 'In' }

local function payload(size: number, compressible: boolean): string
  if compressible then
    local parts = {}
    for index = 1, size // 16 + 1 do
      parts[index] = string.format('%-15d\n', nextRandom() % 1000)
    end

    return string.sub(table.concat(parts), 1, size)
  end

  local out = buffer.create(size)
  for offset = 0, size - 4, 4 do
    buffer.writeu32(out, offset, nextRandom())
  end

  return buffer.tostring(out)
end

local function paths(count: number, extension: string): { string }
  local list = {}
  for index = 1, count do
    local folder = `{Words[nextRandom() % #Words + 1]}\\{Words[nextRandom() % #Words + 1]}`
    list[index] = `{folder}\\File_{index}_{nextRandom() % 100000}{extension}`
  end

  return list
end

type Input = { name: string, data: string, paths: { string }, file: string }

--- The synthetic archives, built once.
local function inputs(): { Input }
  local made: { Input } = {}
  local function add(name: string, builder: any, list: { string })
    local data = builder:toBytes()
    local file = `{scratch}/{name}.archive`
    assert(fs.writeFile(file, data))
    table.insert(made, { name = name, data = data, paths = list, file = file })
  end

  local plain = paths(4000, '.nif')
  local tes3 = port.bsa.tes3.Builder.new()
  for _, path in plain do
    tes3:addBytes(path, payload(1000 + nextRandom() % 3000, false))
  end

  add('tes3 4000 stored', tes3, plain)
  local compressible = paths(1000, '.dds')
  local bodies: { string } = {}
  for index in compressible do
    bodies[index] = payload(16384, true)
  end

  for _, version in { 104, 105 } do
    local builder = port.bsa.tes4.Builder.new()
    builder:setVersion(version)
    builder:setCompressed(true)
    for index, path in compressible do
      builder:addBytes(path, bodies[index])
    end

    add(if version == 104 then 'tes4 1000 zlib' else 'tes4 1000 lz4 frames', builder, compressible)
  end

  for _, codec in { 'zip', 'lz4' } do
    local builder = port.ba2.Builder.new()
    if codec == 'lz4' then builder:setVersion(3) end

    builder:setCompression(codec)
    for index, path in compressible do
      builder:addBytes(path, bodies[index])
    end

    add(`ba2 1000 {codec}`, builder, compressible)
  end

  local textures = paths(300, '.dds')
  local dx10 = port.ba2.Dx10Builder.new()
  local texturePayload = payload(65536 + 16384 + 4096 + 1024 + 256 + 64 + 16 + 16 + 16, false)
  for _, path in textures do
    dx10:addTextureBytes(
      path,
      { height = 256, width = 256, mipCount = 9, format = 98, flags = 0, tileMode = 0 },
      texturePayload
    )
  end

  add('dx10 300 BC7', dx10, textures)
  return made
end

--- Every stored path of `archive`, and the same in upper case with `/` separators.
local function lookups(list: { string }): { string }
  local all = table.clone(list)
  for _, path in list do
    table.insert(all, (string.gsub(string.upper(path), '\\', '/')))
  end

  return all
end

local function readWorkloads(
  group: string,
  data: string?,
  file: string?,
  list: { string },
  sample: number
)
  local function opener(module: any): any
    if data then return module.openBytes(data) end

    return module.openPath(file)
  end

  workload(group, 'open + index', function(module: any)
    return function(): unknown return opener(module):len() end
  end)

  workload(group, 'open + first lookup', function(module: any)
    return function(): unknown return opener(module):contains(list[1]) end
  end)

  local variants = lookups(list)
  workload(group, `{#variants} lookups by path`, function(module: any)
    local opened = opener(module)
    return function(): unknown
      local found = 0
      for _, path in variants do
        if opened:contains(path) then
          found += 1
        end
      end

      return found
    end
  end)

  workload(group, `{#list} lookups by hash`, function(module: any)
    local opened = opener(module)
    local queries = {}
    for _, entry in opened:entries() do
      if entry.directoryHash then
        table.insert(queries, { entry.directoryHash, entry.fileHash, entry.extensionHash })
      elseif entry.folderHash then
        table.insert(queries, { entry.folderHash, entry.hash })
      else
        table.insert(queries, { entry.hash })
      end
    end

    return function(): unknown
      local found = 0
      for _, query in queries do
        if opened:containsHash(table.unpack(query)) then
          found += 1
        end
      end

      return found
    end
  end)

  workload(group, 'list paths and sizes', function(module: any)
    return function(): unknown
      local opened = opener(module)
      local total = 0
      for _, entry in opened:entries() do
        total += #(entry.path or '') + (entry.size or 0)
      end

      return total
    end
  end)

  local step = math.max(1, #list // sample)
  local chosen = {}
  for index = 1, #list, step do
    table.insert(chosen, list[index])
  end

  workload(group, `read {#chosen} files`, function(module: any)
    local opened = opener(module)
    return function(): unknown
      local total = 0
      for _, path in chosen do
        total += #opened:readFileRequired(path)
      end

      return total
    end
  end)

  workload(group, `readInto {#chosen} files`, function(module: any)
    local opened = opener(module)
    local largest = 0
    for _, path in chosen do
      largest = math.max(largest, opened:get(path).size)
    end

    local target = buffer.create(largest)
    return function(): unknown
      local total = 0
      for _, path in chosen do
        total += opened:readInto(path, target)
      end

      return total
    end
  end)
end

local function buildWorkloads()
  local list = paths(2000, '.nif')
  local bodies, compressible = {}, {}
  for index in list do
    bodies[index] = payload(1000 + nextRandom() % 3000, false)
    compressible[index] = payload(8192, true)
  end

  local function builds(name: string, make: (module: any) -> any, contents: { string })
    workload('build', name, function(module: any)
      return function(): unknown
        local builder = make(module)
        for index, path in list do
          builder:addBytes(path, contents[index])
        end

        return buffer.len(builder:toBuffer())
      end
    end)
  end

  builds(
    'tes3, 2000 stored',
    function(module: any): any return module.bsa.tes3.Builder.new() end,
    bodies
  )

  builds('tes4 104, 2000 zlib', function(module: any): any
    local builder = module.bsa.tes4.Builder.new()
    builder:setCompressed(true)
    return builder
  end, compressible)

  builds('tes4 105, 2000 lz4 frames', function(module: any): any
    local builder = module.bsa.tes4.Builder.new()
    builder:setVersion(105)
    builder:setCompressed(true)
    return builder
  end, compressible)

  builds('ba2, 2000 zip', function(module: any): any
    local builder = module.ba2.Builder.new()
    builder:setCompression 'zip'
    return builder
  end, compressible)

  builds('ba2 v3, 2000 lz4', function(module: any): any
    local builder = module.ba2.Builder.new()
    builder:setVersion(3)
    builder:setCompression 'lz4'
    return builder
  end, compressible)
end

local function extractWorkloads(input: Input)
  local counter = 0
  workload('extract', `{input.name} to disk`, function(module: any)
    local opened = module.openBytes(input.data)
    return function(): unknown
      counter += 1
      local target = `{scratch}/extract-{counter}`
      local written = opened:extractTo(target)
      assert(fs.remove(target, { recursive = true }))
      return written
    end
  end, 'on one thread')
end

local made = inputs()
for _, input in made do
  readWorkloads(`{input.name} (bytes)`, input.data, nil, input.paths, 1000)
  readWorkloads(`{input.name} (path)`, nil, input.file, input.paths, 1000)
end

buildWorkloads()
for _, input in made do
  if input.name == 'tes3 4000 stored' or input.name == 'ba2 1000 zip' then
    extractWorkloads(input)
  end
end

for _, path in realArchives do
  local opened = port.openPath(path)
  local list = {}
  for _, entry in opened:entries() do
    if entry.path then table.insert(list, entry.path) end
  end

  readWorkloads(string.match(path, '[^/]*$') :: string, nil, path, list, 2000)
end

print(string.format('%-34s %-30s %12s', 'group', 'workload', 'seconds'))
for _, row in rows do
  local note = if row.note then `  ({row.note})` else ''
  print(string.format('%-34s %-30s %10.6f s%s', row.group, row.name, row.port, note))
end

return rows
