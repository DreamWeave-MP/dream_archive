+++
title = "bsa"
description = "What the two BSA generations share: legacy filename encodings, FilenameEncodeError, and NormalizedPath."
weight = 40

[extra]
kind = "api"
+++

The `bsa` module exists with `bsa-tes3` or `bsa-tes4`, and holds `bsa::tes3` and `bsa::tes4`
with them. Both generations share its `Error` and `Result`, on
[Errors](@/docs/api/errors.md#bsa-error). [Archive paths](@/docs/paths.md#legacy-code-pages) is the
guide.

## Filename encodings

{{ api_signature(value="enum FilenameEncoding { Utf8, Windows1250, Windows1251, Windows1252, Cp437 }") }}

A code page for archive paths: UTF-8, Windows-1250 (Central European), Windows-1251 (Cyrillic),
Windows-1252 (Western European), or IBM code page 437. `Clone`, `Copy`, `Debug`, `Eq`, `Hash`,
`PartialEq`; `#[non_exhaustive]`.

{{ api_signature(value="fn encode_filename(text: &str, encoding: FilenameEncoding) -> Result<Cow<'_, [u8]>, FilenameEncodeError>") }}

`text` as archive path bytes in `encoding`. `Utf8` borrows the text's own bytes, and so does
`Cp437` for ASCII text. A character the code page cannot represent is an error: nothing is
replaced or transliterated.

{{ api_signature(value="fn decode_filename_lossy(bytes: &[u8], encoding: FilenameEncoding) -> Cow<'_, str>") }}

Archive path bytes as text, for display. With `Utf8`, invalid sequences become U+FFFD. Borrowed
when no conversion was needed.

{{ api_signature(value="struct FilenameEncodeError") }}

The error `encode_filename` returns. It displays as "filename can not be represented losslessly
as" and the encoding's name, and converts into `bsa::Error::FilenameEncoding`. `Clone`, `Copy`,
`Debug`, `Display`, `Eq`, `Error`, `PartialEq`.

{{ api_signature(value="const fn encoding(self) -> FilenameEncoding") }}

The encoding that could not represent the text.

## NormalizedPath

{{ api_signature(value="pub use dream_path::NormalizedPath") }}

dream-path's owned, normalized lookup key, the one the TES3 and TES4 archives' `get_normalized`
and `contains_normalized` take. `NormalizedPath::new(path)` normalizes any spelling once.
[dream-path's documentation](https://DreamWeave-MP.github.io/dream_path/docs/api/normalized-path/)
has the whole type.
