use std::borrow::Cow;

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
