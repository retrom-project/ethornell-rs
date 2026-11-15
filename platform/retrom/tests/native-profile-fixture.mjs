// Project-owned PE32 metadata fixture. It is inspected as data, never executed.
// Exercises native_profile.rs's old BP tags and verified Graph91 algorithms.
const imageBase = 0x400000;
const textRva = 0x1000, dataRva = 0x2000, resourceRva = 0x3000;
const align4 = value => (value + 3) & ~3;
const wide = value => Buffer.from(`${value}\0`, "utf16le");

function versionBlock(key, type, value = Buffer.alloc(0), children = []) {
  const name = wide(key);
  const valueOffset = align4(6 + name.length);
  const childrenOffset = align4(valueOffset + value.length);
  const result = Buffer.alloc(childrenOffset + children.reduce((n, child) => n + align4(child.length), 0));
  result.writeUInt16LE(result.length);
  result.writeUInt16LE(type === 1 ? value.length / 2 : value.length, 2);
  result.writeUInt16LE(type, 4);
  name.copy(result, 6);
  value.copy(result, valueOffset);
  let offset = childrenOffset;
  for (const child of children) {child.copy(result, offset); offset += align4(child.length);}
  return result;
}

function versionResource() {
  const fixed = Buffer.alloc(52);
  fixed.writeUInt32LE(0xfeef04bd);
  fixed.writeUInt32LE(0x10000, 4);
  fixed.writeUInt32LE(0x3f, 24);
  fixed.writeUInt32LE(0x40004, 32);
  fixed.writeUInt32LE(1, 36);
  const strings = versionBlock("StringFileInfo", 1, undefined, [
    versionBlock("040904B0", 1, undefined, [versionBlock("InternalName", 1, wide("Ethornell"))]),
  ]);
  const vars = versionBlock("VarFileInfo", 1, undefined, [
    versionBlock("Translation", 0, Buffer.from([9, 4, 0xb0, 4])),
  ]);
  return versionBlock("VS_VERSION_INFO", 0, fixed, [strings, vars]);
}

export function extendedGraphicsExecutable() {
  const pe = Buffer.alloc(0x1400);
  pe.write("MZ"); pe.writeUInt32LE(0x80, 0x3c); pe.write("PE\0\0", 0x80);
  pe.writeUInt16LE(0x14c, 0x84); pe.writeUInt16LE(3, 0x86);
  pe.writeUInt16LE(0xe0, 0x94); pe.writeUInt16LE(0x102, 0x96);
  const optional = 0x98;
  pe.writeUInt16LE(0x10b, optional);
  for (const [at, value] of [[4, 0x400], [8, 0xe00], [16, textRva], [20, textRva],
    [24, dataRva], [28, imageBase], [32, 0x1000], [36, 0x200], [56, 0x4000],
    [60, 0x200], [72, 0x100000], [76, 0x1000], [80, 0x100000], [84, 0x1000], [92, 16]]) {
    pe.writeUInt32LE(value, optional + at);
  }
  pe.writeUInt16LE(6, optional + 40); pe.writeUInt16LE(6, optional + 48);
  pe.writeUInt16LE(3, optional + 68);
  const sections = [[".text", textRva, 0x200, 0x400, 0x60000020],
    [".data", dataRva, 0x600, 0xa00, 0xc0000040],
    [".rsrc", resourceRva, 0x1000, 0x400, 0x40000040]];
  sections.forEach(([name, rva, raw, size, flags], index) => {
    const at = optional + 0xe0 + index * 40;
    pe.write(name, at); pe.writeUInt32LE(size, at + 8); pe.writeUInt32LE(rva, at + 12);
    pe.writeUInt32LE(size, at + 16); pe.writeUInt32LE(raw, at + 20); pe.writeUInt32LE(flags, at + 36);
  });
  const address = offset => imageBase + textRva + offset;
  const code = (offset, bytes) => Buffer.from(bytes).copy(pe, 0x200 + offset);
  const integer = value => {const result = Buffer.alloc(4); result.writeUInt32LE(value >>> 0); return [...result];};
  const call = (offset, target) => code(offset, [0xe8, ...integer(address(target) - address(offset) - 5)]);
  code(0, [0xc3]); code(0x10, [0x33, 0xc0, 0xc3]); code(0x30, [0xc3]);
  // BP04 uses the older thread-data tag. It must not select Modern.
  code(0x20, [0x0d, 0, 0, 0, 8, 0xc3]);
  const state = offset => imageBase + dataRva + 0x800 + offset;
  // Unsigned mode <= 1; preserve invalid input, return accepted boolean.
  code(0x40, [0x83, 0xf9, 1, 0x77, 12, 0x89, 0x0d, ...integer(state(0)),
    0xb8, 1, 0, 0, 0, 0xc3, 0x33, 0xc0, 0xc3]);
  // Exactly the two raster-option keys; unknown key returns status 25.
  code(0x80, [0x81, 0xf9, 0, 0, 0x22, 1, 0x74, 23,
    0x81, 0xf9, 0, 0x16, 0x11, 0x80, 0x74, 6, 0xb8, 25, 0, 0, 0, 0xc3,
    0x89, 0x15, ...integer(state(4)), 0x33, 0xc0, 0xc3,
    0x89, 0x15, ...integer(state(8)), 0x33, 0xc0, 0xc3]);
  call(0xd0, 0x10); code(0xd5, [0x89, 0x0d, ...integer(state(12)), 0x33, 0xc0, 0xc3]);
  [0x10, 0x40, 0x30].forEach((target, index) => call(0x100 + index * 5, target));
  code(0x10f, [0x33, 0xc0, 0xc3]);
  [0x10, 0x10, 0x80, 0x30].forEach((target, index) => call(0x180 + index * 5, target));
  code(0x194, [0x33, 0xc0, 0xc3]);
  const main = [0, 1, 2, 4, 5, 6, 8, 9, 10, 11, 12, 16, 17, 20, 21, 22, 23,
    128, 129, 144, 145, 146, 160, 176, 192, 255];
  for (const id of main) pe.writeUInt32LE(address(id === 4 ? 0x20 : 0), 0x600 + id * 4);
  for (const id of [0, 3, 6, 9, 10, 11, 12, 13, 14, 15]) {
    const offset = id === 0 ? 0xd0 : id === 9 ? 0x100 : id === 10 ? 0x180 : 0;
    pe.writeUInt32LE(address(offset), 0xa00 + id * 4);
  }
  // Resource directories: RT_VERSION / id 1 / en-US -> VS_VERSION_INFO.
  for (const [offset, id, target] of [[0, 16, 0x80000018], [24, 1, 0x80000030], [48, 0x409, 80]]) {
    pe.writeUInt16LE(1, 0x1000 + offset + 14);
    pe.writeUInt32LE(id, 0x1000 + offset + 16); pe.writeUInt32LE(target, 0x1000 + offset + 20);
  }
  const version = versionResource();
  pe.writeUInt32LE(resourceRva + 96, 0x1050); pe.writeUInt32LE(version.length, 0x1054);
  pe.writeUInt32LE(1200, 0x1058); version.copy(pe, 0x1060);
  pe.writeUInt32LE(resourceRva, optional + 112); pe.writeUInt32LE(96 + version.length, optional + 116);
  return ["owned-graph-abi.exe", pe.toString("base64")];
}
