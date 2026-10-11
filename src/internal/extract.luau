--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Writing members to the host: archive paths made into safe output paths below a folder, and each
-- file written beside its target and renamed over it, so a failed write leaves the old file whole.
local util = require './util'
local fs = require '@dream/fs'

local fail, Messages = util.fail, util.Messages

--- `root` with one more component, as `PathBuf::push` joins a relative one.
local function join(root: string, component: string): string
  if root == '' then return component end

  if string.byte(root, -1) == 47 then return root .. component end

  return `{root}/{component}`
end

--- The output path of `archivePath` below `root`: both separators split it, empty and `.`
--- components are dropped, and `..`, NUL, and `:` are refused, so extraction can't leave `root`.
--- `decode` turns each component into the text the host path holds, when an encoding is given.
local function outputPath(root: string, archivePath: string, decode: ((string) -> string)?): string
  local path = root
  local pushed = false
  for component in string.gmatch(archivePath, '[^/\\]+') do
    if component == '.' then continue end

    if
      component == '..'
      or string.find(component, '\0', 1, true)
      or string.find(component, ':', 1, true)
    then
      fail(Messages.unsafeOutput)
    end

    path = join(path, if decode then decode(component) else component)
    pushed = true
  end

  if not pushed then fail(Messages.noFileName) end

  return path
end

--- The folder `path` is in, as `Path::parent` has it: `''` for a bare name, nil for nothing.
local function parent(path: string): string?
  local trimmed = string.match(path, '^(.-)/*$') :: string
  if trimmed == '' then return nil end

  local at = string.find(trimmed, '/[^/]*$')
  if not at then return '' end

  if at == 1 then return '/' end

  return (string.gsub(string.sub(trimmed, 1, at - 1), '/+$', ''))
end

--- The last component of `path`, as `Path::file_name` has it: nil for a path ending in `..`, a
--- root, or nothing.
local function fileName(path: string): string?
  local last: string? = nil
  for component in string.gmatch(path, '[^/]+') do
    if component ~= '.' then last = component end
  end

  if last == '..' then return nil end

  return last
end

local function createDir(path: string)
  if path == '' then return end

  local created, message = fs.mkdir(path, { recursive = true })
  util.hostResult(created, message, path)
end

local Salt = math.random(0, 0x7fffffff)
local counter = 0

local function removeQuietly(path: string)
  fs.remove(path)
end

--- Writes what `produce` makes to `path` through a temporary file in the same folder, renamed
--- into place. The folder is made first and the payload decoded after, as the crate orders it, so
--- a payload that fails to decode leaves no file behind.
local function writeAtomically(path: string, produce: () -> buffer): number
  local folder = parent(path)
  if folder then createDir(folder) end

  local name = fileName(path)
  if not name then fail(Messages.outputNoFileName) end

  local data = produce()
  counter += 1
  local temporary = join(folder or '', `.{name}.dream-archive-tmp-{Salt}-{counter}`)
  local written, writeError = fs.writeFile(temporary, data)
  if not written then
    removeQuietly(temporary)
    fail(util.hostMessage(writeError, temporary))
  end

  local renamed, renameError = fs.rename(temporary, path)
  if not renamed then
    removeQuietly(temporary)
    fail(util.hostMessage(renameError, temporary))
  end

  return buffer.len(data)
end

--- Creates the folder `path` goes in when it differs from the last one made.
local function ensureParent(path: string, state: { last: string? })
  local folder = parent(path)
  if not folder or folder == state.last then return end

  createDir(folder)
  state.last = folder
end

return table.freeze {
  join = join,
  outputPath = outputPath,
  parent = parent,
  fileName = fileName,
  createDir = createDir,
  writeAtomically = writeAtomically,
  ensureParent = ensureParent,
}
