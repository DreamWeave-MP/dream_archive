use bstr::ByteSlice as _;
use dream_archive::{
    CompressionOverride,
    ba2::{
        Archive, ArchiveVersion, Ba2CompressionFormat, Builder, Dx10Builder, Error, PayloadFormat,
        TextureHeader,
    },
};
use flate2::{Compression, write::ZlibEncoder};
use std::path::PathBuf;

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const GNRL: u32 = u32::from_le_bytes(*b"GNRL");
const DX10: u32 = u32::from_le_bytes(*b"DX10");
const GNMF: u32 = u32::from_le_bytes(*b"GNMF");
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;
const FILE_HEADER_SIZE_GNRL: u16 = 0x10;
const FILE_HEADER_SIZE_DX10: u16 = 0x18;
const FILE_HEADER_SIZE_GNMF: u16 = 0x30;
const DDSD_CAPS: u32 = 0x0000_0001;
const DDSD_HEIGHT: u32 = 0x0000_0002;
const DDSD_WIDTH: u32 = 0x0000_0004;
const DDSD_PIXELFORMAT: u32 = 0x0000_1000;
const DDSD_MIPMAPCOUNT: u32 = 0x0002_0000;
const DDSD_LINEARSIZE: u32 = 0x0008_0000;
const DDPF_FOURCC: u32 = 0x0000_0004;
const DDSCAPS_TEXTURE: u32 = 0x0000_1000;
const DDS_DIMENSION_TEXTURE2D: u32 = 3;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy)]
struct TinyArchiveOptions<'a> {
    version: u32,
    format: u32,
    compression_code: Option<u32>,
    string_table_offset: u64,
    name: Option<&'a [u8]>,
    chunk_offset: u64,
    chunk_packed_size: u32,
    chunk_size: u32,
    sentinel: u32,
    payload: &'a [u8],
}

impl Default for TinyArchiveOptions<'_> {
    fn default() -> Self {
        Self {
            version: 1,
            format: GNRL,
            compression_code: None,
            string_table_offset: 60,
            name: Some(b"hello.txt"),
            chunk_offset: 60,
            chunk_packed_size: 0,
            chunk_size: 5,
            sentinel: CHUNK_SENTINEL,
            payload: b"hello",
        }
    }
}

fn tiny_archive(options: TinyArchiveOptions<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, options.version);
    push_u32(&mut bytes, options.format);
    push_u32(&mut bytes, 1);
    push_u64(&mut bytes, options.string_table_offset);
    if matches!(options.version, 2 | 3) {
        push_u64(&mut bytes, 1);
    }
    if let Some(code) = options.compression_code {
        push_u32(&mut bytes, code);
    }

    let (hash, _) = dream_archive::ba2::hash_file(b"hello.txt".as_bstr());
    push_u32(&mut bytes, hash.file);
    push_u32(&mut bytes, hash.extension);
    push_u32(&mut bytes, hash.directory);
    bytes.push(0);
    bytes.push(1);
    push_u16(&mut bytes, FILE_HEADER_SIZE_GNRL);
    push_u64(&mut bytes, options.chunk_offset);
    push_u32(&mut bytes, options.chunk_packed_size);
    push_u32(&mut bytes, options.chunk_size);
    push_u32(&mut bytes, options.sentinel);

    if let Some(name) = options.name {
        assert_eq!(
            u64::try_from(bytes.len()).unwrap(),
            options.string_table_offset
        );
        push_u16(&mut bytes, name.len().try_into().unwrap());
        bytes.extend_from_slice(name);
    }
    bytes.extend_from_slice(options.payload);
    bytes
}

fn two_entry_gnrl_archive_with_colliding_hashes() -> Vec<u8> {
    let names = [b"a.txt".as_slice(), b"b.txt".as_slice()];
    let payloads = [b"first".as_slice(), b"second".as_slice()];
    let table_size = 24 + 36 * names.len();
    let payload_offset = table_size;
    let string_table_offset =
        payload_offset + payloads.iter().map(|payload| payload.len()).sum::<usize>();
    let (hash, _) = dream_archive::ba2::hash_file(b"ghost.txt".as_bstr());

    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, GNRL);
    push_u32(&mut bytes, names.len().try_into().unwrap());
    push_u64(&mut bytes, string_table_offset.try_into().unwrap());

    let mut next_payload = payload_offset;
    for payload in payloads {
        push_u32(&mut bytes, hash.file);
        push_u32(&mut bytes, hash.extension);
        push_u32(&mut bytes, hash.directory);
        bytes.push(0);
        bytes.push(1);
        push_u16(&mut bytes, FILE_HEADER_SIZE_GNRL);
        push_u64(&mut bytes, next_payload.try_into().unwrap());
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, payload.len().try_into().unwrap());
        push_u32(&mut bytes, CHUNK_SENTINEL);
        next_payload += payload.len();
    }
    assert_eq!(bytes.len(), payload_offset);
    for payload in payloads {
        bytes.extend_from_slice(payload);
    }
    assert_eq!(bytes.len(), string_table_offset);
    for name in names {
        push_u16(&mut bytes, name.len().try_into().unwrap());
        bytes.extend_from_slice(name);
    }
    bytes
}

fn tiny_gnmf_archive(metadata: [u32; 8], payload: &[u8]) -> Vec<u8> {
    let payload_offset = 24 + 12 + 4 + 32 + 24;
    let (hash, _) = dream_archive::ba2::hash_file(b"mesh.bin".as_bstr());
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, GNMF);
    push_u32(&mut bytes, 1);
    push_u64(&mut bytes, 0);
    push_u32(&mut bytes, hash.file);
    push_u32(&mut bytes, hash.extension);
    push_u32(&mut bytes, hash.directory);
    bytes.push(0);
    bytes.push(1);
    push_u16(&mut bytes, FILE_HEADER_SIZE_GNMF);
    for word in metadata {
        push_u32(&mut bytes, word);
    }
    push_u64(&mut bytes, payload_offset);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u16(&mut bytes, 2);
    push_u16(&mut bytes, 5);
    push_u32(&mut bytes, CHUNK_SENTINEL);
    assert_eq!(bytes.len(), usize::try_from(payload_offset).unwrap());
    bytes.extend_from_slice(payload);
    bytes
}

fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, bytes).unwrap();
    encoder.finish().unwrap()
}

fn output_dir(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target/test-extract");
    path.push(format!("{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn assert_no_temp_extract_files(dir: &std::path::Path) {
    if !dir.exists() {
        return;
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        assert!(
            !name.to_string_lossy().contains(".dream-archive-tmp-"),
            "temporary extraction file was not cleaned up: {name:?}"
        );
    }
}

#[test]
fn writes_ba2_gnrl_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();
    builder.add_bytes("textures/bar.dds", b"texture").unwrap();

    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.info().format, PayloadFormat::GNRL);
    assert_eq!(archive.info().version, ArchiveVersion::v1);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
    assert!(archive.info().strings);
    assert_eq!(archive.len(), 2);
    assert_eq!(
        archive.read_file("meshes/foo.nif").unwrap().unwrap(),
        b"mesh"
    );
    assert_eq!(
        archive.read_file("textures/bar.dds").unwrap().unwrap(),
        b"texture"
    );
}

#[test]
fn ba2_extract_entry_to_path_creates_parent_directories() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();
    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();
    let out = output_dir("ba2-entry-create-parent");
    let file_path = out.join("missing/parents/foo.nif");

    assert_eq!(
        archive
            .extract_entry_to_path(&archive.entries()[0], &file_path)
            .unwrap(),
        4
    );
    assert_eq!(std::fs::read(&file_path).unwrap(), b"mesh");
    assert_no_temp_extract_files(file_path.parent().unwrap());
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn ba2_writer_places_payloads_before_string_table() {
    let mut builder = Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder
        .add_bytes_with_compression(
            "data/compressed.txt",
            b"payload payload payload",
            CompressionOverride::Inherit,
        )
        .unwrap();
    builder
        .add_bytes_with_compression("data/plain.txt", b"plain", CompressionOverride::Store)
        .unwrap();
    builder
        .add_bytes_with_compression(
            "data/forced.txt",
            b"forced forced forced",
            CompressionOverride::Compress,
        )
        .unwrap();

    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();
    let string_table_offset = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let mut payload_end = 24 + 36 * archive.len();

    for entry in archive.entries() {
        let chunk = &entry.file().chunks()[0];
        assert!(chunk.offset() < string_table_offset);
        assert_eq!(chunk.offset(), u64::try_from(payload_end).unwrap());
        payload_end += usize::try_from(chunk.stored_size()).unwrap();
    }
    assert_eq!(u64::try_from(payload_end).unwrap(), string_table_offset);

    let mut cursor = usize::try_from(string_table_offset).unwrap();
    for entry in archive.entries() {
        let len = usize::from(u16::from_le_bytes(
            bytes[cursor..cursor + 2].try_into().unwrap(),
        ));
        cursor += 2;
        assert_eq!(bytes[cursor..cursor + len].as_bstr(), entry.name());
        cursor += len;
    }
    assert_eq!(cursor, bytes.len());
    assert_eq!(
        archive.read_file("data/compressed.txt").unwrap().unwrap(),
        b"payload payload payload"
    );
    assert_eq!(
        archive.read_file("data/plain.txt").unwrap().unwrap(),
        b"plain"
    );
    assert_eq!(
        archive.read_file("data/forced.txt").unwrap().unwrap(),
        b"forced forced forced"
    );
}

#[test]
fn writes_ba2_v3_archive_from_bytes() {
    let mut builder = Builder::new();
    builder.set_version(ArchiveVersion::v3);
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v3);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn writes_zlib_compressed_ba2_gnrl_archive() {
    let mut builder = Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder
        .add_bytes("data/file.txt", b"payload payload payload")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
    let entry = &archive.entries()[0];
    assert!(entry.file().chunks()[0].is_compressed());
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload payload payload"
    );
}

#[test]
fn writes_lz4_compressed_ba2_v3_gnrl_archive() {
    let mut builder = Builder::new();
    builder.set_version(ArchiveVersion::v3);
    builder.set_compression(Some(Ba2CompressionFormat::LZ4));
    builder
        .add_bytes("data/file.txt", b"payload payload payload")
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v3);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::LZ4);
    let entry = &archive.entries()[0];
    assert!(entry.file().chunks()[0].is_compressed());
    assert_eq!(
        archive.read_file("data/file.txt").unwrap().unwrap(),
        b"payload payload payload"
    );
}

#[test]
fn ba2_writer_can_leave_one_file_uncompressed() {
    let mut builder = Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder
        .add_bytes_with_compression(
            "compressed.txt",
            b"compressed payload",
            CompressionOverride::Inherit,
        )
        .unwrap();
    builder
        .add_bytes_with_compression("plain.txt", b"plain payload", CompressionOverride::Store)
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert!(archive.get("compressed.txt").unwrap().file().chunks()[0].is_compressed());
    assert!(!archive.get("plain.txt").unwrap().file().chunks()[0].is_compressed());
    assert_eq!(
        archive.read_file("plain.txt").unwrap().unwrap(),
        b"plain payload"
    );
}

#[cfg(unix)]
#[test]
fn ba2_writer_add_dir_follows_file_symlinks_at_relative_path() {
    let root = output_dir("ba2-add-dir-symlink");
    let external = output_dir("ba2-add-dir-external");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(external.join("different.dds"), b"target bytes").unwrap();
    std::os::unix::fs::symlink(external.join("different.dds"), root.join("someThing.dds")).unwrap();

    let mut builder = Builder::new();
    builder.add_dir(&root).unwrap();
    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(
        archive.read_file("something.dds").unwrap().unwrap(),
        b"target bytes"
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(external).unwrap();
}

#[test]
fn ba2_lz4_writer_requires_v3_header() {
    let mut builder = Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::LZ4));
    builder.add_bytes("data/file.txt", b"payload").unwrap();

    assert!(matches!(
        builder.to_vec(),
        Err(Error::NotImplemented("BA2 LZ4 writer requires version 3"))
    ));
}

#[test]
fn ba2_writer_output_is_deterministic() {
    let mut first = Builder::new();
    first.add_bytes("b.txt", b"b").unwrap();
    first.add_bytes("a.txt", b"a").unwrap();

    let mut second = Builder::new();
    second.add_bytes("a.txt", b"a").unwrap();
    second.add_bytes("b.txt", b"b").unwrap();

    assert_eq!(first.to_vec().unwrap(), second.to_vec().unwrap());
}

#[test]
fn ba2_writer_rejects_duplicate_normalized_paths() {
    let mut builder = Builder::new();
    builder.add_bytes("Meshes/Foo.NIF", b"mesh").unwrap();

    assert!(matches!(
        builder.add_bytes("meshes\\foo.nif", b"other"),
        Err(Error::DuplicatePath)
    ));
}

#[test]
fn ba2_writer_rejects_unsafe_paths() {
    for path in ["", ".", "../evil.txt", "bad:name.txt", "bad\0name.txt"] {
        let mut builder = Builder::new();
        assert!(matches!(
            builder.add_bytes(path.as_bytes(), b"payload"),
            Err(Error::InvalidArchivePath)
        ));
    }
}

#[test]
fn ba2_dx10_writer_rejects_duplicate_and_unsafe_paths() {
    let mut builder = Dx10Builder::new();
    builder
        .add_texture_bytes("Textures/Tiny.DDS", tiny_texture_header(), [0xab; 16])
        .unwrap();
    assert!(matches!(
        builder.add_texture_bytes("textures\\tiny.dds", tiny_texture_header(), [0xcd; 16]),
        Err(Error::DuplicatePath)
    ));

    for path in ["", ".", "../evil.dds", "bad:name.dds", "bad\0name.dds"] {
        let mut builder = Dx10Builder::new();
        assert!(matches!(
            builder.add_texture_bytes(path.as_bytes(), tiny_texture_header(), [0xab; 16]),
            Err(Error::InvalidArchivePath)
        ));
    }
}

#[test]
fn ba2_named_lookup_prefers_string_table_over_hash_collision() {
    let bytes = two_entry_gnrl_archive_with_colliding_hashes();
    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.read_file("a.txt").unwrap().unwrap(), b"first");
    assert_eq!(archive.read_file("b.txt").unwrap().unwrap(), b"second");
    assert!(archive.read_file("ghost.txt").unwrap().is_none());
}

fn tiny_texture_header() -> TextureHeader {
    TextureHeader {
        height: 4,
        width: 4,
        mip_count: 1,
        format: 98,
        flags: 0,
        tile_mode: 0,
    }
}

fn push_zeros(out: &mut Vec<u8>, count: usize) {
    out.resize(out.len() + count, 0);
}

fn dx10_dds(width: u32, height: u32, mip_count: u32, format: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DDS ");
    push_u32(&mut bytes, 124);
    push_u32(
        &mut bytes,
        DDSD_CAPS
            | DDSD_HEIGHT
            | DDSD_WIDTH
            | DDSD_PIXELFORMAT
            | DDSD_MIPMAPCOUNT
            | DDSD_LINEARSIZE,
    );
    push_u32(&mut bytes, height);
    push_u32(&mut bytes, width);
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, mip_count);
    push_zeros(&mut bytes, 44);
    push_u32(&mut bytes, 32);
    push_u32(&mut bytes, DDPF_FOURCC);
    push_u32(&mut bytes, DX10);
    push_zeros(&mut bytes, 20);
    push_u32(&mut bytes, DDSCAPS_TEXTURE);
    push_zeros(&mut bytes, 16);
    push_u32(&mut bytes, format);
    push_u32(&mut bytes, DDS_DIMENSION_TEXTURE2D);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 0);
    assert_eq!(bytes.len(), 148);
    bytes.extend_from_slice(payload);
    bytes
}

fn dx10_cubemap_dds(
    width: u32,
    height: u32,
    mip_count: u32,
    format: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut bytes = dx10_dds(width, height, mip_count, format, payload);
    set_u32(&mut bytes, 112, 0xFE00);
    set_u32(&mut bytes, 136, 4);
    set_u32(&mut bytes, 140, 6);
    bytes
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn legacy_fourcc_dds(
    width: u32,
    height: u32,
    mip_count: u32,
    fourcc: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DDS ");
    push_u32(&mut bytes, 124);
    push_u32(
        &mut bytes,
        DDSD_CAPS
            | DDSD_HEIGHT
            | DDSD_WIDTH
            | DDSD_PIXELFORMAT
            | DDSD_MIPMAPCOUNT
            | DDSD_LINEARSIZE,
    );
    push_u32(&mut bytes, height);
    push_u32(&mut bytes, width);
    push_u32(&mut bytes, payload.len().try_into().unwrap());
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, mip_count);
    push_zeros(&mut bytes, 44);
    push_u32(&mut bytes, 32);
    push_u32(&mut bytes, DDPF_FOURCC);
    push_u32(&mut bytes, fourcc);
    push_zeros(&mut bytes, 20);
    push_u32(&mut bytes, DDSCAPS_TEXTURE);
    push_zeros(&mut bytes, 16);
    assert_eq!(bytes.len(), 128);
    bytes.extend_from_slice(payload);
    bytes
}

fn plain_rgba_dds(width: u32, height: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DDS ");
    push_u32(&mut bytes, 124);
    push_u32(
        &mut bytes,
        DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT,
    );
    push_u32(&mut bytes, height);
    push_u32(&mut bytes, width);
    push_u32(&mut bytes, width * 4);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 1);
    push_zeros(&mut bytes, 44);
    push_u32(&mut bytes, 32);
    push_u32(&mut bytes, 0x41);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 32);
    push_u32(&mut bytes, 0x0000_00ff);
    push_u32(&mut bytes, 0x0000_ff00);
    push_u32(&mut bytes, 0x00ff_0000);
    push_u32(&mut bytes, 0xff00_0000);
    push_u32(&mut bytes, DDSCAPS_TEXTURE);
    push_zeros(&mut bytes, 16);
    assert_eq!(bytes.len(), 128);
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn writes_ba2_dx10_archive_from_texture_payload() {
    let mut builder = Dx10Builder::new();
    builder
        .add_texture_bytes("Textures/Tiny.DDS", tiny_texture_header(), [0xab; 16])
        .unwrap();

    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();

    assert_eq!(archive.info().format, PayloadFormat::DX10);
    assert_eq!(archive.info().version, ArchiveVersion::v1);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
    assert!(archive.info().strings);
    assert_eq!(archive.entries()[0].file().chunks()[0].mips, Some(0..=0));

    let data = archive.read_file("textures/tiny.dds").unwrap().unwrap();
    assert_eq!(&data[0..4], b"DDS ");
    assert_eq!(u32::from_le_bytes(data[12..16].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(data[16..20].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(data[28..32].try_into().unwrap()), 1);
    assert_eq!(&data[84..88], b"DX10");
    assert_eq!(u32::from_le_bytes(data[128..132].try_into().unwrap()), 98);
    assert_eq!(&data[148..], &[0xab; 16]);
}

#[test]
fn ba2_dx10_writer_places_payloads_before_string_table() {
    let mut builder = Dx10Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder
        .add_texture_bytes_with_compression(
            "textures/compressed.dds",
            tiny_texture_header(),
            [0xcd; 16],
            CompressionOverride::Inherit,
        )
        .unwrap();
    builder
        .add_texture_bytes_with_compression(
            "textures/plain.dds",
            tiny_texture_header(),
            [0xef; 16],
            CompressionOverride::Store,
        )
        .unwrap();

    let bytes = builder.to_vec().unwrap();
    let archive = Archive::from_slice(&bytes).unwrap();
    let string_table_offset = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let mut payload_end = 24 + 48 * archive.len();

    for entry in archive.entries() {
        let chunk = &entry.file().chunks()[0];
        assert!(chunk.offset() < string_table_offset);
        assert_eq!(chunk.offset(), u64::try_from(payload_end).unwrap());
        payload_end += usize::try_from(chunk.stored_size()).unwrap();
    }
    assert_eq!(u64::try_from(payload_end).unwrap(), string_table_offset);
    assert_eq!(
        archive
            .read_file("textures/compressed.dds")
            .unwrap()
            .unwrap()
            .split_off(148),
        [0xcd; 16]
    );
    assert_eq!(
        archive
            .read_file("textures/plain.dds")
            .unwrap()
            .unwrap()
            .split_off(148),
        [0xef; 16]
    );
}

#[test]
fn writes_lz4_compressed_ba2_v3_dx10_archive() {
    let mut builder = Dx10Builder::new();
    builder.set_version(ArchiveVersion::v3);
    builder.set_compression(Some(Ba2CompressionFormat::LZ4));
    builder
        .add_texture_bytes("textures/tiny.dds", tiny_texture_header(), [0x12; 16])
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();

    assert_eq!(archive.info().version, ArchiveVersion::v3);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::LZ4);
    assert!(archive.entries()[0].file().chunks()[0].is_compressed());
    assert_eq!(
        archive
            .read_file("textures/tiny.dds")
            .unwrap()
            .unwrap()
            .split_off(148),
        [0x12; 16]
    );
}

#[test]
fn ba2_dx10_lz4_writer_requires_v3_header() {
    let mut builder = Dx10Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::LZ4));
    builder
        .add_texture_bytes("textures/tiny.dds", tiny_texture_header(), [0xab; 16])
        .unwrap();

    assert!(matches!(
        builder.to_vec(),
        Err(Error::NotImplemented("BA2 LZ4 writer requires version 3"))
    ));
}

#[test]
fn ba2_dx10_writer_rejects_invalid_texture_metadata() {
    let mut builder = Dx10Builder::new();
    assert!(matches!(
        builder.add_texture_bytes(
            "textures/tiny.dds",
            TextureHeader {
                width: 0,
                ..tiny_texture_header()
            },
            b"texture"
        ),
        Err(Error::Dds("zero-sized texture"))
    ));
    assert!(matches!(
        builder.add_texture_bytes(
            "textures/tiny.dds",
            TextureHeader {
                format: 255,
                ..tiny_texture_header()
            },
            b"texture"
        ),
        Err(Error::Dds("unsupported DXGI format"))
    ));
    assert!(matches!(
        builder.add_texture_bytes("textures/tiny.dds", tiny_texture_header(), b"short"),
        Err(Error::Dds("DDS payload size does not match metadata"))
    ));
}

#[test]
fn ba2_dx10_writer_ingests_dx10_dds() {
    let payload = [0xabu8; 16];
    let dds = dx10_dds(4, 4, 1, 98, &payload);
    let mut builder = Dx10Builder::new();
    builder.add_dds_bytes("textures/tiny.dds", &dds).unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let entry = archive.get("textures/tiny.dds").unwrap();
    assert_eq!(
        entry.file().header,
        dream_archive::ba2::FileHeader::DX10(TextureHeader {
            height: 4,
            width: 4,
            mip_count: 1,
            format: 98,
            flags: 0,
            tile_mode: 0,
        })
    );
    assert_eq!(archive.read_entry(entry).unwrap().split_off(148), payload);
}

#[test]
fn reconstructed_dx10_dds_uses_zero_depth_for_2d_textures() {
    let mut builder = Dx10Builder::new();
    builder
        .add_texture_bytes("textures/tiny.dds", tiny_texture_header(), [0xab; 16])
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let data = archive.read_file("textures/tiny.dds").unwrap().unwrap();

    assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 0);
}

#[test]
fn ba2_dx10_writer_ingests_legacy_dxt1_dds() {
    let payload = [0xcdu8; 8];
    let dds = legacy_fourcc_dds(4, 4, 1, u32::from_le_bytes(*b"DXT1"), &payload);
    let mut builder = Dx10Builder::new();
    builder.add_dds_bytes("textures/tiny.dds", &dds).unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let entry = archive.get("textures/tiny.dds").unwrap();
    assert_eq!(
        entry.file().header,
        dream_archive::ba2::FileHeader::DX10(TextureHeader {
            height: 4,
            width: 4,
            mip_count: 1,
            format: 71,
            flags: 0,
            tile_mode: 0,
        })
    );
    assert_eq!(archive.read_entry(entry).unwrap().split_off(128), payload);
}

#[test]
fn ba2_dx10_writer_ingests_legacy_block_format_matrix() {
    for (fourcc, expected_format, payload_len) in [
        (*b"DXT3", 74, 16),
        (*b"DXT5", 77, 16),
        (*b"BC4U", 80, 8),
        (*b"BC4S", 81, 8),
        (*b"BC5U", 83, 16),
        (*b"BC5S", 84, 16),
    ] {
        let payload = vec![expected_format; payload_len];
        let dds = legacy_fourcc_dds(4, 4, 1, u32::from_le_bytes(fourcc), &payload);
        let mut builder = Dx10Builder::new();
        builder.add_dds_bytes("textures/block.dds", &dds).unwrap();

        let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
        let dream_archive::ba2::FileHeader::DX10(header) = archive.entries()[0].file().header
        else {
            panic!("expected DX10 texture header");
        };
        assert_eq!(header.format, expected_format);
        assert_eq!(
            archive
                .read_entry(&archive.entries()[0])
                .unwrap()
                .split_off(128),
            payload
        );
    }
}

#[test]
fn ba2_dx10_writer_ingests_non_square_multimip_dx10_dds() {
    let payload = [0x98u8; 80];
    let dds = dx10_dds(8, 4, 4, 98, &payload);
    let mut builder = Dx10Builder::new();
    builder
        .add_dds_bytes("textures/non-square.dds", &dds)
        .unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let entry = archive.get("textures/non-square.dds").unwrap();
    assert_eq!(archive.read_entry(entry).unwrap().split_off(148), payload);
}

#[test]
fn ba2_dx10_writer_rejects_unsupported_legacy_fourcc() {
    let dds = legacy_fourcc_dds(4, 4, 1, u32::from_le_bytes(*b"NOPE"), &[0; 8]);
    let mut builder = Dx10Builder::new();
    assert!(matches!(
        builder.add_dds_bytes("textures/nope.dds", &dds),
        Err(Error::Dds("unsupported DDS FourCC"))
    ));
}

#[test]
fn ba2_dx10_writer_ingests_plain_rgba_dds() {
    let payload = [0x34u8; 64];
    let dds = plain_rgba_dds(4, 4, &payload);
    let mut builder = Dx10Builder::new();
    builder.add_dds_bytes("textures/rgba.dds", &dds).unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let entry = archive.get("textures/rgba.dds").unwrap();
    assert_eq!(
        entry.file().header,
        dream_archive::ba2::FileHeader::DX10(TextureHeader {
            height: 4,
            width: 4,
            mip_count: 1,
            format: 28,
            flags: 0,
            tile_mode: 0,
        })
    );
    assert_eq!(archive.read_entry(entry).unwrap().split_off(128), payload);
}

#[test]
fn ba2_dx10_writer_ingests_dx10_cubemap_dds() {
    let payload = [0x56u8; 96];
    let dds = dx10_cubemap_dds(4, 4, 1, 98, &payload);
    let mut builder = Dx10Builder::new();
    builder.add_dds_bytes("textures/cube.dds", &dds).unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    let data = archive.read_file("textures/cube.dds").unwrap().unwrap();
    assert_eq!(u32::from_le_bytes(data[136..140].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 6);
    assert_eq!(&data[148..], payload);
}

#[test]
fn ba2_dx10_writer_rejects_dds_payload_size_mismatch() {
    let dds = dx10_dds(4, 4, 1, 98, &[0; 15]);
    let mut builder = Dx10Builder::new();

    assert!(matches!(
        builder.add_dds_bytes("textures/tiny.dds", &dds),
        Err(Error::Dds("DDS payload size does not match metadata"))
    ));
}

#[test]
fn ba2_dx10_writer_rejects_dds_arrays_and_volumes() {
    let mut array = dx10_dds(4, 4, 1, 98, &[0; 32]);
    set_u32(&mut array, 140, 2);
    let mut builder = Dx10Builder::new();
    assert!(matches!(
        builder.add_dds_bytes("textures/array.dds", &array),
        Err(Error::Dds("unsupported DDS array size"))
    ));

    let mut volume = dx10_dds(4, 4, 1, 98, &[0; 16]);
    set_u32(&mut volume, 132, 4);
    assert!(matches!(
        builder.add_dds_bytes("textures/volume.dds", &volume),
        Err(Error::Dds("unsupported DDS resource dimension"))
    ));

    let mut legacy_volume = legacy_fourcc_dds(4, 4, 1, u32::from_le_bytes(*b"DXT1"), &[0; 8]);
    set_u32(&mut legacy_volume, 24, 1);
    assert!(matches!(
        builder.add_dds_bytes("textures/legacy-volume.dds", &legacy_volume),
        Err(Error::Dds("unsupported DDS volume texture"))
    ));
}

#[test]
fn ba2_dx10_writer_rejects_malformed_dds_headers() {
    let mut builder = Dx10Builder::new();
    assert!(matches!(
        builder.add_dds_bytes("textures/bad.dds", b"not a dds"),
        Err(Error::Dds("truncated DDS header"))
    ));

    let mut invalid_magic = dx10_dds(4, 4, 1, 98, &[0; 16]);
    invalid_magic[0..4].copy_from_slice(b"NOPE");
    assert!(matches!(
        builder.add_dds_bytes("textures/bad.dds", &invalid_magic),
        Err(Error::Dds("invalid DDS magic"))
    ));

    let mut invalid_header_size = dx10_dds(4, 4, 1, 98, &[0; 16]);
    set_u32(&mut invalid_header_size, 4, 120);
    assert!(matches!(
        builder.add_dds_bytes("textures/bad.dds", &invalid_header_size),
        Err(Error::Dds("invalid DDS header size"))
    ));

    let mut truncated_dx10 = dx10_dds(4, 4, 1, 98, &[0; 16]);
    truncated_dx10.truncate(147);
    assert!(matches!(
        builder.add_dds_bytes("textures/bad.dds", &truncated_dx10),
        Err(Error::Dds("truncated DDS DX10 header"))
    ));
}

#[test]
fn ba2_dx10_writer_validates_multimip_block_payload_size() {
    let payload = [0x55; 96];
    let dds = dx10_dds(8, 8, 3, 98, &payload);
    let mut builder = Dx10Builder::new();
    builder.add_dds_bytes("textures/mips.dds", &dds).unwrap();

    let archive = Archive::from_slice(&builder.to_vec().unwrap()).unwrap();
    assert_eq!(
        archive
            .read_file("textures/mips.dds")
            .unwrap()
            .unwrap()
            .split_off(148),
        payload
    );

    let short = dx10_dds(8, 8, 3, 98, &[0; 95]);
    assert!(matches!(
        builder.add_dds_bytes("textures/short.dds", &short),
        Err(Error::Dds("DDS payload size does not match metadata"))
    ));
}

#[derive(Clone, Copy)]
struct TinyTextureOptions<'a> {
    height: u16,
    width: u16,
    mip_count: u8,
    format: u8,
    flags: u8,
    tile_mode: u8,
    first_mip: u16,
    last_mip: u16,
    name: &'a [u8],
    payload: &'a [u8],
}

const TINY_TEXTURE_PAYLOAD: [u8; 192] = [0xab; 192];
const TINY_CUBEMAP_PAYLOAD: [u8; 1152] = [0xcd; 1152];

impl Default for TinyTextureOptions<'_> {
    fn default() -> Self {
        Self {
            height: 8,
            width: 16,
            mip_count: 4,
            format: 98,
            flags: 0,
            tile_mode: 0,
            first_mip: 0,
            last_mip: 3,
            name: b"tiny.dds",
            payload: &TINY_TEXTURE_PAYLOAD,
        }
    }
}

fn tiny_texture_archive(options: TinyTextureOptions<'_>) -> Vec<u8> {
    let string_table_offset = 72_u64;
    let payload_offset = string_table_offset + 2 + u64::try_from(options.name.len()).unwrap();
    let mut bytes = Vec::new();
    push_u32(&mut bytes, MAGIC);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, DX10);
    push_u32(&mut bytes, 1);
    push_u64(&mut bytes, string_table_offset);

    let (hash, _) = dream_archive::ba2::hash_file(options.name.as_bstr());
    push_u32(&mut bytes, hash.file);
    push_u32(&mut bytes, hash.extension);
    push_u32(&mut bytes, hash.directory);
    bytes.push(0);
    bytes.push(1);
    push_u16(&mut bytes, FILE_HEADER_SIZE_DX10);
    push_u16(&mut bytes, options.height);
    push_u16(&mut bytes, options.width);
    bytes.push(options.mip_count);
    bytes.push(options.format);
    bytes.push(options.flags);
    bytes.push(options.tile_mode);
    push_u64(&mut bytes, payload_offset);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, options.payload.len().try_into().unwrap());
    push_u16(&mut bytes, options.first_mip);
    push_u16(&mut bytes, options.last_mip);
    push_u32(&mut bytes, CHUNK_SENTINEL);
    assert_eq!(u64::try_from(bytes.len()).unwrap(), string_table_offset);
    push_u16(&mut bytes, options.name.len().try_into().unwrap());
    bytes.extend_from_slice(options.name);
    bytes.extend_from_slice(options.payload);
    bytes
}

#[test]
fn rejects_invalid_magic() {
    let mut bytes = tiny_archive(TinyArchiveOptions::default());
    bytes[0..4].copy_from_slice(&u32::to_le_bytes(0x1234_5678));
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::InvalidMagic(0x1234_5678))
    ));
}

#[test]
fn rejects_invalid_format() {
    let bytes = tiny_archive(TinyArchiveOptions {
        format: u32::from_le_bytes(*b"NOPE"),
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::InvalidFormat(_))
    ));
}

#[test]
fn rejects_invalid_version() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 0x101,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::InvalidVersion(0x101))
    ));
}

#[test]
fn rejects_invalid_chunk_sentinel() {
    let bytes = tiny_archive(TinyArchiveOptions {
        sentinel: 0xDEAD_BEEF,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::InvalidChunkSentinel(0xDEAD_BEEF))
    ));
}

#[test]
fn rejects_truncated_headers_as_unexpected_eof() {
    for bytes in [&b""[..], &b"BTDX"[..], &b"BTDX\x01\0"[..]] {
        assert!(
            matches!(Archive::from_slice(bytes), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
}

#[test]
fn rejects_chunk_offsets_outside_archive() {
    let bytes = tiny_archive(TinyArchiveOptions {
        chunk_offset: 10_000,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_chunk_size_overflow() {
    let bytes = tiny_archive(TinyArchiveOptions {
        chunk_offset: u64::MAX,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::IntegralTruncation | Error::OutOfBounds)
    ));
}

#[test]
fn rejects_string_table_offset_outside_archive() {
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 10_000,
        name: None,
        chunk_offset: 60,
        ..TinyArchiveOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::OutOfBounds)
    ));
}

#[test]
fn rejects_truncated_string_table_entry() {
    let mut bytes = tiny_archive(TinyArchiveOptions::default());
    bytes.truncate(62 + 3);
    assert!(matches!(Archive::from_slice(&bytes), Err(Error::Io(_))));
}

#[test]
fn accepts_v2_extra_header_field() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 2,
        string_table_offset: 68,
        chunk_offset: 68,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(archive.info().version, ArchiveVersion::v2);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
}

#[test]
fn synthetic_texture_archive_reconstructs_dx10_dds_header() {
    let bytes = tiny_texture_archive(TinyTextureOptions::default());
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(archive.info().format, PayloadFormat::DX10);
    let data = archive.read_file("tiny.dds").unwrap().unwrap();

    assert_eq!(&data[0..4], b"DDS ");
    assert_eq!(u32::from_le_bytes(data[12..16].try_into().unwrap()), 8);
    assert_eq!(u32::from_le_bytes(data[16..20].try_into().unwrap()), 16);
    assert_eq!(u32::from_le_bytes(data[28..32].try_into().unwrap()), 4);
    assert_eq!(&data[84..88], b"DX10");
    assert_eq!(u32::from_le_bytes(data[128..132].try_into().unwrap()), 98);
    assert_eq!(u32::from_le_bytes(data[132..136].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 1);
    assert_eq!(&data[148..], TINY_TEXTURE_PAYLOAD);
}

#[test]
fn synthetic_cubemap_sets_dds_cube_metadata() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        flags: 1,
        payload: &TINY_CUBEMAP_PAYLOAD,
        ..TinyTextureOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let data = archive.read_file("tiny.dds").unwrap().unwrap();

    assert_eq!(
        u32::from_le_bytes(data[112..116].try_into().unwrap()),
        0xFE00
    );
    assert_eq!(u32::from_le_bytes(data[136..140].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 6);
}

#[test]
fn rejects_zero_sized_texture_during_extraction() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        width: 0,
        ..TinyTextureOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::Dds("zero-sized texture"))
    ));
}

#[test]
fn rejects_zero_mip_count_texture_during_parse() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        mip_count: 0,
        ..TinyTextureOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::Dds("zero mip count"))
    ));
}

#[test]
fn rejects_dx10_texture_payload_size_mismatch_during_parse() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        payload: b"short",
        ..TinyTextureOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::Dds("DDS payload size does not match metadata"))
    ));
}

#[test]
fn rejects_invalid_dx10_texture_mip_ranges_during_parse() {
    for (first_mip, last_mip) in [(3, 0), (0, 4)] {
        let bytes = tiny_texture_archive(TinyTextureOptions {
            first_mip,
            last_mip,
            ..TinyTextureOptions::default()
        });
        assert!(matches!(
            Archive::from_slice(&bytes),
            Err(Error::Dds("invalid BA2 texture mip range"))
        ));
    }
}

#[test]
fn rejects_missing_dx10_texture_mip_range_during_parse() {
    let payload = [0xef; 176];
    let bytes = tiny_texture_archive(TinyTextureOptions {
        first_mip: 0,
        last_mip: 2,
        payload: &payload,
        ..TinyTextureOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::Dds("missing BA2 texture mip range"))
    ));
}

#[test]
fn rejects_unsupported_dxgi_format_without_partial_output() {
    let bytes = tiny_texture_archive(TinyTextureOptions {
        format: 255,
        ..TinyTextureOptions::default()
    });
    assert!(matches!(
        Archive::from_slice(&bytes),
        Err(Error::Dds("unsupported DXGI format"))
    ));
}

#[test]
fn parses_gnmf_metadata_but_does_not_extract_payload() {
    let metadata = [1, 2, 3, 5, 8, 13, 21, 34];
    let bytes = tiny_gnmf_archive(metadata, b"gnmf-payload");
    let archive = Archive::from_slice(&bytes).unwrap();
    let entry = &archive.entries()[0];

    assert_eq!(archive.info().format, PayloadFormat::GNMF);
    assert_eq!(entry.name(), "");
    assert_eq!(
        entry.file().header,
        dream_archive::ba2::FileHeader::GNMF(metadata)
    );
    assert_eq!(entry.file().chunks()[0].mips, Some(2..=5));
    assert!(matches!(
        archive.read_entry(entry),
        Err(Error::NotImplemented("BA2 GNMF extraction"))
    ));

    let mut out = b"prefix".to_vec();
    assert!(matches!(
        archive.read_entry_into(entry, &mut out),
        Err(Error::NotImplemented("BA2 GNMF extraction"))
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn block_compressed_dds_size_uses_rounded_blocks() {
    let payload = [0x71; 56];
    let bytes = tiny_texture_archive(TinyTextureOptions {
        width: 5,
        height: 5,
        format: 71,
        payload: &payload,
        ..TinyTextureOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let data = archive.read_file("tiny.dds").unwrap().unwrap();
    assert_eq!(u32::from_le_bytes(data[20..24].try_into().unwrap()), 32);
}

#[test]
fn v3_unknown_compression_code_means_zip() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(0xFFFF_FFFE),
        string_table_offset: 72,
        chunk_offset: 72,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(archive.info().version, ArchiveVersion::v3);
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::Zip);
}

#[test]
fn v3_compression_code_three_means_lz4() {
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 72,
        chunk_offset: 72,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(archive.info().compression_format, Ba2CompressionFormat::LZ4);
}

#[test]
fn extracts_synthetic_zlib_chunk() {
    let payload = zlib_compress(b"compressed hello");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 16,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(
        archive.read_entry(&archive.entries()[0]).unwrap(),
        b"compressed hello"
    );
    let mut out = Vec::new();
    assert_eq!(
        archive
            .extract_entry(&archive.entries()[0], &mut out)
            .unwrap(),
        16
    );
    assert_eq!(out, b"compressed hello");
}

#[test]
fn extracts_ba2_archive_to_directory() {
    let archive = Archive::from_slice(&tiny_archive(TinyArchiveOptions {
        name: Some(b"textures/hello.txt"),
        chunk_offset: 60 + 2 + u64::try_from(b"textures/hello.txt".len()).unwrap(),
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2");

    assert_eq!(archive.extract_to(&out).unwrap(), 5);
    assert_eq!(
        std::fs::read(out.join("textures").join("hello.txt")).unwrap(),
        b"hello"
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn ba2_extract_to_rejects_parent_directory_paths() {
    let archive = Archive::from_slice(&tiny_archive(TinyArchiveOptions {
        name: Some(b"../evil.txt"),
        chunk_offset: 60 + 2 + u64::try_from(b"../evil.txt".len()).unwrap(),
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2-traversal");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("evil.txt").exists());
}

#[test]
fn ba2_extract_to_rejects_mixed_separator_traversal_paths() {
    for name in [
        b"textures\\..\\evil.txt".as_slice(),
        b"textures/../../evil.txt".as_slice(),
    ] {
        let archive = Archive::from_slice(&tiny_archive(TinyArchiveOptions {
            name: Some(name),
            chunk_offset: 60 + 2 + u64::try_from(name.len()).unwrap(),
            ..TinyArchiveOptions::default()
        }))
        .unwrap();
        let out = output_dir("ba2-mixed-traversal");

        assert!(
            matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
        );
        assert!(!out.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(out);
    }
}

#[test]
fn ba2_extract_to_rejects_colon_paths() {
    let archive = Archive::from_slice(&tiny_archive(TinyArchiveOptions {
        name: Some(b"textures/bad:name.txt"),
        chunk_offset: 60 + 2 + u64::try_from(b"textures/bad:name.txt".len()).unwrap(),
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2-colon");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(!out.join("textures").join("bad:name.txt").exists());
}

#[test]
fn ba2_extract_to_rejects_unnamed_entries() {
    let archive = Archive::from_slice(&tiny_archive(TinyArchiveOptions {
        name: None,
        string_table_offset: 0,
        ..TinyArchiveOptions::default()
    }))
    .unwrap();
    let out = output_dir("ba2-unnamed");

    assert!(
        matches!(archive.extract_to(&out), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData)
    );
}

#[test]
fn detects_zlib_decompression_size_mismatch() {
    let payload = zlib_compress(b"short");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 99,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert!(matches!(
        archive.read_entry(&archive.entries()[0]),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 5
        })
    ));
}

#[test]
fn failed_zlib_decompression_does_not_leave_partial_output() {
    let payload = zlib_compress(b"short");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 99,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let mut out = b"prefix".to_vec();
    assert!(
        archive
            .read_entry_into(&archive.entries()[0], &mut out)
            .is_err()
    );
    assert_eq!(out, b"prefix");
}

#[test]
fn rejects_synthetic_zlib_trailing_data() {
    let mut payload = zlib_compress(b"compressed hello");
    payload.push(0);
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 16,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert!(matches!(
        archive.read_entry(&archive.entries()[0]),
        Err(Error::TrailingCompressedData)
    ));
}

#[test]
fn zlib_over_expansion_does_not_leave_partial_vec_output() {
    let payload = zlib_compress(b"payload larger than declared");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 7,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.read_entry_into(&archive.entries()[0], &mut out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn failed_extract_to_keeps_existing_ba2_file_and_removes_temp() {
    let payload = zlib_compress(b"payload larger than declared");
    let name = b"textures/hello.txt";
    let bytes = tiny_archive(TinyArchiveOptions {
        name: Some(name),
        chunk_offset: 60 + 2 + u64::try_from(name.len()).unwrap(),
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 7,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let out = output_dir("ba2-atomic-failure");
    let file_path = out.join("textures").join("hello.txt");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"old contents").unwrap();

    assert!(matches!(
        archive.extract_to(&out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(std::fs::read(&file_path).unwrap(), b"old contents");
    assert_no_temp_extract_files(file_path.parent().unwrap());
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn failed_extract_entry_to_path_keeps_existing_ba2_file_and_removes_temp() {
    let payload = zlib_compress(b"payload larger than declared");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 7,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let out = output_dir("ba2-entry-atomic-failure");
    std::fs::create_dir_all(&out).unwrap();
    let file_path = out.join("hello.txt");
    std::fs::write(&file_path, b"old contents").unwrap();

    assert!(matches!(
        archive.extract_entry_to_path(&archive.entries()[0], &file_path),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(std::fs::read(&file_path).unwrap(), b"old contents");
    assert_no_temp_extract_files(&out);
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn zlib_writer_extraction_does_not_write_extra_probe_byte() {
    let payload = zlib_compress(b"payload larger than declared");
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 7,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.extract_entry(&archive.entries()[0], &mut out),
        Err(Error::DecompressionSizeMismatch {
            expected: 7,
            actual: 8
        })
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn zlib_writer_extraction_rejects_trailing_data_before_writing() {
    let mut payload = zlib_compress(b"compressed hello");
    payload.push(0);
    let bytes = tiny_archive(TinyArchiveOptions {
        string_table_offset: 0,
        name: None,
        chunk_offset: 60,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 16,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let mut out = b"prefix".to_vec();

    assert!(matches!(
        archive.extract_entry(&archive.entries()[0], &mut out),
        Err(Error::TrailingCompressedData)
    ));
    assert_eq!(out, b"prefix");
}

#[test]
fn extracts_synthetic_lz4_chunk() {
    let payload = lz4_flex::block::compress(b"lz4 says hello");
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 0,
        name: None,
        chunk_offset: 72,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 14,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert_eq!(
        archive.read_entry(&archive.entries()[0]).unwrap(),
        b"lz4 says hello"
    );
    let mut out = Vec::new();
    assert_eq!(
        archive
            .extract_entry(&archive.entries()[0], &mut out)
            .unwrap(),
        14
    );
    assert_eq!(out, b"lz4 says hello");
}

#[test]
fn detects_lz4_decompression_size_mismatch() {
    let payload = lz4_flex::block::compress(b"lz4 short");
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 0,
        name: None,
        chunk_offset: 72,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 99,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    assert!(matches!(
        archive.read_entry(&archive.entries()[0]),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 9
        })
    ));
}

#[test]
fn failed_lz4_extract_entry_to_path_keeps_existing_ba2_file_and_removes_temp() {
    let payload = lz4_flex::block::compress(b"lz4 short");
    let bytes = tiny_archive(TinyArchiveOptions {
        version: 3,
        compression_code: Some(3),
        string_table_offset: 0,
        name: None,
        chunk_offset: 72,
        chunk_packed_size: payload.len().try_into().unwrap(),
        chunk_size: 99,
        payload: &payload,
        ..TinyArchiveOptions::default()
    });
    let archive = Archive::from_slice(&bytes).unwrap();
    let out = output_dir("ba2-entry-lz4-atomic-failure");
    std::fs::create_dir_all(&out).unwrap();
    let file_path = out.join("hello.txt");
    std::fs::write(&file_path, b"old contents").unwrap();

    assert!(matches!(
        archive.extract_entry_to_path(&archive.entries()[0], &file_path),
        Err(Error::DecompressionSizeMismatch {
            expected: 99,
            actual: 9
        })
    ));
    assert_eq!(std::fs::read(&file_path).unwrap(), b"old contents");
    assert_no_temp_extract_files(&out);
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn ba2_builder_can_defer_existing_archive_entry() {
    let mut source_builder = Builder::new();
    source_builder
        .add_bytes("data/source.txt", b"payload")
        .unwrap();
    let source = std::sync::Arc::new(Archive::from_vec(source_builder.to_vec().unwrap()).unwrap());
    let (id, _) = source.entries_with_ids().next().unwrap();

    let mut builder = Builder::new();
    builder
        .add_archive_entry("data/copied.txt", std::sync::Arc::clone(&source), id)
        .unwrap();
    let archive = Archive::from_vec(builder.to_vec().unwrap()).unwrap();
    assert_eq!(
        archive.read_file_required("data/copied.txt").unwrap(),
        b"payload"
    );
}

#[test]
fn ba2_builder_can_defer_dx10_archive_entry_with_legacy_dds_header() {
    let mut source_builder = Dx10Builder::new();
    source_builder
        .add_texture_bytes(
            "textures/bc1.dds",
            TextureHeader {
                height: 4,
                width: 4,
                mip_count: 1,
                format: 71,
                flags: 0,
                tile_mode: 0,
            },
            [0xab; 8],
        )
        .unwrap();
    let source = std::sync::Arc::new(Archive::from_vec(source_builder.to_vec().unwrap()).unwrap());
    let (id, _) = source.entries_with_ids().next().unwrap();
    assert_eq!(source.extracted_len_by_id(id).unwrap(), 136);

    let mut builder = Builder::new();
    builder
        .add_archive_entry("textures/copied.dds", std::sync::Arc::clone(&source), id)
        .unwrap();
    let archive = Archive::from_vec(builder.to_vec().unwrap()).unwrap();
    let copied = archive.read_file_required("textures/copied.dds").unwrap();
    assert_eq!(copied.len(), 136);
    assert_eq!(&copied[84..88], b"DXT1");
    assert_eq!(&copied[128..], &[0xab; 8]);
}
