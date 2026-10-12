# Archive benchmarks

`bench/bench.luau` times the package through its public API: opening and indexing, lookups by
path and by hash, listing, reads, extraction, and building, over synthetic archives of every
family. Each workload is warmed up, sized to a batch of about a tenth of a second, and reported
as the median of seven batches, with a full collection between them. Every workload's result is
checked: counts and byte totals against what the inputs hold, stored builds against a pinned
SHA-256, compressed builds by reading members back.

```sh
jess run --package jpk/dream-archive --package-path jpk \
  --capability filesystem.read --capability filesystem.write \
  jpk/dream-archive/bench/bench.luau <scratch folder> [filter] [real archive]...
```

## The move onto l3i 2.0, base against head

- Machine: Intel Core i7-10870H @ 2.20 GHz, 16 threads, shared with other work while this ran.
- Date: 2026-10-12.
- Binary: the verified release build of Jess 0.1.0 on l3i 2.0.1 (Luau 0.742), the same one for
  both sides.
- Base: `33f6b95`, the package before the move, with this bench. Head: `987549d`.
- Protocol: the whole bench run against base, then head, three times over, back to back under one
  lock; each cell is the median of its three runs. A difference under 5 percent is called no
  change.

| Workload | Base, µs | Head, µs | Base / head |
|---|---:|---:|---:|
| tes3 4000 stored (bytes): open + index † | 2747.1 | 2813.5 | no change |
| tes3 4000 stored (bytes): open + first lookup † | 2193.9 | 2509.7 | 0.87x |
| tes3 4000 stored (bytes): open + 8000 lookups † | 7692.4 | 6271.2 | 1.23x |
| tes3 4000 stored (bytes): 8000 lookups by path | 572.1 | 582.3 | no change |
| tes3 4000 stored (bytes): 4000 lookups by hash | 316.4 | 332.7 | no change |
| tes3 4000 stored (bytes): list paths and sizes † | 2917.5 | 2771.9 | 1.05x |
| tes3 4000 stored (bytes): read 1000 files | 663.4 | 661.6 | no change |
| tes3 4000 stored (bytes): readInto 1000 files | 257.6 | 270.3 | no change |
| tes3 4000 stored (path): open + index | 81.3 | 67.5 | 1.20x |
| tes3 4000 stored (path): open + first lookup | 850.9 | 764.8 | 1.11x |
| tes3 4000 stored (path): open + 8000 lookups | 4633.5 | 3652.7 | 1.27x |
| tes3 4000 stored (path): 8000 lookups by path | 569.6 | 558.4 | no change |
| tes3 4000 stored (path): 4000 lookups by hash | 317.9 | 316.8 | no change |
| tes3 4000 stored (path): list paths and sizes | 1741.1 | 1728.3 | no change |
| tes3 4000 stored (path): read 1000 files | 1033.6 | 1037.1 | no change |
| tes3 4000 stored (path): readInto 1000 files | 519.4 | 529.7 | no change |
| tes4 1000 zlib (bytes): open + index † | 461.4 | 416.1 | 1.11x |
| tes4 1000 zlib (bytes): open + first lookup † | 703.4 | 641.1 | 1.10x |
| tes4 1000 zlib (bytes): open + 2000 lookups † | 1509.2 | 1188.6 | 1.27x |
| tes4 1000 zlib (bytes): 2000 lookups by path | 127.7 | 131.8 | no change |
| tes4 1000 zlib (bytes): 1000 lookups by hash | 104.9 | 105.0 | no change |
| tes4 1000 zlib (bytes): list paths and sizes † | 934.8 | 925.2 | no change |
| tes4 1000 zlib (bytes): read 1000 files | 36908.5 | 36778.8 | no change |
| tes4 1000 zlib (bytes): readInto 1000 files | 33847.4 | 33831.8 | no change |
| tes4 1000 zlib (path): open + index | 131.2 | 107.8 | 1.22x |
| tes4 1000 zlib (path): open + first lookup | 382.5 | 335.3 | 1.14x |
| tes4 1000 zlib (path): open + 2000 lookups | 1321.4 | 1040.5 | 1.27x |
| tes4 1000 zlib (path): 2000 lookups by path | 131.0 | 129.5 | no change |
| tes4 1000 zlib (path): 1000 lookups by hash | 100.7 | 102.3 | no change |
| tes4 1000 zlib (path): list paths and sizes | 1067.9 | 1071.7 | no change |
| tes4 1000 zlib (path): read 1000 files | 37300.1 | 37305.8 | no change |
| tes4 1000 zlib (path): readInto 1000 files | 34989.0 | 34945.2 | no change |
| tes4 1000 lz4 frames (bytes): open + index † | 719.6 | 776.4 | 0.93x |
| tes4 1000 lz4 frames (bytes): open + first lookup † | 1041.2 | 963.2 | 1.08x |
| tes4 1000 lz4 frames (bytes): open + 2000 lookups † | 1986.6 | 1405.2 | 1.41x |
| tes4 1000 lz4 frames (bytes): 2000 lookups by path | 130.1 | 125.9 | no change |
| tes4 1000 lz4 frames (bytes): 1000 lookups by hash | 104.3 | 99.4 | no change |
| tes4 1000 lz4 frames (bytes): list paths and sizes † | 1156.8 | 1399.3 | 0.83x |
| tes4 1000 lz4 frames (bytes): read 1000 files | 9844.1 | 9826.1 | no change |
| tes4 1000 lz4 frames (bytes): readInto 1000 files | 6828.1 | 6728.6 | no change |
| tes4 1000 lz4 frames (path): open + index | 134.1 | 108.4 | 1.24x |
| tes4 1000 lz4 frames (path): open + first lookup | 380.1 | 343.2 | 1.11x |
| tes4 1000 lz4 frames (path): open + 2000 lookups | 1367.5 | 1039.9 | 1.32x |
| tes4 1000 lz4 frames (path): 2000 lookups by path | 136.6 | 128.1 | 1.07x |
| tes4 1000 lz4 frames (path): 1000 lookups by hash | 109.4 | 102.8 | 1.06x |
| tes4 1000 lz4 frames (path): list paths and sizes | 1102.2 | 1077.4 | no change |
| tes4 1000 lz4 frames (path): read 1000 files | 10331.1 | 10302.4 | no change |
| tes4 1000 lz4 frames (path): readInto 1000 files | 7824.0 | 7862.4 | no change |
| ba2 1000 zip (bytes): open + index † | 355.4 | 417.2 | 0.85x |
| ba2 1000 zip (bytes): open + first lookup † | 501.9 | 504.5 | no change |
| ba2 1000 zip (bytes): open + 2000 lookups † | 1377.9 | 1071.0 | 1.29x |
| ba2 1000 zip (bytes): 2000 lookups by path | 137.4 | 129.8 | 1.06x |
| ba2 1000 zip (bytes): 1000 lookups by hash | 188.1 | 181.8 | no change |
| ba2 1000 zip (bytes): list paths and sizes † | 657.2 | 659.6 | no change |
| ba2 1000 zip (bytes): read 1000 files | 37253.7 | 37229.3 | no change |
| ba2 1000 zip (bytes): readInto 1000 files | 34605.1 | 33697.7 | no change |
| ba2 1000 zip (path): open + index | 78.3 | 77.7 | no change |
| ba2 1000 zip (path): open + first lookup | 229.2 | 225.6 | no change |
| ba2 1000 zip (path): open + 2000 lookups | 1205.0 | 951.2 | 1.27x |
| ba2 1000 zip (path): 2000 lookups by path | 136.3 | 127.8 | 1.07x |
| ba2 1000 zip (path): 1000 lookups by hash | 194.0 | 181.2 | 1.07x |
| ba2 1000 zip (path): list paths and sizes | 509.8 | 506.8 | no change |
| ba2 1000 zip (path): read 1000 files | 38067.2 | 36967.1 | no change |
| ba2 1000 zip (path): readInto 1000 files | 34859.1 | 34464.0 | no change |
| ba2 1000 lz4 (bytes): open + index † | 625.3 | 639.4 | no change |
| ba2 1000 lz4 (bytes): open + first lookup † | 832.6 | 801.7 | no change |
| ba2 1000 lz4 (bytes): open + 2000 lookups † | 1570.8 | 1347.2 | 1.17x |
| ba2 1000 lz4 (bytes): 2000 lookups by path | 132.8 | 126.2 | 1.05x |
| ba2 1000 lz4 (bytes): 1000 lookups by hash | 198.5 | 185.7 | 1.07x |
| ba2 1000 lz4 (bytes): list paths and sizes † | 1090.5 | 1101.0 | no change |
| ba2 1000 lz4 (bytes): read 1000 files | 9476.8 | 9272.3 | no change |
| ba2 1000 lz4 (bytes): readInto 1000 files | 6347.3 | 6211.9 | no change |
| ba2 1000 lz4 (path): open + index | 79.2 | 77.7 | no change |
| ba2 1000 lz4 (path): open + first lookup | 231.2 | 226.0 | no change |
| ba2 1000 lz4 (path): open + 2000 lookups | 1253.4 | 955.0 | 1.31x |
| ba2 1000 lz4 (path): 2000 lookups by path | 144.2 | 131.8 | 1.09x |
| ba2 1000 lz4 (path): 1000 lookups by hash | 198.7 | 186.3 | 1.07x |
| ba2 1000 lz4 (path): list paths and sizes | 514.9 | 501.1 | no change |
| ba2 1000 lz4 (path): read 1000 files | 9770.1 | 9611.3 | no change |
| ba2 1000 lz4 (path): readInto 1000 files | 6769.7 | 6552.4 | no change |
| dx10 300 BC7 (bytes): open + index † | 6030.7 | 7183.6 | 0.84x |
| dx10 300 BC7 (bytes): open + first lookup † | 5169.7 | 6005.1 | 0.86x |
| dx10 300 BC7 (bytes): open + 600 lookups † | 5543.7 | 5525.5 | no change |
| dx10 300 BC7 (bytes): 600 lookups by path | 38.5 | 37.2 | no change |
| dx10 300 BC7 (bytes): 300 lookups by hash | 56.9 | 54.0 | 1.05x |
| dx10 300 BC7 (bytes): list paths and sizes † | 5726.5 | 5507.7 | no change |
| dx10 300 BC7 (bytes): read 300 files | 7835.6 | 7863.9 | no change |
| dx10 300 BC7 (bytes): readInto 300 files | 1494.0 | 1463.2 | no change |
| dx10 300 BC7 (path): open + index | 144.9 | 139.0 | no change |
| dx10 300 BC7 (path): open + first lookup | 195.5 | 192.1 | no change |
| dx10 300 BC7 (path): open + 600 lookups | 500.7 | 404.9 | 1.24x |
| dx10 300 BC7 (path): 600 lookups by path | 39.1 | 35.9 | 1.09x |
| dx10 300 BC7 (path): 300 lookups by hash | 57.4 | 54.1 | 1.06x |
| dx10 300 BC7 (path): list paths and sizes | 291.3 | 282.2 | no change |
| dx10 300 BC7 (path): read 300 files | 8129.4 | 8096.2 | no change |
| dx10 300 BC7 (path): readInto 300 files | 1583.5 | 1610.6 | no change |
| build: tes3, add 2000 | 1880.7 | 1529.7 | 1.23x |
| build: tes4, add 2000 | 5681.4 | 2952.8 | 1.92x |
| build: ba2, add 2000 | 3549.3 | 2529.8 | 1.40x |
| build: tes3, 2000 stored | 3629.0 | 2937.7 | 1.24x |
| build: tes4 104, 2000 stored | 8122.6 | 5112.5 | 1.59x |
| build: ba2, 2000 stored | 5605.7 | 4229.0 | 1.33x |
| build: dx10, 300 stored | 1252.7 | 1011.4 | 1.24x |
| build: tes4 104, 2000 zlib | 290270 | 266702 | 1.09x |
| build: tes4 105, 2000 lz4 frames | 29513.4 | 26367.4 | 1.12x |
| build: ba2, 2000 zip | 270361 | 265745 | no change |
| build: ba2 v3, 2000 lz4 | 23896.1 | 22783.2 | no change |
| extract: tes3 4000 stored to disk | 321307 | 317886 | no change |
| extract: ba2 1000 zip to disk | 130388 | 130878 | no change |

† An archive opened from bytes is copied first, 12 MiB for the TES3 one and 26 MiB for the DX10
one, and that copy is most of these rows. They swing by more than anything above shows: the same
rows over seven rounds each, which is the measurement to believe for them.

| Workload, seven rounds | Base, µs | Head, µs | Base / head |
|---|---:|---:|---:|
| tes3 4000 stored (bytes): open + index | 2979.0 | 2832.2 | 1.05x |
| tes3 4000 stored (bytes): open + first lookup | 2935.6 | 2115.6 | 1.39x |
| tes3 4000 stored (bytes): open + 8000 lookups | 7499.7 | 6055.9 | 1.24x |
| tes4 1000 zlib (bytes): open + index | 452.5 | 516.0 | 0.88x |
| tes4 1000 zlib (bytes): open + first lookup | 707.1 | 596.9 | 1.18x |
| tes4 1000 zlib (bytes): open + 2000 lookups | 1559.3 | 1233.6 | 1.26x |
| tes4 1000 lz4 frames (bytes): open + index | 896.6 | 810.6 | 1.11x |
| tes4 1000 lz4 frames (bytes): open + first lookup | 1166.1 | 884.8 | 1.32x |
| tes4 1000 lz4 frames (bytes): open + 2000 lookups | 2019.7 | 1842.5 | 1.10x |
| ba2 1000 zip (bytes): open + index | 449.8 | 386.1 | 1.17x |
| ba2 1000 zip (bytes): open + first lookup | 606.1 | 533.3 | 1.14x |
| ba2 1000 zip (bytes): open + 2000 lookups | 1333.8 | 1069.8 | 1.25x |
| ba2 1000 lz4 (bytes): open + index | 811.0 | 742.9 | 1.09x |
| ba2 1000 lz4 (bytes): open + first lookup | 974.2 | 1034.2 | 0.94x |
| ba2 1000 lz4 (bytes): open + 2000 lookups | 2349.9 | 1712.3 | 1.37x |
| dx10 300 BC7 (bytes): open + index | 6709.6 | 5308.1 | 1.26x |
| dx10 300 BC7 (bytes): open + first lookup | 5432.9 | 5672.1 | 0.96x |
| dx10 300 BC7 (bytes): open + 600 lookups | 5171.0 | 5622.9 | 0.92x |
| tes3 4000 stored (bytes): list paths and sizes | 3466.9 | 3514.4 | no change |
| tes4 1000 zlib (bytes): list paths and sizes | 948.8 | 936.7 | no change |
| tes4 1000 lz4 frames (bytes): list paths and sizes | 1133.9 | 1171.4 | no change |
| ba2 1000 zip (bytes): list paths and sizes | 654.7 | 658.1 | no change |
| ba2 1000 lz4 (bytes): list paths and sizes | 1009.4 | 968.6 | no change |
| dx10 300 BC7 (bytes): list paths and sizes | 4891.3 | 4725.6 | no change |

The two tables disagree with each other on which of these rows moved and which way, so they are
noise, except where the rows opened from a path show the same change: `open + N lookups`.

### The same again

The whole protocol a second time, the same day on the same binary, by the review of the move:
base `33f6b95`, head `820adb8`, which is `987549d` with more tests. The rows that moved, and the
build rows:

| Workload | Base, µs | Head, µs | Base / head |
|---|---:|---:|---:|
| tes3 4000 stored (path): open + index | 81.3 | 67.9 | 1.20x |
| tes3 4000 stored (path): open + first lookup | 878.2 | 757.9 | 1.16x |
| tes3 4000 stored (path): open + 8000 lookups | 4686.4 | 3586.7 | 1.31x |
| tes4 1000 zlib (path): open + index | 133.2 | 108.6 | 1.23x |
| tes4 1000 zlib (path): open + first lookup | 387.1 | 333.6 | 1.16x |
| tes4 1000 zlib (path): open + 2000 lookups | 1313.4 | 1026.0 | 1.28x |
| tes4 1000 lz4 frames (path): open + index | 132.8 | 106.9 | 1.24x |
| tes4 1000 lz4 frames (path): open + first lookup | 390.9 | 334.1 | 1.17x |
| tes4 1000 lz4 frames (path): open + 2000 lookups | 1318.5 | 1035.3 | 1.27x |
| ba2 1000 zip (path): open + 2000 lookups | 1159.0 | 919.1 | 1.26x |
| ba2 1000 lz4 (path): open + 2000 lookups | 1163.9 | 906.9 | 1.28x |
| dx10 300 BC7 (path): open + 600 lookups | 469.0 | 401.9 | 1.17x |
| build: tes3, add 2000 | 1780.1 | 1526.4 | 1.17x |
| build: tes4, add 2000 | 5472.7 | 2862.5 | 1.91x |
| build: ba2, add 2000 | 3371.1 | 2474.9 | 1.36x |
| build: tes3, 2000 stored | 3501.5 | 2973.4 | 1.18x |
| build: tes4 104, 2000 stored | 7868.6 | 5014.7 | 1.57x |
| build: ba2, 2000 stored | 5523.7 | 4072.4 | 1.36x |
| build: dx10, 300 stored | 1206.4 | 1001.7 | 1.20x |
| build: tes4 104, 2000 zlib | 266280 | 269173 | no change |
| build: tes4 105, 2000 lz4 frames | 28513.3 | 26311.6 | 1.08x |
| build: ba2, 2000 zip | 262900 | 264663 | no change |
| build: ba2 v3, 2000 lz4 | 23625.3 | 22540.8 | no change |

Every row whose code changed repeats to within 7 percent of its ratio above. The zlib build's
1.09x does not: it was deflate's own variance. Of the rest, four rows whose code is what it was differ by 5 to 7
percent, three of them slower at head, and the † rows swung as they do. One of those,
`tes4 1000 lz4 frames (bytes): list paths and sizes`, came out slower at head both times the
whole bench ran (0.83x above, 0.87x here) and no different with its group run alone, five rounds
a side (1158.6 against 1115.7): it follows where the heap is when the whole bench reaches it, not
the package.

The shared sort has refused a NaN key again since `b11ed83`, one read more per sort. The build
rows run alone against that head, five rounds a side on a busier machine: adding 1.14x (TES3),
1.86x (TES4) and 1.34x (BA2); stored builds 1.23x, 1.47x, 1.34x and 1.23x (DX10).

## What moved, and what did not

Building moved most. Adding a member is up to 1.9 times as fast (TES4), 1.4 (BA2) and 1.2 (TES3):
a path's last separator and extension are found with `bytes.rfind` instead of an anchored
pattern, a folder is hashed once per builder instead of once per member, the TES3 hash XORs its
first half a word at a time, and the TES4 checksum wraps by a mask instead of a modulo. Writing
sorts the members by `data.argsort` over a column of keys, where it grouped them by key in tables
and sorted the keys; with the faster adds, a stored archive is built 1.2 to 1.6 times as fast.
A compressed build is the codec's time: the one to three milliseconds saved come off 24 to 290.
The TES4 LZ4 build shows them (1.12x); the zlib builds' 2 and 9 percent are within what the
codec's own time varies by.

Opening an archive and looking every path up once is 1.2 to 1.3 times as fast in every family
(the rows opened from a path). The path index is an `@dream/intern` pool whose rules are the match
itself (case, both separators, leading and repeated ones), so a query is no longer rewritten into
a scratch buffer in Luau before it is found. Opening a TES3 archive from a path is 1.2 times as
fast, its name offsets cleared by one reduction over them; a TES4 one 1.2 times, each file name's
end found by one search instead of a byte loop.

Nothing else moved, and the rows say so: lookups already remembered, lookups by hash, listing,
reads and extraction run the code they ran before, or the codec's, or the kernel's. The dialect
conversions (reducers, comprehensions, record patterns, slices in the builders and readers) lower
to the loops they replaced and are in none of these numbers. Outside the † rows, rows whose code
did not change differ by up to 9 percent between the two sides here; that is the machine, not the
package.

The pool is read through its binding, not through the native code an annotated
`dream_intern_Pool` compiles to: l3i 2.0.1, as 2.0.0 did, leaves that code reading a slot table
the pool has replaced, once a name it already holds is interned at the pool's load limit. When
l3i fixes that, annotating the pool in `src/internal/names.luau` is the next lookup win to
measure.
