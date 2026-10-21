# dream_archive

Read, extract and write Bethesda BSA and BA2 archives, from Morrowind to Starfield, as a Jess
package in Luau: `org.dreamweave.dream-archive`, module `@dream-archive`.

```toml
[dependencies]
"org.dreamweave.dream-archive" = "2"
```

The site at <https://dreamweave-mp.github.io/dream_archive/> is this repository, built by the
DreamWeave Mod Template through jess on every push; every release is a JPK on this repository's
GitHub Releases, and `jess workspace sync` takes it from there.

The Rust crate `dream_archive`, versions 1.1.0 and below, lives on the `rust` branch and stays on
crates.io. It is no longer developed here.

## Developing

`jess workspace test` runs the package's suites. `jess workspace run --trusted dreamweave-check`
validates the site; `dreamweave-site -- public <url>` renders it. Releases are tags
`dream_archive-<version>`, matching `version` in `jess.toml` and a `[[releases]]` entry in
`content/home/mod.toml`.
