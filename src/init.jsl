--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- @dream-archive: Bethesda BA2 and BSA archives in Luau, a port of the dream_archive crate
-- whose `@dream/archive` binding this replaces under its own name: open, list, look up, read,
-- extract, and build TES3 BSAs, TES4-family BSAs, and BA2s.
local open = require '@self/internal/open'
local types = require '@self/internal/types'
local util = require '@self/internal/util'

export type Archive = types.Archive
export type Entry = types.Entry
export type Entries = types.Entries
export type Format = types.Format
export type TextureHeader = types.TextureHeader

--- The dream_archive release this port follows.
local Version = '1.1.0'

local function normalizePath(path: string): string
  return util.lookupKey(util.checkString('dream.archive.normalizePath', path, 1))
end

return table.freeze {
  version = Version,
  openPath = open.openPath,
  openBytes = open.openBytes,
  detectPath = open.detectPath,
  guessFormat = open.guessFormat,
  normalizePath = normalizePath,
  ba2 = require '@self/ba2',
  bsa = require '@self/bsa',
}
