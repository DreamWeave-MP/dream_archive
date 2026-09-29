# Notes for whoever works on dream_archive next

The bugs found on 2026-09-29 while documenting the crate are fixed. What is left is open for the
reason given with it.

## l3i is a path dependency

`l3i = { version = "1.0.0", path = "../dream-binder" }`. CI has no sibling checkout, so any job
that resolves the manifest fails, and 1.0.0 cannot be published to crates.io until l3i 1.0.0 is
on crates.io. Open because it waits on l3i's release, not on this crate.
