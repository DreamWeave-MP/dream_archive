+++
title = "dream_archive"
description = "Read, extract and write Bethesda BSA and BA2 archives, from Morrowind to Starfield, as a Jess package in Luau."

[taxonomies]
tags = ["Jess", "Luau", "BSA", "BA2", "Morrowind", "OpenMW"]

[extra]
sections = ["overview", "releases", "credits"]
+++

Every Bethesda game since Morrowind keeps its meshes, textures and sounds in archives: BSA from
Morrowind to Skyrim, BA2 from Fallout 4 on. Each generation changed the layout, the hashes and the
compression, and a tool that reads one of them usually reads none of the others.

dream_archive reads all three generations through one type. `Archive::open_path` reads the header,
and the same calls then list, look up, read, stream and extract a Morrowind BSA, a Skyrim SE BSA or
a Fallout 4 BA2. Where a format really is different, its own module has the rest: hash-only TES4
archives, BA2 texture metadata, each builder's settings. It writes every family it reads, from
bytes, files, directories, or entries of another archive.

It is Luau all the way down, a Jess package with no native code. Archive paths stay bytes: the
package never guesses a code page, and it tells you when a path cannot be stored or extracted safely
instead of writing it somewhere else.

{{ schematic(data_path="data/schematics/archives.json") }}

```lua
local dreamArchive = require '@dream-archive'

local archive = dreamArchive.openPath 'Morrowind.bsa'
print(archive:format())

for _, entry in archive:entries() do
  if entry.path == [[meshes\m\probe_journeyman_01.nif]] then
    print(entry.index, entry.size)
  end
end

local texture = archive:readFileRequired [[Textures\Tx_Wood.DDS]]
print(#texture, 'bytes')
```

## What it covers

| Family | Games | Read and extract | Write |
|---|---|---|---|
| TES3 BSA | Morrowind | yes | yes |
| TES4 BSA, versions 103 to 105 | Oblivion, Fallout 3, New Vegas, Skyrim, Skyrim SE | yes, including hash-only archives | yes |
| BA2 GNRL | Fallout 4, Fallout 76, Starfield | yes | yes |
| BA2 DX10 | Fallout 4, Fallout 76, Starfield | yes, as DDS files | from DDS files or texture metadata |
| BA2 GNMF | console textures | metadata only | no |

Console layouts and XMem compression are refused with an error.

## Documentation

The package's reference is `src/init.jsl` and the modules beside it: every function is documented
where it is written, and `jess luau-types` prints the module's types. The guide that explained the
Rust crate lives with it on the `rust` branch; the Luau package's own guide is being written.

Want a command line instead of a library?
[dream_archivetool](https://DreamWeave-MP.github.io/dream_archivetool/) lists, verifies, diffs,
extracts, creates and updates archives, built on this package.
