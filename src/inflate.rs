//! zlib inflation straight from a slice into a sized buffer.
//!
//! The archive formats store the decompressed size next to each compressed payload, and the
//! payload is already in memory (owned or mapped), so a streaming reader stack (a `BufReader`
//! with its 32 KiB buffer, a `Take`, `read_to_end`'s probing) is pure overhead: this module
//! runs one inflater over the whole input into the exact output buffer and keeps that inflater
//! per thread, so a decode of a small payload allocates nothing but the output.

use std::cell::RefCell;

use flate2::{Decompress, FlushDecompress, Status};

thread_local! {
    static INFLATER: RefCell<Decompress> = RefCell::new(Decompress::new(true));
}

/// Why an inflation failed; the callers map it onto their format's error type.
#[derive(Debug)]
pub(crate) enum InflateError {
    /// The stream is not valid zlib data (or ends early).
    Corrupt(String),
    /// The stream decoded to a different size than the archive declared. `actual` is capped
    /// at `expected + 1`: one byte past the declared size is proof enough of a longer stream.
    SizeMismatch { expected: usize, actual: usize },
    /// The stream ended before the payload did.
    Trailing,
    /// The output buffer could not be reserved.
    Capacity,
}

/// Appends exactly `expected` inflated bytes of `compressed` to `out`, leaving `out` as it was
/// on any error.
pub(crate) fn inflate_exact(
    compressed: &[u8],
    expected: usize,
    out: &mut Vec<u8>,
) -> Result<(), InflateError> {
    let before = out.len();
    out.try_reserve_exact(expected)
        .map_err(|_| InflateError::Capacity)?;
    out.resize(before + expected, 0);
    let result = INFLATER.with(|inflater| {
        let mut inflater = inflater.borrow_mut();
        inflater.reset(true);
        inflate_into(&mut inflater, compressed, &mut out[before..])
    });
    if result.is_err() {
        out.truncate(before);
    }
    result
}

/// Runs the inflater over `input` into `out` until the stream ends, `out` is full, or no more
/// progress is possible; the totals on the inflater say how far it got.
fn drive(inflater: &mut Decompress, input: &[u8], out: &mut [u8]) -> Result<Status, InflateError> {
    // `out` starts where the inflater's running total stands (the probe buffer is fresh).
    let base = inflater.total_out();
    loop {
        let consumed = usize::try_from(inflater.total_in()).map_err(|_| InflateError::Capacity)?;
        let produced =
            usize::try_from(inflater.total_out() - base).map_err(|_| InflateError::Capacity)?;
        let status = inflater
            .decompress(
                &input[consumed..],
                &mut out[produced..],
                FlushDecompress::None,
            )
            .map_err(|error| InflateError::Corrupt(error.to_string()))?;
        let progressed =
            inflater.total_in() > consumed as u64 || inflater.total_out() - base > produced as u64;
        let full = inflater.total_out() - base >= out.len() as u64;
        if status == Status::StreamEnd || full || !progressed {
            return Ok(status);
        }
    }
}

/// Inflates `compressed` into all of `out`, which must be exactly the declared size.
fn inflate_into(
    inflater: &mut Decompress,
    compressed: &[u8],
    out: &mut [u8],
) -> Result<(), InflateError> {
    let expected = out.len();
    let mut status = drive(inflater, compressed, out)?;
    let produced = usize::try_from(inflater.total_out()).map_err(|_| InflateError::Capacity)?;
    if status != Status::StreamEnd {
        if produced < expected {
            return Err(InflateError::Corrupt(
                "zlib stream ended before its data did".to_owned(),
            ));
        }
        // The output is full: one more byte tells a longer stream from a finished one.
        let mut probe = [0u8; 1];
        status = drive(inflater, compressed, &mut probe)?;
        if inflater.total_out() > produced as u64 {
            return Err(InflateError::SizeMismatch {
                expected,
                actual: expected + 1,
            });
        }
        if status != Status::StreamEnd {
            return Err(InflateError::Corrupt(
                "zlib stream ended before its data did".to_owned(),
            ));
        }
    }
    if produced != expected {
        return Err(InflateError::SizeMismatch {
            expected,
            actual: produced,
        });
    }
    if inflater.total_in() != compressed.len() as u64 {
        return Err(InflateError::Trailing);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{InflateError, inflate_exact};
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write as _;

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn inflates_exact_sizes_and_reports_every_mismatch() {
        let payload: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let compressed = zlib(&payload);
        let mut out = b"prefix".to_vec();
        inflate_exact(&compressed, payload.len(), &mut out).unwrap();
        assert_eq!(&out[..6], b"prefix");
        assert_eq!(&out[6..], &payload[..]);

        let mut out = b"prefix".to_vec();
        let error = inflate_exact(&compressed, 99, &mut out).unwrap_err();
        assert!(
            matches!(
                error,
                InflateError::SizeMismatch {
                    expected: 99,
                    actual: 100
                }
            ),
            "{error:?}"
        );
        assert_eq!(out, b"prefix", "nothing is appended on an error");
        let error = inflate_exact(&compressed, payload.len() + 5, &mut out).unwrap_err();
        assert!(
            matches!(error, InflateError::SizeMismatch { expected, actual } if expected == payload.len() + 5 && actual == payload.len())
        );

        let mut trailing = compressed.clone();
        trailing.push(0);
        assert!(matches!(
            inflate_exact(&trailing, payload.len(), &mut out),
            Err(InflateError::Trailing)
        ));
        assert!(matches!(
            inflate_exact(&compressed[..compressed.len() / 2], payload.len(), &mut out),
            Err(InflateError::Corrupt(_))
        ));
        assert!(matches!(
            inflate_exact(b"not zlib at all", 4, &mut out),
            Err(InflateError::Corrupt(_))
        ));

        let empty = zlib(b"");
        inflate_exact(&empty, 0, &mut out).unwrap();
        assert_eq!(out, b"prefix");
        assert!(matches!(
            inflate_exact(&empty, 1, &mut out),
            Err(InflateError::SizeMismatch {
                expected: 1,
                actual: 0
            })
        ));
        assert!(matches!(
            inflate_exact(&compressed, 0, &mut out),
            Err(InflateError::SizeMismatch {
                expected: 0,
                actual: 1
            })
        ));
    }
}
