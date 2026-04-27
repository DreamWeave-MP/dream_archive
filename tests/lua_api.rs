use mlua::Lua;

fn lua_with_module() -> Lua {
    let lua = Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dream_archive", module).unwrap();
    lua
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
fn lua_builds_and_reads_tes3_and_encodes_paths() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local encoded = dream_archive.bsa.encode_filename("textures/zażółć.dds", dream_archive.bsa.encoding.windows1250)
        assert(encoded == "textures/za\191\243\179\230.dds")
        assert(dream_archive.bsa.decode_filename_lossy(encoded, "windows1250") == "textures/zażółć.dds")

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
        fails_with("unknown compression override", function()
            ba2:add_bytes_with_compression("a.txt", "x", "nonsense")
        end)
        fails_with("unknown filename encoding", function()
            dream_archive.bsa.encode_filename("x", "nonsense")
        end)

        local tes4_builder = dream_archive.bsa.tes4.Builder.new()
        fails_with("unsupported TES4 BSA version", function() tes4_builder:set_version(999) end)
        fails_with("unknown TES4 profile", function() tes4_builder:set_profile("arena") end)
        fails_with("unknown TES4 name mode", function() tes4_builder:set_name_mode("mystery") end)

        ba2:add_bytes("a.txt", "x")
        local archive = dream_archive.ba2.open_bytes(ba2:to_bytes())
        assert(archive:read_entry(1) == "x")
        fails_with("out of bounds", function() archive:read_entry(0) end)
        fails_with("out of bounds", function() archive:extract_entry(2) end)
    "#,
    )
    .exec()
    .unwrap();
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
