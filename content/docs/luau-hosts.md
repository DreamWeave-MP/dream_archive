+++
title = "Embedding Luau"
description = "Give scripts @dream/archive through l3i: the feature, the toolchain, composing the extension, the dreamArchive global, archives opened in Rust, type definitions, and moving from 0.2."
weight = 80

[extra]
kind = "guide"
+++

With the `luau` feature, dream_archive is an [l3i](https://github.com/DreamWeave-MP/l3i)
extension: it describes the `@dream/archive` modules, and a Rust host that runs Luau through l3i
composes it into its runtime. Scripts then open, read, extract and build archives with the same
code as Rust, byte for byte. The crate never creates a VM and never installs a global; both are the
host's decisions.

## Dependencies and toolchain

```toml
[dependencies]
dream_archive = { version = "1", features = ["luau"] }
l3i = "1"
```

`luau` turns on `ba2` and `bsa` too; `lua` is its old name. l3i builds Luau itself, and only with
clang, lld and cross-language thin LTO: its build script refuses any other configuration and names
the missing piece. Cargo does not pass a dependency's configuration on, so the host copies the
policy into its own `.cargo/config.toml`, as this crate does:

```toml
[env]
CXX = "clang++"

[target.x86_64-unknown-linux-gnu]
rustflags = ["-Clinker-plugin-lto", "-Clinker=clang", "-Clink-arg=-fuse-ld=lld"]
```

clang and rustc must use the same LLVM major version. The
[l3i toolchain notes](https://github.com/DreamWeave-MP/l3i/blob/main/TOOLCHAIN.md) have the lines
for macOS and Windows, and the measurements behind the rule. dream_archive's own
`.cargo/config.toml` is a copy of them.

## Composing the extension

`ArchiveExtension` goes into the host's `RuntimePlan`, next to whatever else the host provides.
Every runtime made from the plan can `require("@dream/archive")` and its submodules:

```rust
use dream_archive::luau::ArchiveExtension;
use l3i::Runtime;
use l3i::extension::RuntimePlan;

fn main() -> l3i::Result<()> {
    let plan = RuntimePlan::builder().extension(ArchiveExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;

    runtime.exec(r#"
        local dreamArchive = require("@dream/archive")
        local builder = dreamArchive.bsa.tes3.Builder.new()
        builder:addBytes("meshes/example.nif", "payload")
        local archive = dreamArchive.openBytes(builder:toBytes())
        assert(archive:readFileRequired([[Meshes\Example.NIF]]) == "payload")
    "#)
}
```

Scripts touch the host's file system through `openPath`, `detectPath`, the `extract*` methods,
`writePath`, `addFile`, `addDir` and `addDdsFile`, with the host process's permissions. A host
that runs scripts it does not trust should not give them this module, or should give them only
archives it opened itself.

## The dreamArchive global

Scripts written for 0.2 and earlier used a `dreamArchive` global. A host that still wants one
exposes the module as a compatibility global through its policy; the global and `require` then
return the same table:

```rust
use dream_archive::luau::{ArchiveExtension, MODULE};
use l3i::Runtime;
use l3i::extension::{RuntimePlan, RuntimePolicy};

fn main() -> l3i::Result<()> {
    let policy = RuntimePolicy::new().compat_global(MODULE, "dreamArchive");
    let plan = RuntimePlan::builder().policy(policy).extension(ArchiveExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;

    runtime.exec(r#"assert(dreamArchive == require("@dream/archive"))"#)
}
```

## Archives opened in Rust

A host can open an archive itself and hand it to scripts as the same `dream_archive_Archive` type
they get from `openPath`, wrapped in `luau::Archive`:

```rust
use dream_archive::luau::{Archive, ArchiveExtension};
use l3i::Runtime;
use l3i::extension::RuntimePlan;
use l3i::userdata::Owned;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let plan = RuntimePlan::builder().extension(ArchiveExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;

    let opened = dream_archive::Archive::open_path("Morrowind.bsa")?;
    let archive = Archive::new(opened, Some("Morrowind.bsa".into()));
    runtime.set_global("morrowind", &Owned(archive))?;

    runtime.exec(r#"assert(morrowind:contains("textures/tx_wood.dds"))"#)?;
    Ok(())
}
```

The other way, reading the Rust archive behind a script's value, and adding methods to the archive
type from another extension, are on [ArchiveExtension](@/docs/luau/extension.md#reaching-the-archive-from-rust).

## Types for editors and checks

Every function, method and field carries a Luau signature. `plan.type_definitions()` returns the
`.d.luau` text for everything in the plan, `@dream/archive` included, ready to save for an
editor's language server; [Type definitions](@/docs/luau/types.md) shows the archive's part.

```rust
use dream_archive::luau::ArchiveExtension;
use l3i::extension::RuntimePlan;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let plan = RuntimePlan::builder().extension(ArchiveExtension).finalize()?;
    std::fs::write("dream.d.luau", plan.type_definitions())?;
    Ok(())
}
```

With l3i's `analysis` feature, `plan.check_definitions()` type-checks those definitions with
Luau's own checker. This crate's tests do that, and check a `--!strict` script that requires the
module, indexes and iterates `archive:entries()`, and reads entry fields, behind the
`luau-analysis` feature (`cargo test --features luau-analysis --test luau_api`), so that a plain
test run does not build the analysis frontend.

## What scripts get

- **One archive type.** Every `open*` returns `dream_archive_Archive`; the family-specific methods
  raise on the wrong family.
- **Handles, not copies.** `archive:entries()` is a view, and an entry is an index into the
  archive's table: indexing all 4096 entries of an archive takes 0.73 ms, and nothing is copied
  until a field is read.
- **Byte strings.** Archive paths go in and come out as Luau strings holding raw bytes, UTF-8 or
  not. Host paths must be UTF-8.
- **Buffers for payloads.** Payload arguments take a `buffer` or a string, and `readInto` decodes
  straight into a buffer.
- **64-bit hashes as integers.** TES3 and TES4 hashes are exact, and compare with `==`.
- **Frozen modules.** Scripts cannot add to or replace anything in them.

The [Luau API](@/docs/luau/_index.md) lists every call.

## From 0.2

1.0.0 replaced the `mlua` binding with the l3i extension. For hosts:

- `dream_archive::lua`, its `mlua` types and the `standalone-lua` feature are gone. Compose
  `dream_archive::luau::ArchiveExtension` as above; `luau::Archive` (`archive()`, `path()`,
  `entry(i)`) replaces `lua::LuaArchive`, and other extensions add methods to the archive type
  with l3i's `augment_userdata` instead of `add_archive_methods`.
- The `lua` feature still works, as another name for `luau`.

For scripts:

- `ba2.openBytes`, `bsa.tes3.openBytes` and `bsa.tes4.openBytes`, and the `openPath` forms, return
  the same `Archive` type as the top-level `open*`, with `contains`, `archiveSize`, `info` and the
  `extractToWith*` methods on every archive of the right family.
- `archive:entries()` is a view of entry handles, not a table of tables. `#`, `[i]`, `for` and
  `:toTable()` work; `table.insert`, `ipairs` and `pairs` do not.
- Entry hashes are integers. `entry.hash` replaces TES3's `entry.hash.lo`, `hi` and `hex`, and
  TES4's `entry.fileHash`; `entry.folderHash` is an integer; the hex spellings are
  `entry.hashHex` and `entry.folderHashHex`. BA2's `entry.hash.directory`, `file` and `extension`
  are `entry.directoryHash`, `entry.fileHash` and `entry.extensionHash`.
- Sizes and offsets have one name on every family: `size`, `storedSize` and `offset`, where TES3
  had `entry.size` and `entry.offset` and TES4 `entry.storedSize` and `entry.dataOffset`.
- Integer arguments, such as indices, versions, levels and texture header fields, must be exact:
  `readEntry(1.5)` is an error instead of a rounded read.
- A host path that is not UTF-8 raises "not valid UTF-8" rather than "invalid utf-8".
