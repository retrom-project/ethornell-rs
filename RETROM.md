# Retrom browser integration

The `main` branch mirrors upstream. Retrom maintains `retrom/g7cfc2fba8261`,
based on upstream commit `7cfc2fba8261e59c68095ab1ddc4f1f3534c27d8`.
Use `feat/*`, `fix/*`, `build/*` or `sync/upstream-*` branches for changes.
The authoritative baseline, ABI and immutable release names are in `retrom-fork.json`.

## Build

Use Rust 1.100.0 with `wasm32-unknown-unknown`, Node 24, Python 3 and
wasm-bindgen CLI 0.2.130. `WASM_BINDGEN` can select the pinned CLI executable.
The host regression suite also requires Chrome. Set `CHROME_BIN` to an existing
Chrome executable, or install the browser selected by the locked `playwright-core`
package into an explicit `PLAYWRIGHT_BROWSERS_PATH` with
`npm exec --prefix platform/retrom -- playwright-core install chromium` after
`npm ci --prefix platform/retrom`.
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

Optional installed-folder queries (Sys80:F8) return the native not-found result
when the restricted host has no installation metadata. They do not access the
operating-system registry. Registry mutations and other denied host operations
remain errors; cleanup preserves the original capability failure.
Root-level registry snapshots can declare this mounted installation through a
`HKLM\\Software[\\Wow6432Node]\\company\\product` `InstalledFolder` string.
The reader accepts bounded UTF-16LE or UTF-8 registry text and matches the ANSI
key bytes represented by CP932/GBK. A single declared location relocates to the
read-only resource mount; unrelated components and ambiguous multiple locations
remain not found. The reader never executes registry text or opens its old paths.

## Extended graphics contracts

The extended legacy Graph91 ABI supports `91:00`: consume one raw integer,
push no result, and present idle frames for any nonzero value. A forced frame
leaves the next scheduled deadline and pending redraw intact. The older ABI's
null slot stays unsupported. The ABI follows the existing bounded executable
structure detector; game names and hashes do not select this behavior.

The same extended ABI accepts `81:00` encoding mode 0 (CP932) or 1 (UTF-8)
and returns acceptance. Invalid values leave the current mode unchanged. This
does not enable the absent `81:01..03` language slots.

Extended system calls also provide fixed CP932-to-UTF-8 conversion, the native
three-way byte probe and grouped sentence tables. The probe distinguishes ASCII
from candidate UTF-8 and follows the native three-multibyte-sequence limit; it is
not a general Unicode validator. Sentence imports replace the supplied groups,
preserve other groups and accept last-value duplicate registrations. Null input
clears all groups. Imports remain bounded and fully validated before mutation;
malformed input leaves prior registrations intact. Lookup detects its input
encoding independently of the current mode, supports length queries and preserves
the destination on a miss. Raw guest bytes take precedence over decoded shadows.

The extended `B0:C5` font enumeration consumes destination and charset. A null
destination queries packed-name bytes; a nonnull destination receives consecutive
NUL-terminated names and returns the family count. The extended `C4` shares that
path for charset 128. Names use the current encoding. The portable catalog exposes
only the existing renderable CJK aliases, with Latin/Japanese/Chinese capabilities;
it does not enumerate arbitrary interned names or claim Windows-installed fonts.
The extended `C6` takes an output pointer and a face name, independently detects
the input encoding, and writes only the pitch DWORD for a case-insensitive exact
match. A miss returns zero without changing the output. It does not treat the
output pointer as text. The older ABI's null `C5` slot remains an error. Invalid
heap frees stay errors.

The native text scale setter accepts zero and clears the reciprocal to select
natural glyph metrics. Text properties `80000007` and `80000008` preserve the
signed kinsoku margin multiplier and nonzero ruby effect inset respectively.
The retained compositor redraws started glyphs from elapsed engine time, including
when `80000006` requests full message repaint. These contracts do not imply that
all nonzero native text scale factors or every movie path are implemented.

Inline bitmap glyphs retain an independent cropped image after their source
bitmap is released. Text and ruby use that image's natural width and original
color; clearing a registered slot makes it empty. Sessions keep separate tables.
The extended ABI accepts explicit tagged Unicode slots. A separate bounded PE
capability match detects wrappers that tag codes using the current UTF-8 flag;
direct-forwarding wrappers and older validators retain their own code ranges.

The candidate build runs owned BP/PackFile and synthetic PE fixtures through the
actual Chrome/Wasm interpreter. It also runs the shared presentation scheduler
and bitmap text layout unit tests without requiring the desktop movie backend.

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
