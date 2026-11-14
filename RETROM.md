# Retrom browser integration

The `main` branch mirrors upstream. Retrom maintains `retrom/g7cfc2fba8261`,
based on upstream commit `7cfc2fba8261e59c68095ab1ddc4f1f3534c27d8`.
Use `feat/*`, `fix/*`, `build/*` or `sync/upstream-*` branches for changes.
The authoritative baseline, ABI and immutable release names are in `retrom-fork.json`.

## Build

Use Rust 1.100.0 with `wasm32-unknown-unknown`, Node 24, Python 3 and
wasm-bindgen CLI 0.2.130. `WASM_BINDGEN` can select the pinned CLI executable.
The package lock pins esbuild. The build downloads a pinned OFL font and verifies
its SHA-256 and license before using either.

```sh
.github/rpg-runtime/build-candidate.sh /absolute/empty/output
```

The candidate contains the entry module, engine worker, Wasm module, audio worklet,
font, license notices, deterministic corresponding-source archive and a descriptor
with every file's length and SHA-256. It contains no game data. Build twice from
the same inputs and compare every output, including the complete source archive.
Tags use only `retrom-core-g7cfc2fba8261-rN`, with optional `-rc.N` suffixes.

## Host ABI

`ethornell.mjs` exports `abi = "ethornell-host-v1"` and asynchronous `create`.
The host passes a canvas, verified blob URLs for the worker, Wasm, font and worklet,
an immutable file listing, bounded `readFile(path, offset, length)`, content identity,
`textEncoding` (`cp932` or `gbk`), optional restored save snapshot and failure/exit
callbacks. Resource bytes are read-only. A 1 MiB shared slot forwards synchronous
worker reads to that host callback; it adds no network client or resource cache.
Closing wakes pending readers and terminates the worker. Cross-origin isolation,
SharedArrayBuffer, OffscreenCanvas, module workers, WebGL2 and AudioWorklet are required.

The returned session exposes frames/status, export/acknowledge, screenshot,
pause/resume, volume and close. Keyboard and pointer input stay independent of
the host's standard-gamepad pointer mapping. Pause and exit release held input.
Rendering uses the game's native dimensions rather than scaling to the viewport.

## Native saves

Each worker owns an initially empty in-memory user tree; restored user files are
installed before creating the interpreter. The host owns durable storage and user
isolation. Native `BGI<slot>.cad` files and numbered `*.sud` slots larger than their
64-byte header enable progress export; global settings alone do not. Export captures all user files and an exact
revision. Acknowledging persisted bytes never acknowledges a later revision.
`bgi-save-files` version 1 snapshots contain a content scope and sorted lowercase
safe paths/base64 bytes. The runtime handles the public `GAME_SAVE` envelope and
its single gzip layer. A new launch imports files before initialization, then the
player loads the desired slot in the game's menu. This is not an execution snapshot.

## Scope and licensing

Upstream declares MPL-2.0; the text is included in `LICENSE-MPL-2.0`. Retrom additions
use that license. Dependency notices are derived from the actual wasm dependency
closure and include source links and available license/copyright files. FFmpeg-derived
codec code needs its original attribution and corresponding rebuild inputs;
workspace-only WMV code must not be confused with a linked browser decoder.
The distributed source archive includes build inputs rather than reverse-engineering
captures. Keep notices with the assets and publish source alongside immutable releases.

Compatibility remains specific to tested content and versions. Upstream's Web
limitations include SWF bitmaps and fully decoded movie soundtracks. Original
archives and resource patches can be mounted; Windows patch programs and DLLs
are never executed by this integration.

## Corresponding source and rebuilding a modified decoder

Extract `ethornell-source.tar.gz` and run the candidate command above from its
`ethornell/` directory. Git metadata is optional: `.retrom-source.json` records the
original source identity, including hashes for intentionally omitted research files.
Rust/Cargo and npm download the exact locked dependencies; the pinned default font
also requires network access. Selected Cargo dependency sources are additionally
included under `dependency-sources/`, retaining their copyright headers and notices.
The archive is not an offline toolchain installation.

To modify or replace the MPEG component, edit `crates/na_mpeg2_decoder` in that
source tree and rebuild the complete combined Wasm with the same command. Cargo
uses it as a path dependency; no proprietary object or signing key is needed for
relinking. The command emits new complete Wasm/JS assets and notices. Use those
bytes through the runtime's explicit candidate input process before deployment.
