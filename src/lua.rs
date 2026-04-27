//! Lua bindings for `dream_archive`.
//!
//! Enable the `lua` feature to build an `mlua` interface for embedding
//! applications that choose an `mlua` runtime at their own top level. Enable
//! `standalone-lua` only for this crate's tests, examples, and documentation
//! builds; it selects vendored `LuaJIT` with Lua 5.2 compatibility. Building this
//! crate by itself with `lua` but no `mlua` runtime selected is intentionally
//! incomplete. The API is intentionally byte-first: Lua strings are archive path
//! bytes and payload bytes. Filesystem paths are the few places where strings are
//! interpreted as UTF-8 host paths. This module creates an embedded `mlua` table;
//! it does not install a standalone `require("dream_archive")` module unless the
//! embedding application registers one.
//!
//! In the intended Lua stack, the re-exported [`crate::dream_path`] owns virtual
//! path normalization and path helper semantics, `dream_archive` owns archive
//! mechanics, and `dream_archivetool` owns filesystem/rewrite/diff/verify policy.
//! Keeping those layers separate avoids making the archive crate pretend to be
//! the application.
//!
//! # Registration
//!
//! [`create_module`] returns a Lua table. Register that table as a global, or
//! preload it yourself if you want `require("dream_archive")` to work. The crate
//! does not export a C Lua module. Pretending otherwise would be convenient and
//! false, which is the worst kind of convenient.
//!
//! ```rust,no_run
//! # fn main() -> mlua::Result<()> {
//! let lua = mlua::Lua::new();
//! lua.globals().set(
//!     "dream_path",
//!     dream_archive::dream_path::lua::create_module(&lua)?,
//! )?;
//! let archive = dream_archive::lua::create_module(&lua)?;
//! lua.globals().set("dream_archive", archive)?;
//! lua.load(r#"
//!     local b = dream_archive.ba2.Builder.new()
//!     b:add_bytes("meshes/example.nif", "payload")
//!     local archive = dream_archive.open_bytes(b:to_string())
//!     assert(archive:read_file_required("meshes/example.nif") == "payload")
//! "#).exec()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Lua module shape
//!
//! The returned table contains:
//!
//! - top-level format detection/opening helpers: `open_path`, `open_bytes`,
//!   `detect_path`, `guess_format`, and `normalize_path`;
//! - a generic archive userdata returned by `dream_archive.open_*`, with
//!   `format`, `len`, `is_empty`, `entries`, `read_file*`, `extract_file*`,
//!   `read_entry`, `extract_entry`, `extract_entry_to_path`, and `extract_to`;
//! - `dream_archive.ba2`, with BA2 hash helpers, archive metadata, GNRL builder,
//!   DX10 builder, compression names, and version constants;
//! - `dream_archive.bsa`, with filename encoding/decoding helpers,
//!   `normalize_path`, and the `tes3` / `tes4` submodules;
//! - TES3/TES4 archive userdata, hash helpers, builders, and TES4 profile,
//!   name-mode, and archive-type constants.
//!
//! # String contracts
//!
//! Lua strings are byte buffers. Archive paths and payloads are passed with
//! `LuaString::as_bytes`; non-UTF-8 archive paths are valid when the underlying
//! format accepts them. Filesystem path arguments are different: `open_path`,
//! `detect_path`, `write_path`, `extract_to*`, `extract_entry_to_path`, `add_dir`,
//! and source-file arguments to `add_file` / `add_dds_file` are converted through
//! UTF-8. If you need arbitrary Unix `OsStr` paths, use the Rust API directly.
//!
//! `bsa.encode_filename(text, encoding)` and builder `add_encoded_path(...)` are
//! also UTF-8 text boundaries: they take Unicode text and produce or insert
//! legacy-encoded archive filename bytes.
//!
//! # Absence and errors
//!
//! Optional file APIs return Lua `nil` for missing archive members:
//! `read_file(path)` and `extract_file(path)`. Required variants raise Lua errors:
//! `read_file_required(path)` and `extract_file_required(path)`. Detection helpers
//! return `nil` for unknown or too-short headers, while `open_*` functions raise
//! errors when parsing fails.
//!
//! # Allocation and extraction costs
//!
//! `entries()` materializes a Lua table of entry metadata. `read_file*`,
//! `extract_file*`, `read_entry`, and `extract_entry` materialize full payloads
//! as Lua strings. `open_bytes` copies Lua archive bytes into Rust-owned storage,
//! and builder `to_bytes()` / `to_string()` build an archive buffer and then copy
//! it into Lua. Builder `add_file` records source paths and reads payloads during
//! `write_path` / `to_bytes`, matching the Rust deferred-source API. Builders can
//! also preserve entries from an already-open archive with `add_archive_entry`;
//! the source archive userdata must stay alive through the call, and the builder
//! stores its own Rust archive handle afterwards. For large archives, prefer
//! `open_path`, `write_path`, and path-based extraction.
//!
//! `extract_entry_to_path` and `extract_to` write to the filesystem and return
//! byte counts. They create missing parent directories and write individual files
//! atomically, but archive-wide extraction is not transactional: files written
//! before a later error remain in place.
//!
//! # Hashes and names
//!
//! TES3/TES4 64-bit hashes are exposed as exact component fields plus a fixed
//! width `hex` string instead of lossy `LuaJIT` numbers. Hash-only TES4 entries use
//! `nil` for `path`, `folder`, and `name`; use `folder_hash.hex` and
//! `file_hash.hex` when exact identity is required.
//!
//! `tes4_archive:extract_to_with_paths(target, paths)` expects a contiguous Lua
//! sequence (`paths[1]..paths[n]`) of candidate archive path byte strings for
//! hash-only extraction. It is not a dictionary; non-sequence keys are ignored.

#![expect(
    clippy::needless_pass_by_value,
    reason = "mlua callback arguments are owned Lua values supplied by the VM"
)]

use crate::ByteSlice as _;
use mlua::{
    AnyUserData, Lua, Result, String as LuaString, Table, UserData, UserDataMethods, Value,
};

#[derive(Clone)]
struct LuaArchive(crate::Archive);

#[cfg(feature = "ba2")]
#[derive(Clone)]
struct LuaBa2Archive(crate::ba2::Archive);

#[cfg(feature = "bsa-tes3")]
#[derive(Clone)]
struct LuaTes3Archive(crate::bsa::tes3::Archive);

#[cfg(feature = "bsa-tes4")]
#[derive(Clone)]
struct LuaTes4Archive(crate::bsa::tes4::Archive);

#[cfg(feature = "ba2")]
#[derive(Clone, Debug, Default)]
struct LuaBa2Builder(crate::ba2::Builder);

#[cfg(feature = "ba2")]
#[derive(Clone, Debug, Default)]
struct LuaBa2Dx10Builder(crate::ba2::Dx10Builder);

#[cfg(feature = "bsa-tes3")]
#[derive(Clone, Debug, Default)]
struct LuaTes3Builder(crate::bsa::tes3::Builder);

#[cfg(feature = "bsa-tes4")]
#[derive(Clone, Debug, Default)]
struct LuaTes4Builder(crate::bsa::tes4::Builder);

/// Build the `dream_archive` Lua module table.
///
/// # Errors
///
/// Returns an `mlua` error if table/function/userdata creation fails.
pub fn create_module(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;
    module.set("open_path", lua.create_function(open_path)?)?;
    module.set("open_bytes", lua.create_function(open_bytes)?)?;
    module.set("detect_path", lua.create_function(detect_path)?)?;
    module.set("guess_format", lua.create_function(guess_format)?)?;
    module.set("normalize_path", lua.create_function(normalize_path)?)?;

    #[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
    module.set("bsa", bsa_module(lua)?)?;
    #[cfg(feature = "ba2")]
    module.set("ba2", ba2_module(lua)?)?;

    Ok(module)
}

fn open_path(_lua: &Lua, path: LuaString) -> Result<LuaArchive> {
    Ok(LuaArchive(
        crate::Archive::open_path(path.to_str()?.as_ref()).map_err(mlua::Error::external)?,
    ))
}

fn open_bytes(_lua: &Lua, bytes: LuaString) -> Result<LuaArchive> {
    Ok(LuaArchive(
        crate::Archive::from_slice(bytes.as_bytes().as_ref()).map_err(mlua::Error::external)?,
    ))
}

fn detect_path(_lua: &Lua, path: LuaString) -> Result<Option<String>> {
    match crate::detect_path(path.to_str()?.as_ref()) {
        Ok(format) => Ok(format.map(format_name).map(str::to_owned)),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(mlua::Error::external(error)),
    }
}

fn guess_format(_lua: &Lua, bytes: LuaString) -> Result<Option<String>> {
    let mut cursor = std::io::Cursor::new(bytes.as_bytes());
    match crate::guess_format(&mut cursor) {
        Ok(format) => Ok(format.map(format_name).map(str::to_owned)),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(mlua::Error::external(error)),
    }
}

fn normalize_path(lua: &Lua, path: LuaString) -> Result<LuaString> {
    let normalized = dream_path::normalize_path(path.as_bytes().as_ref());
    lua.create_string(&normalized)
}

fn format_name(format: crate::FileFormat) -> &'static str {
    match format {
        crate::FileFormat::BA2 => "ba2",
        crate::FileFormat::BSA(crate::BsaFormat::TES3) => "bsa-tes3",
        crate::FileFormat::BSA(crate::BsaFormat::TES4) => "bsa-tes4",
    }
}

fn read_optional_bytes(lua: &Lua, bytes: Option<Vec<u8>>) -> Result<Value> {
    match bytes {
        Some(bytes) => Ok(Value::String(lua.create_string(&bytes)?)),
        None => Ok(Value::Nil),
    }
}

fn collect_to_string(
    lua: &Lua,
    f: impl FnOnce(&mut Vec<u8>) -> crate::Result<u64>,
) -> Result<LuaString> {
    let mut out = Vec::new();
    f(&mut out).map_err(mlua::Error::external)?;
    lua.create_string(&out)
}

fn set_bytes_field(lua: &Lua, table: &Table, key: &str, bytes: Option<&[u8]>) -> Result<()> {
    match bytes {
        Some(bytes) => table.set(key, lua.create_string(bytes)?)?,
        None => table.set(key, Value::Nil)?,
    }
    Ok(())
}

fn compression_override(value: Option<String>) -> Result<crate::CompressionOverride> {
    match value.as_deref().unwrap_or("inherit") {
        "inherit" => Ok(crate::CompressionOverride::Inherit),
        "store" => Ok(crate::CompressionOverride::Store),
        "compress" => Ok(crate::CompressionOverride::Compress),
        other => Err(mlua::Error::external(format!(
            "unknown compression override: {other}"
        ))),
    }
}

fn entry_index(index: usize) -> Result<usize> {
    index
        .checked_sub(1)
        .ok_or_else(|| mlua::Error::external("entry index out of bounds"))
}

#[cfg(feature = "ba2")]
fn ba2_entry_id(index: usize) -> Result<crate::ba2::EntryId> {
    Ok(crate::ba2::EntryId::from_index(entry_index(index)?))
}

#[cfg(feature = "bsa-tes3")]
fn tes3_entry_id(index: usize) -> Result<crate::bsa::tes3::EntryId> {
    Ok(crate::bsa::tes3::EntryId::from_index(entry_index(index)?))
}

#[cfg(feature = "bsa-tes4")]
fn tes4_entry_id(index: usize) -> Result<crate::bsa::tes4::EntryId> {
    Ok(crate::bsa::tes4::EntryId::from_index(entry_index(index)?))
}

#[cfg(any(feature = "ba2", feature = "bsa-tes4"))]
fn zlib_level(level: u32) -> Result<flate2::Compression> {
    if level <= 9 {
        Ok(flate2::Compression::new(level))
    } else {
        Err(mlua::Error::external(format!(
            "zlib compression level must be 0..=9, got {level}"
        )))
    }
}

impl UserData for LuaArchive {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("format", |_lua, this, ()| Ok(format_name(this.0.format())));
        methods.add_method("len", |_lua, this, ()| Ok(this.0.len()));
        methods.add_method("is_empty", |_lua, this, ()| Ok(this.0.is_empty()));
        methods.add_method("entries", |lua, this, ()| {
            let entries = lua.create_table_with_capacity(this.0.len(), 0)?;
            for (index, entry) in this.0.entries().enumerate() {
                let table = lua.create_table_with_capacity(0, 3)?;
                table.set("index", index + 1)?;
                table.set("id", index + 1)?;
                table.set("format", format_name(entry.format()))?;
                set_bytes_field(lua, &table, "path", entry.path().map(AsRef::as_ref))?;
                entries.set(index + 1, table)?;
            }
            Ok(entries)
        });
        methods.add_method("read_file", |lua, this, path: LuaString| {
            read_optional_bytes(
                lua,
                this.0
                    .read_file(path.as_bytes().as_ref())
                    .map_err(mlua::Error::external)?,
            )
        });
        methods.add_method("read_file_required", |lua, this, path: LuaString| {
            let bytes = this
                .0
                .read_file_required(path.as_bytes().as_ref())
                .map_err(mlua::Error::external)?;
            lua.create_string(&bytes)
        });
        methods.add_method("extract_file", |lua, this, path: LuaString| {
            let mut out = Vec::new();
            if this
                .0
                .extract_file(path.as_bytes().as_ref(), &mut out)
                .map_err(mlua::Error::external)?
                .is_some()
            {
                Ok(Value::String(lua.create_string(&out)?))
            } else {
                Ok(Value::Nil)
            }
        });
        methods.add_method("extract_file_required", |lua, this, path: LuaString| {
            let mut out = Vec::new();
            this.0
                .extract_file_required(path.as_bytes().as_ref(), &mut out)
                .map_err(mlua::Error::external)?;
            lua.create_string(&out)
        });
        methods.add_method("read_entry", |lua, this, index: usize| {
            lua.create_string(&read_top_level_entry(&this.0, index).map_err(mlua::Error::external)?)
        });
        methods.add_method("extract_entry", |lua, this, index: usize| {
            collect_to_string(lua, |out| extract_top_level_entry(&this.0, index, out))
        });
        methods.add_method(
            "extract_entry_to_path",
            |_lua, this, (index, path): (usize, LuaString)| {
                extract_top_level_entry_to_path(&this.0, index, path.to_str()?.as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("extract_to", |_lua, this, target: LuaString| {
            this.0
                .extract_to(target.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
    }
}

fn top_level_entry_error() -> crate::Error {
    crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "entry index out of bounds",
    ))
}

fn read_top_level_entry(archive: &crate::Archive, index: usize) -> crate::Result<Vec<u8>> {
    match archive {
        #[cfg(feature = "ba2")]
        crate::Archive::BA2(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive.read_entry(entry).map_err(Into::into)
        }
        #[cfg(feature = "bsa-tes3")]
        crate::Archive::Tes3Bsa(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive.read_entry(entry).map_err(Into::into)
        }
        #[cfg(feature = "bsa-tes4")]
        crate::Archive::Tes4Bsa(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive.read_entry(entry).map_err(Into::into)
        }
    }
}

fn extract_top_level_entry(
    archive: &crate::Archive,
    index: usize,
    out: &mut Vec<u8>,
) -> crate::Result<u64> {
    match archive {
        #[cfg(feature = "ba2")]
        crate::Archive::BA2(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive.extract_entry(entry, out).map_err(Into::into)
        }
        #[cfg(feature = "bsa-tes3")]
        crate::Archive::Tes3Bsa(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive.extract_entry(entry, out).map_err(Into::into)
        }
        #[cfg(feature = "bsa-tes4")]
        crate::Archive::Tes4Bsa(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive.extract_entry(entry, out).map_err(Into::into)
        }
    }
}

fn extract_top_level_entry_to_path(
    archive: &crate::Archive,
    index: usize,
    path: &str,
) -> crate::Result<u64> {
    match archive {
        #[cfg(feature = "ba2")]
        crate::Archive::BA2(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive
                .extract_entry_to_path(entry, path)
                .map_err(Into::into)
        }
        #[cfg(feature = "bsa-tes3")]
        crate::Archive::Tes3Bsa(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive
                .extract_entry_to_path(entry, path)
                .map_err(Into::into)
        }
        #[cfg(feature = "bsa-tes4")]
        crate::Archive::Tes4Bsa(archive) => {
            let entry = archive
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(top_level_entry_error)?;
            archive
                .extract_entry_to_path(entry, path)
                .map_err(Into::into)
        }
    }
}

#[cfg(feature = "ba2")]
fn ba2_module(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;
    module.set(
        "open_path",
        lua.create_function(|_lua, path: LuaString| {
            Ok(LuaBa2Archive(
                crate::ba2::Archive::open_path(path.to_str()?.as_ref())
                    .map_err(mlua::Error::external)?,
            ))
        })?,
    )?;
    module.set(
        "open_bytes",
        lua.create_function(|_lua, bytes: LuaString| {
            Ok(LuaBa2Archive(
                crate::ba2::Archive::from_slice(bytes.as_bytes().as_ref())
                    .map_err(mlua::Error::external)?,
            ))
        })?,
    )?;
    module.set("hash_file", lua.create_function(ba2_hash_file)?)?;
    module.set("Builder", constructor_table(lua, LuaBa2Builder::default)?)?;
    module.set(
        "Dx10Builder",
        constructor_table(lua, LuaBa2Dx10Builder::default)?,
    )?;
    module.set(
        "compression",
        enum_table(lua, &["none", "store", "zip", "lz4"])?,
    )?;
    let version = lua.create_table()?;
    version.set("v1", 1)?;
    version.set("v2", 2)?;
    version.set("v3", 3)?;
    version.set("v7", 7)?;
    version.set("v8", 8)?;
    module.set("version", version)?;
    Ok(module)
}

#[cfg(feature = "ba2")]
fn ba2_hash_file(lua: &Lua, path: LuaString) -> Result<Table> {
    let (hash, normalized) = crate::ba2::hash_file(path.as_bytes().as_ref().as_bstr());
    let table = lua.create_table()?;
    table.set("directory", hash.directory)?;
    table.set("file", hash.file)?;
    table.set("extension", hash.extension)?;
    table.set("normalized", lua.create_string(normalized.as_slice())?)?;
    Ok(table)
}

#[cfg(feature = "ba2")]
impl UserData for LuaBa2Archive {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("info", ba2_archive_info);
        methods.add_method("len", |_lua, this, ()| Ok(this.0.len()));
        methods.add_method("is_empty", |_lua, this, ()| Ok(this.0.is_empty()));
        methods.add_method("archive_size", |_lua, this, ()| Ok(this.0.archive_size()));
        methods.add_method("entries", |lua, this, ()| ba2_entries(lua, this));
        methods.add_method("contains", |_lua, this, path: LuaString| {
            Ok(this.0.contains(path.as_bytes().as_ref()))
        });
        methods.add_method("read_file", |lua, this, path: LuaString| {
            read_optional_bytes(
                lua,
                this.0
                    .read_file(path.as_bytes().as_ref())
                    .map_err(mlua::Error::external)?,
            )
        });
        methods.add_method("read_file_required", |lua, this, path: LuaString| {
            lua.create_string(
                &this
                    .0
                    .read_file_required(path.as_bytes().as_ref())
                    .map_err(mlua::Error::external)?,
            )
        });
        methods.add_method("extract_file", |lua, this, path: LuaString| {
            let mut out = Vec::new();
            if this
                .0
                .extract_file(path.as_bytes().as_ref(), &mut out)
                .map_err(mlua::Error::external)?
                .is_some()
            {
                Ok(Value::String(lua.create_string(&out)?))
            } else {
                Ok(Value::Nil)
            }
        });
        methods.add_method("extract_file_required", |lua, this, path: LuaString| {
            let mut out = Vec::new();
            this.0
                .extract_file_required(path.as_bytes().as_ref(), &mut out)
                .map_err(mlua::Error::external)?;
            lua.create_string(&out)
        });
        methods.add_method("read_entry", |lua, this, index: usize| {
            let entry = this
                .0
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(|| mlua::Error::external("entry index out of bounds"))?;
            lua.create_string(&this.0.read_entry(entry).map_err(mlua::Error::external)?)
        });
        methods.add_method("extract_entry", |lua, this, index: usize| {
            let entry = this
                .0
                .entries()
                .get(index.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or_else(|| mlua::Error::external("entry index out of bounds"))?;
            collect_to_string(lua, |out| {
                this.0.extract_entry(entry, out).map_err(Into::into)
            })
        });
        methods.add_method(
            "extract_entry_to_path",
            |_lua, this, (index, path): (usize, LuaString)| {
                let entry = this
                    .0
                    .entries()
                    .get(index.checked_sub(1).unwrap_or(usize::MAX))
                    .ok_or_else(|| mlua::Error::external("entry index out of bounds"))?;
                this.0
                    .extract_entry_to_path(entry, path.to_str()?.as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("extract_to", |_lua, this, target: LuaString| {
            this.0
                .extract_to(target.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
    }
}

#[cfg(feature = "ba2")]
fn ba2_archive_info(lua: &Lua, this: &LuaBa2Archive, (): ()) -> Result<Table> {
    let info = this.0.info();
    let table = lua.create_table()?;
    table.set(
        "format",
        match info.format {
            crate::ba2::PayloadFormat::GNRL => "gnrl",
            crate::ba2::PayloadFormat::DX10 => "dx10",
            crate::ba2::PayloadFormat::GNMF => "gnmf",
        },
    )?;
    table.set("version", info.version as u32)?;
    table.set(
        "compression",
        match info.compression_format {
            crate::ba2::Ba2CompressionFormat::Zip => "zip",
            crate::ba2::Ba2CompressionFormat::LZ4 => "lz4",
        },
    )?;
    table.set("strings", info.strings)?;
    Ok(table)
}

#[cfg(feature = "ba2")]
fn ba2_entries(lua: &Lua, this: &LuaBa2Archive) -> Result<Table> {
    let entries = lua.create_table_with_capacity(this.0.len(), 0)?;
    let has_string_table = this.0.info().strings;
    for (index, entry) in this.0.entries().iter().enumerate() {
        let table = lua.create_table_with_capacity(0, 5)?;
        table.set("index", index + 1)?;
        table.set("id", index + 1)?;
        if has_string_table && !entry.name().is_empty() {
            table.set("name", lua.create_string(entry.name().as_bytes())?)?;
            table.set("path", lua.create_string(entry.name().as_bytes())?)?;
        } else {
            table.set("name", Value::Nil)?;
            table.set("path", Value::Nil)?;
        }
        let hash = lua.create_table_with_capacity(0, 3)?;
        hash.set("directory", entry.hash().directory)?;
        hash.set("file", entry.hash().file)?;
        hash.set("extension", entry.hash().extension)?;
        table.set("hash", hash)?;
        table.set("chunks", entry.file().len())?;
        entries.set(index + 1, table)?;
    }
    Ok(entries)
}

#[cfg(feature = "ba2")]
impl UserData for LuaBa2Builder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("len", |_lua, this, ()| Ok(this.0.len()));
        methods.add_method("is_empty", |_lua, this, ()| Ok(this.0.is_empty()));
        methods.add_method_mut("set_version", |_lua, this, version: u32| {
            this.0.set_version(ba2_version(version)?);
            Ok(())
        });
        methods.add_method_mut(
            "set_compression",
            |_lua, this, compression: Option<String>| {
                this.0.set_compression(ba2_compression(compression)?);
                Ok(())
            },
        );
        methods.add_method_mut("set_zlib_level", |_lua, this, level: u32| {
            this.0.set_zlib_level(zlib_level(level)?);
            Ok(())
        });
        methods.add_method_mut(
            "add_bytes",
            |_lua, this, (path, bytes): (LuaString, LuaString)| {
                this.0
                    .add_bytes(path.as_bytes().as_ref(), bytes.as_bytes().as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_bytes_with_compression",
            |_lua, this, (path, bytes, compression): (LuaString, LuaString, Option<String>)| {
                this.0
                    .add_bytes_with_compression(
                        path.as_bytes().as_ref(),
                        bytes.as_bytes().as_ref(),
                        compression_override(compression)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_file",
            |_lua, this, (archive_path, source): (LuaString, LuaString)| {
                this.0
                    .add_file(archive_path.as_bytes().as_ref(), source.to_str()?.as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_archive_entry",
            |_lua, this, (archive_path, archive, index): (LuaString, AnyUserData, usize)| {
                let archive = archive.borrow::<LuaBa2Archive>()?;
                this.0
                    .add_archive_entry(
                        archive_path.as_bytes().as_ref(),
                        std::sync::Arc::new(archive.0.clone()),
                        ba2_entry_id(index)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_archive_entry_with_compression",
            |_lua,
             this,
             (archive_path, archive, index, compression): (
                LuaString,
                AnyUserData,
                usize,
                Option<String>,
            )| {
                let archive = archive.borrow::<LuaBa2Archive>()?;
                this.0
                    .add_archive_entry_with_compression(
                        archive_path.as_bytes().as_ref(),
                        std::sync::Arc::new(archive.0.clone()),
                        ba2_entry_id(index)?,
                        compression_override(compression)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut("add_dir", |_lua, this, root: LuaString| {
            this.0
                .add_dir(root.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("write_path", |_lua, this, path: LuaString| {
            this.0
                .write_path(path.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("to_string", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
        methods.add_method("to_bytes", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
    }
}

#[cfg(feature = "ba2")]
impl UserData for LuaBa2Dx10Builder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("len", |_lua, this, ()| Ok(this.0.len()));
        methods.add_method("is_empty", |_lua, this, ()| Ok(this.0.is_empty()));
        methods.add_method_mut("set_version", |_lua, this, version: u32| {
            this.0.set_version(ba2_version(version)?);
            Ok(())
        });
        methods.add_method_mut(
            "set_compression",
            |_lua, this, compression: Option<String>| {
                this.0.set_compression(ba2_compression(compression)?);
                Ok(())
            },
        );
        methods.add_method_mut("set_zlib_level", |_lua, this, level: u32| {
            this.0.set_zlib_level(zlib_level(level)?);
            Ok(())
        });
        methods.add_method_mut(
            "add_dds_bytes",
            |_lua, this, (path, dds): (LuaString, LuaString)| {
                this.0
                    .add_dds_bytes(path.as_bytes().as_ref(), dds.as_bytes().as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_texture_bytes",
            |_lua, this, (path, header, bytes): (LuaString, Table, LuaString)| {
                this.0
                    .add_texture_bytes(
                        path.as_bytes().as_ref(),
                        texture_header(header)?,
                        bytes.as_bytes().as_ref(),
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_dds_file",
            |_lua, this, (archive_path, source): (LuaString, LuaString)| {
                this.0
                    .add_dds_file(archive_path.as_bytes().as_ref(), source.to_str()?.as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("write_path", |_lua, this, path: LuaString| {
            this.0
                .write_path(path.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("to_string", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
        methods.add_method("to_bytes", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
    }
}

#[cfg(feature = "ba2")]
fn texture_header(table: Table) -> Result<crate::ba2::TextureHeader> {
    Ok(crate::ba2::TextureHeader {
        height: table.get("height")?,
        width: table.get("width")?,
        mip_count: table.get("mip_count")?,
        format: table.get("format")?,
        flags: table.get("flags")?,
        tile_mode: table.get("tile_mode")?,
    })
}

#[cfg(feature = "ba2")]
fn ba2_version(version: u32) -> Result<crate::ba2::ArchiveVersion> {
    match version {
        1 => Ok(crate::ba2::ArchiveVersion::v1),
        2 => Ok(crate::ba2::ArchiveVersion::v2),
        3 => Ok(crate::ba2::ArchiveVersion::v3),
        7 => Ok(crate::ba2::ArchiveVersion::v7),
        8 => Ok(crate::ba2::ArchiveVersion::v8),
        _ => Err(mlua::Error::external(format!(
            "unsupported BA2 version: {version}"
        ))),
    }
}

#[cfg(feature = "ba2")]
fn ba2_compression(
    compression: Option<String>,
) -> Result<Option<crate::ba2::Ba2CompressionFormat>> {
    match compression.as_deref() {
        None | Some("none" | "store") => Ok(None),
        Some("zip") => Ok(Some(crate::ba2::Ba2CompressionFormat::Zip)),
        Some("lz4") => Ok(Some(crate::ba2::Ba2CompressionFormat::LZ4)),
        Some(other) => Err(mlua::Error::external(format!(
            "unknown BA2 compression: {other}"
        ))),
    }
}

#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
fn bsa_module(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;
    module.set("encode_filename", lua.create_function(encode_filename)?)?;
    module.set(
        "decode_filename_lossy",
        lua.create_function(decode_filename_lossy)?,
    )?;
    module.set("normalize_path", lua.create_function(normalize_path)?)?;
    module.set(
        "encoding",
        enum_table(
            lua,
            &["utf8", "windows1250", "windows1251", "windows1252", "cp437"],
        )?,
    )?;
    #[cfg(feature = "bsa-tes3")]
    module.set("tes3", tes3_module(lua)?)?;
    #[cfg(feature = "bsa-tes4")]
    module.set("tes4", tes4_module(lua)?)?;
    Ok(module)
}

#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
fn encode_filename(lua: &Lua, (path, encoding): (String, String)) -> Result<LuaString> {
    lua.create_string(
        crate::bsa::encode_filename(&path, filename_encoding(&encoding)?)
            .map_err(mlua::Error::external)?
            .as_ref(),
    )
}

#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
fn decode_filename_lossy(_lua: &Lua, (bytes, encoding): (LuaString, String)) -> Result<String> {
    Ok(
        crate::bsa::decode_filename_lossy(bytes.as_bytes().as_ref(), filename_encoding(&encoding)?)
            .into_owned(),
    )
}

#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
fn filename_encoding(encoding: &str) -> Result<crate::bsa::FilenameEncoding> {
    match encoding {
        "utf8" => Ok(crate::bsa::FilenameEncoding::Utf8),
        "windows1250" | "cp1250" => Ok(crate::bsa::FilenameEncoding::Windows1250),
        "windows1251" | "cp1251" => Ok(crate::bsa::FilenameEncoding::Windows1251),
        "windows1252" | "cp1252" => Ok(crate::bsa::FilenameEncoding::Windows1252),
        "cp437" => Ok(crate::bsa::FilenameEncoding::Cp437),
        other => Err(mlua::Error::external(format!(
            "unknown filename encoding: {other}"
        ))),
    }
}

#[cfg(feature = "bsa-tes3")]
fn tes3_module(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;
    module.set(
        "open_path",
        lua.create_function(|_lua, path: LuaString| {
            Ok(LuaTes3Archive(
                crate::bsa::tes3::Archive::open_path(path.to_str()?.as_ref())
                    .map_err(mlua::Error::external)?,
            ))
        })?,
    )?;
    module.set(
        "open_bytes",
        lua.create_function(|_lua, bytes: LuaString| {
            Ok(LuaTes3Archive(
                crate::bsa::tes3::Archive::from_slice(bytes.as_bytes().as_ref())
                    .map_err(mlua::Error::external)?,
            ))
        })?,
    )?;
    module.set("hash_file", lua.create_function(tes3_hash_file)?)?;
    module.set("Builder", constructor_table(lua, LuaTes3Builder::default)?)?;
    Ok(module)
}

#[cfg(feature = "bsa-tes3")]
fn tes3_hash_file(lua: &Lua, path: LuaString) -> Result<Table> {
    let (hash, normalized) = crate::bsa::tes3::hash_file(path.as_bytes().as_ref());
    let table = lua.create_table()?;
    table.set("lo", hash.lo)?;
    table.set("hi", hash.hi)?;
    table.set("hex", u64_hex(hash.numeric()))?;
    table.set("normalized", lua.create_string(&normalized)?)?;
    Ok(table)
}

#[cfg(feature = "bsa-tes3")]
impl UserData for LuaTes3Archive {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        bsa_archive_methods(methods);
        methods.add_method(
            "extract_to_with_encoding",
            |_lua, this, (target, encoding): (LuaString, String)| {
                this.0
                    .extract_to_with_encoding(
                        target.to_str()?.as_ref(),
                        filename_encoding(&encoding)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
    }
}

#[cfg(feature = "bsa-tes3")]
impl UserData for LuaTes3Builder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("len", |_lua, this, ()| Ok(this.0.len()));
        methods.add_method("is_empty", |_lua, this, ()| Ok(this.0.is_empty()));
        methods.add_method_mut(
            "add_bytes",
            |_lua, this, (path, bytes): (LuaString, LuaString)| {
                this.0
                    .add_bytes(path.as_bytes().as_ref(), bytes.as_bytes().as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_encoded_path",
            |_lua, this, (path, encoding, bytes): (String, String, LuaString)| {
                this.0
                    .add_encoded_path(
                        &path,
                        filename_encoding(&encoding)?,
                        bytes.as_bytes().as_ref(),
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_file",
            |_lua, this, (archive_path, source): (LuaString, LuaString)| {
                this.0
                    .add_file(archive_path.as_bytes().as_ref(), source.to_str()?.as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_archive_entry",
            |_lua, this, (archive_path, archive, index): (LuaString, AnyUserData, usize)| {
                let archive = archive.borrow::<LuaTes3Archive>()?;
                this.0
                    .add_archive_entry(
                        archive_path.as_bytes().as_ref(),
                        std::sync::Arc::new(archive.0.clone()),
                        tes3_entry_id(index)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut("add_dir", |_lua, this, root: LuaString| {
            this.0
                .add_dir(root.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("write_path", |_lua, this, path: LuaString| {
            this.0
                .write_path(path.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("to_string", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
        methods.add_method("to_bytes", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
    }
}

#[cfg(feature = "bsa-tes4")]
fn tes4_module(lua: &Lua) -> Result<Table> {
    let module = lua.create_table()?;
    module.set(
        "open_path",
        lua.create_function(|_lua, path: LuaString| {
            Ok(LuaTes4Archive(
                crate::bsa::tes4::Archive::open_path(path.to_str()?.as_ref())
                    .map_err(mlua::Error::external)?,
            ))
        })?,
    )?;
    module.set(
        "open_bytes",
        lua.create_function(|_lua, bytes: LuaString| {
            Ok(LuaTes4Archive(
                crate::bsa::tes4::Archive::from_slice(bytes.as_bytes().as_ref())
                    .map_err(mlua::Error::external)?,
            ))
        })?,
    )?;
    module.set("hash_directory", lua.create_function(tes4_hash_directory)?)?;
    module.set("hash_file", lua.create_function(tes4_hash_file)?)?;
    module.set("Builder", constructor_table(lua, LuaTes4Builder::default)?)?;
    module.set(
        "name_mode",
        enum_table(
            lua,
            &["strings", "hash_only", "embedded", "strings_and_embedded"],
        )?,
    )?;
    module.set(
        "profile",
        enum_table(
            lua,
            &[
                "oblivion",
                "fallout3",
                "fallout_new_vegas",
                "skyrim_le",
                "skyrim_se",
            ],
        )?,
    )?;
    let types = lua.create_table()?;
    types.set("meshes", crate::bsa::tes4::ArchiveTypes::MESHES.bits())?;
    types.set("textures", crate::bsa::tes4::ArchiveTypes::TEXTURES.bits())?;
    types.set("menus", crate::bsa::tes4::ArchiveTypes::MENUS.bits())?;
    types.set("sounds", crate::bsa::tes4::ArchiveTypes::SOUNDS.bits())?;
    types.set("voices", crate::bsa::tes4::ArchiveTypes::VOICES.bits())?;
    types.set("shaders", crate::bsa::tes4::ArchiveTypes::SHADERS.bits())?;
    types.set("trees", crate::bsa::tes4::ArchiveTypes::TREES.bits())?;
    types.set("fonts", crate::bsa::tes4::ArchiveTypes::FONTS.bits())?;
    types.set("misc", crate::bsa::tes4::ArchiveTypes::MISC.bits())?;
    module.set("archive_types", types)?;
    Ok(module)
}

#[cfg(feature = "bsa-tes4")]
fn tes4_hash_directory(lua: &Lua, path: LuaString) -> Result<Table> {
    hash_fields(
        lua,
        crate::bsa::tes4::hash_directory(path.as_bytes().as_ref().as_bstr()).0,
    )
}

#[cfg(feature = "bsa-tes4")]
fn tes4_hash_file(lua: &Lua, path: LuaString) -> Result<Table> {
    hash_fields(
        lua,
        crate::bsa::tes4::hash_file(path.as_bytes().as_ref().as_bstr()).0,
    )
}

#[cfg(feature = "bsa-tes4")]
fn hash_fields(lua: &Lua, hash: crate::bsa::tes4::HashFields) -> Result<Table> {
    let table = lua.create_table()?;
    table.set("last", hash.last)?;
    table.set("last2", hash.last2)?;
    table.set("length", hash.length)?;
    table.set("first", hash.first)?;
    table.set("crc", hash.crc)?;
    table.set("hex", u64_hex(hash.numeric()))?;
    Ok(table)
}

#[cfg(feature = "bsa-tes4")]
impl UserData for LuaTes4Archive {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        bsa_archive_methods(methods);
        methods.add_method(
            "extract_to_with_encoding",
            |_lua, this, (target, encoding): (LuaString, String)| {
                this.0
                    .extract_to_with_encoding(
                        target.to_str()?.as_ref(),
                        filename_encoding(&encoding)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method(
            "extract_to_with_paths",
            |_lua, this, (target, paths): (LuaString, Table)| {
                let mut owned = Vec::new();
                for value in paths.sequence_values::<LuaString>() {
                    owned.push(value?.as_bytes().to_vec());
                }
                this.0
                    .extract_to_with_paths(
                        target.to_str()?.as_ref(),
                        owned.iter().map(Vec::as_slice),
                    )
                    .map_err(mlua::Error::external)
            },
        );
    }
}

#[cfg(feature = "bsa-tes4")]
impl UserData for LuaTes4Builder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("len", |_lua, this, ()| Ok(this.0.len()));
        methods.add_method("is_empty", |_lua, this, ()| Ok(this.0.is_empty()));
        methods.add_method_mut("set_version", |_lua, this, version: u32| {
            this.0.set_version(tes4_version(version)?);
            Ok(())
        });
        methods.add_method_mut("set_profile", |_lua, this, profile: String| {
            this.0.set_profile(game_profile(&profile)?);
            Ok(())
        });
        methods.add_method_mut("set_archive_types", |_lua, this, bits: u16| {
            this.0
                .set_archive_types(crate::bsa::tes4::ArchiveTypes::from_bits_retain(bits));
            Ok(())
        });
        methods.add_method_mut("set_compressed", |_lua, this, compressed: bool| {
            this.0.set_compressed(compressed);
            Ok(())
        });
        methods.add_method_mut("set_name_mode", |_lua, this, mode: String| {
            this.0.set_name_mode(name_mode(&mode)?);
            Ok(())
        });
        methods.add_method_mut("set_zlib_level", |_lua, this, level: u32| {
            this.0.set_zlib_level(zlib_level(level)?);
            Ok(())
        });
        methods.add_method_mut(
            "add_bytes",
            |_lua, this, (path, bytes): (LuaString, LuaString)| {
                this.0
                    .add_bytes(path.as_bytes().as_ref(), bytes.as_bytes().as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_bytes_with_compression",
            |_lua, this, (path, bytes, compression): (LuaString, LuaString, Option<String>)| {
                this.0
                    .add_bytes_with_compression(
                        path.as_bytes().as_ref(),
                        bytes.as_bytes().as_ref(),
                        compression_override(compression)?,
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_encoded_path",
            |_lua, this, (path, encoding, bytes): (String, String, LuaString)| {
                this.0
                    .add_encoded_path(
                        &path,
                        filename_encoding(&encoding)?,
                        bytes.as_bytes().as_ref(),
                    )
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method_mut(
            "add_file",
            |_lua, this, (archive_path, source): (LuaString, LuaString)| {
                this.0
                    .add_file(archive_path.as_bytes().as_ref(), source.to_str()?.as_ref())
                    .map_err(mlua::Error::external)
            },
        );
        tes4_builder_archive_entry_methods(methods);
        methods.add_method_mut("add_dir", |_lua, this, root: LuaString| {
            this.0
                .add_dir(root.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("write_path", |_lua, this, path: LuaString| {
            this.0
                .write_path(path.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        });
        methods.add_method("to_string", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
        methods.add_method("to_bytes", |lua, this, ()| {
            lua.create_string(&this.0.to_vec().map_err(mlua::Error::external)?)
        });
    }
}

#[cfg(feature = "bsa-tes4")]
fn tes4_builder_archive_entry_methods<M>(methods: &mut M)
where
    M: UserDataMethods<LuaTes4Builder>,
{
    methods.add_method_mut(
        "add_archive_entry",
        |_lua, this, (archive_path, archive, index): (LuaString, AnyUserData, usize)| {
            let archive = archive.borrow::<LuaTes4Archive>()?;
            this.0
                .add_archive_entry(
                    archive_path.as_bytes().as_ref(),
                    std::sync::Arc::new(archive.0.clone()),
                    tes4_entry_id(index)?,
                )
                .map_err(mlua::Error::external)
        },
    );
    methods.add_method_mut(
        "add_archive_entry_with_compression",
        |_lua,
         this,
         (archive_path, archive, index, compression): (
            LuaString,
            AnyUserData,
            usize,
            Option<String>,
        )| {
            let archive = archive.borrow::<LuaTes4Archive>()?;
            this.0
                .add_archive_entry_with_compression(
                    archive_path.as_bytes().as_ref(),
                    std::sync::Arc::new(archive.0.clone()),
                    tes4_entry_id(index)?,
                    compression_override(compression)?,
                )
                .map_err(mlua::Error::external)
        },
    );
}

#[cfg(feature = "bsa-tes4")]
fn tes4_version(version: u32) -> Result<crate::bsa::tes4::ArchiveVersion> {
    match version {
        103 => Ok(crate::bsa::tes4::ArchiveVersion::v103),
        104 => Ok(crate::bsa::tes4::ArchiveVersion::v104),
        105 => Ok(crate::bsa::tes4::ArchiveVersion::v105),
        _ => Err(mlua::Error::external(format!(
            "unsupported TES4 BSA version: {version}"
        ))),
    }
}

#[cfg(feature = "bsa-tes4")]
fn name_mode(mode: &str) -> Result<crate::bsa::tes4::NameMode> {
    match mode {
        "strings" => Ok(crate::bsa::tes4::NameMode::Strings),
        "hash_only" => Ok(crate::bsa::tes4::NameMode::HashOnly),
        "embedded" => Ok(crate::bsa::tes4::NameMode::Embedded),
        "strings_and_embedded" => Ok(crate::bsa::tes4::NameMode::StringsAndEmbedded),
        other => Err(mlua::Error::external(format!(
            "unknown TES4 name mode: {other}"
        ))),
    }
}

#[cfg(feature = "bsa-tes4")]
fn game_profile(profile: &str) -> Result<crate::bsa::tes4::GameProfile> {
    match profile {
        "oblivion" => Ok(crate::bsa::tes4::GameProfile::Oblivion),
        "fallout3" => Ok(crate::bsa::tes4::GameProfile::Fallout3),
        "fallout_new_vegas" => Ok(crate::bsa::tes4::GameProfile::FalloutNewVegas),
        "skyrim_le" => Ok(crate::bsa::tes4::GameProfile::SkyrimLe),
        "skyrim_se" => Ok(crate::bsa::tes4::GameProfile::SkyrimSe),
        other => Err(mlua::Error::external(format!(
            "unknown TES4 profile: {other}"
        ))),
    }
}

trait BsaArchiveAccess {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn entries_table(&self, lua: &Lua) -> Result<Table>;
    fn contains(&self, path: &[u8]) -> bool;
    fn archive_size(&self) -> usize;
    fn read_entry_index(&self, index: usize) -> std::result::Result<Vec<u8>, crate::bsa::Error>;
    fn extract_entry_index(
        &self,
        index: usize,
        out: &mut Vec<u8>,
    ) -> std::result::Result<u64, crate::bsa::Error>;
    fn extract_entry_index_to_path(
        &self,
        index: usize,
        path: &str,
    ) -> std::result::Result<u64, crate::bsa::Error>;
    fn read_file(&self, path: &[u8]) -> std::result::Result<Option<Vec<u8>>, crate::bsa::Error>;
    fn read_file_required(&self, path: &[u8]) -> std::result::Result<Vec<u8>, crate::bsa::Error>;
    fn extract_file(
        &self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> std::result::Result<Option<u64>, crate::bsa::Error>;
    fn extract_file_required(
        &self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> std::result::Result<u64, crate::bsa::Error>;
    fn extract_to(&self, target: &str) -> std::result::Result<u64, crate::bsa::Error>;
}

#[cfg(feature = "bsa-tes3")]
impl BsaArchiveAccess for LuaTes3Archive {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn entries_table(&self, lua: &Lua) -> Result<Table> {
        let entries = lua.create_table_with_capacity(self.0.len(), 0)?;
        for (index, entry) in self.0.entries().iter().enumerate() {
            let table = lua.create_table_with_capacity(0, 5)?;
            table.set("index", index + 1)?;
            table.set("id", index + 1)?;
            table.set("path", lua.create_string(entry.path().as_bytes())?)?;
            table.set("hash", tes3_entry_hash(lua, entry.hash())?)?;
            table.set("size", entry.file().size)?;
            table.set("offset", entry.file().offset)?;
            entries.set(index + 1, table)?;
        }
        Ok(entries)
    }
    fn contains(&self, path: &[u8]) -> bool {
        self.0.contains(path)
    }
    fn archive_size(&self) -> usize {
        self.0.archive_size()
    }
    fn read_entry_index(&self, index: usize) -> std::result::Result<Vec<u8>, crate::bsa::Error> {
        let entry = self
            .0
            .entries()
            .get(index.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(crate::bsa::Error::OutOfBounds)?;
        self.0.read_entry(entry)
    }
    fn extract_entry_index(
        &self,
        index: usize,
        out: &mut Vec<u8>,
    ) -> std::result::Result<u64, crate::bsa::Error> {
        let entry = self
            .0
            .entries()
            .get(index.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(crate::bsa::Error::OutOfBounds)?;
        self.0.extract_entry(entry, out)
    }
    fn extract_entry_index_to_path(
        &self,
        index: usize,
        path: &str,
    ) -> std::result::Result<u64, crate::bsa::Error> {
        let entry = self
            .0
            .entries()
            .get(index.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(crate::bsa::Error::OutOfBounds)?;
        self.0.extract_entry_to_path(entry, path)
    }
    fn read_file(&self, path: &[u8]) -> std::result::Result<Option<Vec<u8>>, crate::bsa::Error> {
        self.0.read_file(path)
    }
    fn read_file_required(&self, path: &[u8]) -> std::result::Result<Vec<u8>, crate::bsa::Error> {
        self.0.read_file_required(path)
    }
    fn extract_file(
        &self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> std::result::Result<Option<u64>, crate::bsa::Error> {
        self.0.extract_file(path, out)
    }
    fn extract_file_required(
        &self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> std::result::Result<u64, crate::bsa::Error> {
        self.0.extract_file_required(path, out)
    }
    fn extract_to(&self, target: &str) -> std::result::Result<u64, crate::bsa::Error> {
        self.0.extract_to(target)
    }
}

#[cfg(feature = "bsa-tes4")]
impl BsaArchiveAccess for LuaTes4Archive {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn entries_table(&self, lua: &Lua) -> Result<Table> {
        let entries = lua.create_table_with_capacity(self.0.len(), 0)?;
        for (index, entry) in self.0.entries().iter().enumerate() {
            let table = lua.create_table_with_capacity(0, 8)?;
            table.set("index", index + 1)?;
            table.set("id", index + 1)?;
            set_bytes_field(lua, &table, "path", entry.path().map(AsRef::as_ref))?;
            set_bytes_field(lua, &table, "folder", entry.folder().map(AsRef::as_ref))?;
            set_bytes_field(lua, &table, "name", entry.name().map(AsRef::as_ref))?;
            table.set("folder_hash", hash_fields(lua, entry.folder_hash())?)?;
            table.set("file_hash", hash_fields(lua, entry.file_hash())?)?;
            table.set("stored_size", entry.file().stored_size)?;
            table.set("data_offset", entry.file().data_offset)?;
            entries.set(index + 1, table)?;
        }
        Ok(entries)
    }
    fn contains(&self, path: &[u8]) -> bool {
        self.0.contains(path)
    }
    fn archive_size(&self) -> usize {
        self.0.archive_size()
    }
    fn read_entry_index(&self, index: usize) -> std::result::Result<Vec<u8>, crate::bsa::Error> {
        let entry = self
            .0
            .entries()
            .get(index.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(crate::bsa::Error::OutOfBounds)?;
        self.0.read_entry(entry)
    }
    fn extract_entry_index(
        &self,
        index: usize,
        out: &mut Vec<u8>,
    ) -> std::result::Result<u64, crate::bsa::Error> {
        let entry = self
            .0
            .entries()
            .get(index.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(crate::bsa::Error::OutOfBounds)?;
        self.0.extract_entry(entry, out)
    }
    fn extract_entry_index_to_path(
        &self,
        index: usize,
        path: &str,
    ) -> std::result::Result<u64, crate::bsa::Error> {
        let entry = self
            .0
            .entries()
            .get(index.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(crate::bsa::Error::OutOfBounds)?;
        self.0.extract_entry_to_path(entry, path)
    }
    fn read_file(&self, path: &[u8]) -> std::result::Result<Option<Vec<u8>>, crate::bsa::Error> {
        self.0.read_file(path)
    }
    fn read_file_required(&self, path: &[u8]) -> std::result::Result<Vec<u8>, crate::bsa::Error> {
        self.0.read_file_required(path)
    }
    fn extract_file(
        &self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> std::result::Result<Option<u64>, crate::bsa::Error> {
        self.0.extract_file(path, out)
    }
    fn extract_file_required(
        &self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> std::result::Result<u64, crate::bsa::Error> {
        self.0.extract_file_required(path, out)
    }
    fn extract_to(&self, target: &str) -> std::result::Result<u64, crate::bsa::Error> {
        self.0.extract_to(target)
    }
}

fn bsa_archive_methods<T, M>(methods: &mut M)
where
    T: BsaArchiveAccess + UserData + 'static,
    M: UserDataMethods<T>,
{
    methods.add_method("len", |_lua, this, ()| Ok(this.len()));
    methods.add_method("is_empty", |_lua, this, ()| Ok(this.is_empty()));
    methods.add_method("archive_size", |_lua, this, ()| Ok(this.archive_size()));
    methods.add_method("entries", |lua, this, ()| this.entries_table(lua));
    methods.add_method("contains", |_lua, this, path: LuaString| {
        Ok(this.contains(path.as_bytes().as_ref()))
    });
    methods.add_method("read_entry", |lua, this, index: usize| {
        lua.create_string(
            &this
                .read_entry_index(index)
                .map_err(mlua::Error::external)?,
        )
    });
    methods.add_method("extract_entry", |lua, this, index: usize| {
        let mut out = Vec::new();
        this.extract_entry_index(index, &mut out)
            .map_err(mlua::Error::external)?;
        lua.create_string(&out)
    });
    methods.add_method(
        "extract_entry_to_path",
        |_lua, this, (index, path): (usize, LuaString)| {
            this.extract_entry_index_to_path(index, path.to_str()?.as_ref())
                .map_err(mlua::Error::external)
        },
    );
    methods.add_method("read_file", |lua, this, path: LuaString| {
        read_optional_bytes(
            lua,
            this.read_file(path.as_bytes().as_ref())
                .map_err(mlua::Error::external)?,
        )
    });
    methods.add_method("extract_file", |lua, this, path: LuaString| {
        let mut out = Vec::new();
        if this
            .extract_file(path.as_bytes().as_ref(), &mut out)
            .map_err(mlua::Error::external)?
            .is_some()
        {
            Ok(Value::String(lua.create_string(&out)?))
        } else {
            Ok(Value::Nil)
        }
    });
    methods.add_method("extract_file_required", |lua, this, path: LuaString| {
        let mut out = Vec::new();
        this.extract_file_required(path.as_bytes().as_ref(), &mut out)
            .map_err(mlua::Error::external)?;
        lua.create_string(&out)
    });
    methods.add_method("read_file_required", |lua, this, path: LuaString| {
        lua.create_string(
            &this
                .read_file_required(path.as_bytes().as_ref())
                .map_err(mlua::Error::external)?,
        )
    });
    methods.add_method("extract_to", |_lua, this, target: LuaString| {
        this.extract_to(target.to_str()?.as_ref())
            .map_err(mlua::Error::external)
    });
}

fn constructor_table<T>(lua: &Lua, new: fn() -> T) -> Result<Table>
where
    T: UserData + 'static,
{
    let table = lua.create_table()?;
    table.set("new", lua.create_function(move |_lua, ()| Ok(new()))?)?;
    Ok(table)
}

fn enum_table(lua: &Lua, values: &[&str]) -> Result<Table> {
    let table = lua.create_table()?;
    for value in values {
        table.set(*value, *value)?;
    }
    Ok(table)
}

fn u64_hex(value: u64) -> String {
    format!("{value:016x}")
}

#[cfg(feature = "bsa-tes3")]
fn tes3_entry_hash(lua: &Lua, hash: u64) -> Result<Table> {
    let table = lua.create_table()?;
    let lo = u32::try_from(hash >> 32).expect("upper 32 bits fit in u32 after shifting");
    let hi = u32::try_from(hash & u64::from(u32::MAX)).expect("masked lower 32 bits fit in u32");
    table.set("lo", lo)?;
    table.set("hi", hi)?;
    table.set("hex", u64_hex(hash))?;
    Ok(table)
}
