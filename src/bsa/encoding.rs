use std::{borrow::Cow, fmt};

/// Legacy filename encodings commonly encountered in Bethesda archive tooling.
///
/// Archive paths are still bytes. This is a display helper, not a parser policy
/// knob and not a lookup normalization layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum FilenameEncoding {
    Utf8,
    Windows1250,
    Windows1251,
    Windows1252,
    Cp437,
}

/// Error returned when a Unicode filename can not be represented losslessly in
/// the requested legacy encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilenameEncodeError {
    encoding: FilenameEncoding,
}

impl FilenameEncodeError {
    #[must_use]
    pub const fn encoding(self) -> FilenameEncoding {
        self.encoding
    }
}

impl fmt::Display for FilenameEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "filename can not be represented losslessly as {:?}",
            self.encoding
        )
    }
}

impl std::error::Error for FilenameEncodeError {}

/// Decode archive filename bytes for display.
///
/// This is intentionally lossy. If bytes do not form UTF-8 when `Utf8` is
/// selected, replacement characters are emitted. The byte API remains the
/// canonical archive API; this function is for UI/logging/reporting layers.
#[must_use]
pub fn decode_filename_lossy(bytes: &[u8], encoding: FilenameEncoding) -> Cow<'_, str> {
    match encoding {
        FilenameEncoding::Utf8 => String::from_utf8_lossy(bytes),
        FilenameEncoding::Windows1250 => encoding_rs::WINDOWS_1250.decode(bytes).0,
        FilenameEncoding::Windows1251 => encoding_rs::WINDOWS_1251.decode(bytes).0,
        FilenameEncoding::Windows1252 => encoding_rs::WINDOWS_1252.decode(bytes).0,
        FilenameEncoding::Cp437 => decode_cp437(bytes),
    }
}

/// Encode a Unicode filename to archive bytes without replacement or transliteration.
///
/// This is a helper for callers that choose a localization policy above the
/// archive layer. Archive APIs remain byte-first; if a character can not be
/// represented in the selected legacy code page, this returns an error rather
/// than writing `?` and hoping nobody notices. That would be adorable. And wrong.
///
/// # Errors
///
/// Returns [`FilenameEncodeError`] if `text` can not be represented exactly in
/// `encoding`.
pub fn encode_filename(
    text: &str,
    encoding: FilenameEncoding,
) -> Result<Cow<'_, [u8]>, FilenameEncodeError> {
    match encoding {
        FilenameEncoding::Utf8 => Ok(Cow::Borrowed(text.as_bytes())),
        FilenameEncoding::Windows1250 => {
            encode_with_encoding_rs(text, encoding_rs::WINDOWS_1250, encoding)
        }
        FilenameEncoding::Windows1251 => {
            encode_with_encoding_rs(text, encoding_rs::WINDOWS_1251, encoding)
        }
        FilenameEncoding::Windows1252 => {
            encode_with_encoding_rs(text, encoding_rs::WINDOWS_1252, encoding)
        }
        FilenameEncoding::Cp437 => encode_cp437(text, encoding),
    }
}

fn encode_with_encoding_rs<'a>(
    text: &'a str,
    encoding: &'static encoding_rs::Encoding,
    filename_encoding: FilenameEncoding,
) -> Result<Cow<'a, [u8]>, FilenameEncodeError> {
    let (bytes, _, had_errors) = encoding.encode(text);
    if had_errors {
        Err(FilenameEncodeError {
            encoding: filename_encoding,
        })
    } else {
        Ok(bytes)
    }
}

fn decode_cp437(bytes: &[u8]) -> Cow<'_, str> {
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes);
    }
    let mut out = String::new();
    out.reserve(bytes.len());
    for byte in bytes {
        if byte.is_ascii() {
            out.push(char::from(*byte));
        } else {
            out.push(CP437[usize::from(*byte - 0x80)]);
        }
    }
    Cow::Owned(out)
}

fn encode_cp437(
    text: &str,
    encoding: FilenameEncoding,
) -> Result<Cow<'_, [u8]>, FilenameEncodeError> {
    if text.is_ascii() {
        return Ok(Cow::Borrowed(text.as_bytes()));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(text.len())
        .map_err(|_| FilenameEncodeError { encoding })?;
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch as u8);
        } else if let Some(index) = CP437.iter().position(|cp437| *cp437 == ch) {
            out.push(0x80 + u8::try_from(index).expect("CP437 index fits in u8"));
        } else {
            return Err(FilenameEncodeError { encoding });
        }
    }
    Ok(Cow::Owned(out))
}

#[rustfmt::skip]
const CP437: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', ' ',
];
