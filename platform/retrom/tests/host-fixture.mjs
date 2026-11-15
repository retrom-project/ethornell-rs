// Project-owned synthetic BP/PackFile input; no commercial scripts or game IDs.
import {readFile} from "node:fs/promises";

const root = new URL("../../../", import.meta.url);
export async function buildInputs() {
  const [source, wasm, font] = await Promise.all([
    readFile(new URL("build/retrom/pkg/ethornell_web.js", root), "utf8"),
    readFile(new URL("build/retrom/pkg/ethornell_web_bg.wasm", root)),
    readFile(new URL("dist/retrom-inputs/font/NotoSansSC.ttf", root)),
  ]);
  return {source, wasm: wasm.toString("base64"), font: font.toString("base64")};
}

export class Script {
  code = [];
  strings = [];
  integer(value) {
    const data = Buffer.alloc(4); data.writeUInt32LE(value >>> 0);
    this.code.push(2, ...data); return this;
  }
  string(value) {
    this.strings.push({offset: this.code.length, value});
    this.code.push(5, 0, 0); return this;
  }
  emit(...bytes) {this.code.push(...bytes); return this;}
  store(address, value) {return this.integer(value).integer(address).emit(0x0a, 2);}
  write(path, address, length) {
    return this.string(path).integer(address).integer(length).emit(0x80, 0x32);
  }
  archive() {
    const image = [...this.code, 0x17];
    for (const {offset, value} of this.strings) {
      const relative = image.length - offset;
      image[offset + 1] = relative & 255; image[offset + 2] = relative >> 8;
      image.push(...(Buffer.isBuffer(value) ? value : Buffer.from(value)), 0);
    }
    const header = Buffer.alloc(16);
    header.writeUInt32LE(16); header.writeUInt32LE(image.length, 4);
    const bp = Buffer.concat([header, Buffer.from(image)]);
    const arc = Buffer.alloc(48);
    arc.write("PackFile    "); arc.writeUInt32LE(1, 12);
    arc.write("ipl._bp", 16); arc.writeUInt32LE(bp.length, 36);
    return Buffer.concat([arc, bp]).toString("base64");
  }
}

export async function execute(page, inputs, script, resources = [], colors = []) {
  return page.evaluate(async ({source, wasm, font, archive, resources, colors}) => {
    const bytes = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
    const files = [["system.arc", bytes(archive)], ...resources.map(([path, data]) => [path, bytes(data)])];
    const writes = [];
    globalThis.ethornellReadAt = (id, offset, out) => {
      const data = files[id][1].subarray(offset, offset + out.length);
      out.set(data); return data.length;
    };
    globalThis.ethornellSaveUserFile = (path, data) => {
      writes.push({path, length: data.length, bytes: data.length <= 16 ? [...data] : null});
    };
    globalThis.ethornellRemoveUserFile = () => {};
    const {default: init, WebEngine, createRenderer} = await import("data:text/javascript;base64," + btoa(source));
    await init({module_or_path: bytes(wasm)});
    const engine = new WebEngine(files.map(([path, data]) => [path, data.length]), [], bytes(font), false, true);
    try {
      const initialSize = [engine.width(), engine.height()];
      const visible = document.createElement("canvas");
      visible.width = engine.width(); visible.height = engine.height();
      if (colors.length) document.body.append(visible);
      const canvas = colors.length ? visible.transferControlToOffscreen() : new OffscreenCanvas(engine.width(), engine.height());
      canvas.getContext("webgl2", {preserveDrawingBuffer: true, alpha: false, antialias: false});
      engine.attach(await createRenderer(canvas, canvas.width, canvas.height, engine.font()));
      let error = null;
      try {engine.step(0); engine.step(16); engine.step(32);} catch (value) {error = String(value);}
      const colorCounts = colors.map(() => 0);
      let pixelSummary = [];
      if (colors.length) {
        await new Promise(resolve => setTimeout(resolve, 30));
        const target = new OffscreenCanvas(canvas.width, canvas.height).getContext("2d");
        target.drawImage(visible, 0, 0);
        const pixels = target.getImageData(0, 0, canvas.width, canvas.height).data;
        const histogram = new Map();
        for (let offset = 0; offset < pixels.length; offset += 4) {
          const color = [...pixels.subarray(offset, offset + 4)].join(",");
          histogram.set(color, (histogram.get(color) ?? 0) + 1);
          colors.forEach((color, index) => {
            if (color.every((value, channel) => pixels[offset + channel] === value)) colorCounts[index]++;
          });
        }
        pixelSummary = [...histogram].sort((a, b) => b[1] - a[1]).slice(0, 8);
      }
      return {error, diagnostics: engine.diagnostics(), initialSize, size: [engine.width(), engine.height()], writes, colorCounts, pixelSummary};
    } finally {engine.free();}
  }, {...inputs, archive: script.archive(), resources, colors});
}
