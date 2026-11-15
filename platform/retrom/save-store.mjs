// Retrom additions: MPL-2.0. The host owns persistence and user isolation.
export const maximum = 128 * 1024 * 1024;
export function savePath(path) {
  if (typeof path !== "string" || !path || path.length > 4096 || /[\\:\x00-\x1f]/.test(path) ||
      path.split("/").some(part => !part || part === "." || part === "..")) throw new Error("BGI_SAVE_PATH_INVALID");
  return path.toLowerCase();
}
function base64(bytes) {
  let value = "";
  for (let offset = 0; offset < bytes.length; offset += 8192) value += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  return btoa(value);
}
export class SaveStore {
  constructor(scope, snapshot = null) {
    this.scope = scope; this.files = new Map(); this.revision = 0; this.acknowledged = 0;
    if (!snapshot) return;
    if (snapshot.format !== "bgi-save-files" || snapshot.version !== 1 || snapshot.scope !== scope ||
        !Array.isArray(snapshot.files) || JSON.stringify(snapshot).length > maximum) throw new Error("BGI_SAVE_INVALID");
    for (const entry of snapshot.files) {
      const path = savePath(entry.path);
      if (this.files.has(path) || typeof entry.data !== "string") throw new Error("BGI_SAVE_INVALID");
      const bytes = Uint8Array.from(atob(entry.data), char => char.charCodeAt(0));
      if (base64(bytes) !== entry.data) throw new Error("BGI_SAVE_INVALID");
      this.files.set(path, bytes);
    }
    this.check();
  }
  check() {
    let bytes = 0;
    for (const data of this.files.values()) bytes += data.length;
    if (this.files.size > 10000 || bytes > 64 * 1024 * 1024) throw new Error("BGI_SAVE_LIMIT");
  }
  write(path, bytes) {
    path = savePath(path);
    const previous = this.files.get(path), copy = bytes.slice();
    if (previous?.length === copy.length && previous.every((byte, index) => byte === copy[index])) return;
    this.files.set(path, copy);
    try {this.check();} catch (error) {
      if (previous) this.files.set(path, previous); else this.files.delete(path);
      throw error;
    }
    this.revision++;
  }
  remove(path) {if (this.files.delete(savePath(path))) this.revision++;}
  status() {
    return {progress: [...this.files].some(([path, bytes]) => /(?:^|\/)(?:bgi\d{4,}\.cad|[^/]*\d{3,}\.sud)$/.test(path) && bytes.length > 64),
      revision: String(this.revision), changed: this.revision !== this.acknowledged};
  }
  acknowledge(revision) {if (String(this.revision) === revision) this.acknowledged = this.revision;}
  export() {
    if (!this.status().progress) throw new Error("BGI_NO_SAVE");
    const snapshot = {format: "bgi-save-files", version: 1, scope: this.scope,
      files: [...this.files].sort(([a], [b]) => a < b ? -1 : 1).map(([path, bytes]) => ({path, data: base64(bytes)}))};
    if (JSON.stringify(snapshot).length > maximum) throw new Error("BGI_SAVE_LIMIT");
    return {snapshot, revision: String(this.revision)};
  }
}
