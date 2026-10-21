# Archive fixtures

The small archives from [dream_archive](https://github.com/DreamWeave-MP/dream_archive)'s own
test fixtures (`tests/fixtures/`), copied at the same relative paths so the port's suite runs
without a checkout of the crate. They came to dream_archive from the `bsa-rs` fixture corpus,
which is licensed under the BSD Zero Clause License (`0BSD`); dream_archive is `MIT OR Apache-2.0`.

The large texture fixtures (`ba2/dds`, `ba2/cubemap`, `ba2/next_gen/dx10_*`) are not copied. The
differential suite reads them from a dream_archive checkout when `JESS_ARCHIVE_FIXTURES` names its
`tests/fixtures` folder.
