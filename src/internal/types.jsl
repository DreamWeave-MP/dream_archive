--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- The types every part of the archive port shares.

export type Format = 'ba2' | 'bsaTes3' | 'bsaTes4'

export type Reader = dream_fs_Reader

--- Where an archive's bytes are: the whole archive in a buffer, or a host file read on demand.
--- `head` caches the leading bytes a file-backed archive's index was parsed from.
export type Store = {
  size: number,
  data: buffer?,
  reader: Reader?,
  path: string?,
  head: buffer?,
  headLength: number,
  position: number,
}

--- One entry of an opened archive: a plain table whose fields are computed on first read and
--- kept. `index` and `id` are the 1-based position in archive table order.
export type Entry = {
  read index: number,
  read id: number,
  read format: Format,
  read path: string?,
  read name: string?,
  read folder: string?,
  read size: number?,
  read storedSize: number,
  read offset: number?,
  read compressed: boolean,
  read chunks: number?,
  read hash: integer?,
  read hashHex: string?,
  read folderHash: integer?,
  read folderHashHex: string?,
  read directoryHash: number?,
  read fileHash: number?,
  read extensionHash: number?,
}

--- What a format gives the generic archive: its entries by 1-based index, its lookups, and its
--- payloads. Every function raises the native crate's messages.
export type Backend = {
  format: Format,
  count: number,
  store: Store,
  lookup: (path: string) -> number?,
  hashLookup: (first: unknown, second: unknown, third: unknown) -> number?,
  path: (index: number) -> string?,
  name: (index: number) -> string?,
  folder: (index: number) -> string?,
  --- The decoded size, or nil and the crate's message when the format can't tell.
  size: (index: number) -> (number?, string?),
  storedSize: (index: number) -> number,
  offset: (index: number) -> number?,
  compressed: (index: number) -> boolean,
  chunks: (index: number) -> number?,
  hash: (index: number) -> integer?,
  folderHash: (index: number) -> integer?,
  ba2Hash: (index: number) -> { directory: number, file: number, extension: number }?,
  decode: (index: number) -> buffer,
  decodeString: (index: number) -> string,
  decodeInto: (index: number, target: buffer, offset: number) -> number,
  info: (() -> BA2Info)?,
  --- The entry table for an index, with the fields that are cheap to read already in it; the
  --- rest are computed on first read.
  row: (index: number) -> { [string]: unknown },
  --- BA2: the header facts the DX10 builder checks before it copies chunks verbatim, and the raw
  --- texture records it copies.
  ba2: BA2Facts?,
  --- TES4: the header facts the archive tool reports and a rewrite keeps.
  tes4: Tes4Facts?,
}

export type Tes4Facts = {
  version: number,
  flags: number,
  archiveTypes: number,
}

export type Chunk = {
  offset: number,
  packedSize: number,
  size: number,
  firstMip: number,
  lastMip: number,
}

export type BA2Facts = {
  format: string,
  version: number,
  compression: string,
  texture: (index: number) -> TextureHeader?,
  chunkList: (index: number) -> { Chunk },
}

export type BA2Info = {
  format: string,
  version: number,
  compression: string,
  strings: boolean,
}

--- The entries of an archive: a frozen array of entry tables, with `toTable` for a copy.
export type Entries = { Entry }

export type Archive = {
  format: (self: Archive) -> Format,
  len: (self: Archive) -> number,
  isEmpty: (self: Archive) -> boolean,
  archiveSize: (self: Archive) -> number,
  entries: (self: Archive) -> Entries,
  entry: (self: Archive, index: number) -> Entry?,
  contains: (self: Archive, path: string) -> boolean,
  get: (self: Archive, path: string) -> Entry?,
  containsHash: (self: Archive, first: unknown, second: unknown?, third: unknown?) -> boolean,
  getByHash: (self: Archive, first: unknown, second: unknown?, third: unknown?) -> Entry?,
  readFile: (self: Archive, path: string) -> string?,
  readFileRequired: (self: Archive, path: string) -> string,
  extractFile: (self: Archive, path: string) -> string?,
  extractFileRequired: (self: Archive, path: string) -> string,
  readEntry: (self: Archive, index: number) -> string,
  extractEntry: (self: Archive, index: number) -> string,
  extractEntryToPath: (self: Archive, index: number, path: string) -> number,
  readInto: (self: Archive, target: Entry | string, into: buffer, offset: number?) -> number,
  extractTo: (self: Archive, target: string) -> number,
  extractToWithEncoding: (self: Archive, target: string, encoding: string) -> number,
  extractToWithPaths: (self: Archive, target: string, paths: { string }) -> number,
  info: (self: Archive) -> BA2Info,
}

--- What a builder holds for one member until it is written.
export type Source = {
  kind: 'bytes' | 'file' | 'archive',
  bytes: (buffer | string)?,
  path: string?,
  backend: Backend?,
  index: number?,
  length: number,
}

export type TextureHeader = {
  height: number,
  width: number,
  mipCount: number,
  format: number,
  flags: number,
  tileMode: number,
}

return table.freeze {}
