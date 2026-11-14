// Retrom additions: MPL-2.0. Range transport only; no downloader or resource cache.
import init, {WebEngine, createRenderer, renderAudio, setAudioSampleRate} from "../../build/retrom/pkg/ethornell_web.js";
import {SaveStore} from "./save-store.mjs";
let engine, canvas, store, state, data, stopped = false, paused = false, frames = 0;
let request = 0;
const post = value => globalThis.postMessage(value);
globalThis.ethornellReadAt = (id, offset, out) => {
  for (let copied = 0; copied < out.length;) {
    if (Atomics.load(state, 0) === -2) throw new Error("BGI_READ_ABORTED");
    const length = Math.min(out.length - copied, data.length);
    Atomics.store(state, 0, 1); post({type: "read", file: id, offset: offset + copied, length});
    while (Atomics.load(state, 0) === 1) Atomics.wait(state, 0, 1);
    if (Atomics.load(state, 0) !== 2 || Atomics.load(state, 1) !== length) throw new Error("BGI_READ_FAILED");
    out.set(data.subarray(0, length), copied); copied += length;
  }
  return out.length;
};
globalThis.ethornellSaveUserFile = (path, bytes) => {store.write(path, bytes); post({type: "status", ...store.status(), frames});};
globalThis.ethornellRemoveUserFile = path => {store.remove(path); post({type: "status", ...store.status(), frames});};
function events() {
  for (let event; (event = engine.pollEvent());) {
    const {kind, requestId, code, text} = event; event.free();
    if (kind === 2 || kind === 5) post({type: "dialog", id: requestId, kind, code, text,
      payload: kind === 5 ? engine.requestPayload(requestId) : null});
    else if (kind === 3) {console.info("ethornell: engine exited", engine.diagnostics()); stopped = true; post({type: "exit"});}
    else if (kind !== 4) console.info("ethornell:", text);
  }
}
function tick(now) {
  if (stopped || paused) return;
  try {engine.step(now); frames++; resize(); events(); post({type: "status", ...store.status(), frames});}
  catch (error) {fail(error); return;}
  if (!stopped) request = setTimeout(() => tick(performance.now()), 16);
}
function resize() {
  const width = engine.width(), height = engine.height();
  if (width !== canvas.width || height !== canvas.height) {canvas.width = width; canvas.height = height; engine.resize(width, height); post({type: "size", width, height});}
}
function fail(error) {stopped = true; post({type: "failure", error: String(error?.stack || error)});}
async function start(message) {
  state = new Int32Array(message.shared, 0, 4); data = new Uint8Array(message.shared, 16);
  await init({module_or_path: message.wasmURL});
  setAudioSampleRate(message.sampleRate);
  store = new SaveStore(message.identity, message.restore);
  engine = new WebEngine(message.files.map(file => [file.path, file.sizeBytes]), [...store.files],
    new Uint8Array(message.font), message.textEncoding === "gbk", false);
  canvas = message.canvas; canvas.width = engine.width(); canvas.height = engine.height();
  canvas.getContext("webgl2", {preserveDrawingBuffer: true, alpha: false, antialias: false});
  engine.attach(await createRenderer(canvas, canvas.width, canvas.height, engine.font()));
  if (message.audioPort) message.audioPort.onmessage = event => {
    if (stopped) return;
    try {const chunk = new Float32Array(event.data * 2); renderAudio(chunk); message.audioPort.postMessage(chunk, [chunk.buffer]);}
    catch (error) {fail(error);}
  };
  engine.step(performance.now()); frames++; events();
  post({type: "started", ...store.status(), frames});
  if (!stopped) tick(performance.now());
}
globalThis.onmessage = async event => {
  const message = event.data;
  try {
    if (message.type === "start") {await start(message); return;}
    let value;
    switch (message.type) {
      case "input": if (!stopped) engine.input(...message.values); break;
      case "pause": paused = true; clearTimeout(request); engine.input(7, 0, 0, 0, 0); engine.pause(1); break;
      case "resume": if (!stopped && paused) {engine.pause(0); paused = false; tick(performance.now());} break;
      case "export": value = store.export(); break;
      case "acknowledge": store.acknowledge(message.revision); post({type: "status", ...store.status(), frames}); break;
      case "screenshot": value = await canvas.convertToBlob({type: "image/png"}); break;
      case "answer": engine.completeRequest(message.dialogId, message.result); break;
      case "dialog": engine.completeDialog(message.dialogId, message.response); break;
      case "close": stopped = true; clearTimeout(request); engine.input(7, 0, 0, 0, 0); engine.free(); break;
      default: throw new Error("BGI_COMMAND_INVALID");
    }
    if (engine && !stopped) events();
    if (message.id) post({type: "result", id: message.id, value});
  } catch (error) {
    if (message.id) post({type: "result", id: message.id, error: String(error)});
    else fail(error);
  }
};
