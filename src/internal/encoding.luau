--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- The legacy filename encodings BSA tools wrote paths in. Archive paths stay bytes; these are for
-- showing them and for turning text into them, losslessly or not at all.
local bytes = require '@dream/bytes'
local util = require './util'

local fail = util.fail

export type Encoding = 'utf8' | 'windows1250' | 'windows1251' | 'windows1252' | 'cp437'

local Encodings: { [string]: Encoding } = {
  utf8 = 'utf8',
  windows1250 = 'windows1250',
  cp1250 = 'windows1250',
  windows1251 = 'windows1251',
  cp1251 = 'windows1251',
  windows1252 = 'windows1252',
  cp1252 = 'windows1252',
  cp437 = 'cp437',
}

--- The WHATWG labels `@dream/bytes` takes, and the names the crate's errors print.
local Labels: { [Encoding]: string } =
  { windows1250 = 'windows-1250', windows1251 = 'windows-1251', windows1252 = 'windows-1252' }
local DebugNames: { [Encoding]: string } = {
  utf8 = 'Utf8',
  windows1250 = 'Windows1250',
  windows1251 = 'Windows1251',
  windows1252 = 'Windows1252',
  cp437 = 'Cp437',
}

-- stylua: ignore
local Cp437: { string } = {
  'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
  'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
  'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
  '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
  '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
  '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
  'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
  '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
}

local Cp437Bytes: { [number]: number } = {}
for index, character in Cp437 do
  Cp437Bytes[utf8.codepoint(character)] = 0x7f + index
end

local function parse(name: string): Encoding
  local encoding = Encodings[name]
  if not encoding then fail(`unknown filename encoding: {name}`) end

  return encoding
end

local function unrepresentable(encoding: Encoding): never
  return fail(`filename can not be represented losslessly as {DebugNames[encoding]}`)
end

--- `text` (valid UTF-8) in `encoding`, or an error naming it when a character has no byte there.
local function encode(text: string, encoding: Encoding): string
  if encoding == 'utf8' then return text end

  if encoding == 'cp437' then
    if not string.find(text, '[\128-\255]') then return text end

    local out = buffer.create(#text)
    local length = 0
    for _, codepoint in utf8.codes(text) do
      local byte = if codepoint < 0x80 then codepoint else Cp437Bytes[codepoint]
      if not byte then unrepresentable(encoding) end

      buffer.writeu8(out, length, byte)
      length += 1
    end

    return buffer.readstring(out, 0, length)
  end

  -- bytes.encode throws for a character the code page lacks.
  local ok, result = pcall(bytes.encode, text, Labels[encoding])
  if not ok then unrepresentable(encoding) end

  return buffer.tostring(result :: buffer)
end

--- `data` decoded for display: invalid bytes become U+FFFD.
local function decode(data: string, encoding: Encoding): string
  if encoding == 'utf8' then return util.lossy(data) end

  if encoding == 'cp437' then
    if not string.find(data, '[\128-\255]') then return data end

    local parts = table.create(#data) :: { string }
    for index = 1, #data do
      local byte = string.byte(data, index)
      parts[index] = if byte < 0x80 then string.char(byte) else Cp437[byte - 0x7f]
    end

    return table.concat(parts)
  end

  return bytes.decode(data, Labels[encoding])
end

return table.freeze { parse = parse, encode = encode, decode = decode }
