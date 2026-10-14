--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- @dream-archive/bsa/tes4: TES4-family (Oblivion to Skyrim SE) BSAs, as
-- `@dream/archive/bsa/tes4` has them.
local builder = require '../internal/tes4_builder'
local open = require '../internal/open'
local tes4 = require '../internal/tes4'
local types = require '../internal/types'
local util = require '../internal/util'

type Archive = types.Archive
export type Hash = {
  last: number,
  last2: number,
  length: number,
  first: number,
  crc: number,
  hex: string,
  hash: integer,
}

local function openPath(path: string): Archive
  return open.openPathAs('bsaTes4', 'dream.archive.openPath', path)
end

local function openBytes(data: buffer | string): Archive
  return open.openBytesAs('bsaTes4', 'dream.archive.openBytes', data)
end

local function hashDirectory(path: string): Hash
  return tes4.described(
    tes4.hashDirectory(util.checkString('dream.archive.hashDirectory', path, 1))
  )
end

local function hashFile(path: string): Hash
  return tes4.described(tes4.hashFile(util.checkString('dream.archive.hashFile', path, 1)))
end

return table.freeze {
  openPath = openPath,
  openBytes = openBytes,
  hashDirectory = hashDirectory,
  hashFile = hashFile,
  Builder = table.freeze { new = builder.new },
  nameMode = table.freeze {
    strings = 'strings',
    hashOnly = 'hashOnly',
    embedded = 'embedded',
    stringsAndEmbedded = 'stringsAndEmbedded',
  },

  profile = table.freeze {
    oblivion = 'oblivion',
    fallout3 = 'fallout3',
    falloutNewVegas = 'falloutNewVegas',
    skyrimLe = 'skyrimLe',
    skyrimSe = 'skyrimSe',
  },

  archiveTypes = table.freeze {
    MESHES = 0x1,
    TEXTURES = 0x2,
    MENUS = 0x4,
    SOUNDS = 0x8,
    VOICES = 0x10,
    SHADERS = 0x20,
    TREES = 0x40,
    FONTS = 0x80,
    MISC = 0x100,
  },
}
