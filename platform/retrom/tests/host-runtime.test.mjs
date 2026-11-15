import assert from "node:assert/strict";
import {after, before, test} from "node:test";
import {chromium} from "playwright-core";
import {buildInputs, execute, Script} from "./host-fixture.mjs";
import {extendedGraphicsExecutable} from "./native-profile-fixture.mjs";
import {unicodeGlyphExecutable} from "./glyph-profile-fixture.mjs";

let browser, inputs;
before(async () => {
  inputs = await buildInputs();
  browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROME_BIN ? {executablePath: process.env.CHROME_BIN} : {}),
    args: ["--enable-unsafe-swiftshader"],
  });
});
after(async () => {await browser?.close();});

async function resultFor(script, resources = [], colors = []) {
  const page = await browser.newPage();
  try {
    const result = await execute(page, inputs, script, resources, colors);
    assert.match(result.diagnostics, /steps=[1-9]/, "fixture must actually execute the VM");
    return result;
  } finally {await page.close();}
}

async function run(script, resources = []) {
  const result = await resultFor(script, resources);
  assert.equal(result.error, null, result.error ?? result.diagnostics);
  return result;
}

function guestBytes(script, address, bytes) {
  const padded = Buffer.alloc((bytes.length + 3) & ~3); bytes.copy(padded);
  for (let offset = 0; offset < padded.length; offset += 4) script.store(address + offset, padded.readUInt32LE(offset));
  return script;
}

function sentenceTable(groups) {
  const word = value => {const result = Buffer.alloc(4); result.writeUInt32LE(value); return result;};
  const text = value => Buffer.from(value + "\0");
  return Buffer.concat([word(groups.length), ...groups.flatMap(([name, entries]) =>
    [text(name), word(entries.length), ...entries.flatMap(([key, value]) => [text(key), text(value)])])]);
}

test("a missing optional installation query returns zero and leaves its output buffer intact", async () => {
  const script = new Script().store(4096, 0x12345678)
    .integer(4096).string("FixtureVendor").string("OptionalComponent").emit(0x80, 0xf8)
    .integer(4100).emit(0x0a, 2)
    .write("bgi-user/query.bin", 4096, 8);
  const {writes} = await run(script);
  assert.deepEqual(writes.find(row => row.path === "query.bin")?.bytes, [0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0]);
});

test("global configuration without a script-selected user directory stays in the private user tree", async () => {
  const {writes} = await run(new Script().emit(0x80, 0x81), [["BGI.gdb", Buffer.from("original resources remain unchanged").toString("base64")]]);
  assert.ok(writes.some(row => row.path.toLowerCase() === "bgi.gdb" && row.length > 64));
  assert.ok(writes.every(row => !row.path.includes("bgi-resource") && !row.path.startsWith("/")));
});

test("registry mutations remain denied and retain their original error through VM cleanup", async () => {
  const script = new Script().string("FixtureVendor").string("OptionalComponent").emit(0x80, 0xf9);
  const result = await resultFor(script);
  assert.match(result.error, /unsupported or denied host capability: filesystem\/platform\/installer/);
  assert.doesNotMatch(result.error, /save path denied/);
  assert.ok(result.writes.every(row => !row.path.includes("system.arc")));
});

test("reapplying the default native display mode does not change the initial canvas dimensions", async () => {
  const result = await run(new Script().integer(2).integer(0).integer(0).emit(0x80, 0x60));
  assert.deepEqual(result.size, [800, 600]);
  assert.deepEqual(result.initialSize, result.size);
});

test("an explicit widescreen display mode changes the native dimensions", async () => {
  const result = await run(new Script().integer(6).integer(0).integer(0).emit(0x80, 0x60));
  assert.deepEqual(result.size, [1280, 720]);
});

for (const mode of [0, 1, -1]) {
  test(`extended forced presentation consumes its integer without pushing a result (${mode})`, async () => {
    const script = new Script().integer(0x12345678).integer(mode).emit(0x91, 0)
      .integer(4096).emit(0x0a, 2).write("bgi-user/present.bin", 4096, 4);
    const {writes} = await run(script, [extendedGraphicsExecutable()]);
    assert.deepEqual(writes.find(row => row.path === "present.bin")?.bytes, [0x78, 0x56, 0x34, 0x12]);
  });
}

test("the older graphics ABI keeps its null force-present slot unsupported", async () => {
  const result = await resultFor(new Script().integer(1).emit(0x91, 0));
  assert.match(result.error, /UnknownDispatch/);
});

test("executable structure selects the extended mode and raster-option contracts", async () => {
  const script = new Script().integer(1).emit(0x91, 9)
    .integer(4096).emit(0x0a, 2)
    .integer(0x01220000).integer(1).emit(0x91, 10)
    .integer(4100).emit(0x0a, 2).write("bgi-user/extended.bin", 4096, 8);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  assert.deepEqual(writes.find(row => row.path === "extended.bin")?.bytes, [1, 0, 0, 0, 1, 0, 0, 0]);
});

test("the extended ABI encoding mode validates values and preserves UTF-8 after rejection", async () => {
  const script = new Script();
  for (const [index, mode] of [0, 1, 2, -1].entries()) {
    script.integer(mode).emit(0x81, 0).integer(4096 + index * 4).emit(0x0a, 2);
  }
  script.write("bgi-user/encoding.bin", 4096, 16);
  // BP literals are CP932; native UTF-8 strings live in raw guest buffers.
  const path = Buffer.concat([Buffer.from("bgi-user/日本.bin\0"), Buffer.alloc(4)]);
  for (let offset = 0; offset + 4 <= path.length; offset += 4) {
    script.store(8192 + offset, path.readUInt32LE(offset));
  }
  script.integer(8192).integer(4096).integer(4).emit(0x80, 0x32);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  assert.deepEqual(writes.find(row => row.path === "encoding.bin")?.bytes,
    [1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(writes.find(row => row.path === "日本.bin")?.bytes, [1, 0, 0, 0]);
});

test("extended CP932 conversion writes UTF-8 bytes without pushing a result", async () => {
  const script = new Script().integer(0x12345678).integer(9000)
    .string(Buffer.from([0x93, 0xfa, 0x96, 0x7b])).emit(0x81, 0x21)
    .integer(4096).emit(0x0a, 2)
    .write("bgi-user/converted.bin", 9000, 7)
    .write("bgi-user/conversion-stack.bin", 4096, 4);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  assert.deepEqual(writes.find(row => row.path === "converted.bin")?.bytes,
    [...Buffer.from("日本\0")]);
  assert.deepEqual(writes.find(row => row.path === "conversion-stack.bin")?.bytes, [0x78, 0x56, 0x34, 0x12]);
});

for (const selector of [1, 2, 3]) {
  test(`the extended ABI keeps unregistered language slot ${selector} unsupported`, async () => {
    const result = await resultFor(new Script().integer(1).emit(0x81, selector), [extendedGraphicsExecutable()]);
    assert.match(result.error, /UnknownDispatch/);
  });
}

test("the native UTF-8 probe keeps ASCII and legacy sequences distinct", async () => {
  const cases = [[Buffer.from("ascii\0"), 0x80000000], [Buffer.from("あ\0"), 1],
    [Buffer.from([0x80, 0]), 0], [Buffer.from([0xf8, 0x80, 0x80, 0x80, 0x80, 0]), 1],
    [Buffer.from([0xc2, 0xa2, 0xc2, 0xa2, 0xc2, 0xa2, 0xff, 0]), 1], [Buffer.from([0]), 0x80000000]];
  const script = new Script();
  cases.forEach(([bytes], index) => {
    guestBytes(script, 16384 + index * 64, bytes).integer(16384 + index * 64).emit(0x81, 0x27)
      .integer(4096).emit(0x0a, 2).write(`bgi-user/probe-${index}.bin`, 4096, 4);
  });
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  cases.forEach(([, expected], index) => {
    const bytes = Buffer.alloc(4); bytes.writeUInt32LE(expected);
    assert.deepEqual(writes.find(row => row.path === `probe-${index}.bin`)?.bytes, [...bytes]);
  });
});

test("sentence imports replace groups, preserve others, and roll back malformed input", async () => {
  const first = sentenceTable([["日本", [["key", "保持"]]], ["selected", [["gone", "old"]]]]);
  const second = sentenceTable([["selected", [["name", "replaced"], ["name", "再登録"]]]]);
  const script = new Script();
  const status = name => script.integer(4096).emit(0x0a, 2).write(`bgi-user/${name}.bin`, 4096, 4);
  guestBytes(script, 16384, first).integer(16384).integer(first.length).emit(0x81, 0xd8); status("first");
  guestBytes(script, 16384, second).integer(16384).integer(second.length).emit(0x81, 0xd8); status("second");
  // Query raw UTF-8 group bytes while the current mode is still CP932.
  guestBytes(script, 20000, Buffer.from("日本\0"));
  script.integer(0).integer(20000).string("key").emit(0x81, 0xda); status("kept");
  script.integer(9000).string("selected").string("name").emit(0x81, 0xda); status("found");
  script.integer(9000).string("selected").string("gone").emit(0x81, 0xda); status("missing");
  script.integer(16384).integer(second.length - 1).emit(0x81, 0xd8); status("malformed");
  script.integer(0).string("selected").string("name").emit(0x81, 0xda); status("after-malformed");
  guestBytes(script, 16384, Buffer.alloc(4)).integer(16384).integer(4).emit(0x81, 0xd8); status("empty");
  script.integer(0).string("selected").string("name").emit(0x81, 0xda); status("after-empty");
  script.integer(0).integer(-1).emit(0x81, 0xd8); status("clear");
  script.integer(0).string("selected").string("name").emit(0x81, 0xda); status("after-clear");
  script.write("bgi-user/translation.bin", 9000, 10);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  for (const [name, value] of Object.entries({first: 1, second: 1, kept: 6, found: 9, missing: 0,
    malformed: 0, "after-malformed": 9, empty: 1, "after-empty": 9, clear: 1, "after-clear": 0})) {
    assert.deepEqual(writes.find(row => row.path === name + ".bin")?.bytes, [value, 0, 0, 0]);
  }
  assert.deepEqual(writes.find(row => row.path === "translation.bin")?.bytes, [...Buffer.from("再登録\0")]);
});

test("extended font enumeration consumes charset and destination and writes current-encoding names", async () => {
  const script = new Script().integer(1).emit(0x81, 0).integer(4092).emit(0x0a, 2);
  for (const charset of [0, 128, 134, 136]) {
    script.integer(0).integer(charset).emit(0xb0, 0xc5).integer(4096).emit(0x0a, 2)
      .integer(9000).integer(charset).emit(0xb0, 0xc5).integer(4100).emit(0x0a, 2)
      .write(`bgi-user/fonts-${charset}.bin`, 4096, 8);
  }
  script.write("bgi-user/font-name.bin", 9000, 16)
    .integer(0).emit(0xb0, 0xc4).integer(4096).emit(0x0a, 2).write("bgi-user/font-default.bin", 4096, 4)
    .store(9000, 0x12345678).integer(9000).integer(-1).emit(0xb0, 0xc5)
    .integer(4096).emit(0x0a, 2).write("bgi-user/font-empty.bin", 4096, 4)
    .write("bgi-user/font-untouched.bin", 9000, 4);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  for (const charset of [0, 128, 134, 136]) {
    assert.deepEqual(writes.find(row => row.path === `fonts-${charset}.bin`)?.bytes, [34, 0, 0, 0, 2, 0, 0, 0]);
  }
  assert.deepEqual(writes.find(row => row.path === "font-name.bin")?.bytes, [...Buffer.from("ＭＳ ゴシック\0").subarray(0, 16)]);
  assert.deepEqual(writes.find(row => row.path === "font-default.bin")?.bytes, [34, 0, 0, 0]);
  assert.deepEqual(writes.find(row => row.path === "font-empty.bin")?.bytes, [0, 0, 0, 0]);
  assert.deepEqual(writes.find(row => row.path === "font-untouched.bin")?.bytes, [0x78, 0x56, 0x34, 0x12]);
});

test("the older user ABI rejects its null charset-enumeration slot", async () => {
  const result = await resultFor(new Script().integer(0).integer(128).emit(0xb0, 0xc5));
  assert.match(result.error, /UnknownDispatch/);
});

test("extended font pitch lookup keeps its output pointer raw and detects the input encoding", async () => {
  const script = guestBytes(new Script(), 20000, Buffer.from("ｍｓ ゴシック\0"))
    .store(9000, 0x12345678)
    .integer(9000).integer(20000).emit(0xb0, 0xc6)
    .integer(4096).emit(0x0a, 2).write("bgi-user/pitch-found.bin", 4096, 4)
    .write("bgi-user/pitch.bin", 9000, 4)
    .store(9000, 0x12345678).integer(9000).string("Absent font").emit(0xb0, 0xc6)
    .integer(4096).emit(0x0a, 2).write("bgi-user/pitch-missing.bin", 4096, 4)
    .write("bgi-user/pitch-untouched.bin", 9000, 4);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  assert.deepEqual(writes.find(row => row.path === "pitch-found.bin")?.bytes, [1, 0, 0, 0]);
  assert.deepEqual(writes.find(row => row.path === "pitch.bin")?.bytes, [2, 0, 0, 0]);
  assert.deepEqual(writes.find(row => row.path === "pitch-missing.bin")?.bytes, [0, 0, 0, 0]);
  assert.deepEqual(writes.find(row => row.path === "pitch-untouched.bin")?.bytes, [0x78, 0x56, 0x34, 0x12]);
});

test("explicit Unicode glyph slots preserve the six-argument no-result contract", async () => {
  const script = new Script().integer(0x12345678);
  for (const code of [0x8000f001, 0x8000f100, 0x8000f7ff, 0xff01]) {
    script.integer(code).integer(-1).integer(0).integer(0).integer(0).integer(0).emit(0x92, 0x98);
  }
  script.integer(4096).emit(0x0a, 2).write("bgi-user/glyph-stack.bin", 4096, 4);
  const {writes} = await run(script, [extendedGraphicsExecutable()]);
  assert.deepEqual(writes.find(row => row.path === "glyph-stack.bin")?.bytes, [0x78, 0x56, 0x34, 0x12]);
});

test("the base glyph contract keeps explicit Unicode codes outside its accepted range", async () => {
  const result = await resultFor(new Script().integer(0x8000f001).integer(-1)
    .integer(0).integer(0).integer(0).integer(0).emit(0x92, 0x98));
  assert.match(result.error, /無効な文字コード/);
});

for (const [name, executable, mode, code, accepted] of [
  ["a detected Unicode bridge", unicodeGlyphExecutable, 1, 0xf001, true],
  ["the same bridge in CP932 mode", unicodeGlyphExecutable, 0, 0xf001, false],
  ["a direct-forwarding wrapper in UTF-8 mode", extendedGraphicsExecutable, 1, 0xf001, false],
  ["the Unicode bridge with an incompatible legacy code", unicodeGlyphExecutable, 1, 0xff01, false],
]) {
  test(`${name} preserves the native glyph registration range`, async () => {
    const script = new Script().integer(mode).emit(0x81, 0).integer(4092).emit(0x0a, 2)
      .integer(0x12345678).integer(code).integer(-1)
      .integer(0).integer(0).integer(0).integer(0).emit(0x92, 0x98)
      .integer(4096).emit(0x0a, 2).write("bgi-user/bridge-stack.bin", 4096, 4);
    const result = await resultFor(script, [executable()]);
    if (accepted) {
      assert.equal(result.error, null, result.error);
      assert.deepEqual(result.writes.find(row => row.path === "bridge-stack.bin")?.bytes,
        [0x78, 0x56, 0x34, 0x12]);
    } else {
      assert.match(result.error, /無効な文字コード/);
    }
  });
}


test("cropped inline glyphs keep their original color after the source bitmap is released", async () => {
  const script = guestBytes(new Script(), 12000,
    Buffer.from([255, 255, 255, 0, 0, 255, 0, 255, 0]))
    .integer(17).integer(3).integer(1).integer(1).integer(12000).emit(0x90, 0x14)
    .integer(0xff01).integer(17).integer(1).integer(0).integer(2).integer(1).emit(0x92, 0x98)
    .integer(17).emit(0x90, 0x12).integer(4088).emit(0x0a, 2)
    .write("bgi-user/glyph-source-release.bin", 4088, 4)
    .integer(17).integer(3).integer(1).integer(1).emit(0x90, 0x11)
    .integer(17).integer(0x0000ff).emit(0x90, 0x13)
    .integer(128).integer(64).emit(0x90, 0x80).integer(4096).emit(0x0a, 2)
    .integer(0xb0000000).integer(0).integer(26).integer(100).integer(0).integer(0).integer(0).emit(0x91, 0x88)
    .integer(1).integer(0).emit(0x90, 0x0c);
  // Sentence lookup produces a Unicode value and the actual UTF-8 guest bytes.
  // This tests the presenter independently of the graph input code-page option.
  const table = sentenceTable([["glyphs", [["mark", "\uf001"]]]]);
  guestBytes(script, 16384, table).integer(16384).integer(table.length).emit(0x81, 0xd8)
    .integer(4084).emit(0x0a, 2)
    .integer(24000).string("glyphs").string("mark").emit(0x81, 0xda).integer(4080).emit(0x0a, 2)
    .integer(0xb0000000).integer(24000).integer(0x0000ff);
  for (let index = 0; index < 8; index++) script.integer(0);
  script.emit(0x92, 0x91).integer(1).emit(0x91, 0)
    .emit(0x80, 0x5f, 0x80, 0x5f, 0x80, 0x5f);
  const result = await resultFor(script, [extendedGraphicsExecutable()],
    [[255, 0, 0, 255], [0, 255, 0, 255]]);
  assert.equal(result.error, null, result.error);
  assert.deepEqual(result.writes.find(row => row.path === "glyph-source-release.bin")?.bytes, [1, 0, 0, 0]);
  assert.deepEqual(result.colorCounts, [1, 1], JSON.stringify(result));
});
test("a zero text scale divisor is accepted by the native graphics API", async () => {
  const script = new Script().integer(0).emit(0x91, 0x99)
    .integer(4096).emit(0x0a, 2).write("bgi-user/scale.bin", 4096, 4);
  const {writes} = await run(script);
  assert.deepEqual(writes.find(row => row.path === "scale.bin")?.bytes, [1, 0, 0, 0]);
});

test("unregistered extended graphics selectors still fail", async () => {
  const result = await resultFor(new Script().emit(0x91, 0xfe));
  assert.match(result.error, /UnknownDispatch/);
});

function registration(product = "FixtureProduct", folder = "H:\\OldFolder") {
  const value = folder.replaceAll("\\", "\\\\");
  return `Windows Registry Editor Version 5.00\r\n\r\n[HKEY_LOCAL_MACHINE\\SOFTWARE\\Wow6432Node\\FixtureVendor\\${product}]\r\n"InstalledFolder"="${value}"\r\n`;
}

function snapshot(text) {return Buffer.concat([Buffer.from([255, 254]), Buffer.from(text, "utf16le")]).toString("base64");}

function query(product = "FixtureProduct") {
  return new Script().integer(4096).string("FixtureVendor").string(product).emit(0x80, 0xf8)
    .integer(4160).emit(0x0a, 2)
    .write("bgi-user/found.bin", 4160, 4)
    .write("bgi-user/folder.bin", 4096, 14);
}

test("a package installation snapshot relocates its declared folder to the managed resource root", async () => {
  const {writes} = await run(query(), [["install.reg", snapshot(registration())]]);
  assert.deepEqual(writes.find(row => row.path === "found.bin")?.bytes, [1, 0, 0, 0]);
  assert.equal(Buffer.from(writes.find(row => row.path === "folder.bin").bytes).toString(), "bgi-resource\\\0");
});

test("package installation metadata never invents an absent optional component", async () => {
  const {writes} = await run(query("OptionalComponent"), [["install.reg", snapshot(registration())]]);
  assert.deepEqual(writes.find(row => row.path === "found.bin")?.bytes, [0, 0, 0, 0]);
});

test("an ambiguous snapshot cannot bind multiple installations to the same resource tree", async () => {
  const text = registration() + registration("OtherComponent", "D:\\DifferentFolder").split("\r\n").slice(1).join("\r\n");
  const {writes} = await run(query(), [["install.reg", snapshot(text)]]);
  assert.deepEqual(writes.find(row => row.path === "found.bin")?.bytes, [0, 0, 0, 0]);
});

test("registry ANSI key bytes survive a snapshot made under the other supported code page", async () => {
  const nativeName = Buffer.from([0x93, 0xfa, 0x96, 0x7b, 0x8c, 0xea]);
  const snapshotName = new TextDecoder("gbk").decode(nativeName);
  const {writes} = await run(query(nativeName), [["install.reg", snapshot(registration(snapshotName))]]);
  assert.deepEqual(writes.find(row => row.path === "found.bin")?.bytes, [1, 0, 0, 0]);
});

for (const [name, data] of [
  ["oversized registry text", snapshot(registration() + " ".repeat(65536))],
  ["truncated UTF-16", Buffer.from([255, 254, 65]).toString("base64")],
  ["an escaping installation path", snapshot(registration("FixtureProduct", "H:\\..\\Elsewhere"))],
]) {
  test(`${name} cannot declare an installation`, async () => {
    const {writes} = await run(query(), [["install.reg", data]]);
    assert.deepEqual(writes.find(row => row.path === "found.bin")?.bytes, [0, 0, 0, 0]);
  });
}
