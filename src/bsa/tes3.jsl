--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- @dream-archive/bsa/tes3: TES3 (Morrowind) BSAs, as `@dream/archive/bsa/tes3` has them.
local builder = require '../internal/tes3_builder'
local open = require '../internal/open'
local tes3 = require '../internal/tes3'
local types = require '../internal/types'
local util = require '../internal/util'

type Archive = types.Archive

local function openPath(path: string): Archive
  return open.openPathAs('bsaTes3', 'dream.archive.openPath', path)
end

local function openBytes(data: buffer | string): Archive
  return open.openBytesAs('bsaTes3', 'dream.archive.openBytes', data)
end

local function hashFile(
  path: string
): { lo: number, hi: number, hex: string, hash: integer, normalized: string }
  return tes3.hashFile(util.checkString('dream.archive.hashFile', path, 1))
end

return table.freeze {
  openPath = openPath,
  openBytes = openBytes,
  hashFile = hashFile,
  Builder = table.freeze { new = builder.new },
}
