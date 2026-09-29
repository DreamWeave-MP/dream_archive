// dream_archive's hero: an archive, opened. A glass and steel cartridge holds the three archive
// families dream_archive reads, one at a time: Morrowind's TES3 BSA, the TES4 BSA of Oblivion to
// Skyrim SE, and Fallout 4's BA2. A split-flap display on its crown shows the file's first four
// bytes, which is all `guess_format` reads: `00 01 00 00`, `BSA\0` or `BTDX`. Inside the glass the
// archive's own layout stands in strata: the record table, the name table and the payload, one
// block per member, grouped by folder for TES4 and split into mip-range chunks for BA2 textures.
//
// Members are extracted one after another. Each leaves as a card carrying its real path and the
// hash its family stores for it, computed here with ports of the crate's `tes3::hash_file`,
// `tes4::hash_file`/`hash_directory` and `ba2::hash_file` (they agree with the crate's test
// vectors), with a preview of what it holds: textures, meshes turning as wireframes, animation
// curves, voice waveforms. A compressed member leaves as a dense nugget and inflates, zlib or LZ4
// as its family uses, and a DX10 texture arrives a mip level at a time. Behind it all scrolls a hex
// dump of the current archive's actual header, records, names and hashes, laid out as the format
// lays them out, and its read head seeks to each record as it is read.
//
// The TES3 paths and sizes are Morrowind.bsa's own; the TES4 and BA2 paths are the crate's test
// vectors and fixtures. The pointer leans the cartridge and looks up the record under it; a click
// extracts that record. Typing the BA2 magic with nothing focused, or tapping its four letters in
// order on the display while it shows them, opens the cartridge: every texture tile erupts and lands
// as a mip chain, each level the one above halved, before they are packed back in hash order.
//
// The scene renders to a half-float target with bloom and ACES, reading its colours from the site's
// CSS tokens. It stands beside the hero's text, or above it on a phone. Nothing runs while it is off
// screen or the tab is hidden, the resolution drops if frames run slow, and under reduced motion one
// frame is drawn. Until the first frame, and without WebGL, a still stands in its place.

import * as THREE from './vendor/three.module.min.js';

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
const MONO = '"DejaVu Sans Mono", ui-monospace, Menlo, Consolas, monospace';
const ASPECT = 1.4;

// Hashes: the crate's three, byte for byte ---------------------------------------------------------

function bytesOf(path) {
  return Array.from(path, (character) => character.charCodeAt(0) & 0xff);
}

// bsa::hash::normalize_hash_path
function normalizeHashPath(path) {
  let bytes = bytesOf(path).map((b) => (b === 0x2f ? 0x5c : b >= 0x41 && b <= 0x5a ? b + 32 : b));
  while (bytes.length && bytes[bytes.length - 1] === 0x5c) bytes.pop();
  while (bytes.length && bytes[0] === 0x5c) bytes.shift();
  if (!bytes.length || bytes.length >= 260) bytes = [0x2e];
  return bytes;
}

const rotateRight = (value, amount) => ((value >>> (amount & 31)) | (value << (32 - (amount & 31)))) >>> 0;

// bsa::tes3::hash_file: `lo` from the first half of the path, `hi` from the second.
function tes3Hash(path) {
  const p = normalizeHashPath(path);
  const middle = p.length >> 1;
  let lo = 0;
  let hi = 0;
  for (let i = 0; i < middle; i++) lo = (lo ^ (p[i] << ((i % 4) * 8))) >>> 0;
  for (let i = middle; i < p.length; i++) {
    const rotation = (p[i] << (((i - middle) % 4) * 8)) >>> 0;
    hi = rotateRight((hi ^ rotation) >>> 0, rotation);
  }
  return { lo, hi };
}

const sum1003f = (bytes) => bytes.reduce((crc, b) => (b + Math.imul(crc, 0x1003f)) >>> 0, 0);
const fourCC = (bytes) => bytes.slice(0, 4).reduce((value, b, i) => (value | (b << (i * 8))) >>> 0, 0);

function tes4Fields(p) {
  const fields = { last: 0, last2: 0, length: p.length & 0xff, first: 0, crc: 0 };
  if (p.length >= 3) fields.last2 = p[p.length - 2];
  if (p.length >= 1) {
    fields.last = p[p.length - 1];
    fields.first = p[0];
  }
  if (fields.length > 3) fields.crc = sum1003f(p.slice(1, p.length - 2));
  return fields;
}

const TES4_EXTENSIONS = ['', '.nif', '.kf', '.dds', '.wav', '.adp'].map((extension) => fourCC(bytesOf(extension)));

// bsa::tes4::hash_directory and hash_file, as the `HashFields` the archive stores.
function tes4DirectoryHash(path) {
  return tes4Fields(normalizeHashPath(path));
}

function tes4FileHash(path) {
  let p = normalizeHashPath(path);
  const slash = p.lastIndexOf(0x5c);
  if (slash >= 0) p = p.slice(slash + 1);
  const dot = p.lastIndexOf(0x2e);
  const stem = dot >= 0 ? p.slice(0, dot) : p;
  const extension = dot >= 0 ? p.slice(dot) : [];
  if (!stem.length || stem.length >= 260 || extension.length >= 16) return { last: 0, last2: 0, length: 0, first: 0, crc: 0 };
  const fields = tes4Fields(stem);
  fields.crc = (fields.crc + sum1003f(extension)) >>> 0;
  const index = TES4_EXTENSIONS.indexOf(fourCC(extension));
  if (index >= 0) {
    fields.first = (fields.first + 32 * (index & 0xfc)) & 0xff;
    fields.last = (fields.last + ((index & 0xfe) << 6)) & 0xff;
    fields.last2 = (fields.last2 + (index << 7)) & 0xff;
  }
  return fields;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? (0xedb88320 ^ (c >>> 1)) >>> 0 : c >>> 1;
  return c >>> 0;
});

// ba2::hash: CRC-32 over the ASCII bytes, `/` read as `\`.
function crc32(bytes) {
  let crc = 0;
  for (let b of bytes) {
    if (b > 127) continue;
    if (b === 0x2f) b = 0x5c;
    crc = ((crc >>> 8) ^ CRC_TABLE[(crc ^ b) & 0xff]) >>> 0;
  }
  return crc;
}

function ba2Hash(path) {
  const p = normalizeHashPath(path);
  const slash = p.lastIndexOf(0x5c);
  const parent = slash >= 0 ? p.slice(0, slash) : [];
  const dot = p.lastIndexOf(0x2e);
  const stem = p.slice(slash + 1, dot > slash ? dot : p.length);
  const extension = dot > slash ? p.slice(dot + 1) : [];
  return { file: crc32(stem), extension: fourCC(extension), directory: crc32(parent) };
}

const hex = (value, width) => (value >>> 0).toString(16).padStart(width, '0');
const hex64 = (hi, lo) => hex(hi, 8) + hex(lo, 8);

// The archives ---------------------------------------------------------------------------------------

// Morrowind.bsa's own members and their sizes.
const TES3_MEMBERS = [
  ['textures\\tx_wood.dds', 43808],
  ['meshes\\x\\ex_dwrv_ruin_tower00.nif', 56708],
  ['textures\\tx_ashl_bug_shell_01.dds', 21952],
  ['meshes\\m\\misc_com_tankard_01.nif', 5833],
  ['textures\\tx_dwrv_golemvent00.dds', 11040],
  ['meshes\\r\\xguar.kf', 297104],
  ['textures\\tx_siltstrider_skin_00.dds', 43808],
  ['meshes\\w\\w_longsword_crystal.nif', 15688],
  ['textures\\tx_redware_strip_01.dds', 1472],
  ['meshes\\x\\ex_hlaalu_bridge_06.nif', 63350],
  ['textures\\tx_telv_symbol_stone.dds', 43808],
  ['meshes\\a\\a_art_helm_bearclaw.nif', 26224],
  ['textures\\tx_bc_moss.dds', 43808],
  ['meshes\\f\\flora_ash_grass_b_01.nif', 3740],
  ['icons\\w\\tx_daedric_longsword.dds', 1472],
  ['meshes\\r\\guar.nif', 451155],
  ['textures\\tx_glass_bottle_blue.dds', 5568],
  ['meshes\\x\\ex_imp_plaza_stairs.nif', 26841],
  ['textures\\tx_rock_diamond_01.dds', 43808],
  ['meshes\\f\\xfurn_redoran_flag_01.kf', 1814],
  ['textures\\tx_imp_fireplace_01.dds', 5568],
  ['meshes\\w\\w_longspear_daedric.nif', 27626],
  ['textures\\tx_scroll.dds', 43808],
  ['meshes\\d\\door_cavern_doors20.nif', 20258],
  ['textures\\menu_rightbuttonup_top.dds', 256],
  ['meshes\\m\\misc_com_wood_cup_02.nif', 4438],
  ['shaders\\fauxembm_displace_2.pso', 220],
  ['meshes\\i\\in_dae_hall_l_corner.nif', 84245],
];

// The crate's TES4 test vectors: Skyrim's and Oblivion's paths. A member with no folder lives in
// the root folder, `.`.
const TES4_MEMBERS = [
  ['textures\\armor\\amuletsandrings\\elder council', 'elder_council_amulet_n.dds'],
  ['sound\\voice\\skyrim.esm\\maleuniquedbguardian', 'darkbrotherhood__0007469a_1.fuz'],
  ['meshes\\armor\\iron', 'cuirass.nif'],
  ['.', 'testtoddquest_testtoddhappy_00027fa2_1.mp3'],
  ['textures\\architecture\\windhelm', null],
  ['.', 'Mar\xEDa_F.fuz'],
];

// The crate's BA2 test vectors and fixtures: Fallout 4 Creation Club paths among them.
const BA2_MEMBERS = [
  ['Textures\\CreationClub\\BGSFO4001\\AnimObjects\\PipBoy\\PipBoy02(Black)_d.DDS', 'DX10'],
  ['Materials\\CreationClub\\BGSFO4003\\AnimObjects\\PipBoy\\PipBoyLabels01(Camo01).BGSM', 'GNRL'],
  ['Sound\\Voice\\Fallout4.esm\\RobotMrHandy\\Mar\xEDa_M.fuz', 'GNRL'],
  ['dx10\\Fence006_1K_Roughness.dds', 'DX10'],
  ['Interface\\Credits.txt', 'GNRL'],
  ['sound\\music\\theme.xwm', 'GNRL'],
];

function kindOf(path) {
  const extension = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  if (extension === 'dds') return 'texture';
  if (extension === 'nif') return 'mesh';
  if (extension === 'kf') return 'animation';
  if (['fuz', 'mp3', 'xwm', 'wav'].includes(extension)) return 'sound';
  if (extension === 'bgsm') return 'material';
  if (extension === 'pso') return 'shader';
  return 'text';
}

function splitPath(path) {
  const slash = path.lastIndexOf('\\');
  return slash >= 0 ? { folder: path.slice(0, slash), name: path.slice(slash + 1) } : { folder: '', name: path };
}

function tes3Archive() {
  const members = TES3_MEMBERS.map(([path, size]) => {
    const hash = tes3Hash(path);
    return { path, size, kind: kindOf(path), sortKey: hash.hi + hash.lo * 2 ** 32, hashLines: [`hash  ${hex64(hash.lo, hash.hi)}`], stored: 'stored' };
  });
  return { id: 'tes3', family: 'TES3 BSA', flaps: ['00', '01', '00', '00'], compression: null, members, bytes: tes3Bytes(members) };
}

function tes4Archive() {
  const members = [];
  for (const [folder, name] of TES4_MEMBERS) {
    if (!name) continue;
    const path = folder === '.' ? name : `${folder}\\${name}`;
    const folderHash = tes4DirectoryHash(folder);
    const fileHash = tes4FileHash(name);
    const pack = (f) => [f.crc, (f.first << 24 | f.length << 16 | f.last2 << 8 | f.last) >>> 0];
    members.push({
      path,
      folder,
      kind: kindOf(name),
      sortKey: pack(folderHash)[0] * 2 ** 20 + pack(fileHash)[0] / 2 ** 12,
      hashLines: [`folder ${hex64(...pack(folderHash))}`, `file   ${hex64(...pack(fileHash))}`],
      stored: 'LZ4 frame',
      folderHash: pack(folderHash),
      fileHash: pack(fileHash),
    });
  }
  const folders = [...new Set(TES4_MEMBERS.map(([folder]) => folder))];
  return { id: 'tes4', family: 'TES4 BSA 105', flaps: ['B', 'S', 'A', '␀'], compression: 'LZ4 frame', members, folders, bytes: tes4Bytes(members, folders) };
}

function ba2Archive() {
  const members = BA2_MEMBERS.map(([path, payload]) => {
    const hash = ba2Hash(path);
    const extension = String.fromCharCode(...[0, 8, 16, 24].map((shift) => (hash.extension >>> shift) & 0xff)).replace(/\0+$/, '');
    return {
      path,
      payload,
      kind: kindOf(path),
      sortKey: hash.directory * 2 ** 20 + hash.file / 2 ** 12,
      hashLines: [`dir  ${hex(hash.directory, 8)}`, `file ${hex(hash.file, 8)}  .${extension}`],
      stored: payload === 'DX10' ? 'zlib, by mips' : 'zlib',
      hash,
    };
  });
  return { id: 'ba2', family: 'BA2', flaps: ['B', 'T', 'D', 'X'], compression: 'zlib', members, bytes: ba2Bytes(members) };
}

// Byte images: each archive's header, records, names and hashes, as the format lays them out -------

class ByteWriter {
  constructor() {
    this.bytes = [];
  }
  u8(value) {
    this.bytes.push(value & 0xff);
  }
  u16(value) {
    this.u8(value);
    this.u8(value >>> 8);
  }
  u32(value) {
    for (let i = 0; i < 4; i++) this.u8(value >>> (i * 8));
  }
  u64(hi, lo) {
    this.u32(lo);
    this.u32(hi);
  }
  text(value, terminate = true) {
    for (const b of bytesOf(value)) this.u8(b);
    if (terminate) this.u8(0);
  }
  pad(multiple) {
    while (this.bytes.length % multiple) this.u8(0);
  }
}

// TES3: version 0x100, hash-table offset, count; size and offset per member; name offsets; names;
// then the hashes, `lo` before `hi`. Members are in hash order, as Morrowind's are.
function tes3Bytes(members) {
  const sorted = [...members].sort((a, b) => a.sortKey - b.sortKey);
  const names = sorted.map((member) => member.path);
  const namesLength = names.reduce((total, name) => total + name.length + 1, 0);
  const w = new ByteWriter();
  w.u32(0x100);
  w.u32(sorted.length * 12 + namesLength);
  w.u32(sorted.length);
  let offset = 0;
  for (const member of sorted) {
    member.spans = [w.bytes.length, w.bytes.length + 8];
    w.u32(member.size);
    w.u32(offset);
    offset += member.size;
  }
  let nameOffset = 0;
  for (const name of names) {
    w.u32(nameOffset);
    nameOffset += name.length + 1;
  }
  for (const member of sorted) {
    member.nameSpan = [w.bytes.length, w.bytes.length + member.path.length + 1];
    w.text(member.path);
  }
  for (const member of sorted) {
    const hash = tes3Hash(member.path);
    member.spans.push(w.bytes.length, w.bytes.length + 8);
    w.u32(hash.lo);
    w.u32(hash.hi);
  }
  payloadStubs(w, sorted, 'stored');
  return w;
}

// TES4 version 105: a 36-byte header, 24-byte folder records, each folder's name and file records,
// then the file names. Sizes here are the uncompressed payload's, marked compressed.
function tes4Bytes(members, folders) {
  const w = new ByteWriter();
  const fileNames = members.map((member) => splitPath(member.path).name);
  w.text('BSA', true);
  w.u32(105);
  w.u32(36);
  w.u32(0x1 | 0x2 | 0x4);
  w.u32(folders.length);
  w.u32(members.length);
  w.u32(folders.reduce((total, folder) => total + folder.length + 1, 0));
  w.u32(fileNames.reduce((total, name) => total + name.length + 1, 0));
  w.u16(0x3);
  w.u16(0);
  let offset = 36 + folders.length * 24;
  for (const folder of folders) {
    const hash = tes4DirectoryHash(folder);
    const inside = members.filter((member) => member.folder === folder);
    w.u64(hash.crc, (hash.first << 24 | hash.length << 16 | hash.last2 << 8 | hash.last) >>> 0);
    w.u32(inside.length);
    w.u32(0);
    w.u64(0, offset);
    offset += folder.length + 2 + inside.length * 16;
  }
  let data = offset + fileNames.reduce((total, name) => total + name.length + 1, 0);
  for (const folder of folders) {
    w.u8(folder.length + 1);
    w.text(folder);
    for (const member of members.filter((m) => m.folder === folder)) {
      member.spans = [w.bytes.length, w.bytes.length + 16, 0, 0];
      w.u64(...member.fileHash);
      const size = 4096 + member.path.length * 97;
      w.u32(size | 0x40000000);
      w.u32(data);
      data += size;
    }
  }
  members.forEach((member, index) => {
    member.nameSpan = [w.bytes.length, w.bytes.length + fileNames[index].length + 1];
    w.text(fileNames[index]);
  });
  payloadStubs(w, members, 'lz4');
  return w;
}

// Where the records point: each member's payload begins as its format begins, and the rest is left
// out. Stored DDS files start `DDS `, Morrowind's NIF and KF files with NetImmerse's banner, and
// compressed members with an LZ4 frame's magic or a zlib header.
function payloadStubs(w, members, storage) {
  const rand = random(storage.length * 977 + members.length);
  w.pad(16);
  for (const member of members) {
    const start = w.bytes.length;
    member.payloadSpan = [start, start + 48];
    if (storage === 'lz4') {
      for (const b of [0x04, 0x22, 0x4d, 0x18, 0x64, 0x40, 0xa7]) w.u8(b);
    } else if (storage === 'zlib') {
      w.u8(0x78);
      w.u8(0xda);
    } else if (member.kind === 'texture') {
      w.text('DDS ', false);
      w.u32(124);
      w.u32(0x81007);
      w.u32(128);
      w.u32(128);
    } else if (member.kind === 'mesh' || member.kind === 'animation') {
      w.text('NetImmerse File Format, Version 4.0.0.2\n', false);
    } else {
      w.u32(0xffff0101);
    }
    while (w.bytes.length < start + 48) w.u8(storage === 'stored' ? Math.floor(rand() * 64) : Math.floor(rand() * 256));
  }
  w.pad(16);
}

// BA2: a GNRL archive of the general members, then a DX10 archive of the textures. Every record ends
// in the format's 0xBAADF00D, and the name table follows the payload.
function ba2Bytes(members) {
  const w = new ByteWriter();
  const general = members.filter((member) => member.payload === 'GNRL');
  const textures = members.filter((member) => member.payload === 'DX10');
  for (const [type, group] of [['GNRL', general], ['DX10', textures]]) {
    const start = w.bytes.length;
    w.text('BTDX', false);
    w.u32(1);
    w.text(type, false);
    w.u32(group.length);
    w.u64(0, 0x10000 + group.length * 0x2400);
    for (const [index, member] of group.entries()) {
      member.spans = [w.bytes.length, w.bytes.length + (type === 'GNRL' ? 36 : 24 + 72), 0, 0];
      w.u32(member.hash.file);
      w.u32(member.hash.extension);
      w.u32(member.hash.directory);
      if (type === 'GNRL') {
        w.u32(0);
        w.u64(0, 0x400 + index * 0x2400);
        w.u32(0x1800 + index * 0x31);
        w.u32(0x2400);
        w.u32(0xbaadf00d);
      } else {
        w.u8(0);
        w.u8(3);
        w.u16(24);
        w.u16(1024);
        w.u16(1024);
        w.u8(11);
        w.u8(98);
        w.u8(0);
        w.u8(8);
        for (const [mipStart, mipEnd] of [[0, 0], [1, 1], [2, 10]]) {
          w.u64(0, 0x800 + index * 0x160000 + mipStart * 0x40000);
          w.u32(0x30000 >>> mipStart);
          w.u32(0x100000 >>> (mipStart * 2));
          w.u16(mipStart);
          w.u16(mipEnd);
          w.u32(0xbaadf00d);
        }
      }
    }
    payloadStubs(w, group, 'zlib');
    for (const member of group) {
      member.nameSpan = [w.bytes.length, w.bytes.length + member.path.length + 2];
      w.u16(member.path.length);
      w.text(member.path, false);
    }
    w.pad(16);
    if (w.bytes.length === start) w.pad(16);
  }
  return w;
}

// Procedural art --------------------------------------------------------------------------------------

function random(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function seedOf(text) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return h >>> 0;
}

function valueNoise(seed) {
  const rand = random(seed);
  const size = 64;
  const grid = new Float32Array(size * size);
  for (let i = 0; i < grid.length; i++) grid[i] = rand();
  const at = (x, y) => grid[((y % size + size) % size) * size + ((x % size + size) % size)];
  return (x, y) => {
    const x0 = Math.floor(x);
    const y0 = Math.floor(y);
    const fx = x - x0;
    const fy = y - y0;
    const sx = fx * fx * (3 - 2 * fx);
    const sy = fy * fy * (3 - 2 * fy);
    const a = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * sx;
    const b = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * sx;
    return a + (b - a) * sy;
  };
}

function fbm(noise, x, y, octaves = 4) {
  let sum = 0;
  let amplitude = 0.5;
  let frequency = 1;
  for (let i = 0; i < octaves; i++) {
    sum += amplitude * noise(x * frequency, y * frequency);
    amplitude *= 0.5;
    frequency *= 2;
  }
  return sum;
}

const mixRgb = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
const clamp01 = (value) => Math.min(1, Math.max(0, value));

// A texture's look, chosen from its name the way a modder reads one: wood is wood, moss is moss.
function textureStyle(path) {
  const name = path.toLowerCase();
  const styles = [
    [/wood|table|cup/, { a: [70, 42, 22], b: [150, 98, 52], grain: 'wood' }],
    [/bug_shell|ashl/, { a: [40, 18, 12], b: [140, 70, 38], grain: 'plates' }],
    [/dwrv|golem/, { a: [70, 48, 22], b: [196, 150, 82], grain: 'grille' }],
    [/siltstrider|skin/, { a: [70, 56, 44], b: [150, 128, 100], grain: 'mottle' }],
    [/redware/, { a: [120, 40, 20], b: [210, 92, 48], grain: 'stripes' }],
    [/telv/, { a: [30, 60, 48], b: [120, 170, 130], grain: 'symbol' }],
    [/moss|bc_|scrub|dirt/, { a: [30, 44, 20], b: [96, 120, 50], grain: 'mottle' }],
    [/glass_bottle_blue|glass/, { a: [18, 40, 90], b: [110, 170, 240], grain: 'glass' }],
    [/rock|diamond|cave|stone/, { a: [50, 50, 54], b: [140, 138, 132], grain: 'cells' }],
    [/fireplace|fire/, { a: [60, 20, 8], b: [255, 150, 40], grain: 'fire' }],
    [/scroll|paper/, { a: [150, 120, 80], b: [230, 210, 160], grain: 'lines' }],
    [/menu/, { a: [40, 32, 22], b: [170, 140, 90], grain: 'bevel' }],
    [/daedric|sword|icons/, { a: [20, 16, 20], b: [200, 60, 50], grain: 'icon' }],
    [/pipboy/, { a: [4, 24, 8], b: [80, 255, 120], grain: 'lines' }],
    [/fence|roughness/, { a: [40, 40, 40], b: [190, 190, 190], grain: 'fence' }],
    [/amulet|elder/, { a: [60, 44, 10], b: [240, 200, 90], grain: 'cells' }],
  ];
  for (const [pattern, style] of styles) if (pattern.test(name)) return style;
  return { a: [40, 50, 60], b: [150, 170, 200], grain: 'mottle' };
}

function paintTexture(context, path, x0, y0, size) {
  const style = textureStyle(path);
  const noise = valueNoise(seedOf(path));
  const image = context.createImageData(size, size);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const u = x / size;
      const v = y / size;
      let t = fbm(noise, u * 6, v * 6);
      switch (style.grain) {
        case 'wood': t = 0.5 + 0.5 * Math.sin((v * 22 + fbm(noise, u * 3, v * 9) * 6) * 1.2) * 0.8 + t * 0.2; break;
        case 'plates': t = 0.35 + 0.65 * clamp01(Math.sin(Math.hypot(u - 0.5, v + 0.2) * 28) * 0.5 + 0.5) * t * 1.4; break;
        case 'grille': t = ((Math.floor(v * 8) % 2) ? 0.9 : 0.55) * (0.6 + t * 0.5) * (Math.abs(((u * 8) % 1) - 0.5) < 0.04 ? 0.4 : 1); break;
        case 'stripes': t = (Math.floor(v * 6 + t) % 2 ? 0.8 : 0.35) + t * 0.2; break;
        case 'symbol': { const r = Math.hypot(u - 0.5, v - 0.5); t = t * 0.7 + (Math.abs(r - 0.28) < 0.03 || Math.abs(r - 0.12) < 0.02 ? 0.6 : 0); break; }
        case 'glass': { const r = Math.hypot(u - 0.42, v - 0.38); t = clamp01(1 - r * 1.6) + (r < 0.08 ? 0.6 : 0); break; }
        case 'cells': t = Math.pow(clamp01(Math.abs(Math.sin(u * 13 + t * 5) * Math.sin(v * 11 + t * 4))), 0.5) * 0.6 + t * 0.5; break;
        case 'fire': t = clamp01((1 - v) * 1.3 * t + t * t * 0.5); break;
        case 'lines': t = 0.75 + t * 0.2 - ((v * 14) % 1 < 0.08 ? 0.35 : 0); break;
        case 'bevel': t = 0.5 + (u < 0.08 || v < 0.08 ? 0.35 : 0) - (u > 0.92 || v > 0.92 ? 0.3 : 0) + t * 0.1; break;
        case 'icon': t = Math.abs(u - v) < 0.07 && u > 0.18 && u < 0.86 ? 1 : 0.08 + t * 0.15; break;
        case 'fence': t = t * 0.7 + (Math.abs(((u + v) * 7) % 1 - 0.5) < 0.05 || Math.abs(((u - v) * 7 + 10) % 1 - 0.5) < 0.05 ? 0.35 : 0); break;
        default: break;
      }
      const [r, g, b] = mixRgb(style.a, style.b, clamp01(t));
      const i = (y * size + x) * 4;
      image.data[i] = r;
      image.data[i + 1] = g;
      image.data[i + 2] = b;
      image.data[i + 3] = 255;
    }
  }
  context.putImageData(image, x0, y0);
}

function paintAnimation(context, path, x, y, size, ink) {
  const rand = random(seedOf(path));
  context.fillStyle = 'rgba(8, 11, 15, 0.9)';
  context.fillRect(x, y, size, size);
  context.strokeStyle = 'rgba(255,255,255,0.08)';
  context.lineWidth = 1;
  for (let i = 1; i < 8; i++) {
    context.beginPath();
    context.moveTo(x + (i * size) / 8, y);
    context.lineTo(x + (i * size) / 8, y + size);
    context.stroke();
  }
  const colors = [ink, '#9cc7ff', '#ecd18f'];
  for (let curve = 0; curve < 3; curve++) {
    const phase = rand() * 6;
    const amplitude = 0.18 + rand() * 0.18;
    context.strokeStyle = colors[curve];
    context.lineWidth = 2.5;
    context.beginPath();
    for (let i = 0; i <= 64; i++) {
      const u = i / 64;
      const v = 0.5 + Math.sin(u * 7 + phase) * amplitude * Math.cos(u * 2.3 + curve);
      context.lineTo(x + u * size, y + v * size);
    }
    context.stroke();
    context.fillStyle = colors[curve];
    for (let k = 0; k < 6; k++) {
      const u = (k + 0.5) / 6;
      const v = 0.5 + Math.sin(u * 7 + phase) * amplitude * Math.cos(u * 2.3 + curve);
      context.save();
      context.translate(x + u * size, y + v * size);
      context.rotate(Math.PI / 4);
      context.fillRect(-4, -4, 8, 8);
      context.restore();
    }
  }
}

function paintWaveform(context, path, x, y, size, ink) {
  const noise = valueNoise(seedOf(path));
  context.fillStyle = 'rgba(8, 11, 15, 0.9)';
  context.fillRect(x, y, size, size);
  context.fillStyle = ink;
  const bars = 48;
  for (let i = 0; i < bars; i++) {
    const envelope = Math.sin((i / bars) * Math.PI) ** 0.6;
    const h = (0.15 + noise(i * 0.35, 3) * 0.85) * envelope * size * 0.82;
    context.fillRect(x + (i * size) / bars + 1, y + size / 2 - h / 2, size / bars - 2, Math.max(2, h));
  }
}

function paintMaterial(context, x, y, size, ink) {
  const gradient = context.createRadialGradient(x + size * 0.38, y + size * 0.34, size * 0.02, x + size / 2, y + size / 2, size * 0.46);
  gradient.addColorStop(0, '#ffffff');
  gradient.addColorStop(0.25, ink);
  gradient.addColorStop(1, '#0a0d12');
  context.fillStyle = 'rgba(8, 11, 15, 0.9)';
  context.fillRect(x, y, size, size);
  context.fillStyle = gradient;
  context.beginPath();
  context.arc(x + size / 2, y + size / 2, size * 0.42, 0, Math.PI * 2);
  context.fill();
}

function paintLines(context, path, x, y, size, ink, hexBytes) {
  const rand = random(seedOf(path));
  context.fillStyle = 'rgba(8, 11, 15, 0.9)';
  context.fillRect(x, y, size, size);
  context.fillStyle = ink;
  context.font = `${Math.round(size / 11)}px ${MONO}`;
  for (let line = 0; line < 9; line++) {
    let text = '';
    if (hexBytes) {
      for (let i = 0; i < 6; i++) text += hex(Math.floor(rand() * 256), 2) + ' ';
    } else {
      text = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz'.split('').sort(() => rand() - 0.5).join('').slice(0, 5 + Math.floor(rand() * 10));
    }
    context.fillText(text, x + size * 0.06, y + size * (0.14 + line * 0.1));
  }
}

// Glyphs ------------------------------------------------------------------------------------------------

// Printable ASCII, 16 to a row, for the hex dump.
const DUMP_CELL = { w: 32, h: 48 };
function dumpAtlas() {
  const canvas = document.createElement('canvas');
  canvas.width = DUMP_CELL.w * 16;
  canvas.height = DUMP_CELL.h * 6;
  const context = canvas.getContext('2d');
  context.fillStyle = '#fff';
  context.font = `500 ${DUMP_CELL.h * 0.62}px ${MONO}`;
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  for (let code = 32; code < 128; code++) {
    const index = code - 32;
    const character = code === 127 ? '·' : String.fromCharCode(code);
    context.fillText(character, (index % 16 + 0.5) * DUMP_CELL.w, (Math.floor(index / 16) + 0.52) * DUMP_CELL.h);
  }
  const texture = new THREE.CanvasTexture(canvas);
  texture.generateMipmaps = true;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  return texture;
}

// The split-flap's faces: each generation's four magic bytes, and a few more to flutter through.
const FLAP_GLYPHS = ['00', '01', 'B', 'S', 'A', '␀', 'T', 'D', 'X', '5A', 'E3', '7F', '42', '9C', 'C0', '1F'];
const FLAP_CAPTIONS = { '00': 'byte 0x00', '01': 'byte 0x01', B: 'B  0x42', S: 'S  0x53', A: 'A  0x41', '␀': 'NUL 0x00', T: 'T  0x54', D: 'D  0x44', X: 'X  0x58' };
const FLAP_FACE = { w: 128, h: 192 };
function flapAtlas(ink, glow) {
  const canvas = document.createElement('canvas');
  canvas.width = FLAP_FACE.w * FLAP_GLYPHS.length;
  canvas.height = FLAP_FACE.h;
  const emissive = document.createElement('canvas');
  emissive.width = canvas.width;
  emissive.height = canvas.height;
  const context = canvas.getContext('2d');
  const light = emissive.getContext('2d');
  light.fillStyle = '#000';
  light.fillRect(0, 0, emissive.width, emissive.height);
  FLAP_GLYPHS.forEach((glyph, index) => {
    const x = index * FLAP_FACE.w;
    const face = context.createLinearGradient(0, 0, 0, FLAP_FACE.h);
    face.addColorStop(0, '#1b2129');
    face.addColorStop(0.5, '#10141a');
    face.addColorStop(0.5, '#0c0f13');
    face.addColorStop(1, '#161b22');
    context.fillStyle = face;
    context.fillRect(x + 2, 2, FLAP_FACE.w - 4, FLAP_FACE.h - 4);
    const big = glyph.length > 1 && glyph !== '␀' ? 0.46 : 0.62;
    for (const [target, color] of [[context, ink], [light, glow]]) {
      target.fillStyle = color;
      target.font = `700 ${Math.round(FLAP_FACE.h * big)}px ${MONO}`;
      target.textAlign = 'center';
      target.textBaseline = 'middle';
      target.fillText(glyph, x + FLAP_FACE.w / 2, FLAP_FACE.h * 0.47);
      if (FLAP_CAPTIONS[glyph]) {
        target.globalAlpha = 0.6;
        target.font = `500 ${Math.round(FLAP_FACE.h * 0.075)}px ${MONO}`;
        target.fillText(FLAP_CAPTIONS[glyph], x + FLAP_FACE.w / 2, FLAP_FACE.h * 0.88);
        target.globalAlpha = 1;
      }
    }
    context.fillStyle = 'rgba(0,0,0,0.85)';
    context.fillRect(x, FLAP_FACE.h / 2 - 1.5, FLAP_FACE.w, 3);
    light.fillStyle = '#000';
    light.fillRect(x, FLAP_FACE.h / 2 - 1.5, FLAP_FACE.w, 3);
  });
  const make = (source, colorSpace) => {
    const texture = new THREE.CanvasTexture(source);
    texture.colorSpace = colorSpace;
    texture.anisotropy = 4;
    return texture;
  };
  return { map: make(canvas, THREE.SRGBColorSpace), emissive: make(emissive, THREE.SRGBColorSpace) };
}

// Cards --------------------------------------------------------------------------------------------------

const CARD = { w: 640, h: 360, preview: { x: 20, y: 70, size: 220 } };
const KIND_LABEL = { texture: 'DDS', mesh: 'NIF', animation: 'KF', sound: 'AUDIO', material: 'BGSM', shader: 'PSO', text: 'TXT' };

function wrap(context, text, width, lines) {
  const out = [];
  let rest = text;
  while (rest.length && out.length < lines) {
    let cut = rest.length;
    while (context.measureText(rest.slice(0, cut)).width > width && cut > 1) cut--;
    if (cut < rest.length) {
      const soft = Math.max(rest.lastIndexOf('\\', cut - 1), rest.lastIndexOf('_', cut - 1), rest.lastIndexOf(' ', cut - 1));
      if (soft > cut * 0.5) cut = soft + 1;
    }
    out.push(rest.slice(0, cut));
    rest = rest.slice(cut);
  }
  if (rest.length && out.length) out[out.length - 1] = `${out[out.length - 1].slice(0, -1)}…`;
  return out;
}

function paintCard(canvas, member, archive, colors) {
  const context = canvas.getContext('2d');
  const { w, h, preview } = CARD;
  context.clearRect(0, 0, w, h);
  context.save();
  context.beginPath();
  context.roundRect(4, 4, w - 8, h - 8, 22);
  context.fillStyle = 'rgba(11, 14, 19, 0.94)';
  context.fill();
  context.lineWidth = 3;
  context.strokeStyle = colors.line;
  context.stroke();
  context.clip();

  const ink = colors.kinds[member.kind] || colors.accent;
  context.fillStyle = ink;
  context.font = `700 22px ${MONO}`;
  context.textBaseline = 'middle';
  const label = KIND_LABEL[member.kind];
  const labelWidth = context.measureText(label).width + 24;
  context.globalAlpha = 0.18;
  context.beginPath();
  context.roundRect(20, 20, labelWidth, 34, 8);
  context.fill();
  context.globalAlpha = 1;
  context.fillText(label, 32, 38);
  context.fillStyle = colors.faint;
  context.font = `500 18px ${MONO}`;
  context.textAlign = 'right';
  context.fillText(`${archive.family} · ${member.stored}`, w - 24, 38);
  context.textAlign = 'left';

  const { x, y, size } = preview;
  if (member.kind === 'texture') paintTexture(context, member.path, x, y, size);
  else if (member.kind === 'animation') paintAnimation(context, member.path, x, y, size, ink);
  else if (member.kind === 'sound') paintWaveform(context, member.path, x, y, size, ink);
  else if (member.kind === 'material') paintMaterial(context, x, y, size, ink);
  else if (member.kind === 'shader') paintLines(context, member.path, x, y, size, ink, true);
  else if (member.kind === 'text') paintLines(context, member.path, x, y, size, ink, false);
  else {
    context.fillStyle = 'rgba(8, 11, 15, 0.9)';
    context.fillRect(x, y, size, size);
    context.strokeStyle = 'rgba(255,255,255,0.06)';
    for (let i = 1; i < 6; i++) {
      context.beginPath();
      context.moveTo(x + (i * size) / 6, y);
      context.lineTo(x + (i * size) / 6, y + size);
      context.moveTo(x, y + (i * size) / 6);
      context.lineTo(x + size, y + (i * size) / 6);
      context.stroke();
    }
  }
  context.strokeStyle = 'rgba(255,255,255,0.12)';
  context.lineWidth = 2;
  context.strokeRect(x, y, size, size);

  const column = x + size + 22;
  const width = w - column - 24;
  const { folder, name } = splitPath(member.path);
  let line = y + 12;
  context.textBaseline = 'top';
  context.font = `500 22px ${MONO}`;
  context.fillStyle = colors.faint;
  for (const text of wrap(context, folder ? `${folder}\\` : '.\\', width, 3)) {
    context.fillText(text, column, line);
    line += 27;
  }
  line += 4;
  context.font = `700 30px ${MONO}`;
  context.fillStyle = colors.text;
  for (const text of wrap(context, name, width, 2)) {
    context.fillText(text, column, line);
    line += 36;
  }
  line = Math.max(line + 10, y + 142);
  context.font = `500 22px ${MONO}`;
  context.fillStyle = ink;
  for (const text of member.hashLines) {
    context.fillText(text, column, line);
    line += 28;
  }
  if (member.size) {
    context.fillStyle = colors.faint;
    context.fillText(`${member.size.toLocaleString('en-US').replace(/,/g, ' ')} bytes`, column, line + 2);
  }
  context.restore();
}

// Meshes turn above their cards as wireframes: something of the shape the path names.
function wireframeFor(path, color) {
  const name = path.toLowerCase();
  const parts = [];
  const add = (geometry, position = [0, 0, 0], rotation = [0, 0, 0], scale = [1, 1, 1]) => {
    geometry.scale(...scale);
    geometry.rotateX(rotation[0]);
    geometry.rotateY(rotation[1]);
    geometry.rotateZ(rotation[2]);
    geometry.translate(...position);
    parts.push(geometry);
  };
  const lathe = (profile, segments = 14) => new THREE.LatheGeometry(profile.map(([px, py]) => new THREE.Vector2(px, py)), segments);
  if (/tower|steamstack/.test(name)) {
    add(new THREE.CylinderGeometry(0.28, 0.36, 1.1, 10, 4));
    add(new THREE.CylinderGeometry(0.34, 0.28, 0.16, 10, 1), [0, 0.62, 0]);
    add(new THREE.ConeGeometry(0.3, 0.3, 10, 1), [0, 0.85, 0]);
  } else if (/bridge/.test(name)) {
    add(new THREE.TorusGeometry(0.5, 0.07, 6, 16, Math.PI), [0, -0.2, 0]);
    add(new THREE.BoxGeometry(1.3, 0.08, 0.36, 8, 1, 2), [0, 0.32, 0]);
  } else if (/stairs|stair/.test(name)) {
    for (let i = 0; i < 5; i++) add(new THREE.BoxGeometry(0.9 - i * 0.14, 0.16, 0.6, 1, 1, 1), [0, -0.4 + i * 0.16, 0]);
  } else if (/door/.test(name)) {
    add(new THREE.DodecahedronGeometry(0.55, 0), [0, 0, 0], [0.3, 0.4, 0], [1, 1.2, 0.35]);
  } else if (/tankard|cup/.test(name)) {
    add(lathe([[0, -0.4], [0.3, -0.4], [0.32, -0.36], [0.3, 0.3], [0.33, 0.4]]));
    if (/tankard/.test(name)) add(new THREE.TorusGeometry(0.2, 0.04, 6, 12, Math.PI), [0.33, 0, 0], [0, 0, -Math.PI / 2]);
  } else if (/spear|sword/.test(name)) {
    const long = /spear/.test(name);
    add(new THREE.CylinderGeometry(0.03, 0.03, long ? 1.2 : 0.3, 6, 3), [0, long ? -0.15 : -0.5, 0]);
    add(new THREE.OctahedronGeometry(0.2, 0), [0, long ? 0.6 : 0.2, 0], [0, 0, 0], [0.45, long ? 1.2 : 3.4, 0.15]);
    add(new THREE.BoxGeometry(0.4, 0.05, 0.08), [0, long ? 0.42 : -0.34, 0]);
  } else if (/helm|helmet/.test(name)) {
    add(new THREE.SphereGeometry(0.45, 12, 6, 0, Math.PI * 2, 0, Math.PI / 2));
    add(new THREE.ConeGeometry(0.08, 0.4, 6), [0.36, 0.3, 0], [0, 0, -0.6]);
    add(new THREE.ConeGeometry(0.08, 0.4, 6), [-0.36, 0.3, 0], [0, 0, 0.6]);
  } else if (/grass|flora/.test(name)) {
    for (let i = 0; i < 9; i++) add(new THREE.PlaneGeometry(0.06, 0.7 + (i % 3) * 0.2, 1, 3), [(i - 4) * 0.1, 0, Math.sin(i) * 0.15], [0, i * 0.7, (i - 4) * 0.08]);
  } else if (/guar|kwama/.test(name)) {
    add(new THREE.IcosahedronGeometry(0.42, 1), [0, 0, 0], [0, 0, 0], [1.3, 0.8, 0.8]);
    add(new THREE.IcosahedronGeometry(0.2, 1), [0.62, 0.2, 0]);
    for (const [lx, lz] of [[-0.3, 0.2], [0.3, 0.2], [-0.3, -0.2], [0.3, -0.2]]) add(new THREE.CylinderGeometry(0.05, 0.04, 0.4, 5), [lx, -0.45, lz]);
  } else if (/cuirass|armor/.test(name)) {
    add(lathe([[0.22, -0.45], [0.36, -0.2], [0.42, 0.2], [0.26, 0.42], [0.12, 0.46]], 12), [0, 0, 0], [0, 0, 0], [1, 1, 0.6]);
  } else if (/hall|corner|doorjamb/.test(name)) {
    add(new THREE.BoxGeometry(1, 0.8, 0.2, 4, 3, 1), [0, 0, -0.4]);
    add(new THREE.BoxGeometry(0.2, 0.8, 1, 1, 3, 4), [-0.4, 0, 0]);
  } else {
    add(new THREE.TorusKnotGeometry(0.34, 0.1, 48, 6));
  }
  const group = new THREE.Group();
  const material = new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.95, depthWrite: false });
  for (const geometry of parts) {
    const edges = new THREE.EdgesGeometry(geometry, 20);
    geometry.dispose();
    group.add(new THREE.LineSegments(edges, material));
  }
  return group;
}

// Shaders ------------------------------------------------------------------------------------------------

const FULLSCREEN_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = vec4(position.xy, 0.0, 1.0);
  }
`;

// Any NaN or infinity a driver produces is zeroed and bright values capped before the bloom, which
// would otherwise smear one bad pixel into a black square.
const SCRUB = /* glsl */ `
  vec3 scrub(vec3 c) {
    if (any(isnan(c)) || any(isinf(c)) || c.r != c.r || c.g != c.g || c.b != c.b) return vec3(0.0);
    return clamp(c, 0.0, 64.0);
  }
`;

const HASH = /* glsl */ `
  float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
  }
`;

// The hex dump: offset, sixteen bytes in two groups of eight, and the bytes as ASCII, exactly as
// `xxd` or a hex editor draws them, read from the current archive's byte image. Bytes inside the
// record being read glow, and the dump brightens around the cartridge and fades behind the text.
const DUMP_FRAGMENT = /* glsl */ `
  uniform sampler2D tAtlas;
  uniform sampler2D tBytes;
  uniform float uRows;
  uniform vec2 uCell;
  uniform vec2 uResolution;
  uniform float uScroll;
  uniform vec2 uPan;
  uniform vec4 uSpan;
  uniform vec4 uSpan2;
  uniform vec3 uInk;
  uniform vec3 uHot;
  uniform vec3 uBack;
  uniform vec3 uBackTop;
  uniform vec3 uFocus;
  uniform vec4 uTextBox;
  uniform float uFade;
  uniform float uTime;
  varying vec2 vUv;
  ${SCRUB}

  // The cell coordinate jumps at every cell edge, so the gradients are given, not derived: a
  // derived one would pick the coarsest mip along each edge and outline every glyph.
  float glyphAlpha(float code, vec2 cell) {
    float index = code - 32.0;
    vec2 tile = vec2(mod(index, 16.0), floor(index / 16.0));
    vec2 inner = vec2(0.06, 0.04) + cell * vec2(0.88, 0.92);
    vec2 uv = vec2((tile.x + inner.x) / 16.0, 1.0 - (tile.y + inner.y) / 6.0);
    vec2 gradX = vec2(0.88 / (16.0 * uCell.x), 0.0);
    vec2 gradY = vec2(0.0, 0.92 / (6.0 * uCell.y));
    return textureGrad(tAtlas, uv, gradX, gradY).a;
  }
  float hexCode(float nibble) {
    return nibble < 10.0 ? 48.0 + nibble : 87.0 + nibble;
  }
  float byteAt(float index) {
    float row = floor(index / 16.0);
    float column = index - row * 16.0;
    return floor(texture2D(tBytes, vec2((column + 0.5) / 16.0, (row + 0.5) / uRows)).r * 255.0 + 0.5);
  }
  bool inside(float index, vec4 span) {
    return (index >= span.x && index < span.y) || (index >= span.z && index < span.w);
  }

  void main() {
    vec2 pixel = vec2(gl_FragCoord.x, uResolution.y - gl_FragCoord.y) + vec2(uPan.x, uScroll + uPan.y);
    vec3 back = mix(uBackTop, uBack, clamp(gl_FragCoord.y / max(uResolution.y, 1.0), 0.0, 1.0));
    float column = floor(pixel.x / uCell.x);
    float line = floor(pixel.y / uCell.y);
    vec2 cell = fract(pixel / uCell);
    float block = floor(column / 82.0);
    float c = column - block * 82.0;
    float row = mod(line + block * 29.0, uRows);
    float code = 32.0;
    float byteIndex = -1.0;
    float weight = 0.55;
    if (c < 8.0) {
      float offset = row * 16.0;
      float nibble = mod(floor(offset / exp2(4.0 * (7.0 - c))), 16.0);
      code = hexCode(nibble);
      weight = 0.34;
    } else if ((c >= 10.0 && c < 34.0) || (c >= 35.0 && c < 59.0)) {
      float k = c < 34.0 ? c - 10.0 : c - 35.0;
      float i = floor(k / 3.0) + (c < 34.0 ? 0.0 : 8.0);
      float within = k - floor(k / 3.0) * 3.0;
      if (within < 2.0) {
        byteIndex = row * 16.0 + i;
        float value = byteAt(byteIndex);
        code = hexCode(within < 0.5 ? floor(value / 16.0) : mod(value, 16.0));
        weight = value < 0.5 ? 0.16 : 0.66;
      }
    } else if (c == 60.0 || c == 77.0) {
      code = 124.0;
      weight = 0.3;
    } else if (c >= 61.0 && c < 77.0) {
      byteIndex = row * 16.0 + (c - 61.0);
      float value = byteAt(byteIndex);
      code = value >= 32.0 && value < 127.0 ? value : 46.0;
      weight = value >= 32.0 && value < 127.0 ? 0.72 : 0.22;
    }
    float alpha = code > 32.5 ? glyphAlpha(code, cell) : 0.0;
    float blockLeft = block * 82.0 * uCell.x - uPan.x;
    bool whole = blockLeft >= 0.0 && blockLeft + 78.0 * uCell.x <= uResolution.x;
    bool hot = whole && byteIndex >= 0.0 && inside(byteIndex, uSpan);
    bool warm = whole && byteIndex >= 0.0 && inside(byteIndex, uSpan2);
    vec2 screen = gl_FragCoord.xy;
    float focus = exp(-dot(screen - uFocus.xy, screen - uFocus.xy) / max(uFocus.z * uFocus.z, 1.0));
    vec2 p = gl_FragCoord.xy;
    float outside = max(max(uTextBox.x - p.x, p.x - uTextBox.z), max(uTextBox.y - p.y, p.y - uTextBox.w));
    float textSide = smoothstep(-30.0, 240.0, outside);
    float light = weight * mix(0.16, 1.0, textSide) * (0.22 + 0.78 * focus) * uFade;
    vec3 ink = hot ? uHot * 1.5 : warm ? mix(uInk, uHot, 0.5) * 1.15 : uInk;
    float hotRow = hot || warm ? textSide : 0.0;
    if (!(hot || warm) || textSide < 0.5) ink = uInk;
    vec3 color = back + ink * alpha * (light + hotRow * 0.45 * uFade);
    gl_FragColor = vec4(scrub(color), 1.0);
  }
`;

// A compressed payload before it is inflated: a dense cube of shimmering noise.
const NUGGET_VERTEX = /* glsl */ `
  varying vec3 vLocal;
  varying vec3 vNormalView;
  varying vec3 vView;
  void main() {
    vLocal = position;
    vec4 view = modelViewMatrix * vec4(position, 1.0);
    vView = -view.xyz;
    vNormalView = normalMatrix * normal;
    gl_Position = projectionMatrix * view;
  }
`;
const NUGGET_FRAGMENT = /* glsl */ `
  uniform vec3 uColor;
  uniform float uTime;
  uniform float uOpacity;
  varying vec3 vLocal;
  varying vec3 vNormalView;
  varying vec3 vView;
  ${HASH}
  void main() {
    vec3 grid = floor(vLocal * 22.0 + 11.0);
    float n = hash12(grid.xy + grid.z * 7.0 + floor(uTime * 12.0));
    float rim = 1.0 - clamp(abs(dot(normalize(vNormalView + 1e-5), normalize(vView + 1e-5))), 0.0, 1.0);
    vec3 color = uColor * (0.25 + n * n * 2.2) + uColor * rim * rim * 1.8;
    gl_FragColor = vec4(color * uOpacity, 1.0);
  }
`;

// The secret's tiles: each shows its patch of the mip atlas, and a sweep of light crosses them.
const TILE_VERTEX = /* glsl */ `
  attribute vec4 aRect;
  attribute float aLevel;
  varying vec2 vUv;
  varying vec2 vLocal;
  varying float vLevel;
  varying vec3 vWorld;
  void main() {
    vUv = aRect.xy + uv * aRect.zw;
    vLocal = uv;
    vLevel = aLevel;
    vec4 world = modelMatrix * instanceMatrix * vec4(position, 1.0);
    vWorld = world.xyz;
    gl_Position = projectionMatrix * viewMatrix * world;
  }
`;
const TILE_FRAGMENT = /* glsl */ `
  uniform sampler2D tAtlas;
  uniform float uSweep;
  uniform float uGlow;
  uniform vec3 uEdge;
  varying vec2 vUv;
  varying vec2 vLocal;
  varying float vLevel;
  varying vec3 vWorld;
  void main() {
    vec3 color = texture2D(tAtlas, vUv).rgb;
    vec2 edge = min(vLocal, 1.0 - vLocal);
    float frame = 1.0 - smoothstep(0.0, 0.05, min(edge.x, edge.y));
    float sweep = exp(-abs(vWorld.x - uSweep) * 6.0);
    color = color * (0.85 + uGlow * 0.6) + uEdge * frame * (0.35 + sweep * 2.0) + color * sweep * 1.4;
    gl_FragColor = vec4(color, 1.0);
  }
`;

const BRIGHT_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform float uThreshold;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    vec3 c = scrub(texture2D(tInput, vUv).rgb);
    float luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    gl_FragColor = vec4(c * smoothstep(uThreshold, uThreshold + 0.8, luma), 1.0);
  }
`;

const BLUR_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform vec2 uDirection;
  varying vec2 vUv;
  void main() {
    vec3 sum = texture2D(tInput, vUv).rgb * 0.2270270270;
    sum += texture2D(tInput, vUv + uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv - uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv + uDirection * 3.2307692308).rgb * 0.0702702703;
    sum += texture2D(tInput, vUv - uDirection * 3.2307692308).rgb * 0.0702702703;
    gl_FragColor = vec4(sum, 1.0);
  }
`;

const COMPOSITE_FRAGMENT = /* glsl */ `
  uniform sampler2D tScene;
  uniform sampler2D tBloomNear;
  uniform sampler2D tBloomFar;
  uniform float uTime;
  uniform float uFlash;
  varying vec2 vUv;
  vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
  }
  float dither(vec2 p) {
    return fract(sin(dot(p + fract(uTime), vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
  }
  ${SCRUB}
  void main() {
    vec3 color = scrub(texture2D(tScene, vUv).rgb);
    color += scrub(texture2D(tBloomNear, vUv).rgb) * (0.55 + uFlash * 0.4) + scrub(texture2D(tBloomFar, vUv).rgb) * (0.45 + uFlash * 0.5);
    color = aces(color * 0.95);
    color = pow(max(color, vec3(0.0)), vec3(1.0 / 2.2));
    color += dither(gl_FragCoord.xy) / 255.0;
    gl_FragColor = vec4(color, 1.0);
  }
`;

function fullscreenMaterial(fragmentShader, uniforms) {
  return new THREE.ShaderMaterial({ vertexShader: FULLSCREEN_VERTEX, fragmentShader, uniforms, depthTest: false, depthWrite: false });
}

// Glass: a physical material with a fresnel rim that glows in the site's accent.
function glassMaterial(accent) {
  const material = new THREE.MeshPhysicalMaterial({
    color: new THREE.Color(0.2, 0.25, 0.32),
    metalness: 0,
    roughness: 0.14,
    clearcoat: 0.7,
    clearcoatRoughness: 0.12,
    transparent: true,
    opacity: 0.16,
    depthWrite: false,
    envMapIntensity: 1.1,
    side: THREE.DoubleSide,
  });
  const rim = { uRim: { value: accent.clone().multiplyScalar(0.9) } };
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, rim);
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nuniform vec3 uRim;')
      .replace('#include <emissivemap_fragment>', `#include <emissivemap_fragment>
        float facing = clamp(abs(dot(normalize(normal), normalize(vViewPosition))), 0.0, 1.0);
        float edge = 1.0 - facing;
        float edge2 = edge * edge;
        totalEmissiveRadiance += uRim * edge2 * edge2 * 0.45;
        diffuseColor.a = clamp(diffuseColor.a + edge2 * 0.22, 0.0, 0.5);`);
  };
  material.customProgramCacheKey = () => 'dream-archive-glass';
  return { material, rim };
}

// A card: its canvas is its colour and its glow. It dissolves in and out along a noise, and its
// preview can be drawn as a coarser mip, down to one texel, while a DX10 texture arrives.
function cardMaterial(texture) {
  const material = new THREE.MeshStandardMaterial({ map: texture, roughness: 0.42, metalness: 0.0, transparent: false, alphaTest: 0.5, side: THREE.DoubleSide, envMapIntensity: 0.6 });
  const uniforms = {
    uReveal: { value: 1 },
    uCells: { value: 0 },
    uGlow: { value: 0.85 },
    uEdgeColor: { value: new THREE.Color(1, 1, 1) },
    uPreview: { value: new THREE.Vector4(CARD.preview.x / CARD.w, 1 - (CARD.preview.y + CARD.preview.size) / CARD.h, CARD.preview.size / CARD.w, CARD.preview.size / CARD.h) },
  };
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>
        uniform float uReveal;
        uniform float uCells;
        uniform float uGlow;
        uniform vec3 uEdgeColor;
        uniform vec4 uPreview;
        ${HASH}`)
      .replace('#include <map_fragment>', `
        vec2 cardUv = vMapUv;
        vec2 local = (cardUv - uPreview.xy) / uPreview.zw;
        if (uCells > 0.5 && local.x >= 0.0 && local.x <= 1.0 && local.y >= 0.0 && local.y <= 1.0) {
          local = (floor(local * uCells) + 0.5) / uCells;
          cardUv = uPreview.xy + local * uPreview.zw;
        }
        vec4 sampledDiffuseColor = texture2D(map, cardUv);
        diffuseColor *= sampledDiffuseColor;
        float grain = hash12(floor(vMapUv * vec2(128.0, 72.0)));
        if (grain > uReveal) discard;
        float burn = smoothstep(uReveal - 0.12, uReveal, grain) * step(uReveal, 0.999);`)
      .replace('#include <emissivemap_fragment>', `#include <emissivemap_fragment>
        totalEmissiveRadiance += diffuseColor.rgb * uGlow + uEdgeColor * burn * 1.2;`);
  };
  material.customProgramCacheKey = () => 'dream-archive-card';
  return { material, uniforms };
}

// Layout ---------------------------------------------------------------------------------------------------

// Where the hero's words and controls are, so the scene can stand clear of them.
function textRects(text) {
  const rects = [];
  const range = document.createRange();
  const walker = document.createTreeWalker(text, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (node.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    range.selectNodeContents(node);
    for (const rect of range.getClientRects()) rects.push(rect);
  }
  for (const element of text.querySelectorAll('a, button, input, select, img, svg, .dw-command, .dw-badge')) rects.push(element.getBoundingClientRect());
  return rects.filter((rect) => rect.width > 0 && rect.height > 0);
}

// The largest ASPECT-shaped box clear of the text: beside it all, beside the title rows, or above
// it all where the stylesheet leaves room on a phone. Relative to the art; `height` is the scene's.
function placement(root) {
  const hero = root.closest('.dw-hero') || root.parentElement;
  const box = root.getBoundingClientRect();
  const text = hero.querySelector('.dw-hero__text') || hero.querySelector('.dw-shell');
  const strip = hero.querySelector('.dw-strip');
  const shellElement = hero.querySelector('.dw-hero__grid') || hero.querySelector('.dw-shell') || hero;
  const shellStyle = getComputedStyle(shellElement);
  const shellBox = shellElement.getBoundingClientRect();
  const shell = strip ? strip.getBoundingClientRect() : { left: shellBox.left + parseFloat(shellStyle.paddingLeft), right: shellBox.right - parseFloat(shellStyle.paddingRight) };
  const summary = hero.querySelector('.dw-hero__summary');
  const floor = strip ? strip.getBoundingClientRect().top : box.bottom - 24;
  const rects = text ? textRects(text) : [];
  if (!rects.length) return { x: box.width * 0.72, y: box.height * 0.45, height: Math.min(box.width * 0.3, box.height * 0.7), above: false, textBox: { left: 0, top: 0, right: 0, bottom: 0 } };
  const gap = 28;
  const right = Math.max(...rects.map((rect) => rect.right));
  const top = Math.min(...rects.map((rect) => rect.top));
  const summaryTop = summary ? summary.getBoundingClientRect().top : floor;
  const headRects = rects.filter((rect) => rect.bottom <= summaryTop + 1);
  const headRight = headRects.length ? Math.max(...headRects.map((rect) => rect.right)) : right;
  const candidates = [
    { x0: right + gap, x1: shell.right + 12, y0: box.top + 8, y1: floor - 10, above: false },
    { x0: headRight + gap, x1: shell.right + 12, y0: box.top + 8, y1: summaryTop - 8, above: false },
    { x0: shell.left - 8, x1: shell.right + 8, y0: box.top + 6, y1: top - 10, above: true },
  ].map((region) => ({ ...region, height: Math.max(0, Math.min(region.y1 - region.y0, (region.x1 - region.x0) / ASPECT)) }));
  const best = candidates.reduce((a, b) => (b.height > a.height ? b : a));
  const height = Math.min(best.height, 470);
  const width = height * ASPECT;
  const x = best.above ? (best.x0 + best.x1) / 2 : Math.min(best.x1 - width / 2, (best.x0 + best.x1) / 2 + (best.x1 - best.x0 - width) * 0.3);
  const textBox = {
    left: Math.min(...rects.map((rect) => rect.left)) - box.left,
    top: top - box.top,
    right: right - box.left,
    bottom: Math.max(floor, ...rects.map((rect) => rect.bottom)) - box.top,
  };
  return { x: x - box.left, y: (best.y0 + best.y1) / 2 - box.top, height, above: best.above, textBox };
}

function cssColor(name, fallback) {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const color = new THREE.Color();
  try {
    color.setStyle(value || fallback);
  } catch {
    color.setStyle(fallback);
  }
  return color;
}

function cssText(name, fallback) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

// A studio for the steel and glass to reflect: strip lights in the accent, a warm key, a cool fill.
function environment(renderer, accent) {
  const scene = new THREE.Scene();
  const disposables = [];
  const add = (geometry, color, position) => {
    const material = new THREE.MeshBasicMaterial({ color, side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(...position);
    mesh.lookAt(0, 0, 0);
    scene.add(mesh);
    disposables.push(geometry, material);
  };
  const room = new THREE.Mesh(new THREE.BoxGeometry(24, 16, 24), new THREE.MeshBasicMaterial({ color: new THREE.Color(0.02, 0.025, 0.035), side: THREE.BackSide }));
  scene.add(room);
  disposables.push(room.geometry, room.material);
  add(new THREE.PlaneGeometry(10, 1.2), new THREE.Color(1.0, 0.94, 0.86).multiplyScalar(2.2), [-6, 6, 7]);
  add(new THREE.PlaneGeometry(1.0, 12), accent.clone().multiplyScalar(1.5), [8, 0, 5]);
  add(new THREE.PlaneGeometry(1.0, 12), accent.clone().multiplyScalar(1.1), [-9, 0, 3]);
  add(new THREE.PlaneGeometry(14, 0.5), new THREE.Color(0.7, 0.85, 1.0).multiplyScalar(1.6), [0, 7, -8]);
  add(new THREE.PlaneGeometry(20, 20), new THREE.Color(0.08, 0.1, 0.14), [0, -7, 0]);
  const generator = new THREE.PMREMGenerator(renderer);
  const target = generator.fromScene(scene, 0.03);
  generator.dispose();
  for (const item of disposables) item.dispose();
  return target;
}

function roundedSlab(width, height, depth, radius) {
  const shape = new THREE.Shape();
  const x = -width / 2;
  const y = -height / 2;
  shape.moveTo(x + radius, y);
  shape.lineTo(x + width - radius, y);
  shape.quadraticCurveTo(x + width, y, x + width, y + radius);
  shape.lineTo(x + width, y + height - radius);
  shape.quadraticCurveTo(x + width, y + height, x + width - radius, y + height);
  shape.lineTo(x + radius, y + height);
  shape.quadraticCurveTo(x, y + height, x, y + height - radius);
  shape.lineTo(x, y + radius);
  shape.quadraticCurveTo(x, y, x + radius, y);
  const geometry = new THREE.ExtrudeGeometry(shape, { depth: depth - 0.04, bevelEnabled: true, bevelThickness: 0.02, bevelSize: 0.02, bevelSegments: 3, curveSegments: 6 });
  geometry.translate(0, 0, -(depth - 0.04) / 2);
  geometry.computeVertexNormals();
  return geometry;
}

// Flap faces: a plane showing the top or bottom half of one glyph, turned half a turn for a flap's
// back so it reads upright once the flap has fallen.
function flapGeometry(width, height, glyph, half, flipped) {
  const geometry = new THREE.PlaneGeometry(width, height / 2);
  geometry.translate(0, half === 'top' ? height / 4 : -height / 4, 0);
  setFlapUv(geometry, glyph, half, flipped);
  return geometry;
}

function setFlapUv(geometry, glyph, half, flipped) {
  const count = FLAP_GLYPHS.length;
  const u0 = glyph / count + 0.5 / (FLAP_FACE.w * count);
  const u1 = (glyph + 1) / count - 0.5 / (FLAP_FACE.w * count);
  const [v0, v1] = half === 'top' ? [0.5, 1] : [0, 0.5];
  const uv = geometry.attributes.uv;
  const corners = flipped ? [[u1, v0], [u0, v0], [u1, v1], [u0, v1]] : [[u0, v1], [u1, v1], [u0, v0], [u1, v0]];
  corners.forEach(([u, v], i) => uv.setXY(i, u, v));
  uv.needsUpdate = true;
}

const ease = (t) => (t <= 0 ? 0 : t >= 1 ? 1 : t * t * (3 - 2 * t));
const easeOut = (t) => (t <= 0 ? 0 : t >= 1 ? 1 : 1 - (1 - t) ** 3);
const window01 = (t, a, b) => clamp01((t - a) / (b - a));

// The cartridge's shape, in its own units: the scene is 2.4 tall.
const CART = { w: 0.92, h: 1.36, d: 0.46 };
const SCENE_HEIGHT = 2.4;
const CART_X = -0.82;
const STACK = { x: 0.46, y: 0.5, pitch: 0.36, depth: 0.2, turn: -0.26, keep: 4 };
const SECRET_WORD = 'btdx';

// Everything the scene can draw, in the rig's units, at its extremes: the cartridge at every lean
// with its lower shell dropped, the card stack and a card in flight, the lookup tag, and the
// secret's mip chain and eruption. Points, not boxes: the camera's perspective is applied to them.
function sceneReach() {
  const points = [];
  const euler = new THREE.Euler();
  const add = (x, y, z) => points.push(new THREE.Vector3(x, y, z));
  // The crown sways a little; the lower shell, dropped by the secret, also tilts.
  const tilt = new THREE.Euler();
  for (const [y, rz] of [[CART.h / 2 + 0.39, 0.015], [-CART.h / 2 - 0.07 - 0.4, 0.06]]) {
    for (const x of [-0.53, 0.53]) for (const z of [-0.26, 0.26]) for (const sway of [-rz, rz]) {
      tilt.set(0, 0, sway);
      const corner = new THREE.Vector3(x, y, z).applyEuler(tilt);
      for (const rx of [-0.17, 0, 0.17]) for (const ry of [-0.76, -0.55, -0.32, -0.1, 0.12]) {
        euler.set(rx, ry, 0.012);
        const v = corner.clone().applyEuler(euler);
        add(v.x + CART_X, v.y + 0.025, v.z);
      }
    }
  }
  const cardHalf = { w: 0.61, h: 0.61 * (CARD.h / CARD.w) };
  const cardCorners = (center, size, turn) => {
    euler.set(0.03, turn, 0);
    for (const sx of [-1, 1]) for (const sy of [-1, 1]) {
      const v = new THREE.Vector3(sx * cardHalf.w * size, sy * cardHalf.h * size, 0).applyEuler(euler);
      add(center.x + v.x, center.y + v.y, center.z + v.z);
    }
  };
  for (let k = 0; k <= STACK.keep; k++) cardCorners(new THREE.Vector3(STACK.x + k * 0.07, STACK.y - k * STACK.pitch, 0.22 - k * STACK.depth), 1 - k * 0.07, STACK.turn);
  for (const ry of [-0.76, 0.12]) {
    euler.set(0, ry, 0);
    const start = new THREE.Vector3(CART.w / 2, 0.6, 0.05).applyEuler(euler);
    start.x += CART_X;
    const slot = new THREE.Vector3(STACK.x, STACK.y, 0.22);
    for (let step = 0; step <= 10; step++) {
      const out = easeOut(step / 10);
      const at = start.clone().lerp(slot, out);
      at.y += Math.sin(out * Math.PI) * 0.28;
      at.z += Math.sin(out * Math.PI) * 0.4;
      cardCorners(at, 0.3 + 0.7 * out, STACK.turn * out);
    }
  }
  for (const x of [-0.85, 0.85]) for (const y of [-0.13, 0.13]) add(CART_X + 0.12 + x, -CART.h / 2 - 0.2 + y, 0.45);
  for (const x of [-1.42, 1.3]) for (const y of [0.98, -0.86]) add(x, y, 0.62);
  for (let i = 0; i < 24; i++) {
    const angle = (i / 24) * Math.PI * 2;
    const x = Math.cos(angle) * 1.1 * 1.2;
    const z = Math.sin(angle) * 1.1 * 0.45 + 0.3;
    for (const dx of [-0.14, 0.14]) for (const y of [-0.42, 0.84]) for (const dz of [-0.14, 0.14]) add(x + dx, y, z + dz);
  }
  return points;
}
const SECRET_LENGTH = 9.4;

// The scene ------------------------------------------------------------------------------------------------

async function mount(root) {
  const still = document.createElement('img');
  still.className = 'da-hero__still';
  still.alt = '';
  still.decoding = 'async';
  still.src = new URL('../img/dream-archive-still.webp', import.meta.url).href;
  root.append(still);

  function placeStill() {
    const spot = placement(root);
    const width = spot.height * ASPECT;
    Object.assign(still.style, { left: `${spot.x - width / 2}px`, top: `${spot.y - spot.height / 2}px`, width: `${width}px`, height: `${spot.height}px` });
    root.classList.add('is-placed');
  }
  placeStill();

  const canvas = document.createElement('canvas');
  canvas.className = 'da-hero__canvas';
  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: 'high-performance' });
  } catch {
    return;
  }
  if (!renderer.capabilities.isWebGL2) {
    renderer.dispose();
    return;
  }
  if (document.fonts) {
    await Promise.race([
      Promise.all([document.fonts.load(`700 24px ${MONO}`), document.fonts.load(`500 24px ${MONO}`)]).catch(() => null),
      new Promise((resolve) => setTimeout(resolve, 1500)),
    ]);
  }
  renderer.autoClear = false;
  renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
  root.append(canvas);

  const floatTargets = renderer.extensions.has('EXT_color_buffer_float') || renderer.extensions.has('EXT_color_buffer_half_float');
  const targetType = floatTargets ? THREE.HalfFloatType : THREE.UnsignedByteType;
  const makeTarget = () => new THREE.WebGLRenderTarget(1, 1, { type: targetType, depthBuffer: false });
  const sceneTarget = new THREE.WebGLRenderTarget(1, 1, { type: targetType, samples: 4 });
  const bloomTargets = [makeTarget(), makeTarget(), makeTarget(), makeTarget()];
  const anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
  const small = Math.min(innerWidth, innerHeight) < 700;

  // Colours, from the site's tokens.
  const accent = cssColor('--dw-accent', '#a9bdd8');
  const bg0 = cssColor('--dw-bg-0', '#090b0f');
  const bg1 = cssColor('--dw-bg-1', '#101318');
  const kindColor = {
    texture: cssText('--dw-accent', '#a9bdd8'),
    mesh: cssText('--dw-warn', '#ecd18f'),
    animation: cssText('--dw-ok', '#8fdcb0'),
    sound: cssText('--dw-info', '#9cc7ff'),
    material: cssText('--dw-danger', '#ffaea2'),
    shader: cssText('--dw-danger', '#ffaea2'),
    text: cssText('--dw-text-muted', '#b3b9c2'),
  };
  const cardColors = { accent: cssText('--dw-accent', '#a9bdd8'), text: cssText('--dw-text', '#e5e8ed'), faint: cssText('--dw-text-faint', '#898f98'), line: cssText('--dw-line-strong', '#3c4653'), kinds: kindColor };
  const kindThree = Object.fromEntries(Object.entries(kindColor).map(([kind, value]) => [kind, new THREE.Color(value)]));

  // The archives, and the byte image each dumps.
  const archives = [tes3Archive(), tes4Archive(), ba2Archive()];
  for (const archive of archives) {
    const bytes = archive.bytes.bytes;
    const rows = Math.ceil(bytes.length / 16) + 2;
    const data = new Uint8Array(rows * 16);
    data.set(bytes);
    archive.rows = rows;
    archive.texture = new THREE.DataTexture(data, 16, rows, THREE.RedFormat, THREE.UnsignedByteType);
    archive.texture.minFilter = THREE.NearestFilter;
    archive.texture.magFilter = THREE.NearestFilter;
    archive.texture.needsUpdate = true;
  }

  const camera = new THREE.PerspectiveCamera(30, 1, 0.1, 80);
  camera.position.set(0, 0.6, 10);
  camera.lookAt(0, 0, 0);
  const scene = new THREE.Scene();
  const envTarget = environment(renderer, accent);
  scene.environment = envTarget.texture;

  // The hex dump, drawn first, as the background.
  const quad = new THREE.PlaneGeometry(2, 2);
  const dumpUniforms = {
    tAtlas: { value: dumpAtlas() },
    tBytes: { value: archives[0].texture },
    uRows: { value: archives[0].rows },
    uCell: { value: new THREE.Vector2(9, 16) },
    uResolution: { value: new THREE.Vector2(1, 1) },
    uScroll: { value: 0 },
    uPan: { value: new THREE.Vector2() },
    uSpan: { value: new THREE.Vector4(-1, -1, -1, -1) },
    uSpan2: { value: new THREE.Vector4(-1, -1, -1, -1) },
    uInk: { value: accent.clone().multiplyScalar(0.28) },
    uHot: { value: accent.clone().lerp(new THREE.Color(1, 1, 1), 0.35) },
    uBack: { value: bg0.clone() },
    uBackTop: { value: bg1.clone() },
    uFocus: { value: new THREE.Vector3(0, 0, 400) },
    uTextBox: { value: new THREE.Vector4(-1, -1, -1, -1) },
    uFade: { value: 1 },
    uTime: { value: 0 },
  };
  const dump = new THREE.Mesh(quad, fullscreenMaterial(DUMP_FRAGMENT, dumpUniforms));
  dump.frustumCulled = false;
  const dumpScene = new THREE.Scene();
  dumpScene.add(dump);

  // The rig: everything that belongs to the cartridge, in its own units.
  const rig = new THREE.Group();
  scene.add(rig);
  const cartridge = new THREE.Group();
  rig.add(cartridge);
  cartridge.position.x = CART_X;

  const steel = new THREE.MeshPhysicalMaterial({ color: new THREE.Color(0.55, 0.58, 0.62), metalness: 1, roughness: 0.36, anisotropy: 0.6, clearcoat: 0.2, clearcoatRoughness: 0.3, envMapIntensity: 0.9 });
  const darkSteel = new THREE.MeshPhysicalMaterial({ color: new THREE.Color(0.12, 0.13, 0.15), metalness: 1, roughness: 0.38, envMapIntensity: 1.0 });

  // The glass, in two halves that meet under a steel band.
  const glass = glassMaterial(accent);
  const halfGeometry = roundedSlab(CART.w, CART.h / 2, CART.d, 0.05);
  const upper = new THREE.Mesh(halfGeometry, glass.material);
  const lower = new THREE.Mesh(halfGeometry, glass.material);
  upper.position.y = CART.h / 4;
  lower.position.y = -CART.h / 4;
  upper.renderOrder = 10;
  lower.renderOrder = 10;
  const band = new THREE.Mesh(new THREE.BoxGeometry(CART.w + 0.03, 0.035, CART.d + 0.03), steel);
  const upperShell = new THREE.Group();
  const lowerShell = new THREE.Group();
  upperShell.add(upper);
  lowerShell.add(lower, band);
  cartridge.add(upperShell, lowerShell);

  // Corner rails and a foot, in brushed steel.
  for (const [sx, sz] of [[-1, -1], [1, -1], [-1, 1], [1, 1]]) {
    const top = new THREE.Mesh(new THREE.BoxGeometry(0.034, CART.h / 2, 0.034), darkSteel);
    top.position.set(sx * (CART.w / 2 + 0.012), CART.h / 4, sz * (CART.d / 2 + 0.012));
    upperShell.add(top);
    const bottom = top.clone();
    bottom.position.y = -CART.h / 4;
    lowerShell.add(bottom);
  }
  const foot = new THREE.Mesh(new THREE.BoxGeometry(CART.w + 0.1, 0.06, CART.d + 0.1), darkSteel);
  foot.position.y = -CART.h / 2 - 0.03;
  lowerShell.add(foot);

  // The crown: a steel housing with the four-cell split-flap display.
  const housing = new THREE.Mesh(new THREE.BoxGeometry(CART.w + 0.1, 0.38, 0.3), darkSteel);
  housing.position.set(0, CART.h / 2 + 0.19, 0);
  upperShell.add(housing);
  const flapTextures = flapAtlas('#e9eef5', '#ffffff');
  const cell = { w: 0.2, h: 0.3, pitch: 0.225 };
  const cells = [];
  for (let i = 0; i < 4; i++) {
    const material = new THREE.MeshStandardMaterial({ map: flapTextures.map, emissiveMap: flapTextures.emissive, emissive: accent.clone(), emissiveIntensity: 0.55, roughness: 0.5, metalness: 0.2, envMapIntensity: 0.5 });
    const group = new THREE.Group();
    group.position.set((i - 1.5) * cell.pitch, CART.h / 2 + 0.19, 0.152);
    const glyph = FLAP_GLYPHS.indexOf(archives[0].flaps[i]);
    const top = new THREE.Mesh(flapGeometry(cell.w, cell.h, glyph, 'top', false), material);
    const bottom = new THREE.Mesh(flapGeometry(cell.w, cell.h, glyph, 'bottom', false), material);
    const pivot = new THREE.Group();
    pivot.position.z = 0.004;
    const front = new THREE.Mesh(flapGeometry(cell.w, cell.h, glyph, 'top', false), material);
    const backGeometry = flapGeometry(cell.w, cell.h, glyph, 'top', true);
    backGeometry.rotateY(Math.PI);
    const back = new THREE.Mesh(backGeometry, material);
    pivot.add(front, back);
    pivot.visible = false;
    const frame = new THREE.Mesh(new THREE.BoxGeometry(cell.w + 0.02, cell.h + 0.02, 0.01), steel);
    frame.position.z = -0.006;
    group.add(frame, top, bottom, pivot);
    upperShell.add(group);
    cells.push({ group, top, bottom, pivot, front, back, material, glyph, queue: [], progress: 0, glow: 0, from: glyph });
  }

  // The strata: records, the name table, and the payload.
  const MAX_MEMBERS = 40;
  const recordBars = new THREE.InstancedMesh(new THREE.BoxGeometry(1, 1, 1), new THREE.MeshBasicMaterial({ toneMapped: false }), MAX_MEMBERS);
  recordBars.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
  recordBars.setColorAt(0, new THREE.Color());
  const noiseCanvas = document.createElement('canvas');
  noiseCanvas.width = 128;
  noiseCanvas.height = 128;
  {
    const context = noiseCanvas.getContext('2d');
    const rand = random(7);
    for (let y = 0; y < 128; y += 2) for (let x = 0; x < 128; x += 2) {
      const v = Math.floor(90 + rand() * 165);
      context.fillStyle = `rgb(${v},${v},${v})`;
      context.fillRect(x, y, 2, 2);
    }
  }
  const noiseTexture = new THREE.CanvasTexture(noiseCanvas);
  noiseTexture.wrapS = THREE.RepeatWrapping;
  noiseTexture.wrapT = THREE.RepeatWrapping;
  noiseTexture.magFilter = THREE.NearestFilter;
  const blockMaterial = new THREE.MeshPhysicalMaterial({ roughness: 0.35, metalness: 0.15, clearcoat: 0.6, emissive: new THREE.Color(1, 1, 1), emissiveIntensity: 0.1, envMapIntensity: 0.9 });
  const blockEmissive = { uGlowBlock: { value: -1 }, uCompressed: { value: 0 }, uPulse: { value: 0 } };
  blockMaterial.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, blockEmissive);
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying float vInstance;\nvarying vec3 vBlockLocal;')
      .replace('#include <begin_vertex>', '#include <begin_vertex>\nvInstance = float(gl_InstanceID);\nvBlockLocal = position;');
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>\nvarying float vInstance;\nvarying vec3 vBlockLocal;\nuniform float uGlowBlock;\nuniform float uCompressed;\nuniform float uPulse;\n${HASH}`)
      .replace('#include <emissivemap_fragment>', `#include <emissivemap_fragment>
        float chosen = 1.0 - step(0.5, abs(vInstance - uGlowBlock));
        float grain = hash12(floor(vBlockLocal.xy * vec2(48.0, 14.0) + vInstance * 13.0));
        float packed = mix(1.0, 0.35 + grain * 1.3, uCompressed);
        totalEmissiveRadiance = diffuseColor.rgb * (0.16 * packed + chosen * 1.6 + uPulse * 0.9);`);
  };
  blockMaterial.customProgramCacheKey = () => 'dream-archive-block';
  const blocks = new THREE.InstancedMesh(new THREE.BoxGeometry(1, 1, 1), blockMaterial, MAX_MEMBERS * 3);
  blocks.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
  blocks.setColorAt(0, new THREE.Color());
  const strata = new THREE.Group();
  strata.add(recordBars, blocks);
  cartridge.add(strata);

  const tableCanvas = document.createElement('canvas');
  tableCanvas.width = 512;
  tableCanvas.height = 96;
  const tableTexture = new THREE.CanvasTexture(tableCanvas);
  tableTexture.colorSpace = THREE.SRGBColorSpace;
  const table = new THREE.Mesh(new THREE.PlaneGeometry(CART.w - 0.14, 0.13), new THREE.MeshBasicMaterial({ map: tableTexture, transparent: true, toneMapped: false, depthWrite: false }));
  table.position.set(0, 0.235, 0.05);
  strata.add(table);

  function paintTable(archive) {
    const context = tableCanvas.getContext('2d');
    context.clearRect(0, 0, 512, 96);
    context.font = `500 11px ${MONO}`;
    context.fillStyle = cardColors.accent;
    context.globalAlpha = 0.9;
    let x = 4;
    let y = 12;
    for (const member of [...archive.members, ...archive.members]) {
      const text = `${member.path}␀`;
      const width = context.measureText(text).width + 6;
      if (x + width > 508) {
        x = 4;
        y += 14;
        if (y > 92) break;
      }
      context.fillText(text, x, y);
      x += width;
    }
    tableTexture.needsUpdate = true;
  }

  // Layouts for each archive: bar and block transforms, and which block belongs to which member.
  const dummy = new THREE.Object3D();
  function strataLayout(archive) {
    const members = archive.members;
    const bars = [];
    const pitch = 0.2 / Math.max(members.length, 6);
    const maxSize = Math.max(...members.map((member) => member.size || 1));
    members.forEach((member, index) => {
      const fraction = member.size ? Math.log(member.size) / Math.log(maxSize) : 0.55 + ((seedOf(member.path) % 100) / 100) * 0.4;
      const width = 0.2 + fraction * 0.5;
      bars.push({ position: [-0.36 + width / 2, 0.6 - (index + 0.5) * pitch, -0.02], scale: [width, pitch * 0.62, 0.24], color: kindThree[member.kind].clone().multiplyScalar(0.55) });
    });
    const pieces = [];
    const top = 0.15;
    const bottom = -0.62;
    if (archive.id === 'tes3') {
      const weights = members.map((member) => Math.sqrt(member.size));
      const total = weights.reduce((a, b) => a + b, 0);
      const available = top - bottom - members.length * 0.006;
      let y = top;
      members.forEach((member, index) => {
        const height = (weights[index] / total) * available;
        pieces.push({ member: index, position: [0, y - height / 2, 0], scale: [0.74, Math.max(height, 0.004), 0.3] });
        y -= height + 0.006;
      });
    } else if (archive.id === 'tes4') {
      const groups = archive.folders.map((folder) => members.map((member, index) => ({ member, index })).filter(({ member }) => member.folder === folder)).filter((group) => group.length);
      const available = top - bottom - groups.length * 0.05;
      const count = members.length;
      let y = top;
      for (const group of groups) {
        for (const { index } of group) {
          const height = available / count - 0.01;
          pieces.push({ member: index, position: [0.03, y - height / 2, 0], scale: [0.68, height, 0.3] });
          y -= height + 0.01;
        }
        y -= 0.04;
      }
    } else {
      const shares = members.map((member) => (member.payload === 'DX10' ? 2.2 : 1));
      const total = shares.reduce((a, b) => a + b, 0);
      const available = top - bottom - members.length * 0.012;
      let y = top;
      members.forEach((member, index) => {
        const height = (shares[index] / total) * available;
        if (member.payload === 'DX10') {
          const chunks = [[0.5, 0.74], [0.3, 0.52], [0.2, 0.37]];
          for (const [part, width] of chunks) {
            const h = height * part - 0.004;
            pieces.push({ member: index, position: [0, y - h / 2, 0], scale: [width, h, 0.3] });
            y -= h + 0.004;
          }
        } else {
          pieces.push({ member: index, position: [0, y - height / 2, 0], scale: [0.74, height, 0.3] });
          y -= height;
        }
        y -= 0.012;
      });
    }
    return { bars, pieces };
  }

  const layouts = archives.map(strataLayout);
  let strataFrom = layouts[0];
  let strataTo = layouts[0];
  let strataMix = 1;
  function applyStrata() {
    const t = ease(strataMix);
    const lerp3 = (a, b) => [0, 1, 2].map((i) => (a ? a[i] + (b[i] - a[i]) * t : b[i]));
    const barCount = Math.max(strataFrom.bars.length, strataTo.bars.length);
    for (let i = 0; i < MAX_MEMBERS; i++) {
      const a = strataFrom.bars[i];
      const b = strataTo.bars[i];
      if (i >= barCount || (!b && t > 0.5) || (!a && t < 0.5 && !b)) {
        dummy.scale.set(0, 0, 0);
      } else {
        const target = b || a;
        const source = a || b;
        dummy.position.fromArray(lerp3(source.position, target.position));
        const scale = lerp3(source.scale, target.scale);
        dummy.scale.set(scale[0] * (b ? 1 : 1 - t), scale[1], scale[2]);
      }
      dummy.rotation.set(0, 0, 0);
      dummy.updateMatrix();
      recordBars.setMatrixAt(i, dummy.matrix);
      const bar = (b || a);
      if (bar) recordBars.setColorAt(i, bar.color.clone().multiplyScalar(i === hoverRecord || i === readRecord ? 3.4 : 1));
    }
    recordBars.instanceMatrix.needsUpdate = true;
    if (recordBars.instanceColor) recordBars.instanceColor.needsUpdate = true;
    const pieceCount = Math.max(strataFrom.pieces.length, strataTo.pieces.length);
    for (let i = 0; i < MAX_MEMBERS * 3; i++) {
      const a = strataFrom.pieces[i];
      const b = strataTo.pieces[i];
      if (i >= pieceCount || (!a && !b)) {
        dummy.scale.set(0, 0, 0);
      } else {
        const target = b || a;
        const source = a || b;
        dummy.position.fromArray(lerp3(source.position, target.position));
        const scale = lerp3(source.scale, target.scale);
        const fade = b ? (a ? 1 : t) : 1 - t;
        dummy.scale.set(scale[0] * fade, scale[1] * Math.max(fade, 0.001), scale[2] * fade);
      }
      dummy.updateMatrix();
      blocks.setMatrixAt(i, dummy.matrix);
      const piece = b || a;
      if (piece) {
        const member = archives[generation].members[piece.member] || archives[generation].members[0];
        blocks.setColorAt(i, kindThree[member.kind]);
      }
    }
    blocks.instanceMatrix.needsUpdate = true;
    if (blocks.instanceColor) blocks.instanceColor.needsUpdate = true;
  }

  // Cards: a small pool, reused.
  const cardCount = small ? 3 : 5;
  const cardSize = { w: 1.22, h: 1.22 * (CARD.h / CARD.w) };
  let spawned = 0;
  const cards = [];
  for (let i = 0; i < cardCount; i++) {
    const cardCanvas = document.createElement('canvas');
    cardCanvas.width = CARD.w;
    cardCanvas.height = CARD.h;
    const texture = new THREE.CanvasTexture(cardCanvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.anisotropy = anisotropy;
    const { material, uniforms } = cardMaterial(texture);
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(cardSize.w, cardSize.h), material);
    mesh.visible = false;
    const nuggetUniforms = { uColor: { value: accent.clone() }, uTime: { value: 0 }, uOpacity: { value: 1 } };
    const nugget = new THREE.Mesh(new THREE.BoxGeometry(0.13, 0.13, 0.13), new THREE.ShaderMaterial({ vertexShader: NUGGET_VERTEX, fragmentShader: NUGGET_FRAGMENT, uniforms: nuggetUniforms }));
    nugget.visible = false;
    rig.add(mesh, nugget);
    cards.push({ canvas: cardCanvas, texture, mesh, uniforms, nugget, nuggetUniforms, wire: null, active: false, age: 0, member: null, archive: null, start: new THREE.Vector3(), angle: 0, y: 0 });
  }

  // The lookup tag: what the pointer is over, looked up by its hash.
  const tagCanvas = document.createElement('canvas');
  tagCanvas.width = 640;
  tagCanvas.height = 96;
  const tagTexture = new THREE.CanvasTexture(tagCanvas);
  tagTexture.colorSpace = THREE.SRGBColorSpace;
  const tag = new THREE.Mesh(new THREE.PlaneGeometry(1.7, 0.255), new THREE.MeshBasicMaterial({ map: tagTexture, transparent: true, toneMapped: false, depthWrite: false, opacity: 0 }));
  tag.renderOrder = 20;
  rig.add(tag);
  let tagMember = -1;
  function paintTag(member) {
    const context = tagCanvas.getContext('2d');
    context.clearRect(0, 0, 640, 96);
    context.beginPath();
    context.roundRect(2, 2, 636, 92, 14);
    context.fillStyle = 'rgba(9, 11, 15, 0.9)';
    context.fill();
    context.strokeStyle = cardColors.line;
    context.lineWidth = 2;
    context.stroke();
    context.font = `600 25px ${MONO}`;
    context.fillStyle = kindColor[member.kind];
    context.fillText(member.hashLines[member.hashLines.length - 1].replace(/\s+/g, ' '), 18, 36);
    context.fillStyle = cardColors.text;
    context.font = `700 25px ${MONO}`;
    const [line] = wrap(context, `→ ${member.path}`, 600, 1);
    context.fillText(line, 18, 72);
    tagTexture.needsUpdate = true;
  }

  // Motes of bytes drifting round the cartridge.
  const moteCount = small ? 90 : 200;
  const moteGeometry = new THREE.BufferGeometry();
  const motePositions = new Float32Array(moteCount * 3);
  const moteSeeds = new Float32Array(moteCount * 3);
  {
    const rand = random(31);
    for (let i = 0; i < moteCount; i++) {
      moteSeeds[i * 3] = rand() * Math.PI * 2;
      moteSeeds[i * 3 + 1] = 0.6 + rand() * 1.4;
      moteSeeds[i * 3 + 2] = rand() * 2 - 1;
    }
  }
  moteGeometry.setAttribute('position', new THREE.BufferAttribute(motePositions, 3));
  const spriteCanvas = document.createElement('canvas');
  spriteCanvas.width = 32;
  spriteCanvas.height = 32;
  {
    const context = spriteCanvas.getContext('2d');
    const gradient = context.createRadialGradient(16, 16, 0, 16, 16, 16);
    gradient.addColorStop(0, 'rgba(255,255,255,1)');
    gradient.addColorStop(0.3, 'rgba(255,255,255,0.5)');
    gradient.addColorStop(1, 'rgba(255,255,255,0)');
    context.fillStyle = gradient;
    context.fillRect(0, 0, 32, 32);
  }
  const motes = new THREE.Points(moteGeometry, new THREE.PointsMaterial({ map: new THREE.CanvasTexture(spriteCanvas), color: accent.clone().multiplyScalar(1.6), size: 0.035, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending, toneMapped: false }));
  motes.frustumCulled = false;
  rig.add(motes);

  // Lights: a warm key, an accent rim, a cool fill, and the pointer's lamp.
  const key = new THREE.DirectionalLight(new THREE.Color(1.0, 0.95, 0.88), 2.2);
  key.position.set(-4, 6, 7);
  const rimLight = new THREE.DirectionalLight(accent, 3.0);
  rimLight.position.set(6, 2, -5);
  const fill = new THREE.DirectionalLight(new THREE.Color(0.55, 0.7, 1.0), 0.8);
  fill.position.set(-6, -2, 3);
  const lamp = new THREE.PointLight(new THREE.Color(1, 0.96, 0.9), 0, 0, 2);
  scene.add(key, rimLight, fill, lamp);

  // The secret's tiles: 64 textures, and the mip chain of their mosaic, 85 tiles in all.
  const TILE = 0.2;
  let tiles = null;
  function buildTiles() {
    const atlas = document.createElement('canvas');
    atlas.width = 1024;
    atlas.height = 512;
    const context = atlas.getContext('2d');
    context.fillStyle = '#000';
    context.fillRect(0, 0, 1024, 512);
    const textures = [...TES3_MEMBERS.map(([path]) => path), ...BA2_MEMBERS.map(([path]) => path), 'textures\\architecture\\windhelm\\stone.dds', 'elder_council_amulet_n.dds'].filter((path) => kindOf(path) === 'texture');
    for (let i = 0; i < 64; i++) paintTexture(context, `${textures[i % textures.length]}#${i}`, (i % 8) * 64, Math.floor(i / 8) * 64, 64);
    // Each level is the one above, halved: a real mip chain of the mosaic.
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = 'high';
    context.drawImage(atlas, 0, 0, 512, 512, 528, 0, 256, 256);
    context.drawImage(atlas, 528, 0, 256, 256, 528, 272, 128, 128);
    context.drawImage(atlas, 528, 272, 128, 128, 672, 272, 64, 64);
    const texture = new THREE.CanvasTexture(atlas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.flipY = false;
    texture.minFilter = THREE.LinearFilter;
    texture.generateMipmaps = false;
    const specs = [];
    const place = (level, count, originX, originY, atlasX, atlasY) => {
      for (let j = 0; j < count; j++) for (let i = 0; i < count; i++) {
        specs.push({ level, target: new THREE.Vector3(originX + (i + 0.5) * TILE, originY - (j + 0.5) * TILE, 0.62), rect: [(atlasX + i * 64) / 1024, (atlasY + j * 64) / 512, 64 / 1024, 64 / 512] });
      }
    };
    const left = -1.3;
    const topY = 0.86;
    place(0, 8, left, topY, 0, 0);
    place(1, 4, left + 8 * TILE + 0.08, topY, 528, 0);
    place(2, 2, left + 8 * TILE + 0.08, topY - 4 * TILE - 0.08, 528, 272);
    place(3, 1, left + 10 * TILE + 0.16, topY - 4 * TILE - 0.08, 672, 272);
    const geometry = new THREE.PlaneGeometry(1, 1);
    const rects = new Float32Array(specs.length * 4);
    const levels = new Float32Array(specs.length);
    specs.forEach((spec, index) => {
      rects.set(spec.rect, index * 4);
      levels[index] = spec.level;
    });
    geometry.setAttribute('aRect', new THREE.InstancedBufferAttribute(rects, 4));
    geometry.setAttribute('aLevel', new THREE.InstancedBufferAttribute(levels, 1));
    const uniforms = { tAtlas: { value: texture }, uSweep: { value: -9 }, uGlow: { value: 0 }, uEdge: { value: accent.clone() } };
    const mesh = new THREE.InstancedMesh(geometry, new THREE.ShaderMaterial({ vertexShader: TILE_VERTEX, fragmentShader: TILE_FRAGMENT, uniforms, side: THREE.DoubleSide }), specs.length);
    mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    mesh.frustumCulled = false;
    mesh.visible = false;
    rig.add(mesh);
    const order = specs.map((spec, index) => ({ index, key: tes3Hash(`textures\\mip${spec.level}\\${index}.dds`).hi })).sort((a, b) => a.key - b.key);
    const repackRank = new Float32Array(specs.length);
    order.forEach(({ index }, rank) => { repackRank[index] = rank; });
    const seeds = specs.map((_, index) => random(index * 7919 + 3)());
    tiles = { mesh, specs, uniforms, repackRank, seeds, from: specs.map(() => new THREE.Vector3()) };
  }

  // State ---------------------------------------------------------------------------------------------------

  let generation = 0;
  let generationClock = 0;
  let hoverRecord = -1;
  let readRecord = -1;
  let queue = 0;
  let spawnClock = 1.2;
  let secret = -1;
  let hintClock = 0;
  let tapIndex = 0;
  let tapAt = 0;
  let simulating = false;
  paintTable(archives[0]);

  function setSpans(member) {
    const [a = -1, b = -1, c = -1, d = -1] = member ? member.spans : [];
    dumpUniforms.uSpan.value.set(a, b, c, d);
    const [e = -1, f = -1] = member ? member.nameSpan || [] : [];
    const [g = -1, h = -1] = member ? member.payloadSpan || [] : [];
    dumpUniforms.uSpan2.value.set(e, f, g, h);
  }

  function flipTo(glyphs, flutter) {
    cells.forEach((flap, index) => {
      const target = FLAP_GLYPHS.indexOf(glyphs[index]);
      const steps = [];
      for (let k = 0; k < flutter + index; k++) steps.push(9 + Math.floor(random(seedOf(glyphs.join('')) + k * 31 + index)() * 7));
      steps.push(target);
      flap.queue = steps.filter((glyph, k) => k === steps.length - 1 || glyph !== flap.glyph);
    });
  }

  function switchGeneration(next) {
    if (next === generation) return;
    strataFrom = layouts[generation];
    strataTo = layouts[next];
    strataMix = 0;
    generation = next;
    generationClock = 0;
    queue = 0;
    readRecord = -1;
    hoverRecord = -1;
    flipTo(archives[next].flaps, 3);
    blockEmissive.uCompressed.value = archives[next].compression ? 1 : 0;
    paintTable(archives[next]);
    fadeTo = next;
  }
  let fadeTo = -1;

  function spawn(memberIndex) {
    const archive = archives[generation];
    const card = cards.find((candidate) => !candidate.active) || cards.reduce((a, b) => (a.age > b.age ? a : b));
    const member = archive.members[memberIndex % archive.members.length];
    card.active = true;
    card.age = 0;
    card.order = spawned++;
    card.leaving = -1;
    card.member = member;
    card.archive = archive;
    card.memberIndex = memberIndex % archive.members.length;
    paintCard(card.canvas, member, archive, cardColors);
    card.texture.needsUpdate = true;
    if (card.wire) {
      card.mesh.remove(card.wire);
      card.wire.traverse((object) => object.geometry && object.geometry.dispose());
      card.wire = null;
    }
    if (member.kind === 'mesh') {
      card.wire = wireframeFor(member.path, kindThree.mesh.clone().multiplyScalar(1.8));
      const px = (CARD.preview.x + CARD.preview.size / 2) / CARD.w - 0.5;
      const py = 0.5 - (CARD.preview.y + CARD.preview.size / 2) / CARD.h;
      card.wire.position.set(px * cardSize.w, py * cardSize.h, 0.05);
      card.wire.scale.setScalar(0.19);
      card.mesh.add(card.wire);
    }
    const bar = layouts[generation].bars[card.memberIndex];
    card.start.set(CART.w / 2, bar ? bar.position[1] : 0.4, 0.05);
    cartridge.localToWorld(card.start);
    rig.worldToLocal(card.start);
    card.mesh.position.copy(card.start);
    card.compressed = !!archive.compression;
    card.mips = archive.id === 'ba2' && member.payload === 'DX10';
    card.nuggetUniforms.uColor.value.copy(kindThree[member.kind]);
    readRecord = card.memberIndex;
    setSpans(member);
  }

  // Pointer: a lamp, a lean, a lookup; a click extracts what is under it, or plays a flap.
  const pointer = new THREE.Vector2();
  let pointerActive = false;
  let lastPointer = 0;
  const raycaster = new THREE.Raycaster();
  const lean = new THREE.Vector2();
  const lampTarget = new THREE.Vector3();
  const tmp = new THREE.Vector3();
  const hitLocal = new THREE.Vector3();

  function pointerTo(event) {
    const rect = root.getBoundingClientRect();
    pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -((event.clientY - rect.top) / rect.height * 2 - 1));
    pointerActive = true;
    lastPointer = performance.now();
  }
  function lookup() {
    raycaster.setFromCamera(pointer, camera);
    const flapHits = raycaster.intersectObjects(cells.map((flap) => flap.group), true);
    if (flapHits.length) return { flap: cells.findIndex((flap) => flapHits[0].object.parent === flap.group || flapHits[0].object.parent?.parent === flap.group) };
    const hits = raycaster.intersectObjects([upper, lower], false);
    if (!hits.length) return null;
    cartridge.worldToLocal(hitLocal.copy(hits[0].point));
    const layout = layouts[generation];
    const y = hitLocal.y;
    let best = -1;
    let distance = Infinity;
    layout.bars.forEach((bar, index) => {
      const d = Math.abs(bar.position[1] - y);
      if (d < distance) {
        distance = d;
        best = index;
      }
    });
    if (distance < 0.02) return { member: best };
    for (const piece of layout.pieces) if (Math.abs(piece.position[1] - y) <= piece.scale[1] / 2 + 0.004) return { member: piece.member };
    return null;
  }

  function onMove(event) {
    pointerTo(event);
  }
  function onDown(event) {
    pointerTo(event);
    if (secret >= 0) return;
    const hit = lookup();
    if (!hit) return;
    if (hit.flap >= 0) {
      const flap = cells[hit.flap];
      const current = archives[generation].flaps[hit.flap];
      const expected = SECRET_WORD[tapIndex].toUpperCase();
      const now = performance.now();
      if (now - tapAt > 4000) tapIndex = 0;
      tapAt = now;
      if (archives[generation].id === 'ba2' && current === expected) {
        tapIndex += 1;
        if (tapIndex === SECRET_WORD.length) {
          tapIndex = 0;
          startSecret();
          return;
        }
      } else {
        tapIndex = current === 'B' && archives[generation].id === 'ba2' ? 1 : 0;
      }
      const back = flap.glyph;
      flap.queue = [9 + (now % 7 | 0), back];
      flap.glow = 1;
      return;
    }
    if (hit.member >= 0) {
      spawn(hit.member);
      spawnClock = 2.6;
    }
  }
  function onLeave() {
    pointerActive = false;
  }
  const hero = root.closest('.dw-hero') || root;
  hero.addEventListener('pointermove', onMove, { passive: true });
  hero.addEventListener('pointerdown', onDown, { passive: true });
  hero.addEventListener('pointerleave', onLeave, { passive: true });

  // The secret: type the BA2 magic with nothing focused, or tap its letters on the display.
  let typed = '';
  let typedAt = 0;
  document.addEventListener('keydown', (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey || event.isComposing || event.key.length !== 1) return;
    const target = event.target;
    if (target instanceof Element && target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return;
    const now = performance.now();
    if (now - typedAt > 2500) typed = '';
    typedAt = now;
    typed = (typed + event.key.toLowerCase()).slice(-SECRET_WORD.length);
    if (typed === SECRET_WORD) {
      typed = '';
      startSecret();
    }
  });

  function startSecret() {
    if (secret >= 0 || lost) return;
    if (!tiles) buildTiles();
    if (archives[generation].id !== 'ba2') switchGeneration(2);
    secret = 0;
    tiles.mesh.visible = true;
    for (const card of cards) if (card.active && card.leaving < 0) card.leaving = 0;
    for (const flap of cells) flap.glow = 1;
    if (reduceMotion) {
      secret = 5.2;
      requestFrame();
      setTimeout(() => {
        secret = SECRET_LENGTH;
        requestFrame();
      }, 6000);
    }
    requestFrame();
  }

  // Layout -----------------------------------------------------------------------------------------------

  let width = 1;
  let height = 1;
  let scale = 1;
  let place = { x: 0, y: 0, height: 0, above: false, textBox: { left: 0, top: 0, right: 0, bottom: 0 } };
  const anchor = new THREE.Vector3();
  const plane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  const ndc = new THREE.Vector2();
  const quality = { level: 1, slow: 0 };
  const reach = sceneReach();
  const projected = new THREE.Vector3();

  // Places the rig so its whole reach, seen through the camera, fills the box `place` gives.
  function fitRig() {
    const boxWidth = place.height * ASPECT;
    const target = { x: place.x, y: place.y, width: boxWidth, height: place.height };
    let cx = place.x;
    let cy = place.y;
    const unitsPerPixel = (at) => 2 * camera.position.distanceTo(at) * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) / height;
    const aim = () => {
      ndc.set(cx / width * 2 - 1, -(cy / height * 2 - 1));
      raycaster.setFromCamera(ndc, camera);
      raycaster.ray.intersectPlane(plane, anchor);
    };
    aim();
    scale = Math.max(0.05, place.height * unitsPerPixel(anchor) / SCENE_HEIGHT);
    for (let pass = 0; pass < 6; pass++) {
      rig.position.copy(anchor);
      rig.scale.setScalar(scale);
      rig.updateMatrixWorld(true);
      let left = Infinity;
      let right = -Infinity;
      let top = Infinity;
      let bottom = -Infinity;
      for (const point of reach) {
        projected.copy(point).applyMatrix4(rig.matrixWorld).project(camera);
        const px = (projected.x + 1) / 2 * width;
        const py = (1 - projected.y) / 2 * height;
        left = Math.min(left, px);
        right = Math.max(right, px);
        top = Math.min(top, py);
        bottom = Math.max(bottom, py);
      }
      const k = Math.min(target.width / (right - left), target.height / (bottom - top));
      const settled = pass >= 3 && Math.abs(k - 1) < 0.002 && Math.abs((left + right) / 2 - target.x) < 0.5 && Math.abs((top + bottom) / 2 - target.y) < 0.5;
      if (settled) break;
      const factor = pass < 5 ? k : Math.min(k, 1) * 0.995;
      scale *= factor;
      cx = target.x - ((left + right) / 2 - cx) * factor;
      cy = target.y - ((top + bottom) / 2 - cy) * factor;
      aim();
    }
    rig.position.copy(anchor);
    rig.scale.setScalar(scale);
  }

  function layout() {
    const rect = root.getBoundingClientRect();
    width = Math.max(1, Math.round(rect.width));
    height = Math.max(1, Math.round(rect.height));
    const dpr = Math.min(window.devicePixelRatio || 1, small ? 1.5 : 1.75) * quality.level;
    renderer.setPixelRatio(dpr);
    renderer.setSize(width, height, false);
    const w = Math.max(1, Math.floor(width * dpr));
    const h = Math.max(1, Math.floor(height * dpr));
    sceneTarget.setSize(w, h);
    bloomTargets[0].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[1].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[2].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    bloomTargets[3].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();

    place = placement(root);
    placeStill();
    fitRig();
    const cellPx = Math.max(7, Math.min(10, width / 150)) * dpr;
    dumpUniforms.uCell.value.set(cellPx, cellPx * 1.78);
    dumpUniforms.uResolution.value.set(w, h);
    dumpUniforms.uFocus.value.set(place.x * dpr, (height - place.y) * dpr, place.height * 0.9 * dpr);
    const box = place.textBox;
    dumpUniforms.uTextBox.value.set(box.left * dpr, (height - box.bottom) * dpr, box.right * dpr, (height - box.top) * dpr);
    motes.material.size = 0.03 * scale;
  }

  // The loop ------------------------------------------------------------------------------------------------

  const postScene = new THREE.Scene();
  const postCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const postQuad = new THREE.Mesh(quad);
  postQuad.frustumCulled = false;
  postScene.add(postQuad);
  const brightMaterial = fullscreenMaterial(BRIGHT_FRAGMENT, { tInput: { value: sceneTarget.texture }, uThreshold: { value: 1.15 } });
  const blurMaterial = fullscreenMaterial(BLUR_FRAGMENT, { tInput: { value: null }, uDirection: { value: new THREE.Vector2() } });
  const copyMaterial = fullscreenMaterial(/* glsl */ `
    uniform sampler2D tInput;
    varying vec2 vUv;
    void main() { gl_FragColor = texture2D(tInput, vUv); }
  `, { tInput: { value: null } });
  const compositeMaterial = fullscreenMaterial(COMPOSITE_FRAGMENT, {
    tScene: { value: sceneTarget.texture },
    tBloomNear: { value: bloomTargets[0].texture },
    tBloomFar: { value: bloomTargets[2].texture },
    uTime: { value: 0 },
    uFlash: { value: 0 },
  });
  function pass(material, target) {
    postQuad.material = material;
    renderer.setRenderTarget(target);
    renderer.render(postScene, postCamera);
  }
  function blur(target, scratch, radius) {
    blurMaterial.uniforms.tInput.value = target.texture;
    blurMaterial.uniforms.uDirection.value.set(radius / target.width, 0);
    pass(blurMaterial, scratch);
    blurMaterial.uniforms.tInput.value = scratch.texture;
    blurMaterial.uniforms.uDirection.value.set(0, radius / target.height);
    pass(blurMaterial, target);
  }

  const clock = new THREE.Clock();
  let time = 0;
  let visible = false;
  let running = false;
  let first = true;
  let lost = false;

  function stepFlaps(dt) {
    for (const flap of cells) {
      flap.glow = Math.max(0, flap.glow - dt * 0.9);
      if (!flap.pivot.visible) {
        if (!flap.queue.length) continue;
        const next = flap.queue.shift();
        flap.from = flap.glyph;
        flap.to = next;
        setFlapUv(flap.top.geometry, next, 'top', false);
        setFlapUv(flap.bottom.geometry, flap.from, 'bottom', false);
        setFlapUv(flap.front.geometry, flap.from, 'top', false);
        setFlapUv(flap.back.geometry, next, 'bottom', true);
        flap.pivot.rotation.x = 0;
        flap.pivot.visible = true;
        flap.progress = 0;
      }
      flap.progress += dt / (flap.queue.length ? 0.075 : 0.14);
      const p = Math.min(1, flap.progress);
      flap.pivot.rotation.x = Math.PI * p * p;
      if (p >= 1) {
        flap.glyph = flap.to;
        setFlapUv(flap.bottom.geometry, flap.glyph, 'bottom', false);
        flap.pivot.visible = false;
      }
    }
  }

  const slotTarget = new THREE.Vector3();
  function stepCards(dt) {
    const ranked = cards.filter((card) => card.active).sort((a, b) => b.order - a.order);
    for (const card of cards) {
      if (!card.active) continue;
      card.age += dt;
      const age = card.age;
      const inflateAt = 0.85;
      const landAt = card.compressed ? 1.4 : 0.95;
      const slot = ranked.indexOf(card);
      if (slot >= STACK.keep && card.leaving < 0) card.leaving = 0;
      if (card.leaving >= 0) card.leaving += dt;
      if (card.leaving > 0.9) {
        card.active = false;
        card.mesh.visible = false;
        card.nugget.visible = false;
        continue;
      }
      // Out of the cartridge's side, then onto the stack, newest in front; older cards step back.
      const k = Math.min(slot, STACK.keep);
      slotTarget.set(STACK.x + k * 0.07, STACK.y - k * STACK.pitch, 0.22 - k * STACK.depth);
      const out = easeOut(window01(age, 0, inflateAt));
      if (age < inflateAt) {
        const position = card.start.clone().lerp(slotTarget, out);
        position.y += Math.sin(out * Math.PI) * 0.28;
        position.z += Math.sin(out * Math.PI) * 0.4;
        card.mesh.position.copy(position);
      } else {
        card.mesh.position.lerp(slotTarget, reduceMotion && !simulating ? 1 : Math.min(1, dt * 5));
      }
      const position = card.mesh.position;
      card.mesh.rotation.set(0.03, STACK.turn * out, 0);
      const reveal = card.compressed ? window01(age, inflateAt, landAt) : window01(age, 0.08, 0.6);
      const stackScale = 1 - k * 0.07;
      card.mesh.scale.setScalar(stackScale * (card.compressed ? 0.12 + 0.88 * easeOut(reveal) : 0.3 + 0.7 * out));
      card.uniforms.uReveal.value = card.leaving >= 0 ? 1 - window01(card.leaving, 0, 0.9) : reveal;
      card.uniforms.uGlow.value = 0.62 + (slot === 0 ? 0.28 : 0);
      card.uniforms.uEdgeColor.value.copy(kindThree[card.member.kind]);
      card.mesh.visible = card.uniforms.uReveal.value > 0.001;
      if (card.mips) {
        const level = Math.floor(window01(age, landAt - 0.2, landAt + 1.2) * 8);
        card.uniforms.uCells.value = level >= 8 ? 0 : 2 ** level;
      } else {
        card.uniforms.uCells.value = 0;
      }
      card.nugget.visible = card.compressed && age < landAt;
      if (card.nugget.visible) {
        card.nugget.position.copy(position);
        card.nugget.rotation.set(age * 2.1, age * 2.7, 0);
        const shrink = 1 - window01(age, inflateAt, landAt);
        card.nugget.scale.setScalar(0.6 + 0.8 * shrink);
        card.nuggetUniforms.uOpacity.value = shrink;
        card.nuggetUniforms.uTime.value = time;
      }
      if (card.wire) card.wire.rotation.set(0.35, age * 0.9, 0);
    }
  }

  function stepSecret(dt) {
    if (secret < 0) return;
    if (!reduceMotion) secret += dt;
    const t = secret;
    const open = ease(window01(t, 0.25, 1.0)) - ease(window01(t, 8.2, 9.0));
    lowerShell.position.y = -open * 0.4;
    lowerShell.rotation.z = -open * 0.04;
    const assembled = ease(window01(t, 2.4, 3.6)) - ease(window01(t, 6.6, 7.6));
    cartridge.position.z = -0.9 * assembled;
    blockEmissive.uPulse.value = ease(window01(t, 0.3, 0.9)) * (1 - ease(window01(t, 2.0, 3.0)));
    tiles.uniforms.uSweep.value = -1.6 + window01(t, 4.6, 6.4) * 3.4;
    tiles.uniforms.uGlow.value = assembled;
    compositeMaterial.uniforms.uFlash.value = Math.exp(-((t - 3.7) ** 2) * 4) * 0.8;
    const origin = new THREE.Vector3(CART_X, -0.24, 0);
    tiles.specs.forEach((spec, index) => {
      const seed = tiles.seeds[index];
      const erupt = easeOut(window01(t, 0.8 + index * 0.006, 2.4 + index * 0.004));
      const angle = seed * Math.PI * 2 + erupt * 5.5;
      const radius = 0.15 + erupt * (0.8 + seed * 0.15);
      const spiral = new THREE.Vector3(Math.cos(angle) * radius * 1.2, origin.y + erupt * 1.05 - erupt * erupt * 0.4 + seed * 0.25, Math.sin(angle) * radius * 0.45 + 0.3);
      const settle = ease(window01(t, 2.6 + spec.level * 0.18 + (index % 8) * 0.025, 3.9 + spec.level * 0.18 + (index % 8) * 0.025));
      const rank = tiles.repackRank[index];
      const repack = ease(window01(t, 6.6 + rank * 0.018, 7.2 + rank * 0.018));
      const at = spiral.lerp(spec.target, settle);
      at.lerp(origin, repack);
      at.y += Math.sin(repack * Math.PI) * 0.35;
      dummy.position.copy(at);
      const spin = (1 - settle) * erupt;
      dummy.rotation.set(spin * seed * 9, spin * (seed * 7 + 2), spin * 3);
      const size = TILE * (0.25 + 0.72 * Math.max(erupt * 0.6, settle)) * (1 - repack) * (t < 0.8 ? 0 : 1);
      dummy.scale.set(size * 0.96, size * 0.96, 1);
      dummy.updateMatrix();
      tiles.mesh.setMatrixAt(index, dummy.matrix);
    });
    tiles.mesh.instanceMatrix.needsUpdate = true;
    if (t > 8.1 && t - dt <= 8.1) flipTo(archives[generation].flaps, 4);
    for (const flap of cells) flap.glow = Math.max(flap.glow, t < 8.4 ? 0.9 : 0);
    if (t >= SECRET_LENGTH) {
      secret = -1;
      tiles.mesh.visible = false;
      upperShell.position.y = 0;
      lowerShell.position.y = 0;
      upperShell.rotation.z = 0;
      lowerShell.rotation.z = 0;
      cartridge.position.z = 0;
      compositeMaterial.uniforms.uFlash.value = 0;
      blockEmissive.uPulse.value = 0;
      generationClock = 0;
      spawnClock = 0.8;
    }
  }

  function frame() {
    running = false;
    if (lost) return;
    const rawDt = clock.getDelta();
    const dt = reduceMotion ? 0 : Math.min(rawDt, 0.05);
    if (!reduceMotion && rawDt < 0.5) {
      quality.slow = rawDt > 1 / 40 ? quality.slow + rawDt : Math.max(0, quality.slow - rawDt * 0.5);
      if (quality.slow > 1.5 && quality.level > 0.5) {
        quality.level = Math.max(0.5, quality.level - 0.2);
        quality.slow = 0;
        layout();
      }
    }
    update(dt);
    render();
    if (first) {
      first = false;
      root.classList.add('is-live');
    }
    if (visible && !document.hidden && !reduceMotion) requestFrame();
  }

  function update(dt) {
    time += dt;
    compositeMaterial.uniforms.uTime.value = time;

    // The families take turns; the dump fades across when they change.
    if (secret < 0) generationClock += dt;
    if (secret < 0 && generationClock > 17) switchGeneration((generation + 1) % archives.length);
    if (fadeTo >= 0) {
      dumpUniforms.uFade.value = Math.max(0, dumpUniforms.uFade.value - dt * 2.5);
      if (dumpUniforms.uFade.value <= 0 || reduceMotion) {
        dumpUniforms.tBytes.value = archives[fadeTo].texture;
        dumpUniforms.uRows.value = archives[fadeTo].rows;
        fadeTo = -1;
      }
    } else {
      dumpUniforms.uFade.value = Math.min(1, dumpUniforms.uFade.value + dt * 1.5);
    }
    if (reduceMotion) dumpUniforms.uFade.value = 1;
    const scrollSpeed = 7 + (secret >= 0 ? Math.exp(-((secret - 5.4) ** 2) * 1.2) * 260 : 0);
    dumpUniforms.uScroll.value += dt * scrollSpeed * (window.devicePixelRatio || 1);
    strataMix = Math.min(1, strataMix + dt / 1.2);
    if (reduceMotion && !simulating) strataMix = 1;

    // Extraction: one member after another, in archive order.
    if (secret < 0 && (!reduceMotion || simulating)) {
      spawnClock -= dt;
      if (spawnClock <= 0) {
        spawn(queue++);
        spawnClock = small ? 3.2 : 2.3;
      }
    }

    // The hint: while the display reads BTDX, its letters catch the light in order now and then.
    hintClock += dt;
    if (archives[generation].id === 'ba2' && secret < 0) {
      const phase = hintClock % 7.5;
      cells.forEach((flap, index) => {
        const lit = Math.exp(-((phase - 1.0 - index * 0.45) ** 2) * 18) * 0.7;
        flap.glow = Math.max(flap.glow, lit);
      });
    }
    stepFlaps(dt);
    cells.forEach((flap) => {
      flap.material.emissiveIntensity = 0.5 + flap.glow * 2.2;
    });

    // The lamp and the lean.
    const idle = !pointerActive || performance.now() - lastPointer > 4000;
    rig.position.copy(anchor);
    rig.updateMatrixWorld();
    if (idle) {
      tmp.set(Math.sin(time * 0.4) * 1.2, 0.3 + Math.cos(time * 0.3) * 0.6, 1.6);
      rig.localToWorld(tmp);
      lampTarget.copy(tmp);
    } else {
      raycaster.setFromCamera(pointer, camera);
      plane.constant = -(anchor.z + 2.4 * scale);
      if (raycaster.ray.intersectPlane(plane, tmp)) lampTarget.copy(tmp);
      plane.constant = 0;
    }
    lamp.position.lerp(lampTarget, reduceMotion ? 1 : Math.min(1, dt * 5));
    lamp.intensity = (idle ? 1.4 : 3.2) * scale * scale;
    const dx = (lamp.position.x - anchor.x) / scale;
    const dy = (lamp.position.y - anchor.y) / scale;
    const follow = reduceMotion ? 1 : Math.min(1, dt * 2.2);
    lean.x += (THREE.MathUtils.clamp(-dy * 0.06, -0.14, 0.14) - lean.x) * follow;
    lean.y += (THREE.MathUtils.clamp(dx * 0.1, -0.3, 0.3) - lean.y) * follow;
    cartridge.rotation.set(lean.x + Math.sin(time * 0.31) * 0.02, -0.32 + lean.y + Math.sin(time * 0.17) * 0.12, Math.sin(time * 0.23) * 0.012);
    cartridge.position.y = Math.sin(time * 0.6) * 0.02;

    // The lookup: the record under the pointer, with its hash and path.
    let looked = null;
    if (!idle && secret < 0) {
      const hit = lookup();
      if (hit && hit.member >= 0) looked = hit.member;
    }
    hoverRecord = looked ?? -1;
    if (looked !== null && looked !== tagMember) {
      tagMember = looked;
      paintTag(archives[generation].members[looked]);
    }
    const tagTarget = looked !== null ? 1 : 0;
    tag.material.opacity += (tagTarget - tag.material.opacity) * (reduceMotion ? 1 : Math.min(1, dt * 8));
    if (tagMember >= 0) {
      const bar = layouts[generation].bars[tagMember];
      tag.position.set(CART_X + 0.12, -CART.h / 2 - 0.2, 0.45);
      tag.quaternion.copy(camera.quaternion);
    }
    if (looked !== null) setSpans(archives[generation].members[looked]);
    blockEmissive.uGlowBlock.value = -1;
    const shownMember = looked ?? readRecord;
    layouts[generation].pieces.forEach((piece, index) => {
      if (piece.member === shownMember && strataMix >= 1) blockEmissive.uGlowBlock.value = index;
    });
    applyStrata();

    // Motes.
    for (let i = 0; i < moteCount; i++) {
      const angle = moteSeeds[i * 3] + time * 0.08 * (1 + moteSeeds[i * 3 + 2] * 0.3);
      const radius = moteSeeds[i * 3 + 1];
      motePositions[i * 3] = CART_X * 0.5 + Math.cos(angle) * radius * 1.25;
      motePositions[i * 3 + 1] = moteSeeds[i * 3 + 2] * 1.1 + Math.sin(time * 0.3 + i) * 0.05;
      motePositions[i * 3 + 2] = Math.sin(angle) * radius * 0.7;
    }
    moteGeometry.attributes.position.needsUpdate = true;

    stepCards(dt);
    stepSecret(dt);
    dumpUniforms.uPan.value.set(lean.y * 30, -lean.x * 20);
    scene.updateMatrixWorld();
  }

  function render() {
    renderer.setRenderTarget(sceneTarget);
    renderer.setClearColor(0x000000, 1);
    renderer.clear(true, true, true);
    renderer.render(dumpScene, postCamera);
    renderer.render(scene, camera);
    pass(brightMaterial, bloomTargets[0]);
    blur(bloomTargets[0], bloomTargets[1], 1.0);
    blur(bloomTargets[0], bloomTargets[1], 2.0);
    copyMaterial.uniforms.tInput.value = bloomTargets[0].texture;
    pass(copyMaterial, bloomTargets[2]);
    blur(bloomTargets[2], bloomTargets[3], 1.5);
    blur(bloomTargets[2], bloomTargets[3], 3.0);
    pass(compositeMaterial, null);
  }

  function requestFrame() {
    if (running || lost) return;
    running = true;
    requestAnimationFrame(frame);
  }


  canvas.addEventListener('webglcontextlost', (event) => {
    event.preventDefault();
    lost = true;
    root.classList.remove('is-live');
  });
  canvas.addEventListener('webglcontextrestored', () => {
    canvas.remove();
    still.remove();
    root.classList.remove('is-live', 'is-placed');
    mount(root);
  });

  layout();
  if (reduceMotion) {
    simulating = true;
    for (let i = 0; i < 150; i++) update(0.05);
    simulating = false;
  }
  new ResizeObserver(() => {
    layout();
    requestFrame();
  }).observe(root);
  new IntersectionObserver((entries) => {
    visible = entries.some((entry) => entry.isIntersecting);
    if (visible) {
      clock.getDelta();
      requestFrame();
    }
  }).observe(root);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && visible) {
      clock.getDelta();
      requestFrame();
    }
  });
  requestFrame();
}

for (const root of document.querySelectorAll('[data-dw-hero-art]')) mount(root);
