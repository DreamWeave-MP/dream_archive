--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Opening archives: by format, or by the first four bytes, from a host path or from bytes.
local Store = require './store'
local archive = require './archive'
local ba2 = require './ba2'
local tes3 = require './tes3'
local tes4 = require './tes4'
local types = require './types'
local util = require './util'
local fs = require '@dream/fs'

type Archive = types.Archive
type Format = types.Format
type Store = types.Store

local fail, Messages = util.fail, util.Messages

local Openers: { [Format]: (Store) -> types.Backend } =
  { ba2 = ba2.open, bsaTes3 = tes3.open, bsaTes4 = tes4.open }

--- The family the first four bytes name, or nil: weak detection, not validation.
local function guess(magic: string): Format?
  if magic == 'BTDX' then return 'ba2' end

  if magic == 'BSA\0' then return 'bsaTes4' end

  if magic == '\0\1\0\0' then return 'bsaTes3' end

  return nil
end

local function storeAt(what: string, path: unknown): (Store, string)
  local file = util.checkHostPath(what, path, 1)
  local info = fs.stat(file)
  if info and info.isDir then fail 'Is a directory (os error 21)' end

  return Store.fromPath(file), file
end

local function openPathAs(format: Format, what: string, path: unknown): Archive
  local store, file = storeAt(what, path)
  return archive.new(Openers[format](store), file)
end

local function openBytesAs(format: Format, what: string, data: unknown): Archive
  local store = Store.fromBytes(util.checkBytes(what, data, 1))
  return archive.new(Openers[format](store), nil)
end

local function detected(store: Store): Format
  if store.size < 4 then fail(Messages.shortRead) end

  local format = guess(buffer.readstring(Store.head(store, 4), 0, 4))
  if not format then fail(Messages.unknownFormat) end

  return format :: Format
end

local function openPath(path: string): Archive
  local store, file = storeAt('dream.archive.openPath', path)
  return archive.new(Openers[detected(store)](store), file)
end

local function openBytes(data: buffer | string): Archive
  local store = Store.fromBytes(util.checkBytes('dream.archive.openBytes', data, 1))
  return archive.new(Openers[detected(store)](store), nil)
end

local function detectPath(path: string): Format?
  local file = util.checkHostPath('dream.archive.detectPath', path, 1)
  local result, message = fs.readAt(file, 0, 4)
  local magic = util.hostResult(result, message, file)
  if buffer.len(magic) < 4 then return nil end

  return guess(buffer.readstring(magic, 0, 4))
end

local function guessFormat(data: buffer | string): Format?
  local bytes = util.checkBytes('dream.archive.guessFormat', data, 1)
  if type(bytes) == 'string' then return guess(string.sub(bytes, 1, 4)) end

  local source = bytes :: buffer
  if buffer.len(source) < 4 then return nil end

  return guess(buffer.readstring(source, 0, 4))
end

return table.freeze {
  openPathAs = openPathAs,
  openBytesAs = openBytesAs,
  openPath = openPath,
  openBytes = openBytes,
  detectPath = detectPath,
  guessFormat = guessFormat,
}
