import {test} from "node:test";
import assert from "node:assert/strict";
import {SaveStore} from "./save-store.mjs";
const progress = () => new Uint8Array(128).fill(7);
test("numbered native SUD progress survives restoration while settings do not enable capture", () => {
  const first = new SaveStore("scope");
  first.write("UserData/config.sud", progress());
  assert.equal(first.status().progress, false);
  first.write("UserData/Story001.sud", progress());
  const next = new SaveStore("scope", first.export().snapshot);
  assert.equal(next.status().progress, true);
  assert.deepEqual(next.files.get("userdata/story001.sud"), progress());
});
test("game progress survives a new store and settings alone never enable capture", () => {
  const first = new SaveStore("scope"); first.write("BGI.gdb", progress());
  assert.equal(first.status().progress, false); assert.throws(() => first.export(), /BGI_NO_SAVE/);
  first.write("UserData/BGI0000.cad", progress());
  const captured = first.export(), next = new SaveStore("scope", captured.snapshot);
  assert.deepEqual(next.files.get("userdata/bgi0000.cad"), progress());
  assert.equal(next.status().progress, true); assert.equal(next.status().changed, false);
  assert.throws(() => new SaveStore("different", captured.snapshot), /BGI_SAVE_INVALID/);
});
test("acknowledging a persisted revision preserves a subsequent save and copies writes", () => {
  const store = new SaveStore("scope"), bytes = progress(); store.write("BGI0001.cad", bytes);
  bytes.fill(0); assert.equal(store.files.get("bgi0001.cad")[0], 7);
  const captured = store.export(); store.write("BGI0002.cad", progress()); store.acknowledge(captured.revision);
  assert.equal(store.status().changed, true); store.acknowledge(store.status().revision);
  assert.equal(store.status().changed, false); store.remove("BGI0002.cad"); assert.equal(store.status().changed, true);
});
test("rejects escaped paths, case aliases, invalid base64 and unbounded user files", () => {
  const store = new SaveStore("scope");
  for (const path of ["../save", "/save", "a/../save", "a\\save", "C:save", "a//save"]) assert.throws(() => store.write(path, progress()));
  const snapshot = {format: "bgi-save-files", version: 1, scope: "scope", files: [{path: "a", data: "AQ=="}, {path: "A", data: "AQ=="}]};
  assert.throws(() => new SaveStore("scope", snapshot));
  assert.throws(() => store.write("huge", new Uint8Array(64 * 1024 * 1024 + 1)), /BGI_SAVE_LIMIT/);
  assert.equal(store.files.size, 0);
});
