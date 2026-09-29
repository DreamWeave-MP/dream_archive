# Notes for whoever works on dream_archive next

The bugs found on 2026-09-29 while documenting the crate are fixed. What is left is open for the
reason given with it.

## l3i is a path dependency

`l3i = { version = "1.0.0", path = "../dream-binder" }`. CI has no sibling checkout, so any job
that resolves the manifest fails, and 1.0.0 cannot be published to crates.io until l3i 1.0.0 is
on crates.io. Open because it waits on l3i's release, not on this crate.

## Building with only the `ba2` feature fails

Found while fixing the notes above, and not part of that work, so it is left for its own change.

```sh
cargo build --no-default-features --features ba2
# error[E0603]: module `compress` is private
#   --> src/ba2/builder.rs: lz4_flex::block::compress(&bytes)
```

`ba2` enables `lz4_flex` with default features off, and `lz4_flex::block::compress` is only public
when one of its features (`std` or `alloc`) is on. Only `bsa-tes4` turns one on, through
`lz4_flex/frame`, so the default and all-features builds hide it. The `ba2` feature should enable
the `lz4_flex` feature it needs. The single-family builds also warn about dead code
(`stream::ChainReader` with `bsa-tes4` alone; that and `stream::owned_reader` and
`read::Cursor::u64` with `bsa-tes3` alone).
