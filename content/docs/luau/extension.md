+++
title = "ArchiveExtension"
description = "The Rust side of the Luau binding: the l3i extension that provides @dream/archive, its constants, and the Archive, Entry and Entries types a host can reach."
weight = 50

[extra]
kind = "api"
+++

Rust, in the `luau` module, behind the `luau` feature. [Embedding Luau](@/docs/luau-hosts.md)
shows it in a host.

## ArchiveExtension

{{ api_signature(value="struct ArchiveExtension") }}

The `dream.archive` extension: an `l3i::extension::Extension` that describes the five
`@dream/archive` modules, the archive and entry types, the entries view and the four builders,
with a Luau signature on every member. It holds no state; `Clone`, `Copy`, `Debug`, `Default`.
Add it to a plan with `RuntimePlan::builder().extension(ArchiveExtension)`.

The archive and entry types ask for userdata tags, which make their methods and fields faster to
reach; when a plan has run out of tags they work untagged, with the same behavior. The builders
and the entries view never take one. The extension never creates a runtime or installs a global.

## Constants

{{ api_signature(value='const EXTENSION_ID: &str = "dream.archive"') }}

The extension's id in the plan, for another extension's `requires`.

{{ api_signature(value='const MODULE: &str = "@dream/archive"') }}

{{ api_signature(value='const BA2_MODULE: &str = "@dream/archive/ba2"') }}

{{ api_signature(value='const BSA_MODULE: &str = "@dream/archive/bsa"') }}

{{ api_signature(value='const TES3_MODULE: &str = "@dream/archive/bsa/tes3"') }}

{{ api_signature(value='const TES4_MODULE: &str = "@dream/archive/bsa/tes4"') }}

The paths scripts `require`. A host that wants the conventional global adds
`RuntimePolicy::new().compat_global(MODULE, "dreamArchive")`.

{{ api_signature(value='const ARCHIVE_KEY: &str = "dream.archive.Archive"') }}

{{ api_signature(value='const ENTRY_KEY: &str = "dream.archive.Entry"') }}

{{ api_signature(value='const ENTRIES_KEY: &str = "dream.archive.Entries"') }}

The userdata keys of the archive, entry and entries types, for a plan's `tag_of` and for another
extension's `augment_userdata`.

## Archive

{{ api_signature(value="struct Archive") }}

An opened archive as scripts hold it: the Rust type behind `dream_archive_Archive`. `Clone`,
cheaply: clones, entry handles and the entries view share one parsed archive.

{{ api_signature(value="fn new(archive: dream_archive::Archive, path: Option<PathBuf>) -> Archive") }}

Wraps an archive opened in Rust, to hand to a script; `path` is the host path it came from, if
any, and shows in `tostring(archive)`.

{{ api_signature(value="fn archive(&self) -> &dream_archive::Archive") }}

{{ api_signature(value="fn path(&self) -> Option<&Path>") }}

The archive, and the host path it was opened from; `None` for one opened from bytes.

{{ api_signature(value="fn entry(&self, index: usize) -> Option<Entry>") }}

The entry handle at a 0-based index, or `None` past the end.

## Entry

{{ api_signature(value="struct Entry") }}

An entry handle, the Rust type behind `dream_archive_Entry`: the archive and an index. `Clone`.

{{ api_signature(value="fn index(&self) -> usize") }}

{{ api_signature(value="fn archive(&self) -> Archive") }}

{{ api_signature(value="fn facade(&self) -> dream_archive::Entry<'_>") }}

Its 0-based index, the archive it belongs to, and the entry as the Rust
[facade](@/docs/api/facade.md#entry) sees it.

## Entries

{{ api_signature(value="struct Entries") }}

The entries view behind `dream_archive_Entries`, an `l3i::sequence::SequenceSource` of entry
handles. Scripts get one from `archive:entries()`; nothing else makes one.

## Reaching the archive from Rust

A host can read the Rust archive behind a script's value:

```rust
use dream_archive::luau::{Archive, ArchiveExtension};
use l3i::Runtime;
use l3i::extension::RuntimePlan;

fn main() -> l3i::Result<()> {
    let plan = RuntimePlan::builder().extension(ArchiveExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;
    runtime.exec(r#"opened = require("@dream/archive").openPath("Morrowind.bsa")"#)?;

    runtime.stack().with_frame(|frame| {
        let value = l3i::value::Value::get_global(frame, "opened")?.push_to(frame)?;
        let archive = l3i::userdata::check_receiver::<Archive>(value)?;
        println!("{:?}, {} entries", archive.archive().format(), archive.archive().len());
        Ok(())
    })
}
```

And another extension can add methods to the archive type, without wrapping it:

```rust
use dream_archive::luau::{ARCHIVE_KEY, Archive, ArchiveExtension, EXTENSION_ID};
use l3i::Runtime;
use l3i::extension::{Extension, ExtensionDescriptor, RuntimePlan};

struct Inventory;

impl Extension for Inventory {
    fn id(&self) -> &'static str {
        "my.inventory"
    }

    fn describe(&self, d: &mut ExtensionDescriptor) -> l3i::Result<()> {
        d.requires(EXTENSION_ID);
        d.augment_userdata::<Archive>(ARCHIVE_KEY)
            .method("openedFromDisk", |archive: &Archive| archive.path().is_some())
            .signature("(self): boolean");
        Ok(())
    }
}

fn main() -> l3i::Result<()> {
    let plan = RuntimePlan::builder().extension(Inventory).extension(ArchiveExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;
    runtime.exec(r#"
        local archive = require("@dream/archive").openPath("Morrowind.bsa")
        assert(archive:openedFromDisk())
    "#)
}
```
