//! The `dream.archive` extension: the plan's declared types check with Luau's own frontend, a
//! strict script requiring `@dream/archive` type checks (both under the `luau-analysis`
//! feature), and the behaviour contracts ported from the mlua bindings hold in tagged and
//! untagged runtimes.

#[cfg(feature = "luau-analysis")]
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

#[cfg(feature = "luau-analysis")]
use dream_archive::luau::ENTRIES_KEY;
use dream_archive::luau::{ARCHIVE_KEY, Archive, ArchiveExtension, ENTRY_KEY};
#[cfg(feature = "luau-analysis")]
use l3i::analysis::{Mode, ModuleConfig, SourceCode, SourceProvider};
use l3i::extension::{Extension, ExtensionDescriptor, RuntimePlan, RuntimePolicy};
use l3i::{Runtime, TAG_LIMIT};

fn policy() -> RuntimePolicy {
    RuntimePolicy::new().compat_global("@dream/archive", "dreamArchive")
}

fn plan_with(policy: RuntimePolicy) -> Rc<RuntimePlan> {
    RuntimePlan::builder()
        .policy(policy)
        .extension(ArchiveExtension)
        .finalize()
        .unwrap()
}

fn plan() -> Rc<RuntimePlan> {
    plan_with(policy())
}

/// A tagged runtime and one where the archive type fell back to the untagged path (the
/// bridge's client and the entry handle take the two remaining tags): identical semantics.
fn runtimes() -> Vec<Runtime> {
    let tagged = plan();
    let untagged = plan_with(policy().first_tag(TAG_LIMIT - 2));
    assert!(tagged.tag_of(ARCHIVE_KEY).is_some());
    assert!(untagged.tag_of(ARCHIVE_KEY).is_none());
    assert!(
        untagged.tag_of(ENTRY_KEY).is_some(),
        "direct fields need a tag"
    );
    vec![
        Runtime::from_plan(&tagged).unwrap(),
        Runtime::from_plan(&untagged).unwrap(),
    ]
}

fn run(script: &str) {
    for runtime in runtimes() {
        runtime.exec(script).unwrap();
    }
}

fn run_with(globals: &[(&str, &str)], script: &str) {
    for runtime in runtimes() {
        for (name, value) in globals {
            runtime.set_global(name, *value).unwrap();
        }
        runtime.exec(script).unwrap();
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "dream-archive-luau-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn fixture(path: &str) -> String {
    std::env::current_dir()
        .unwrap()
        .join(path)
        .to_string_lossy()
        .into_owned()
}

const FAILS_WITH: &str = r"
    local function fails_with(needle, f)
        local ok, err = pcall(f)
        assert(not ok, 'expected an error containing ' .. needle)
        assert(string.find(tostring(err), needle, 1, true), tostring(err))
    end
";

#[cfg(feature = "luau-analysis")]
struct Scripts(HashMap<&'static str, &'static str>);

#[cfg(feature = "luau-analysis")]
impl SourceProvider for Scripts {
    fn read_source(&self, name: &str) -> Option<SourceCode> {
        self.0.get(name).map(|text| SourceCode {
            text: (*text).to_owned(),
            is_script: true,
        })
    }
    fn resolve_module(&self, _requirer: &str, _required: &str) -> Option<String> {
        None
    }
    fn module_config(&self, _name: &str) -> ModuleConfig {
        ModuleConfig {
            mode: Mode::Strict,
            ..ModuleConfig::default()
        }
    }
}

#[cfg(feature = "luau-analysis")]
const STRICT_SCRIPT: &str = "--!strict
local dreamArchive = require('@dream/archive')
local ba2 = require('@dream/archive/ba2')
local builder = dreamArchive.ba2.Builder.new()
builder:setCompression(dreamArchive.ba2.compression.zip)
builder:addBytes('meshes/example.nif', buffer.create(4))
local archive = dreamArchive.openBytes(builder:toBytes())
local other = ba2.openPath('x.ba2')
local entries = archive:entries()
-- The view declares its element type: `#`, `[i]`, and `for` type check without `:toTable()`.
local first: dream_archive_Entry? = entries[1]
local count: number = #entries + archive:len()
for i, row in entries do
    local position: number = i + row.index
    count += position
end
local rows: { dream_archive_Entry } = entries:toTable()
count += #rows
local hit: boolean = archive:contains('meshes/example.nif') and archive:containsHash(1, 2, 3)
local found = archive:getByHash(1, 2, 3)
local data: string? = archive:readFile('meshes/example.nif')
local n: number = archive:readInto('meshes/example.nif', buffer.create(64), 0)
local e = archive:entry(1)
if first and e then
    local id: number = first.id + e.index
    local path: string? = first.path
    local size: number? = first.size
    local h: integer? = first.hash
    n += archive:readInto(e, buffer.create(64))
    print(id, path, size, h)
end
local tes3 = dreamArchive.bsa.tes3.hashFile('a')
local hash: integer = tes3.hash
local tes4 = require('@dream/archive/bsa/tes4').hashDirectory('a')
local folder: integer = tes4.hash
print(count, hit, found, data, n, other, hash, folder, dreamArchive.bsa.tes4.archiveTypes.MESHES)
";

#[cfg(feature = "luau-analysis")]
#[test]
fn the_plan_type_checks_and_strict_scripts_pass() {
    let plan = plan();
    plan.check_definitions().unwrap();
    let definitions = plan.type_definitions();
    assert!(
        definitions.contains("declare extern type dream_archive_Archive with"),
        "{definitions}"
    );
    assert!(
        definitions.contains("declare extern type dream_archive_Entry with"),
        "{definitions}"
    );
    for fallback in ["(self, ...any): any", "(...any) -> ...any", ": any,\n"] {
        assert!(
            !definitions.contains(fallback),
            "{fallback:?} in:\n{definitions}"
        );
    }
    let options = l3i::analysis::AnalysisOptions {
        definitions: vec![l3i::analysis::Definitions {
            name: "dream.d.luau".to_owned(),
            source: definitions.clone(),
        }],
        ..Default::default()
    };
    let sources = plan.analysis_sources(Scripts(HashMap::from([("strict", STRICT_SCRIPT)])));
    let analysis = l3i::analysis::Analysis::new(sources, options).unwrap();
    let report = analysis.check("strict", false);
    let text: Vec<String> = report
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "strict:{}:{}: {}",
                d.span.begin_line + 1,
                d.span.begin_column + 1,
                d.text
            )
        })
        .collect();
    assert!(report.is_clean(), "{}\n---\n{definitions}", text.join("\n"));
    // The composition: one archive type, one entry type, one view, four builders.
    let keys: Vec<&str> = plan.userdata().iter().map(|u| u.key.as_str()).collect();
    for key in [
        ARCHIVE_KEY,
        ENTRY_KEY,
        ENTRIES_KEY,
        "dream.archive.Ba2Builder",
    ] {
        assert!(keys.contains(&key), "{keys:?}");
    }
    assert_eq!(plan.installation_order(), ["dream.archive", "dream.net"]);
}

#[test]
fn builds_and_reads_ba2_through_the_top_level_module() {
    run(r#"
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
        assert(archive.openFileRequired == nil)
        assert(archive:readEntry(1) == "mesh payload")
        assert(archive:extractEntry(1) == "mesh payload")
        local entries = archive:entries()
        assert(entries[1].path == "meshes\\foo.nif")
        assert(entries[1].format == "ba2")

        local ba2 = dreamArchive.ba2.openBytes(bytes)
        assert(ba2:info().format == "gnrl")
        assert(ba2:entries()[1].path == "meshes\\foo.nif")
        assert(ba2:entries()[1].name == "foo.nif")
        assert(ba2:entries()[1].folder == "meshes")
        assert(tostring(archive):find("dream.archive.Archive(ba2", 1, true))
    "#);
}

#[test]
fn entry_names_are_the_last_path_component_in_every_family() {
    run(r#"
        local builders = {
            dreamArchive.bsa.tes3.Builder.new(),
            dreamArchive.bsa.tes4.Builder.new(),
            dreamArchive.ba2.Builder.new(),
        }
        for _, builder in builders do
            builder:addBytes("Meshes/X/Foo.NIF", "nested")
            builder:addBytes("readme.txt", "root")
            local archive = dreamArchive.openBytes(builder:toBytes())
            local nested = archive:get("meshes/x/foo.nif")
            local root = archive:get("readme.txt")
            local format = archive:format()
            assert(nested.path == "meshes\\x\\foo.nif", format)
            assert(nested.name == "foo.nif", format .. ": " .. tostring(nested.name))
            assert(nested.folder == "meshes\\x", format .. ": " .. tostring(nested.folder))
            assert(root.name == "readme.txt", format .. ": " .. tostring(root.name))
        end
    "#);
}

#[test]
fn entries_are_a_view_of_handles() {
    run(r#"
        local builder = dreamArchive.bsa.tes3.Builder.new()
        for i = 1, 5 do builder:addBytes("data/file" .. i .. ".txt", string.rep("x", i)) end
        local archive = dreamArchive.openBytes(builder:toBytes())
        local es = archive:entries()
        assert(#es == 5 and es[0] == nil and es[6] == nil and es[1.5] == nil)
        local seen = 0
        for i, e in es do
            assert(e.index == i and e.id == i, "index and id are the 1-based position")
            assert(es[i] == e, "handles of one entry compare equal")
            assert(e.size == #archive:readEntry(i) and e.storedSize == e.size, "size")
            assert(type(e.offset) == "number" and not e.compressed and e.chunks == nil)
            assert(type(e.hash) == "integer" and #e.hashHex == 16, "hash")
            assert(e.folderHash == nil and e.directoryHash == nil, "format-specific fields are nil")
            assert(archive:get(e.path) == e and archive:entry(i) == e)
            assert(archive:getByHash(e.hash) == e and archive:containsHash(e.hash))
            assert(e.folder == "data" and string.sub(e.name, 1, 4) == "file")
            seen += 1
        end
        assert(seen == 5)
        local t = es:toTable()
        assert(#t == 5 and t[3] == es[3])
        assert(archive:entry(0) == nil and archive:entry(6) == nil and archive:get("missing") == nil)
        assert(not archive:containsHash(12345i) and archive:getByHash(12345i) == nil)
        assert(tostring(es[1]):find("dream.archive.Entry(1", 1, true))
    "#);
}

#[test]
fn hashes_are_integers_that_match_the_hash_helpers() {
    run(r#"
        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        tes3_builder:addBytes("Meshes/Foo.NIF", "tes3")
        local tes3 = dreamArchive.openBytes(tes3_builder:toBytes())
        local h = dreamArchive.bsa.tes3.hashFile("meshes\\foo.nif")
        assert(tes3:entries()[1].hash == h.hash and tes3:entries()[1].hashHex == h.hex)
        assert(tes3:containsHash(h.hash) and tes3:getByHash(h.hash).path == "meshes\\foo.nif")
        assert(not pcall(function() return tes3:containsHash(1) end), "a number is not a hash pattern")
        assert(not pcall(function() return tes3:containsHash(h.hash, h.hash) end), "one argument")

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_builder:addBytes("textures/foo.dds", "tes4")
        local tes4 = dreamArchive.openBytes(tes4_builder:toBytes())
        local folder = dreamArchive.bsa.tes4.hashDirectory("textures")
        local file = dreamArchive.bsa.tes4.hashFile("foo.dds")
        local e = tes4:entries()[1]
        assert(e.hash == file.hash and e.folderHash == folder.hash)
        assert(e.hashHex == file.hex and e.folderHashHex == folder.hex)
        assert(tes4:containsHash(folder.hash, file.hash) and tes4:getByHash(folder.hash, file.hash) == e)
        assert(not tes4:containsHash(file.hash, folder.hash))
        assert(not pcall(function() return tes4:containsHash(folder.hash) end), "two arguments")

        local ba2_builder = dreamArchive.ba2.Builder.new()
        ba2_builder:addBytes("Meshes/Foo.NIF", "ba2")
        local ba2 = dreamArchive.openBytes(ba2_builder:toBytes())
        local h2 = dreamArchive.ba2.hashFile("meshes/foo.nif")
        local b = ba2:entries()[1]
        assert(b.hash == nil and b.hashHex == nil)
        assert(b.directoryHash == h2.directory and b.fileHash == h2.file and b.extensionHash == h2.extension)
        assert(ba2:containsHash(h2.directory, h2.file, h2.extension))
        assert(ba2:getByHash(h2.directory, h2.file, h2.extension) == b)
        assert(not ba2:containsHash(h2.directory, h2.file, h2.extension + 1))
        assert(not pcall(function() return ba2:containsHash(h2.directory, h2.file) end), "three arguments")
        assert(not pcall(function() return ba2:containsHash(1.5, 2, 3) end), "exact integers")
    "#);
}

#[test]
fn read_into_decodes_straight_into_a_buffer() {
    run(&format!(
        r#"
        {FAILS_WITH}
        local payload = string.rep("abcdefgh", 8192)
        for _, make in {{ dreamArchive.bsa.tes3.Builder.new, dreamArchive.bsa.tes4.Builder.new, dreamArchive.ba2.Builder.new }} do
            local builder = make()
            if builder.setCompressed then builder:setCompressed(true) end
            if make == dreamArchive.ba2.Builder.new then builder:setCompression("zip") end
            builder:addBytes("big/payload.bin", payload)
            builder:addBytes("small.txt", "tiny")
            local archive = dreamArchive.openBytes(builder:toBytes())
            local buf = buffer.create(#payload + 8)
            assert(archive:readInto("big/payload.bin", buf) == #payload)
            assert(buffer.readstring(buf, 0, #payload) == payload)
            local e = archive:get("small.txt")
            assert(archive:readInto(e, buf, 4) == 4 and buffer.readstring(buf, 4, 4) == "tiny")
            assert(archive:readInto(e, buf, #payload + 4) == 4, "exactly fits")
            fails_with("does not fit", function() archive:readInto(e, buf, #payload + 5) end)
            fails_with("out of bounds", function() archive:readInto(e, buf, #payload + 9) end)
            fails_with("not found", function() archive:readInto("missing", buf) end)
            fails_with("entry handle or an archive path", function() archive:readInto(7, buf) end)
            local other = dreamArchive.openBytes(builder:toBytes())
            fails_with("different archive", function() other:readInto(e, buf) end)
            assert(#archive:readFile("big/payload.bin") == #payload, "the slow path still works")
        end
    "#
    ));
}

#[test]
fn payloads_accept_buffers_and_strings() {
    run(r#"
        local bytes = buffer.fromstring("mesh payload")
        local builder = dreamArchive.ba2.Builder.new()
        builder:addBytes("meshes/foo.nif", bytes)
        builder:addBytesWithCompression("meshes/bar.nif", "text", "store")
        local archive = dreamArchive.openBytes(builder:toBuffer())
        assert(archive:readFileRequired("meshes/foo.nif") == "mesh payload")
        assert(dreamArchive.guessFormat(builder:toBuffer()) == "ba2")
        assert(dreamArchive.guessFormat(buffer.create(2)) == nil)
        local tes3 = dreamArchive.bsa.tes3.Builder.new()
        tes3:addBytes("a.txt", buffer.fromstring("a"))
        assert(dreamArchive.bsa.tes3.openBytes(buffer.fromstring(tes3:toBytes())):readEntry(1) == "a")
    "#);
}

#[test]
fn ba2_stringless_entries_report_nil_paths() {
    run_with(
        &[(
            "fixture_path",
            &fixture("tests/fixtures/ba2/missing_string_table/in.ba2"),
        )],
        r"
        local generic = dreamArchive.openPath(fixture_path)
        assert(generic:entries()[1].path == nil)

        local ba2 = dreamArchive.ba2.openPath(fixture_path)
        local entry = ba2:entries()[1]
        assert(entry.path == nil)
        assert(entry.name == nil)
        assert(entry.folder == nil)
        assert(ba2:readEntry(1) ~= nil)
        assert(ba2:containsHash(entry.directoryHash, entry.fileHash, entry.extensionHash))
        assert(ba2:readInto(entry, buffer.create(entry.size)) == entry.size)
    ",
    );
}

#[test]
fn archive_userdata_can_be_read_from_rust() {
    let runtime = Runtime::from_plan(&plan()).unwrap();
    let path = fixture("tests/fixtures/ba2/missing_string_table/in.ba2");
    runtime.set_global("fixture_path", path.as_str()).unwrap();
    runtime
        .exec(
            r#"
            opened = dreamArchive.openPath(fixture_path)
            local builder = dreamArchive.ba2.Builder.new()
            builder:addBytes("meshes/example.nif", "payload")
            fromBytes = dreamArchive.openBytes(builder:toBytes())
            "#,
        )
        .unwrap();
    let stack = runtime.stack();
    stack
        .with_frame(|frame| {
            let view = l3i::value::Value::get_global(frame, "opened")?.push_to(frame)?;
            let archive = l3i::userdata::check_receiver::<Archive>(view)?;
            assert_eq!(archive.archive().format(), dream_archive::FileFormat::BA2);
            assert_eq!(
                archive
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .as_deref(),
                Some(path.as_str())
            );
            let view = l3i::value::Value::get_global(frame, "fromBytes")?.push_to(frame)?;
            let archive = l3i::userdata::check_receiver::<Archive>(view)?;
            assert_eq!(archive.archive().len(), 1);
            assert_eq!(archive.path(), None);
            let entry = archive.entry(0).unwrap();
            assert_eq!(entry.index(), 0);
            assert_eq!(
                entry.facade().path().map(|p| p.to_vec()),
                Some(b"meshes\\example.nif".to_vec())
            );
            assert!(archive.entry(1).is_none());
            Ok(())
        })
        .unwrap();
}

/// Another extension adds methods to `dream.archive.Archive` without a second wrapper.
#[test]
fn the_archive_type_can_be_augmented() {
    struct Tools;
    impl Extension for Tools {
        fn id(&self) -> &'static str {
            "dream.tests.tools"
        }
        fn describe(&self, d: &mut ExtensionDescriptor) -> l3i::Result<()> {
            d.requires("dream.archive");
            let mut archive = d.augment_userdata::<Archive>(ARCHIVE_KEY);
            archive
                .method("policyLen", |a: &Archive| a.archive().len())
                .signature("(self): number");
            archive
                .method("hasOpenPath", |a: &Archive| a.path().is_some())
                .signature("(self): boolean");
            Ok(())
        }
    }
    let plan = RuntimePlan::builder()
        .policy(policy())
        .extension(Tools)
        .extension(ArchiveExtension)
        .finalize()
        .unwrap();
    #[cfg(feature = "luau-analysis")]
    plan.check_definitions().unwrap();
    let runtime = Runtime::from_plan(&plan).unwrap();
    runtime
        .set_global(
            "fixture_path",
            fixture("tests/fixtures/ba2/missing_string_table/in.ba2").as_str(),
        )
        .unwrap();
    runtime
        .exec(
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
        .unwrap();
}

#[test]
fn exports_the_expected_module_shape() {
    run(r#"
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
        assert(dreamArchive.open_bytes == nil)
        assert(dreamArchive.bsa.tes4.archive_types == nil)
        local builder = dreamArchive.ba2.Builder.new()
        assert(builder.add_bytes == nil)
        assert(type(builder.addBytes) == "function")
        -- The nested tables are the nested modules, and everything is frozen.
        assert(require("@dream/archive/ba2") == dreamArchive.ba2)
        assert(require("@dream/archive/bsa/tes3") == dreamArchive.bsa.tes3)
        assert(not pcall(function() dreamArchive.ba2.version.V9 = 9 end))
        assert(not pcall(function() dreamArchive.ba2.Builder.new = nil end))
        assert(not pcall(function() dreamArchive.ba2 = nil end))
    "#);
}

#[test]
fn rejects_snake_case_enum_values() {
    run(r#"
        local tes4 = dreamArchive.bsa.tes4.Builder.new()
        assert(not pcall(function() tes4:setProfile("skyrim_se") end))
        assert(not pcall(function() tes4:setProfile("fallout_new_vegas") end))
        assert(not pcall(function() tes4:setNameMode("hash_only") end))
        assert(not pcall(function() tes4:setNameMode("strings_and_embedded") end))
        tes4:setProfile("falloutNewVegas")
        tes4:setProfile("skyrimLe")
        tes4:setNameMode("stringsAndEmbedded")
    "#);
}

#[test]
fn the_module_is_required_by_its_canonical_path_without_a_global() {
    let plan = plan_with(RuntimePolicy::new());
    let runtime = Runtime::from_plan(&plan).unwrap();
    runtime
        .exec(
            r#"
            assert(dreamArchive == nil, "no compatibility global unless the host asks")
            local dreamArchive = require("@dream/archive")
            local builder = dreamArchive.bsa.tes3.Builder.new()
            builder:addBytes(dreamArchive.normalizePath("Meshes\\Foo.NIF"), "payload")
            local archive = dreamArchive.openBytes(builder:toBytes())
            assert(archive:format() == "bsaTes3")
            assert(archive:readFileRequired("meshes/foo.nif") == "payload")
            "#,
        )
        .unwrap();
}

#[test]
fn builds_and_reads_tes3_and_encodes_paths() {
    run(r#"
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

        local entry = archive:entries()[1]
        local fileHash = dreamArchive.bsa.tes3.hashFile("Meshes/Foo.NIF")
        assert(entry.hash == fileHash.hash, "entry hash matches hashFile")
        assert(entry.hashHex == fileHash.hex, entry.hashHex .. " ~= " .. fileHash.hex)
    "#);
}

#[test]
fn the_top_level_module_reads_tes3_and_tes4() {
    run(r#"
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
    "#);
}

#[test]
fn builds_and_reads_tes4_with_policy_knobs() {
    run(r#"
        local builder = dreamArchive.bsa.tes4.Builder.new()
        builder:setProfile(dreamArchive.bsa.tes4.profile.skyrimLe)
        builder:setArchiveTypes(dreamArchive.bsa.tes4.archiveTypes.MISC)
        builder:setNameMode(dreamArchive.bsa.tes4.nameMode.stringsAndEmbedded)
        builder:setCompressed(true)
        builder:addBytesWithCompression("textures/foo.dds", "tes4 payload", "store")
        builder:addBytes("textures/bar.dds", "compressed payload")
        local archive = dreamArchive.bsa.tes4.openBytes(builder:toString())
        assert(archive:len() == 2)
        assert(archive:readFileRequired("textures/foo.dds") == "tes4 payload")
        local entry = archive:get("textures/foo.dds")
        assert(entry.path == "textures\\foo.dds")
        assert(entry.folder == "textures")
        assert(entry.name == "foo.dds")
        assert(not entry.compressed and entry.size == #"tes4 payload")
        local other = archive:get("textures/bar.dds")
        assert(other.compressed and other.size == #"compressed payload" and other.storedSize ~= other.size)
        assert(archive:readEntry(entry.index) == "tes4 payload")
    "#);
}

#[test]
fn preserves_byte_strings_and_optional_absence() {
    run(r#"
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
    "#);
}

#[test]
fn preserves_raw_archive_path_bytes() {
    run(r#"
        local path = "bytes/\255.bin"
        local builder = dreamArchive.ba2.Builder.new()
        builder:addBytes(path, "payload")
        local archive = dreamArchive.ba2.openBytes(builder:toBytes())
        assert(archive:contains(path))
        assert(archive:readFileRequired(path) == "payload")
        local entry_path = archive:entries()[1].path
        assert(string.byte(entry_path, 7) == 255)
        assert(entry_path == "bytes\\\255.bin")

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
    "#);
}

#[test]
fn reports_option_and_index_errors() {
    run(&format!(
        r#"
        {FAILS_WITH}
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
        fails_with("not valid UTF-8", function()
            dreamArchive.bsa.encodeFilename("bad\255", "windows1250")
        end)

        local tes4_builder = dreamArchive.bsa.tes4.Builder.new()
        fails_with("unsupported TES4 BSA version", function() tes4_builder:setVersion(999) end)
        fails_with("unknown TES4 profile", function() tes4_builder:setProfile("arena") end)
        fails_with("unknown TES4 name mode", function() tes4_builder:setNameMode("mystery") end)
        fails_with("zlib compression level", function() tes4_builder:setZlibLevel(999) end)
        fails_with("not an exact", function() tes4_builder:setZlibLevel(1.5) end)

        ba2:addBytes("a.txt", "x")
        local archive = dreamArchive.ba2.openBytes(ba2:toBytes())
        assert(archive:readEntry(1) == "x")
        fails_with("out of bounds", function() archive:readEntry(0) end)
        fails_with("out of bounds", function() archive:extractEntry(2) end)
        fails_with("out of bounds", function() archive:readEntry(-1) end)
        fails_with("not an exact", function() archive:readEntry(1.5) end)

        local generic = dreamArchive.openBytes(ba2:toBytes())
        fails_with("out of bounds", function() generic:readEntry(0) end)
        fails_with("needs a BSA archive", function() generic:extractToWithEncoding("x", "utf8") end)
        fails_with("needs a TES4 BSA archive", function() generic:extractToWithPaths("x", {{}}) end)

        local tes3_builder = dreamArchive.bsa.tes3.Builder.new()
        fails_with("not valid UTF-8", function()
            tes3_builder:addEncodedPath("bad\255", "windows1250", "x")
        end)
        tes3_builder:addBytes("a.txt", "x")
        local tes3_archive = dreamArchive.bsa.tes3.openBytes(tes3_builder:toBytes())
        fails_with("out of bounds", function() tes3_archive:readEntry(0) end)
        fails_with("out of bounds", function() tes3_archive:extractEntry(2) end)
        fails_with("needs a BA2 archive", function() tes3_archive:info() end)

        fails_with("not valid UTF-8", function()
            tes4_builder:addEncodedPath("bad\255", "windows1250", "x")
        end)
        tes4_builder:addBytes("a.txt", "x")
        local tes4_archive = dreamArchive.bsa.tes4.openBytes(tes4_builder:toBytes())
        fails_with("out of bounds", function() tes4_archive:readEntry(0) end)
        fails_with("out of bounds", function() tes4_archive:extractEntry(2) end)
        fails_with("needs a BA2 archive", function() ba2:addArchiveEntry("b.txt", tes4_archive, 1) end)
    "#
    ));
}

#[test]
fn unknown_detection_returns_nil() {
    let root = temp_dir("detect-nil");
    let unknown = root.join("unknown.bin");
    let short = root.join("short.bin");
    std::fs::write(&unknown, b"not an archive").unwrap();
    std::fs::write(&short, b"").unwrap();
    run_with(
        &[
            ("unknown_path", &unknown.to_string_lossy()),
            ("short_path", &short.to_string_lossy()),
        ],
        r#"
        assert(dreamArchive.guessFormat("not an archive") == nil)
        assert(dreamArchive.guessFormat("") == nil)
        assert(dreamArchive.detectPath(unknown_path) == nil)
        assert(dreamArchive.detectPath(short_path) == nil)
        assert(not pcall(dreamArchive.detectPath, unknown_path .. ".missing"), "a missing file is an error")
    "#,
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exposes_exact_hash_shapes_and_values() {
    run(r#"
        local ba2_hash = dreamArchive.ba2.hashFile("Meshes/Foo.NIF")
        assert(type(ba2_hash.directory) == "number")
        assert(type(ba2_hash.file) == "number")
        assert(ba2_hash.normalized == "meshes\\foo.nif")

        local tes3_hash = dreamArchive.bsa.tes3.hashFile("Meshes/Foo.NIF")
        assert(type(tes3_hash.lo) == "number")
        assert(type(tes3_hash.hi) == "number")
        assert(type(tes3_hash.hex) == "string" and #tes3_hash.hex == 16)
        assert(type(tes3_hash.hash) == "integer")
        assert(tes3_hash.normalized == "meshes\\foo.nif")

        local tes4_hash = dreamArchive.bsa.tes4.hashFile("Meshes/Foo.NIF")
        assert(type(tes4_hash.crc) == "number")
        assert(type(tes4_hash.hex) == "string")
        assert(type(tes4_hash.hash) == "integer")
        assert(tes4_hash.numeric == nil)

        local ba2 = dreamArchive.ba2.hashFile("Textures\\CreationClub\\BGSFO4001\\AnimObjects\\PipBoy\\PipBoy02(Black)_d.DDS")
        assert(ba2.directory == 0x23157A84)
        assert(ba2.file == 0x69E1E82C)
        assert(ba2.extension == 0x00736464)
        assert(ba2.normalized == "textures\\creationclub\\bgsfo4001\\animobjects\\pipboy\\pipboy02(black)_d.dds")

        local tes3 = dreamArchive.bsa.tes3.hashFile("meshes/c/artifact_bloodring_01.nif")
        assert(tes3.lo == 0x1c3c1149)
        assert(tes3.hi == 0x920d5f0c)
        assert(tes3.hex == "1c3c1149920d5f0c")
        assert(tes3.hash == 0x1c3c1149920d5f0ci)

        local dir = dreamArchive.bsa.tes4.hashDirectory("textures/armor/amuletsandrings/elder council")
        assert(dir.hex == "04bc422c742c696c")
        assert(dir.hash == 0x04bc422c742c696ci)
        assert(dir.crc == 0x04bc422c)
        assert(dir.first == string.byte("t"))
        assert(dir.last == string.byte("l"))
        assert(dir.last2 == string.byte("i"))
        assert(dir.length == 44)

        local file = dreamArchive.bsa.tes4.hashFile("elder_council_amulet_n.dds")
        assert(file.hex == "dc531e2f6516dfee")
        assert(file.hash == 0xdc531e2f6516dfeei, "all sixty-four bits, the top one set")
        assert(file.crc == 0xdc531e2f)
        assert(file.first == string.byte("e"))
        assert(file.last == 0xee)
        assert(file.last2 == 0xdf)
        assert(file.length == 22)
    "#);
}

#[test]
fn covers_tes4_hash_only_and_dx10_builder_surfaces() {
    run(r#"
        local tes4 = dreamArchive.bsa.tes4.Builder.new()
        tes4:setNameMode(dreamArchive.bsa.tes4.nameMode.hashOnly)
        tes4:addBytes("textures/foo.dds", "payload")
        local archive = dreamArchive.bsa.tes4.openBytes(tes4:toBytes())
        local entry = archive:entries()[1]
        assert(entry.path == nil)
        assert(entry.folder == nil)
        assert(entry.name == nil)
        assert(type(entry.folderHashHex) == "string" and type(entry.folderHash) == "integer")
        assert(archive:contains("textures/foo.dds"), "hash-only archives still resolve paths")
        assert(archive:readInto(entry, buffer.create(7)) == 7)

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
        local e = ba2:entries()[1]
        assert(e.size == #ba2:readEntry(1) and e.chunks == 1)
    "#);
}

#[test]
fn rejects_non_utf8_filesystem_paths() {
    run(r#"
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
    "#);
}

#[test]
fn open_bytes_errors_are_not_detection_nil() {
    run(r#"
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
    "#);
}

#[test]
fn required_missing_paths_raise_errors_for_all_families() {
    run(r#"
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
    "#);
}

#[test]
fn filesystem_extraction_and_builder_paths_round_trip() {
    let root = temp_dir("fs-round-trip");
    let source = root.join("source.txt");
    std::fs::write(&source, b"from source file").unwrap();
    let dir = root.join("dir");
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested/from-dir.txt"), b"from directory").unwrap();
    let archive_path = root.join("test.ba2");
    let entry_out = root.join("entry.bin");
    let extract_dir = root.join("out");

    let runtime = Runtime::from_plan(&plan()).unwrap();
    for (name, value) in [
        ("source_path", &source),
        ("dir_path", &dir),
        ("archive_path", &archive_path),
        ("entry_out", &entry_out),
        ("extract_dir", &extract_dir),
    ] {
        runtime
            .set_global(name, value.to_string_lossy().as_ref())
            .unwrap();
    }
    runtime
        .exec(
            r#"
        local builder = dreamArchive.ba2.Builder.new()
        builder:addFile("from-source.txt", source_path)
        builder:addDir(dir_path)
        builder:writePath(archive_path)

        assert(dreamArchive.detectPath(archive_path) == "ba2")
        local archive = dreamArchive.ba2.openPath(archive_path)
        assert(archive:readFileRequired("from-source.txt") == "from source file")
        assert(archive:readFileRequired("nested/from-dir.txt") == "from directory")
        assert(archive:extractEntryToPath(1, entry_out) == #"from source file")
        assert(archive:extractTo(extract_dir) > 0)
        assert(archive:archiveSize() > 0)
    "#,
        )
        .unwrap();

    assert_eq!(std::fs::read(entry_out).unwrap(), b"from source file");
    assert_eq!(
        std::fs::read(extract_dir.join("nested/from-dir.txt")).unwrap(),
        b"from directory"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bsa_extracts_entries_and_encoded_paths_to_the_filesystem() {
    let root = temp_dir("bsa-extract");
    let entry_out = root.join("entry.bin");
    let tes3_out = root.join("tes3-out");
    let tes4_out = root.join("tes4-out");
    let generic_out = root.join("generic-out");
    let runtime = Runtime::from_plan(&plan()).unwrap();
    for (name, value) in [
        ("entry_out", &entry_out),
        ("tes3_out", &tes3_out),
        ("tes4_out", &tes4_out),
        ("generic_out", &generic_out),
    ] {
        runtime
            .set_global(name, value.to_string_lossy().as_ref())
            .unwrap();
    }
    runtime
        .exec(
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

        local generic_builder = dreamArchive.bsa.tes4.Builder.new()
        generic_builder:addBytes("textures/foo.dds", "tes4")
        local generic = dreamArchive.openBytes(generic_builder:toBytes())
        generic:extractTo(generic_out)
    "#,
        )
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
    assert_eq!(
        std::fs::read(generic_out.join("textures/foo.dds")).unwrap(),
        b"tes4"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn tes4_extract_to_with_paths_uses_sequence_values() {
    let root = temp_dir("tes4-path-sequence");
    let extract_dir = root.join("out");
    let runtime = Runtime::from_plan(&plan()).unwrap();
    runtime
        .set_global("extract_dir", extract_dir.to_string_lossy().as_ref())
        .unwrap();
    runtime
        .exec(
            r#"
        local builder = dreamArchive.bsa.tes4.Builder.new()
        builder:setNameMode(dreamArchive.bsa.tes4.nameMode.hashOnly)
        builder:addBytes("textures/foo.dds", "payload")
        local archive = dreamArchive.bsa.tes4.openBytes(builder:toBytes())
        local entry = archive:entries()[1]
        assert(entry.path == nil)
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
        .unwrap();

    assert_eq!(
        std::fs::read(extract_dir.join("textures/foo.dds")).unwrap(),
        b"payload"
    );
    assert!(!root.join("out-bad/textures/foo.dds").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn accepts_default_compression_options_and_rejects_bad_dx10_headers() {
    run(&format!(
        r#"
        {FAILS_WITH}
        local ba2 = dreamArchive.ba2.Builder.new()
        assert(ba2:isEmpty())
        ba2:setZlibLevel(0)
        ba2:setZlibLevel(9)
        ba2:setCompression(nil)
        ba2:setCompression()
        ba2:addBytesWithCompression("a.txt", "x", nil)
        ba2:addBytesWithCompression("b.txt", "y", "inherit")
        ba2:addBytesWithCompression("c.txt", "z")
        assert(ba2:len() == 3)

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
        fails_with("missing required option 'format'", function()
            dx10:addTextureBytes("bad.dds", {{
                height = 1,
                width = 1,
                mipCount = 1,
                flags = 0,
                tileMode = 0,
            }}, "\127")
        end)
        fails_with("height", function()
            dx10:addTextureBytes("bad2.dds", {{
                height = "bad",
                width = 1,
                mipCount = 1,
                format = 61,
                flags = 0,
                tileMode = 0,
            }}, "\127")
        end)
        fails_with("unknown option 'depth'", function()
            dx10:addTextureBytes("bad3.dds", {{
                height = 1, width = 1, mipCount = 1, format = 61, flags = 0, tileMode = 0, depth = 1,
            }}, "\127")
        end)
    "#
    ));
}

#[test]
fn builders_defer_existing_archive_entries() {
    run(r#"
        local ba2_source_builder = dreamArchive.ba2.Builder.new()
        ba2_source_builder:addBytes("data/source.txt", "ba2 payload")
        local ba2_source = dreamArchive.ba2.openBytes(ba2_source_builder:toBytes())
        local ba2_entry = ba2_source:entries()[1]
        assert(ba2_entry.id == 1)

        local ba2_dest_builder = dreamArchive.ba2.Builder.new()
        ba2_dest_builder:addArchiveEntry("data/copied.txt", ba2_source, ba2_entry.id)
        ba2_dest_builder:addArchiveEntry("data/again.txt", ba2_source, ba2_entry.id)
        local ba2_dest = dreamArchive.ba2.openBytes(ba2_dest_builder:toBytes())
        assert(ba2_dest:readFileRequired("data/copied.txt") == "ba2 payload")
        assert(ba2_dest:readFileRequired("data/again.txt") == "ba2 payload")

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
        local tes4_source = dreamArchive.openBytes(tes4_source_builder:toBytes())
        local tes4_entry = tes4_source:entries()[1]
        assert(tes4_entry.id == 1)

        local tes4_dest_builder = dreamArchive.bsa.tes4.Builder.new()
        tes4_dest_builder:addArchiveEntryWithCompression("data/copied.txt", tes4_source, tes4_entry.id, "store")
        local tes4_dest = dreamArchive.bsa.tes4.openBytes(tes4_dest_builder:toBytes())
        assert(tes4_dest:readFileRequired("data/copied.txt") == "tes4 payload")
        assert(not pcall(function() tes4_dest_builder:addArchiveEntry("x", tes4_source, 2) end), "id out of range")
    "#);
}
