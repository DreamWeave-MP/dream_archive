use mlua::Lua;
use std::path::PathBuf;

fn lua_with_module() -> Lua {
    let lua = Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dream_archive", module).unwrap();
    lua
}

fn temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dream-archive-lua-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn lua_builds_and_reads_ba2_through_top_level_facade() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local builder = dream_archive.ba2.Builder.new()
        builder:set_compression(dream_archive.ba2.compression.zip)
        builder:set_version(dream_archive.ba2.version.v1)
        builder:add_bytes("Meshes/Foo.NIF", "mesh payload")
        local bytes = builder:to_bytes()
        assert(dream_archive.guess_format(bytes) == "ba2")

        local archive = dream_archive.open_bytes(bytes)
        assert(archive:format() == "ba2")
        assert(archive:len() == 1)
        assert(archive:read_file_required("meshes/foo.nif") == "mesh payload")
        assert(archive:read_entry(1) == "mesh payload")
        assert(archive:extract_entry(1) == "mesh payload")
        local entries = archive:entries()
        assert(entries[1].path == "meshes\\foo.nif")

        local ba2 = dream_archive.ba2.open_bytes(bytes)
        assert(ba2:info().format == "gnrl")
        assert(ba2:entries()[1].path == "meshes\\foo.nif")
        assert(ba2:entries()[1].name == "meshes\\foo.nif")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_ba2_stringless_entries_report_nil_paths() {
    let lua = lua_with_module();
    let fixture = std::env::current_dir()
        .unwrap()
        .join("tests/fixtures/ba2/missing_string_table/in.ba2");
    lua.globals()
        .set("fixture_path", fixture.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r"
        local generic = dream_archive.open_path(fixture_path)
        assert(generic:entries()[1].path == nil)

        local ba2 = dream_archive.ba2.open_path(fixture_path)
        local entry = ba2:entries()[1]
        assert(entry.path == nil)
        assert(entry.name == nil)
        assert(ba2:read_entry(1) ~= nil)
    ",
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_exports_expected_module_shape() {
    let lua = lua_with_module();

    lua.load(
        r#"
        assert(type(dream_archive.open_bytes) == "function")
        assert(type(dream_archive.detect_path) == "function")
        assert(type(dream_archive.ba2.Builder.new) == "function")
        assert(type(dream_archive.ba2.Dx10Builder.new) == "function")
        assert(dream_archive.ba2.compression.none == "none")
        assert(dream_archive.ba2.version.v8 == 8)
        assert(dream_archive.bsa.encoding.cp437 == "cp437")
        assert(type(dream_archive.bsa.tes3.Builder.new) == "function")
        assert(type(dream_archive.bsa.tes4.Builder.new) == "function")
        assert(dream_archive.bsa.tes4.profile.skyrim_se == "skyrim_se")
        assert(dream_archive.bsa.tes4.name_mode.hash_only == "hash_only")
        assert(type(dream_archive.bsa.tes4.archive_types.textures) == "number")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_builds_and_reads_tes3_and_encodes_paths() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local encoded = dream_archive.bsa.encode_filename("textures/zażółć.dds", dream_archive.bsa.encoding.windows1250)
        assert(encoded == "textures/za\191\243\179\230.dds")
        assert(dream_archive.bsa.decode_filename_lossy(encoded, "windows1250") == "textures/zażółć.dds")
        assert(dream_archive.bsa.encode_filename("a", "cp1250") == "a")
        assert(dream_archive.bsa.encode_filename("a", "cp1251") == "a")
        assert(dream_archive.bsa.encode_filename("a", "cp1252") == "a")

        local builder = dream_archive.bsa.tes3.Builder.new()
        builder:add_bytes("Meshes/Foo.NIF", "tes3 payload")
        local archive = dream_archive.bsa.tes3.open_bytes(builder:to_string())
        assert(archive:contains("meshes/foo.nif"))
        assert(archive:read_file_required("meshes/foo.nif") == "tes3 payload")
        assert(archive:entries()[1].path == "meshes\\foo.nif")
        assert(archive:read_entry(1) == "tes3 payload")
        assert(archive:extract_file_required("meshes/foo.nif") == "tes3 payload")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_top_level_facade_reads_tes3_and_tes4() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        tes3_builder:add_bytes("meshes/foo.nif", "tes3")
        local tes3 = dream_archive.open_bytes(tes3_builder:to_bytes())
        assert(tes3:format() == "bsa-tes3")
        assert(tes3:entries()[1].format == "bsa-tes3")
        assert(tes3:read_file_required("meshes/foo.nif") == "tes3")
        assert(tes3:read_entry(1) == "tes3")

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        tes4_builder:add_bytes("textures/foo.dds", "tes4")
        local tes4 = dream_archive.open_bytes(tes4_builder:to_bytes())
        assert(tes4:format() == "bsa-tes4")
        assert(tes4:entries()[1].format == "bsa-tes4")
        assert(tes4:read_file_required("textures/foo.dds") == "tes4")
        assert(tes4:extract_entry(1) == "tes4")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_builds_and_reads_tes4_with_policy_knobs() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local builder = dream_archive.bsa.tes4.Builder.new()
        builder:set_profile(dream_archive.bsa.tes4.profile.skyrim_le)
        builder:set_archive_types(dream_archive.bsa.tes4.archive_types.misc)
        builder:set_name_mode(dream_archive.bsa.tes4.name_mode.strings_and_embedded)
        builder:set_compressed(true)
        builder:add_bytes_with_compression("textures/foo.dds", "tes4 payload", "store")
        local archive = dream_archive.bsa.tes4.open_bytes(builder:to_string())
        assert(archive:len() == 1)
        assert(archive:read_file_required("textures/foo.dds") == "tes4 payload")
        local entry = archive:entries()[1]
        assert(entry.path == "textures\\foo.dds")
        assert(entry.folder == "textures")
        assert(entry.name == "foo.dds")
        assert(archive:read_entry(1) == "tes4 payload")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_preserves_byte_strings_and_optional_absence() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local payload = "a\0\255b"
        local builder = dream_archive.ba2.Builder.new()
        builder:add_bytes("bytes/payload.bin", payload)
        local archive = dream_archive.open_bytes(builder:to_bytes())

        local read = archive:read_file_required("bytes/payload.bin")
        assert(#read == 4)
        assert(string.byte(read, 2) == 0)
        assert(string.byte(read, 3) == 255)
        assert(archive:extract_file_required("bytes/payload.bin") == payload)
        assert(archive:read_file("missing.bin") == nil)
        assert(archive:extract_file("missing.bin") == nil)

        local normalized = dream_archive.normalize_path("A/B\255/C")
        assert(string.byte(normalized, 4) == 255)
        assert(normalized == "a/b\255/c")
        assert(dream_archive.bsa.normalize_path("A\\B\255/C") == "a/b\255/c")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_preserves_raw_archive_path_bytes() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local path = "bytes/\255.bin"
        local builder = dream_archive.ba2.Builder.new()
        builder:add_bytes(path, "payload")
        local archive = dream_archive.ba2.open_bytes(builder:to_bytes())
        assert(archive:contains(path))
        assert(archive:read_file_required(path) == "payload")
        local entry_path = archive:entries()[1].path
        assert(string.byte(entry_path, 7) == 255)
        assert(entry_path == "bytes\\\255.bin")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_preserves_bsa_raw_archive_path_bytes() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local path = "bytes/\255.bin"

        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        tes3_builder:add_bytes(path, "tes3")
        local tes3 = dream_archive.bsa.tes3.open_bytes(tes3_builder:to_bytes())
        assert(tes3:contains(path))
        assert(tes3:read_file_required(path) == "tes3")
        assert(string.byte(tes3:entries()[1].path, 7) == 255)

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        tes4_builder:add_bytes(path, "tes4")
        local tes4 = dream_archive.bsa.tes4.open_bytes(tes4_builder:to_bytes())
        assert(tes4:contains(path))
        assert(tes4:read_file_required(path) == "tes4")
        local entry = tes4:entries()[1]
        assert(string.byte(entry.path, 7) == 255)
        assert(entry.folder == "bytes")
        assert(string.byte(entry.name, 1) == 255)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_reports_option_and_index_errors() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local function fails_with(needle, f)
            local ok, err = pcall(f)
            assert(not ok)
            assert(string.find(tostring(err), needle, 1, true), tostring(err))
        end

        local ba2 = dream_archive.ba2.Builder.new()
        fails_with("unsupported BA2 version", function() ba2:set_version(9) end)
        fails_with("unknown BA2 compression", function() ba2:set_compression("nonsense") end)
        fails_with("zlib compression level", function() ba2:set_zlib_level(10) end)
        fails_with("unknown compression override", function()
            ba2:add_bytes_with_compression("a.txt", "x", "nonsense")
        end)
        fails_with("unknown filename encoding", function()
            dream_archive.bsa.encode_filename("x", "nonsense")
        end)
        fails_with("invalid utf-8", function()
            dream_archive.bsa.encode_filename("bad\255", "windows1250")
        end)

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        fails_with("unsupported TES4 BSA version", function() tes4_builder:set_version(999) end)
        fails_with("unknown TES4 profile", function() tes4_builder:set_profile("arena") end)
        fails_with("unknown TES4 name mode", function() tes4_builder:set_name_mode("mystery") end)
        fails_with("zlib compression level", function() tes4_builder:set_zlib_level(999) end)

        ba2:add_bytes("a.txt", "x")
        local archive = dream_archive.ba2.open_bytes(ba2:to_bytes())
        assert(archive:read_entry(1) == "x")
        fails_with("out of bounds", function() archive:read_entry(0) end)
        fails_with("out of bounds", function() archive:extract_entry(2) end)

        local generic = dream_archive.open_bytes(ba2:to_bytes())
        fails_with("out of bounds", function() generic:read_entry(0) end)

        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        fails_with("invalid utf-8", function()
            tes3_builder:add_encoded_path("bad\255", "windows1250", "x")
        end)
        tes3_builder:add_bytes("a.txt", "x")
        local tes3_archive = dream_archive.bsa.tes3.open_bytes(tes3_builder:to_bytes())
        fails_with("out of bounds", function() tes3_archive:read_entry(0) end)
        fails_with("out of bounds", function() tes3_archive:extract_entry(2) end)

        fails_with("invalid utf-8", function()
            tes4_builder:add_encoded_path("bad\255", "windows1250", "x")
        end)
        tes4_builder:add_bytes("a.txt", "x")
        local tes4_archive = dream_archive.bsa.tes4.open_bytes(tes4_builder:to_bytes())
        fails_with("out of bounds", function() tes4_archive:read_entry(0) end)
        fails_with("out of bounds", function() tes4_archive:extract_entry(2) end)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_unknown_detection_returns_nil() {
    let lua = lua_with_module();
    let root = temp_dir("detect-nil");
    let unknown = root.join("unknown.bin");
    let short = root.join("short.bin");
    std::fs::write(&unknown, b"not an archive").unwrap();
    std::fs::write(&short, b"").unwrap();
    lua.globals()
        .set("unknown_path", unknown.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("short_path", short.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r#"
        assert(dream_archive.guess_format("not an archive") == nil)
        assert(dream_archive.guess_format("") == nil)
        assert(dream_archive.detect_path(unknown_path) == nil)
        assert(dream_archive.detect_path(short_path) == nil)
    "#,
    )
    .exec()
    .unwrap();

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_exposes_exact_hash_shapes() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local ba2_hash = dream_archive.ba2.hash_file("Meshes/Foo.NIF")
        assert(type(ba2_hash.directory) == "number")
        assert(type(ba2_hash.file) == "number")
        assert(ba2_hash.normalized == "meshes\\foo.nif")

        local tes3_hash = dream_archive.bsa.tes3.hash_file("Meshes/Foo.NIF")
        assert(type(tes3_hash.lo) == "number")
        assert(type(tes3_hash.hi) == "number")
        assert(type(tes3_hash.hex) == "string")
        assert(#tes3_hash.hex == 16)
        assert(tes3_hash.normalized == "meshes\\foo.nif")

        local tes4_hash = dream_archive.bsa.tes4.hash_file("Meshes/Foo.NIF")
        assert(type(tes4_hash.crc) == "number")
        assert(type(tes4_hash.hex) == "string")
        assert(tes4_hash.numeric == nil)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_covers_tes4_hash_only_and_dx10_builder_surfaces() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local tes4 = dream_archive.bsa.tes4.Builder.new()
        tes4:set_name_mode(dream_archive.bsa.tes4.name_mode.hash_only)
        tes4:add_bytes("textures/foo.dds", "payload")
        local archive = dream_archive.bsa.tes4.open_bytes(tes4:to_bytes())
        local entry = archive:entries()[1]
        assert(entry.path == nil)
        assert(entry.folder == nil)
        assert(entry.name == nil)
        assert(type(entry.folder_hash.hex) == "string")

        local dx10 = dream_archive.ba2.Dx10Builder.new()
        dx10:add_texture_bytes("textures/one.dds", {
            height = 1,
            width = 1,
            mip_count = 1,
            format = 61,
            flags = 0,
            tile_mode = 0,
        }, "\127")
        local ba2 = dream_archive.ba2.open_bytes(dx10:to_bytes())
        assert(ba2:info().format == "dx10")
        assert(string.sub(ba2:read_entry(1), 1, 4) == "DDS ")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_rejects_non_utf8_filesystem_paths() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local function fails(f)
            local ok = pcall(f)
            assert(not ok)
        end

        fails(function() dream_archive.open_path("bad\255.ba2") end)
        fails(function() dream_archive.detect_path("bad\255.ba2") end)

        local builder = dream_archive.ba2.Builder.new()
        builder:add_bytes("a.txt", "x")
        local archive = dream_archive.ba2.open_bytes(builder:to_bytes())
        fails(function() archive:extract_to("bad\255dir") end)
        fails(function() archive:extract_entry_to_path(1, "bad\255file") end)
        fails(function() builder:write_path("bad\255.ba2") end)
        fails(function() builder:add_file("b.txt", "bad\255source") end)

        local dx10 = dream_archive.ba2.Dx10Builder.new()
        fails(function() dx10:write_path("bad\255.ba2") end)
        fails(function() dx10:add_dds_file("textures/a.dds", "bad\255source") end)

        local tes3 = dream_archive.bsa.tes3.Builder.new()
        fails(function() tes3:add_file("b.txt", "bad\255source") end)
        fails(function() tes3:add_dir("bad\255dir") end)
        fails(function() tes3:write_path("bad\255.bsa") end)

        local tes4 = dream_archive.bsa.tes4.Builder.new()
        fails(function() tes4:add_file("b.txt", "bad\255source") end)
        fails(function() tes4:add_dir("bad\255dir") end)
        fails(function() tes4:write_path("bad\255.bsa") end)
        tes4:add_bytes("a.txt", "x")
        local tes4_archive = dream_archive.bsa.tes4.open_bytes(tes4:to_bytes())
        fails(function() tes4_archive:extract_to("bad\255dir") end)
        fails(function() tes4_archive:extract_entry_to_path(1, "bad\255file") end)
        fails(function() tes4_archive:extract_to_with_encoding("bad\255dir", "utf8") end)
        fails(function() tes4_archive:extract_to_with_paths("bad\255dir", { "a.txt" }) end)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_open_bytes_errors_are_not_detection_nil() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local ba2_builder = dream_archive.ba2.Builder.new()
        ba2_builder:add_bytes("a.txt", "ba2")
        local ba2_bytes = ba2_builder:to_bytes()

        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        tes3_builder:add_bytes("a.txt", "tes3")
        local tes3_bytes = tes3_builder:to_bytes()

        assert(not pcall(function() dream_archive.open_bytes("not an archive") end))
        assert(not pcall(function() dream_archive.ba2.open_bytes(tes3_bytes) end))
        assert(not pcall(function() dream_archive.bsa.tes3.open_bytes(ba2_bytes) end))
        assert(not pcall(function() dream_archive.bsa.tes4.open_bytes(tes3_bytes) end))
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_required_missing_paths_raise_errors_for_all_families() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local function assert_missing_errors(archive)
            assert(archive:read_file("missing.txt") == nil)
            assert(archive:extract_file("missing.txt") == nil)
            assert(not pcall(function() archive:read_file_required("missing.txt") end))
            assert(not pcall(function() archive:extract_file_required("missing.txt") end))
        end

        local ba2_builder = dream_archive.ba2.Builder.new()
        ba2_builder:add_bytes("present.txt", "ba2")
        assert_missing_errors(dream_archive.open_bytes(ba2_builder:to_bytes()))
        assert_missing_errors(dream_archive.ba2.open_bytes(ba2_builder:to_bytes()))

        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        tes3_builder:add_bytes("present.txt", "tes3")
        assert_missing_errors(dream_archive.bsa.tes3.open_bytes(tes3_builder:to_bytes()))

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        tes4_builder:add_bytes("present.txt", "tes4")
        assert_missing_errors(dream_archive.bsa.tes4.open_bytes(tes4_builder:to_bytes()))
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_filesystem_extraction_and_builder_paths_round_trip() {
    let lua = lua_with_module();
    let root = temp_dir("fs-round-trip");
    let source = root.join("source.txt");
    std::fs::write(&source, b"from source file").unwrap();
    let dir = root.join("dir");
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested/from-dir.txt"), b"from directory").unwrap();
    let archive_path = root.join("test.ba2");
    let entry_out = root.join("entry.bin");
    let extract_dir = root.join("out");

    lua.globals()
        .set("source_path", source.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("dir_path", dir.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("archive_path", archive_path.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("entry_out", entry_out.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("extract_dir", extract_dir.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r#"
        local builder = dream_archive.ba2.Builder.new()
        builder:add_file("from-source.txt", source_path)
        builder:add_dir(dir_path)
        builder:write_path(archive_path)

        assert(dream_archive.detect_path(archive_path) == "ba2")
        local archive = dream_archive.ba2.open_path(archive_path)
        assert(archive:read_file_required("from-source.txt") == "from source file")
        assert(archive:read_file_required("nested/from-dir.txt") == "from directory")
        archive:extract_entry_to_path(1, entry_out)
        archive:extract_to(extract_dir)
    "#,
    )
    .exec()
    .unwrap();

    assert_eq!(std::fs::read(entry_out).unwrap(), b"from source file");
    assert_eq!(
        std::fs::read(extract_dir.join("nested/from-dir.txt")).unwrap(),
        b"from directory"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_generic_facade_extracts_entries_and_archives() {
    let lua = lua_with_module();
    let root = temp_dir("generic-extract");
    let entry_out = root.join("entry.bin");
    let tes3_out = root.join("tes3-out");
    let tes4_out = root.join("tes4-out");
    lua.globals()
        .set("entry_out", entry_out.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("tes3_out", tes3_out.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("tes4_out", tes4_out.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r#"
        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        tes3_builder:add_bytes("meshes/foo.nif", "tes3")
        local tes3 = dream_archive.open_bytes(tes3_builder:to_bytes())
        tes3:extract_entry_to_path(1, entry_out)
        tes3:extract_to(tes3_out)

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        tes4_builder:add_bytes("textures/foo.dds", "tes4")
        local tes4 = dream_archive.open_bytes(tes4_builder:to_bytes())
        tes4:extract_to(tes4_out)
    "#,
    )
    .exec()
    .unwrap();

    assert_eq!(std::fs::read(entry_out).unwrap(), b"tes3");
    assert_eq!(
        std::fs::read(tes3_out.join("meshes/foo.nif")).unwrap(),
        b"tes3"
    );
    assert_eq!(
        std::fs::read(tes4_out.join("textures/foo.dds")).unwrap(),
        b"tes4"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_bsa_extracts_entries_and_encoded_paths_to_filesystem() {
    let lua = lua_with_module();
    let root = temp_dir("bsa-extract");
    let entry_out = root.join("entry.bin");
    let tes3_out = root.join("tes3-out");
    let tes4_out = root.join("tes4-out");

    lua.globals()
        .set("entry_out", entry_out.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("tes3_out", tes3_out.to_string_lossy().as_ref())
        .unwrap();
    lua.globals()
        .set("tes4_out", tes4_out.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r#"
        local encoded = "textures/za\191\243\179\230.dds"

        local tes3_builder = dream_archive.bsa.tes3.Builder.new()
        tes3_builder:add_encoded_path("textures/zażółć.dds", "windows1250", "tes3")
        local tes3 = dream_archive.bsa.tes3.open_bytes(tes3_builder:to_bytes())
        tes3:extract_entry_to_path(1, entry_out)
        tes3:extract_to_with_encoding(tes3_out, "windows1250")
        assert(tes3:read_file_required(encoded) == "tes3")

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        tes4_builder:add_encoded_path("textures/zażółć.dds", "windows1250", "tes4")
        local tes4 = dream_archive.bsa.tes4.open_bytes(tes4_builder:to_bytes())
        tes4:extract_to_with_encoding(tes4_out, "windows1250")
    "#,
    )
    .exec()
    .unwrap();

    assert_eq!(std::fs::read(entry_out).unwrap(), b"tes3");
    assert_eq!(
        std::fs::read(tes3_out.join("textures/zażółć.dds")).unwrap(),
        b"tes3"
    );
    assert_eq!(
        std::fs::read(tes4_out.join("textures/zażółć.dds")).unwrap(),
        b"tes4"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_tes4_extract_to_with_paths_uses_sequence_values() {
    let lua = lua_with_module();
    let root = temp_dir("tes4-path-sequence");
    let extract_dir = root.join("out");
    lua.globals()
        .set("extract_dir", extract_dir.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r#"
        local builder = dream_archive.bsa.tes4.Builder.new()
        builder:set_name_mode(dream_archive.bsa.tes4.name_mode.hash_only)
        builder:add_bytes("textures/foo.dds", "payload")
        local archive = dream_archive.bsa.tes4.open_bytes(builder:to_bytes())
        local entry = archive:entries()[1]
        assert(entry.path == nil)
        assert(entry.folder == nil)
        assert(entry.name == nil)
        archive:extract_to_with_paths(extract_dir, {
            "meshes/not-this.nif",
            "textures/foo.dds",
        })

        local bad_out = extract_dir .. "-bad"
        archive:extract_to_with_paths(bad_out, {
            [2] = "textures/foo.dds",
            candidate = "textures/foo.dds",
        })
    "#,
    )
    .exec()
    .unwrap();

    assert_eq!(
        std::fs::read(extract_dir.join("textures/foo.dds")).unwrap(),
        b"payload"
    );
    assert!(!root.join("out-bad/textures/foo.dds").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_exposes_exact_hash_values() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local ba2 = dream_archive.ba2.hash_file("Textures\\CreationClub\\BGSFO4001\\AnimObjects\\PipBoy\\PipBoy02(Black)_d.DDS")
        assert(ba2.directory == 0x23157A84)
        assert(ba2.file == 0x69E1E82C)
        assert(ba2.extension == 0x00736464)
        assert(ba2.normalized == "textures\\creationclub\\bgsfo4001\\animobjects\\pipboy\\pipboy02(black)_d.dds")

        local tes3 = dream_archive.bsa.tes3.hash_file("meshes/c/artifact_bloodring_01.nif")
        assert(tes3.lo == 0x1c3c1149)
        assert(tes3.hi == 0x920d5f0c)
        assert(tes3.hex == "1c3c1149920d5f0c")

        local dir = dream_archive.bsa.tes4.hash_directory("textures/armor/amuletsandrings/elder council")
        assert(dir.hex == "04bc422c742c696c")
        assert(dir.crc == 0x04bc422c)
        assert(dir.first == string.byte("t"))
        assert(dir.last == string.byte("l"))
        assert(dir.last2 == string.byte("i"))
        assert(dir.length == 44)
        assert(dir.numeric == nil)

        local file = dream_archive.bsa.tes4.hash_file("elder_council_amulet_n.dds")
        assert(file.hex == "dc531e2f6516dfee")
        assert(file.crc == 0xdc531e2f)
        assert(file.first == string.byte("e"))
        assert(file.last == 0xee)
        assert(file.last2 == 0xdf)
        assert(file.length == 22)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_accepts_default_compression_options_and_rejects_bad_dx10_headers() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local ba2 = dream_archive.ba2.Builder.new()
        assert(ba2:is_empty())
        ba2:set_zlib_level(0)
        ba2:set_zlib_level(9)
        ba2:set_compression(nil)
        ba2:add_bytes_with_compression("a.txt", "x", nil)
        ba2:add_bytes_with_compression("b.txt", "y", "inherit")
        assert(ba2:len() == 2)

        local tes3 = dream_archive.bsa.tes3.Builder.new()
        assert(tes3:is_empty())

        local tes4 = dream_archive.bsa.tes4.Builder.new()
        assert(tes4:is_empty())
        tes4:set_zlib_level(0)
        tes4:set_zlib_level(9)
        tes4:add_bytes_with_compression("a.txt", "x", nil)
        tes4:add_bytes_with_compression("b.txt", "y", "compress")
        assert(tes4:len() == 2)

        local dx10 = dream_archive.ba2.Dx10Builder.new()
        assert(dx10:is_empty())
        assert(not pcall(function()
            dx10:add_texture_bytes("bad.dds", {
                height = 1,
                width = 1,
                mip_count = 1,
                flags = 0,
                tile_mode = 0,
            }, "\127")
        end))
        assert(not pcall(function()
            dx10:add_texture_bytes("bad2.dds", {
                height = "bad",
                width = 1,
                mip_count = 1,
                format = 61,
                flags = 0,
                tile_mode = 0,
            }, "\127")
        end))
    "#,
    )
    .exec()
    .unwrap();
}
