use mlua::{AnyUserData, Lua, UserDataMethods};
use std::path::PathBuf;

fn lua_with_module() -> Lua {
    let lua = Lua::new();
    let module = dream_archive::lua::create_module(&lua).unwrap();
    lua.globals().set("dreamArchive", module).unwrap();
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
        local builder = dreamArchive.ba2.Builder.new()
        builder:setCompression(dreamArchive.ba2.compression.zip)
        builder:setVersion(dreamArchive.ba2.version.V1)
        builder:addBytes("Meshes/Foo.NIF", "mesh payload")
        local bytes = builder:toBytes()
        assert(dreamArchive.guessFormat(bytes) == "ba2")

        local archive = dreamArchive.openBytes(bytes)
        assert(archive:format() == "ba2")
        assert(archive:len() == 1)
        assert(archive:readFileRequired("meshes/foo.nif") == "mesh payload")
        assert(type(archive.readFileRequired) == "function")
        assert(archive.openFileRequired == nil)
        assert(archive:readEntry(1) == "mesh payload")
        assert(archive:extractEntry(1) == "mesh payload")
        local entries = archive:entries()
        assert(entries[1].path == "meshes\\foo.nif")

        local ba2 = dreamArchive.ba2.openBytes(bytes)
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
        local generic = dreamArchive.openPath(fixture_path)
        assert(generic:entries()[1].path == nil)

        local ba2 = dreamArchive.ba2.openPath(fixture_path)
        local entry = ba2:entries()[1]
        assert(entry.path == nil)
        assert(entry.name == nil)
        assert(ba2:readEntry(1) ~= nil)
    ",
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_top_level_archive_userdata_can_be_borrowed_from_rust() {
    let lua = lua_with_module();
    let fixture = std::env::current_dir()
        .unwrap()
        .join("tests/fixtures/ba2/missing_string_table/in.ba2");
    lua.globals()
        .set("fixture_path", fixture.to_string_lossy().as_ref())
        .unwrap();

    let value: AnyUserData = lua
        .load("return dreamArchive.openPath(fixture_path)")
        .eval()
        .unwrap();
    let archive = value.borrow::<dream_archive::lua::LuaArchive>().unwrap();
    assert_eq!(archive.archive().format(), dream_archive::FileFormat::BA2);
    assert_eq!(archive.path(), Some(fixture.as_path()));

    let value: AnyUserData = lua
        .load(
            r#"
            local builder = dreamArchive.ba2.Builder.new()
            builder:addBytes("meshes/example.nif", "payload")
            return dreamArchive.openBytes(builder:toBytes())
        "#,
        )
        .eval()
        .unwrap();
    let archive = value.borrow::<dream_archive::lua::LuaArchive>().unwrap();
    assert_eq!(archive.archive().len(), 1);
    assert_eq!(archive.path(), None);
}

#[test]
fn lua_top_level_archive_methods_can_be_extended_from_rust() {
    let lua = Lua::new();
    let module = dream_archive::lua::create_module_with_archive_methods(&lua, |methods| {
        methods.add_method("policyLen", |_lua, this, ()| Ok(this.archive().len()));
        methods.add_method("hasOpenPath", |_lua, this, ()| Ok(this.path().is_some()));
    })
    .unwrap();
    lua.globals().set("dreamArchive", module).unwrap();

    let fixture = std::env::current_dir()
        .unwrap()
        .join("tests/fixtures/ba2/missing_string_table/in.ba2");
    lua.globals()
        .set("fixture_path", fixture.to_string_lossy().as_ref())
        .unwrap();

    lua.load(
        r#"
        local archive = dreamArchive.openPath(fixture_path)
        assert(archive:policyLen() == archive:len())
        assert(archive:hasOpenPath())

        local builder = dreamArchive.ba2.Builder.new()
        builder:addBytes("meshes/example.nif", "payload")
        local bytes_archive = dreamArchive.openBytes(builder:toBytes())
        assert(bytes_archive:policyLen() == 1)
        assert(not bytes_archive:hasOpenPath())
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_exports_expected_module_shape() {
    let lua = lua_with_module();

    lua.load(
        r#"
        assert(type(dreamArchive.openBytes) == "function")
        assert(type(dreamArchive.detectPath) == "function")
        assert(type(dreamArchive.ba2.Builder.new) == "function")
        assert(type(dreamArchive.ba2.Dx10Builder.new) == "function")
        assert(dreamArchive.ba2.compression.none == "none")
        assert(dreamArchive.ba2.version.V8 == 8)
        assert(dreamArchive.bsa.encoding.cp437 == "cp437")
        assert(type(dreamArchive.bsa.tes3.Builder.new) == "function")
        assert(type(dreamArchive.bsa.tes4.Builder.new) == "function")
        assert(dreamArchive.bsa.tes4.profile.skyrimSe == "skyrimSe")
        assert(dreamArchive.bsa.tes4.nameMode.hashOnly == "hashOnly")
        assert(type(dreamArchive.bsa.tes4.archiveTypes.TEXTURES) == "number")
        assert(dreamArchive.bsa.tes4.profile.falloutNewVegas == "falloutNewVegas")
        assert(dreamArchive.bsa.tes4.nameMode.stringsAndEmbedded == "stringsAndEmbedded")

        -- 0.2.0 renamed the Lua surface to camelCase; the old names are gone.
        assert(dreamArchive.open_bytes == nil)
        assert(dreamArchive.bsa.tes4.archive_types == nil)
        assert(dreamArchive.bsa.tes4.name_mode == nil)
        assert(dreamArchive.bsa.tes4.profile.skyrim_se == nil)
        assert(dreamArchive.ba2.version.v8 == nil)
        assert(dreamArchive.bsa.tes4.archiveTypes.textures == nil)
        local builder = dreamArchive.ba2.Builder.new()
        assert(builder.add_bytes == nil)
        assert(type(builder.addBytes) == "function")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_rejects_snake_case_enum_values() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local tes4 = dreamArchive.bsa.tes4.Builder.new()
        assert(not pcall(function() tes4:setProfile("skyrim_se") end))
        assert(not pcall(function() tes4:setProfile("fallout_new_vegas") end))
        assert(not pcall(function() tes4:setNameMode("hash_only") end))
        assert(not pcall(function() tes4:setNameMode("strings_and_embedded") end))
        tes4:setProfile("falloutNewVegas")
        tes4:setProfile("skyrimLe")
        tes4:setNameMode("stringsAndEmbedded")
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_module_can_be_required_through_luau_registered_modules() {
    let lua = Lua::new();
    lua.register_module(
        "@dreamArchive",
        dream_archive::lua::create_module(&lua).unwrap(),
    )
    .unwrap();
    lua.register_module(
        "@dreamPath",
        dream_archive::dream_path::lua::create_module(&lua).unwrap(),
    )
    .unwrap();

    lua.load(
        r#"
        local dreamArchive = require("@dreamArchive")
        local dreamPath = require("@dreamPath")
        local builder = dreamArchive.bsa.tes3.Builder.new()
        builder:addBytes(dreamPath.normalize("Meshes\\Foo.NIF"), "payload")
        local archive = dreamArchive.openBytes(builder:toBytes())
        assert(archive:format() == "bsaTes3")
        assert(archive:readFileRequired("meshes/foo.nif") == "payload")
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
        local encoded = dreamArchive.bsa.encodeFilename("textures/zażółć.dds", dreamArchive.bsa.encoding.windows1250)
        assert(encoded == "textures/za\191\243\179\230.dds")
        assert(dreamArchive.bsa.decodeFilenameLossy(encoded, "windows1250") == "textures/zażółć.dds")
        assert(dreamArchive.bsa.encodeFilename("a", "cp1250") == "a")
        assert(dreamArchive.bsa.encodeFilename("a", "cp1251") == "a")
        assert(dreamArchive.bsa.encodeFilename("a", "cp1252") == "a")

        local builder = dreamArchive.bsa.tes3.Builder.new()
        builder:addBytes("Meshes/Foo.NIF", "tes3 payload")
        local archive = dreamArchive.bsa.tes3.openBytes(builder:toString())
        assert(archive:contains("meshes/foo.nif"))
        assert(archive:readFileRequired("meshes/foo.nif") == "tes3 payload")
        assert(archive:entries()[1].path == "meshes\\foo.nif")
        assert(archive:readEntry(1) == "tes3 payload")
        assert(archive:extractFileRequired("meshes/foo.nif") == "tes3 payload")

        local entryHash = archive:entries()[1].hash
        local fileHash = dreamArchive.bsa.tes3.hashFile("Meshes/Foo.NIF")
        assert(entryHash.lo == fileHash.lo, "entry lo matches hashFile lo")
        assert(entryHash.hi == fileHash.hi, "entry hi matches hashFile hi")
        assert(entryHash.hex == fileHash.hex, entryHash.hex .. " ~= " .. fileHash.hex)
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
        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addBytes("meshes/foo.nif", "tes3")
        local tes3 = dreamArchive.openBytes(tes3_builder:toBytes())
        assert(tes3:format() == "bsaTes3")
        assert(tes3:entries()[1].format == "bsaTes3")
        assert(tes3:readFileRequired("meshes/foo.nif") == "tes3")
        assert(tes3:readEntry(1) == "tes3")

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_builder:addBytes("textures/foo.dds", "tes4")
        local tes4 = dreamArchive.openBytes(tes4_builder:toBytes())
        assert(tes4:format() == "bsaTes4")
        assert(tes4:entries()[1].format == "bsaTes4")
        assert(tes4:readFileRequired("textures/foo.dds") == "tes4")
        assert(tes4:extractEntry(1) == "tes4")
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
        local builder = dreamArchive.bsa.tes4.Builder.new()
        builder:setProfile(dreamArchive.bsa.tes4.profile.skyrimLe)
        builder:setArchiveTypes(dreamArchive.bsa.tes4.archiveTypes.MISC)
        builder:setNameMode(dreamArchive.bsa.tes4.nameMode.stringsAndEmbedded)
        builder:setCompressed(true)
        builder:addBytesWithCompression("textures/foo.dds", "tes4 payload", "store")
        local archive = dreamArchive.bsa.tes4.openBytes(builder:toString())
        assert(archive:len() == 1)
        assert(archive:readFileRequired("textures/foo.dds") == "tes4 payload")
        local entry = archive:entries()[1]
        assert(entry.path == "textures\\foo.dds")
        assert(entry.folder == "textures")
        assert(entry.name == "foo.dds")
        assert(archive:readEntry(1) == "tes4 payload")
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
        local builder = dreamArchive.ba2.Builder.new()
        builder:addBytes("bytes/payload.bin", payload)
        local archive = dreamArchive.openBytes(builder:toBytes())

        local read = archive:readFileRequired("bytes/payload.bin")
        assert(#read == 4)
        assert(string.byte(read, 2) == 0)
        assert(string.byte(read, 3) == 255)
        assert(archive:extractFileRequired("bytes/payload.bin") == payload)
        assert(archive:readFile("missing.bin") == nil)
        assert(archive:extractFile("missing.bin") == nil)

        local normalized = dreamArchive.normalizePath("A/B\255/C")
        assert(string.byte(normalized, 4) == 255)
        assert(normalized == "a/b\255/c")
        assert(dreamArchive.bsa.normalizePath("A\\B\255/C") == "a/b\255/c")
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
        local builder = dreamArchive.ba2.Builder.new()
        builder:addBytes(path, "payload")
        local archive = dreamArchive.ba2.openBytes(builder:toBytes())
        assert(archive:contains(path))
        assert(archive:readFileRequired(path) == "payload")
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

        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addBytes(path, "tes3")
        local tes3 = dreamArchive.bsa.tes3.openBytes(tes3_builder:toBytes())
        assert(tes3:contains(path))
        assert(tes3:readFileRequired(path) == "tes3")
        assert(string.byte(tes3:entries()[1].path, 7) == 255)

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_builder:addBytes(path, "tes4")
        local tes4 = dreamArchive.bsa.tes4.openBytes(tes4_builder:toBytes())
        assert(tes4:contains(path))
        assert(tes4:readFileRequired(path) == "tes4")
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

        local ba2 = dreamArchive.ba2.Builder.new()
        fails_with("unsupported BA2 version", function() ba2:setVersion(9) end)
        fails_with("unknown BA2 compression", function() ba2:setCompression("nonsense") end)
        fails_with("zlib compression level", function() ba2:setZlibLevel(10) end)
        fails_with("unknown compression override", function()
            ba2:addBytesWithCompression("a.txt", "x", "nonsense")
        end)
        fails_with("unknown filename encoding", function()
            dreamArchive.bsa.encodeFilename("x", "nonsense")
        end)
        fails_with("invalid utf-8", function()
            dreamArchive.bsa.encodeFilename("bad\255", "windows1250")
        end)

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        fails_with("unsupported TES4 BSA version", function() tes4_builder:setVersion(999) end)
        fails_with("unknown TES4 profile", function() tes4_builder:setProfile("arena") end)
        fails_with("unknown TES4 name mode", function() tes4_builder:setNameMode("mystery") end)
        fails_with("zlib compression level", function() tes4_builder:setZlibLevel(999) end)

        ba2:addBytes("a.txt", "x")
        local archive = dreamArchive.ba2.openBytes(ba2:toBytes())
        assert(archive:readEntry(1) == "x")
        fails_with("out of bounds", function() archive:readEntry(0) end)
        fails_with("out of bounds", function() archive:extractEntry(2) end)

        local generic = dreamArchive.openBytes(ba2:toBytes())
        fails_with("out of bounds", function() generic:readEntry(0) end)

        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        fails_with("invalid utf-8", function()
            tes3_builder:addEncodedPath("bad\255", "windows1250", "x")
        end)
        tes3_builder:addBytes("a.txt", "x")
        local tes3_archive = dreamArchive.bsa.tes3.openBytes(tes3_builder:toBytes())
        fails_with("out of bounds", function() tes3_archive:readEntry(0) end)
        fails_with("out of bounds", function() tes3_archive:extractEntry(2) end)

        fails_with("invalid utf-8", function()
            tes4_builder:addEncodedPath("bad\255", "windows1250", "x")
        end)
        tes4_builder:addBytes("a.txt", "x")
        local tes4_archive = dreamArchive.bsa.tes4.openBytes(tes4_builder:toBytes())
        fails_with("out of bounds", function() tes4_archive:readEntry(0) end)
        fails_with("out of bounds", function() tes4_archive:extractEntry(2) end)
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
        assert(dreamArchive.guessFormat("not an archive") == nil)
        assert(dreamArchive.guessFormat("") == nil)
        assert(dreamArchive.detectPath(unknown_path) == nil)
        assert(dreamArchive.detectPath(short_path) == nil)
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
        local ba2_hash = dreamArchive.ba2.hashFile("Meshes/Foo.NIF")
        assert(type(ba2_hash.directory) == "number")
        assert(type(ba2_hash.file) == "number")
        assert(ba2_hash.normalized == "meshes\\foo.nif")

        local tes3_hash = dreamArchive.bsa.tes3.hashFile("Meshes/Foo.NIF")
        assert(type(tes3_hash.lo) == "number")
        assert(type(tes3_hash.hi) == "number")
        assert(type(tes3_hash.hex) == "string")
        assert(#tes3_hash.hex == 16)
        assert(tes3_hash.normalized == "meshes\\foo.nif")

        local tes4_hash = dreamArchive.bsa.tes4.hashFile("Meshes/Foo.NIF")
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
        local tes4 = dreamArchive.bsa.tes4.Builder.new()
        tes4:setNameMode(dreamArchive.bsa.tes4.nameMode.hashOnly)
        tes4:addBytes("textures/foo.dds", "payload")
        local archive = dreamArchive.bsa.tes4.openBytes(tes4:toBytes())
        local entry = archive:entries()[1]
        assert(entry.path == nil)
        assert(entry.folder == nil)
        assert(entry.name == nil)
        assert(type(entry.folderHash.hex) == "string")

        local dx10 = dreamArchive.ba2.Dx10Builder.new()
        dx10:addTextureBytes("textures/one.dds", {
            height = 1,
            width = 1,
            mipCount = 1,
            format = 61,
            flags = 0,
            tileMode = 0,
        }, "\127")
        local ba2 = dreamArchive.ba2.openBytes(dx10:toBytes())
        assert(ba2:info().format == "dx10")
        assert(string.sub(ba2:readEntry(1), 1, 4) == "DDS ")
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

        fails(function() dreamArchive.openPath("bad\255.ba2") end)
        fails(function() dreamArchive.detectPath("bad\255.ba2") end)

        local builder = dreamArchive.ba2.Builder.new()
        builder:addBytes("a.txt", "x")
        local archive = dreamArchive.ba2.openBytes(builder:toBytes())
        fails(function() archive:extractTo("bad\255dir") end)
        fails(function() archive:extractEntryToPath(1, "bad\255file") end)
        fails(function() builder:writePath("bad\255.ba2") end)
        fails(function() builder:addFile("b.txt", "bad\255source") end)

        local dx10 = dreamArchive.ba2.Dx10Builder.new()
        fails(function() dx10:writePath("bad\255.ba2") end)
        fails(function() dx10:addDdsFile("textures/a.dds", "bad\255source") end)

        local tes3 = dreamArchive.bsa.tes3.Builder.new()
        fails(function() tes3:addFile("b.txt", "bad\255source") end)
        fails(function() tes3:addDir("bad\255dir") end)
        fails(function() tes3:writePath("bad\255.bsa") end)

        local tes4 = dreamArchive.bsa.tes4.Builder.new()
        fails(function() tes4:addFile("b.txt", "bad\255source") end)
        fails(function() tes4:addDir("bad\255dir") end)
        fails(function() tes4:writePath("bad\255.bsa") end)
        tes4:addBytes("a.txt", "x")
        local tes4_archive = dreamArchive.bsa.tes4.openBytes(tes4:toBytes())
        fails(function() tes4_archive:extractTo("bad\255dir") end)
        fails(function() tes4_archive:extractEntryToPath(1, "bad\255file") end)
        fails(function() tes4_archive:extractToWithEncoding("bad\255dir", "utf8") end)
        fails(function() tes4_archive:extractToWithPaths("bad\255dir", { "a.txt" }) end)
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
        local ba2_builder = dreamArchive.ba2.Builder.new()
        ba2_builder:addBytes("a.txt", "ba2")
        local ba2_bytes = ba2_builder:toBytes()

        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addBytes("a.txt", "tes3")
        local tes3_bytes = tes3_builder:toBytes()

        assert(not pcall(function() dreamArchive.openBytes("not an archive") end))
        assert(not pcall(function() dreamArchive.ba2.openBytes(tes3_bytes) end))
        assert(not pcall(function() dreamArchive.bsa.tes3.openBytes(ba2_bytes) end))
        assert(not pcall(function() dreamArchive.bsa.tes4.openBytes(tes3_bytes) end))
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
            assert(archive:readFile("missing.txt") == nil)
            assert(archive:extractFile("missing.txt") == nil)
            assert(not pcall(function() archive:readFileRequired("missing.txt") end))
            assert(not pcall(function() archive:extractFileRequired("missing.txt") end))
        end

        local ba2_builder = dreamArchive.ba2.Builder.new()
        ba2_builder:addBytes("present.txt", "ba2")
        assert_missing_errors(dreamArchive.openBytes(ba2_builder:toBytes()))
        assert_missing_errors(dreamArchive.ba2.openBytes(ba2_builder:toBytes()))

        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addBytes("present.txt", "tes3")
        assert_missing_errors(dreamArchive.bsa.tes3.openBytes(tes3_builder:toBytes()))

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_builder:addBytes("present.txt", "tes4")
        assert_missing_errors(dreamArchive.bsa.tes4.openBytes(tes4_builder:toBytes()))
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
        local builder = dreamArchive.ba2.Builder.new()
        builder:addFile("from-source.txt", source_path)
        builder:addDir(dir_path)
        builder:writePath(archive_path)

        assert(dreamArchive.detectPath(archive_path) == "ba2")
        local archive = dreamArchive.ba2.openPath(archive_path)
        assert(archive:readFileRequired("from-source.txt") == "from source file")
        assert(archive:readFileRequired("nested/from-dir.txt") == "from directory")
        archive:extractEntryToPath(1, entry_out)
        archive:extractTo(extract_dir)
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
        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addBytes("meshes/foo.nif", "tes3")
        local tes3 = dreamArchive.openBytes(tes3_builder:toBytes())
        tes3:extractEntryToPath(1, entry_out)
        tes3:extractTo(tes3_out)

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_builder:addBytes("textures/foo.dds", "tes4")
        local tes4 = dreamArchive.openBytes(tes4_builder:toBytes())
        tes4:extractTo(tes4_out)
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

        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addEncodedPath("textures/zażółć.dds", "windows1250", "tes3")
        local tes3 = dreamArchive.bsa.tes3.openBytes(tes3_builder:toBytes())
        tes3:extractEntryToPath(1, entry_out)
        tes3:extractToWithEncoding(tes3_out, "windows1250")
        assert(tes3:readFileRequired(encoded) == "tes3")

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_builder:addEncodedPath("textures/zażółć.dds", "windows1250", "tes4")
        local tes4 = dreamArchive.bsa.tes4.openBytes(tes4_builder:toBytes())
        tes4:extractToWithEncoding(tes4_out, "windows1250")
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
        local builder = dreamArchive.bsa.tes4.Builder.new()
        builder:setNameMode(dreamArchive.bsa.tes4.nameMode.hashOnly)
        builder:addBytes("textures/foo.dds", "payload")
        local archive = dreamArchive.bsa.tes4.openBytes(builder:toBytes())
        local entry = archive:entries()[1]
        assert(entry.path == nil)
        assert(entry.folder == nil)
        assert(entry.name == nil)
        archive:extractToWithPaths(extract_dir, {
            "meshes/not-this.nif",
            "textures/foo.dds",
        })

        local bad_out = extract_dir .. "-bad"
        archive:extractToWithPaths(bad_out, {
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
        local ba2 = dreamArchive.ba2.hashFile("Textures\\CreationClub\\BGSFO4001\\AnimObjects\\PipBoy\\PipBoy02(Black)_d.DDS")
        assert(ba2.directory == 0x23157A84)
        assert(ba2.file == 0x69E1E82C)
        assert(ba2.extension == 0x00736464)
        assert(ba2.normalized == "textures\\creationclub\\bgsfo4001\\animobjects\\pipboy\\pipboy02(black)_d.dds")

        local tes3 = dreamArchive.bsa.tes3.hashFile("meshes/c/artifact_bloodring_01.nif")
        assert(tes3.lo == 0x1c3c1149)
        assert(tes3.hi == 0x920d5f0c)
        assert(tes3.hex == "1c3c1149920d5f0c")

        local dir = dreamArchive.bsa.tes4.hashDirectory("textures/armor/amuletsandrings/elder council")
        assert(dir.hex == "04bc422c742c696c")
        assert(dir.crc == 0x04bc422c)
        assert(dir.first == string.byte("t"))
        assert(dir.last == string.byte("l"))
        assert(dir.last2 == string.byte("i"))
        assert(dir.length == 44)
        assert(dir.numeric == nil)

        local file = dreamArchive.bsa.tes4.hashFile("elder_council_amulet_n.dds")
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
        local ba2 = dreamArchive.ba2.Builder.new()
        assert(ba2:isEmpty())
        ba2:setZlibLevel(0)
        ba2:setZlibLevel(9)
        ba2:setCompression(nil)
        ba2:addBytesWithCompression("a.txt", "x", nil)
        ba2:addBytesWithCompression("b.txt", "y", "inherit")
        assert(ba2:len() == 2)

        local tes3 = dreamArchive.bsa.tes3.Builder.new()
        assert(tes3:isEmpty())

        local tes4 = dreamArchive.bsa.tes4.Builder.new()
        assert(tes4:isEmpty())
        tes4:setZlibLevel(0)
        tes4:setZlibLevel(9)
        tes4:addBytesWithCompression("a.txt", "x", nil)
        tes4:addBytesWithCompression("b.txt", "y", "compress")
        assert(tes4:len() == 2)

        local dx10 = dreamArchive.ba2.Dx10Builder.new()
        assert(dx10:isEmpty())
        assert(not pcall(function()
            dx10:addTextureBytes("bad.dds", {
                height = 1,
                width = 1,
                mipCount = 1,
                flags = 0,
                tileMode = 0,
            }, "\127")
        end))
        assert(not pcall(function()
            dx10:addTextureBytes("bad2.dds", {
                height = "bad",
                width = 1,
                mipCount = 1,
                format = 61,
                flags = 0,
                tileMode = 0,
            }, "\127")
        end))
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn lua_builders_defer_existing_archive_entries() {
    let lua = lua_with_module();

    lua.load(
        r#"
        local ba2_source_builder = dreamArchive.ba2.Builder.new()
        ba2_source_builder:addBytes("data/source.txt", "ba2 payload")
        local ba2_source = dreamArchive.ba2.openBytes(ba2_source_builder:toBytes())
        local ba2_entry = ba2_source:entries()[1]
        assert(ba2_entry.id == 1)

        local ba2_dest_builder = dreamArchive.ba2.Builder.new()
        ba2_dest_builder:addArchiveEntry("data/copied.txt", ba2_source, ba2_entry.id)
        local ba2_dest = dreamArchive.ba2.openBytes(ba2_dest_builder:toBytes())
        assert(ba2_dest:readFileRequired("data/copied.txt") == "ba2 payload")

        local tes3_source_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_source_builder:addBytes("data/source.txt", "tes3 payload")
        local tes3_source = dreamArchive.bsa.tes3.openBytes(tes3_source_builder:toBytes())
        local tes3_entry = tes3_source:entries()[1]
        assert(tes3_entry.id == 1)

        local tes3_dest_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_dest_builder:addArchiveEntry("data/copied.txt", tes3_source, tes3_entry.id)
        local tes3_dest = dreamArchive.bsa.tes3.openBytes(tes3_dest_builder:toBytes())
        assert(tes3_dest:readFileRequired("data/copied.txt") == "tes3 payload")

        local tes4_source_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_source_builder:setCompressed(true)
        tes4_source_builder:addBytes("data/source.txt", "tes4 payload")
        local tes4_source = dreamArchive.bsa.tes4.openBytes(tes4_source_builder:toBytes())
        local tes4_entry = tes4_source:entries()[1]
        assert(tes4_entry.id == 1)

        local tes4_dest_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_dest_builder:addArchiveEntryWithCompression("data/copied.txt", tes4_source, tes4_entry.id, "store")
        local tes4_dest = dreamArchive.bsa.tes4.openBytes(tes4_dest_builder:toBytes())
        assert(tes4_dest:readFileRequired("data/copied.txt") == "tes4 payload")
    "#,
    )
    .exec()
    .unwrap();
}
