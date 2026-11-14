// Retrom additions: MPL-2.0. All game bytes arrive through the host's bounded reader.
import {attachInput} from "./input.mjs";
import {openAudio} from "./audio.mjs";
import {showDialog} from "./dialogs.mjs";
export const abi = "ethornell-host-v1";
export async function create(options) {
  const {canvas, files, readFile, onFailure, onExit} = options;
  if (!globalThis.crossOriginIsolated || typeof SharedArrayBuffer !== "function") throw new Error("BGI_ISOLATION_REQUIRED");
  const shared = new SharedArrayBuffer(16 + 1024 * 1024), state = new Int32Array(shared, 0, 4), data = new Uint8Array(shared, 16);
  let worker, audio, disposeInput, stopped = false, readyState = false, nextId = 0, frames = 0, closing;
  let status = {progress: false, revision: "0", changed: false};
  const pending = new Map();
  const command = (type, values = {}) => new Promise((resolve, reject) => {
    if (stopped) {reject(new Error("BGI_STOPPED")); return;}
    const id = ++nextId; pending.set(id, {resolve, reject}); worker.postMessage({type, id, ...values});
  });
  const notify = (type, values = {}) => {if (!stopped && (type !== "input" || readyState)) worker.postMessage({type, ...values});};
  const close = () => closing ??= (async () => {
    stopped = true; disposeInput?.();
    Atomics.store(state, 0, -2); Atomics.notify(state, 0);
    worker?.terminate();
    for (const {reject} of pending.values()) reject(new Error("BGI_STOPPED"));
    pending.clear(); await audio?.close();
  })();
  const failure = error => {if (!stopped) {onFailure(error); void close().catch(onFailure);}};
  try {
    worker = new Worker(options.workerURL, {type: "module", name: "Retrom Ethornell"});
    worker.onerror = event => failure(new Error(event.message));
    const ready = new Promise((resolve, reject) => {
      pending.set(0, {resolve, reject});
      worker.onmessage = async event => {
        const message = event.data;
        if (stopped) return;
        if (message.type === "size") {canvas.style.aspectRatio = `${message.width} / ${message.height}`;}
        else if (message.type === "read") {
          try {
            const file = files[message.file];
            if (!file || !Number.isSafeInteger(message.offset) || message.offset < 0 ||
                !Number.isSafeInteger(message.length) || message.length < 1 || message.length > data.length ||
                message.offset + message.length > file.sizeBytes) throw new Error("BGI_READ_INVALID");
            const bytes = await readFile(file.path, message.offset, message.length);
            if (stopped || Atomics.load(state, 0) !== 1) return;
            if (bytes.length !== message.length) throw new Error("BGI_READ_LENGTH_MISMATCH");
            data.set(bytes); Atomics.store(state, 1, bytes.length); Atomics.store(state, 0, 2); Atomics.notify(state, 0);
          } catch (error) {
            Atomics.store(state, 0, -1); Atomics.notify(state, 0); failure(error);
          }
        } else if (message.type === "started" || message.type === "status") {
          frames = message.frames; status = {progress: message.progress, revision: message.revision, changed: message.changed};
          if (message.type === "started") {pending.get(0)?.resolve(); pending.delete(0);}
        } else if (message.type === "result") {
          const call = pending.get(message.id); pending.delete(message.id);
          if (message.error) call?.reject(new Error(message.error)); else call?.resolve(message.value);
        } else if (message.type === "failure") failure(new Error(message.error));
        else if (message.type === "exit") onExit?.();
        else if (message.type === "dialog") void showDialog(message, command).catch(failure);
      };
    });
    void ready.catch(() => {});
    const font = await fetch(options.fontURL).then(response => {if (!response.ok) throw new Error("BGI_FONT_LOAD_FAILED"); return response.arrayBuffer();});
    audio = await openAudio(options.audioWorkletURL);
    disposeInput = attachInput(canvas, values => notify("input", {values}), () => audio.resume().catch(failure));
    const offscreen = canvas.transferControlToOffscreen();
    worker.postMessage({type: "start", files, canvas: offscreen, shared, wasmURL: options.wasmURL,
      identity: options.identity, restore: options.restore, textEncoding: options.textEncoding,
      font, sampleRate: audio.sampleRate, audioPort: audio.port}, [offscreen, font, audio.port]);
    await ready; readyState = true;
    return {
      frames: () => frames, status: () => status,
      export: () => command("export"), acknowledge: revision => notify("acknowledge", {revision}),
      screenshot: () => command("screenshot"),
      pause: async () => {disposeInput.release(); await command("pause"); await audio.pause();},
      resume: async () => {await command("resume"); await audio.resume();},
      volume: value => audio.volume(value), close,
    };
  } catch (error) {await close(); throw error;}
}
globalThis.retromEthornell = Object.freeze({abi, create});
