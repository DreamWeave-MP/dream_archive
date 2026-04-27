use mlua::Lua;

#[test]
fn lua_builds_and_reads_ba2_through_top_level_facade() {
    let lua = Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dream_archive", module).unwrap();

    lua.load(
        r#"
        local builder = dream_archive.ba2.Builder.new()
        builder:set_compression("zip")
        builder:add_bytes("Meshes/Foo.NIF", "mesh payload")
        local bytes = builder:to_string()
        assert(dream_archive.guess_format(bytes) == "ba2")

        local archive = dream_archive.open_bytes(bytes)
        assert(archive:format() == "ba2")
        assert(archive:len() == 1)
        assert(archive:read_file_required("meshes/foo.nif") == "mesh payload")
        local entries = archive:entries()
        assert(entries[1].path == "meshes\\foo.nif")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_builds_and_reads_tes3_and_encodes_paths() {
    let lua = Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dream_archive", module).unwrap();

    lua.load(
        r#"
        local encoded = dream_archive.bsa.encode_filename("textures/zażółć.dds", "windows1250")
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
    let lua = Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dream_archive", module).unwrap();

    lua.load(
        r#"
        local builder = dream_archive.bsa.tes4.Builder.new()
        builder:set_profile("skyrim_le")
        builder:set_archive_types(dream_archive.bsa.tes4.archive_types.misc)
        builder:set_name_mode("strings_and_embedded")
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
