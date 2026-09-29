//! The one path matching every archive family uses: [`dream_path`]'s normalization.

/// Runs `body` on the lookup-normalized form of `path` (the [`dream_path`] rules: `\\` to
/// `/`, ASCII lowercase, leading and repeated separators dropped) without allocating for
/// paths up to 512 bytes, which is every archive path in practice.
pub(crate) fn with_lookup_path<R>(path: &[u8], body: impl FnOnce(&[u8]) -> R) -> R {
    let mut stack = [0u8; 512];
    if path.len() > stack.len() {
        return body(&dream_path::normalize_path(path));
    }
    let mut len = 0;
    for &byte in path {
        let byte = match byte {
            b'\\' => b'/',
            b'A'..=b'Z' => byte + 32,
            _ => byte,
        };
        if byte == b'/' && (len == 0 || stack[len - 1] == b'/') {
            continue;
        }
        stack[len] = byte;
        len += 1;
    }
    body(&stack[..len])
}

/// Whether a lookup-normalized path can name a file. An empty path, or one that ends in a
/// separator, names a directory. Stored names never match one, but the archive hashes drop
/// trailing separators, so a lookup by hash has to ask.
#[cfg(any(feature = "ba2", feature = "bsa-tes4"))]
pub(crate) fn names_a_file(normalized: &[u8]) -> bool {
    normalized.last().is_some_and(|&byte| byte != b'/')
}

#[cfg(test)]
mod tests {
    use super::with_lookup_path;

    #[test]
    fn matches_dream_path_for_every_length() {
        let long = "A\\".repeat(400);
        for path in [
            "",
            "/",
            "\\\\Meshes//Foo.NIF",
            "a/b/",
            "MIXED\\Case/x.dds",
            long.as_str(),
        ] {
            let expected = dream_path::normalize_path(path.as_bytes());
            with_lookup_path(path.as_bytes(), |normalized| {
                assert_eq!(normalized, expected.as_slice(), "{path}");
            });
        }
    }

    #[cfg(any(feature = "ba2", feature = "bsa-tes4"))]
    #[test]
    fn only_a_path_with_a_last_component_names_a_file() {
        use super::names_a_file;

        assert!(names_a_file(b"meshes/door.nif"));
        assert!(names_a_file(b"door.nif"));
        assert!(!names_a_file(b"meshes/door.nif/"));
        assert!(!names_a_file(b""));
    }
}
