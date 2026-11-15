"""Build the fixed Ethornell Web ABI and deterministic corresponding source archive."""
import gzip
import hashlib
import io
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from retrom_licenses import dependency_graph, write_notices

ROOT = Path(__file__).resolve().parents[1]
FONT_COMMIT = "a85815a42757630ce188fdad368c2dfc444d4773"
FONT_SHA = "a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da"
FONT_LICENSE_SHA = "1c05c68c34f9708415aada51f17e1b0092d2cea709bf4a94cd38114f9e73d7d9"


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def git(*args):
    if not (ROOT / ".git").exists():
        identity = json.loads((ROOT / ".retrom-source.json").read_text())
        dirty = identity["dirty"] or source_rows() != identity["rows"]
        return {("branch", "--show-current"): identity["branch"],
                ("rev-parse", "HEAD"): identity["commit"],
                ("status", "--porcelain"): "modified source snapshot" if dirty else ""}[args]
    return subprocess.check_output(["git", *args], cwd=ROOT).decode().strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    if not (ROOT / ".git").exists():
        return json.loads((ROOT / ".retrom-source.json").read_text())["files"]
    return sorted(set(git("ls-files", "--cached", "--others", "--exclude-standard", "-z").split("\0")) - {""})


def source_rows():
    rows = {} if (ROOT / ".git").exists() else {r["path"]: r for r in json.loads((ROOT / ".retrom-source.json").read_text())["rows"]}
    for name in sources():
        path = ROOT / name
        if path.is_file():
            rows[name] = {"path": name, "mode": "100755" if path.stat().st_mode & 0o111 else "100644", "sha256": digest(path)}
        else:
            rows.pop(name, None)
    return [rows[name] for name in sorted(rows)]


def tree_digest():
    return hashlib.sha256(json.dumps(source_rows(), sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def fetch_verified(url, path, expected):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        with urllib.request.urlopen(url, timeout=60) as response:
            path.write_bytes(response.read())
    if digest(path) != expected:
        raise ValueError("ETHORNELL_BUILD_INPUT_HASH_MISMATCH:" + path.name)


def build_input(name):
    if re.fullmatch(r"(?:.*/)?(?:BGI\.gdb|BGI\d+\.cad|\.ethornell-platform-store)", name, re.IGNORECASE):
        return False
    return (name.startswith(("crates/", "platform/retrom/", ".cargo/", "tools/", ".github/rpg-runtime/")) or
            name in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "LICENSE-MPL-2.0", "retrom-fork.json",
                     "RETROM.md", "RETROM-NOTICES.md", "platform/wasm/audio-worklet.js"})


def archive(output, packages):
    files = [name for name in sources() if build_input(name) and (ROOT / name).is_file()]
    rows = source_rows()
    identity = {"files": files, "rows": rows, "branch": git("branch", "--show-current"),
                "commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain"))}
    entries = {name: (ROOT / name, None) for name in files}
    entries[".retrom-source.json"] = (None, (json.dumps(identity, sort_keys=True, indent=2) + "\n").encode())
    # Include the complete selected registry/build source, preserving embedded notices.
    # Cargo.lock still resolves dependencies normally; no offline rebuild is claimed.
    for package in packages:
        if package["source"] is None:
            continue
        folder = Path(package["manifest_path"]).parent
        for path in sorted(folder.rglob("*")):
            if path.is_file() and not path.is_symlink():
                name = "dependency-sources/" + package["name"] + "-" + package["version"] + "/" + path.relative_to(folder).as_posix()
                entries[name] = (path, None)
    with output.open("wb") as file, gzip.GzipFile(filename="", mode="wb", fileobj=file, mtime=0) as compressed, tarfile.open(fileobj=compressed, mode="w|") as tar:
        for name, (path, content) in sorted(entries.items()):
            content = content if content is not None else path.read_bytes()
            entry = tarfile.TarInfo("ethornell/" + name)
            entry.size = len(content)
            entry.mode = 0o755 if path and path.stat().st_mode & 0o111 else 0o644
            tar.addfile(entry, io.BytesIO(content))


def main():
    output = Path(sys.argv[1])
    if not output.is_absolute() or output.is_symlink() or output.exists() and any(output.iterdir()):
        raise ValueError("ETHORNELL_EMPTY_ABSOLUTE_OUTPUT_REQUIRED")
    output.mkdir(parents=True, exist_ok=True)
    run("npm", "ci", "--prefix", "platform/retrom")
    run("npm", "test", "--prefix", "platform/retrom")
    presentation_test = ROOT / "build/retrom/graph-presentation-tests"
    presentation_test.parent.mkdir(parents=True, exist_ok=True)
    run("rustc", "--edition=2024", "--test", "crates/ethornell-app/src/graph_presentation.rs", "-o", str(presentation_test))
    run(str(presentation_test))
    layout_test = ROOT / "build/retrom/text-layout-tests"
    run("rustc", "--edition=2024", "--test", "crates/ethornell-app/src/text_layout.rs", "-o", str(layout_test))
    run(str(layout_test))
    probe_test = ROOT / "build/retrom/native-utf8-probe-tests"
    run("rustc", "--edition=2024", "--test", "crates/ethornell-vm/src/native_utf8_probe.rs", "-o", str(probe_test))
    run(str(probe_test))
    catalog_test = ROOT / "build/retrom/font-catalog-tests"
    run("rustc", "--edition=2024", "--test", "crates/ethornell-vm/src/font_catalog.rs", "-o", str(catalog_test))
    run(str(catalog_test))
    glyph_profile_test = ROOT / "build/retrom/native-glyph-bridge-tests"
    run("rustc", "--edition=2024", "--test", "crates/ethornell-app/src/native_glyph_bridge.rs", "-o", str(glyph_profile_test))
    run(str(glyph_profile_test))
    source = tree_digest()
    font_root = ROOT / "dist/retrom-inputs/font"
    font = font_root / "NotoSansSC.ttf"
    font_license = font_root / "OFL.txt"
    fetch_verified(f"https://raw.githubusercontent.com/google/fonts/{FONT_COMMIT}/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf", font, FONT_SHA)
    fetch_verified(f"https://raw.githubusercontent.com/google/fonts/{FONT_COMMIT}/ofl/notosanssc/OFL.txt", font_license, FONT_LICENSE_SHA)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
    environment = dict(os.environ, RUSTFLAGS=f'--cfg getrandom_backend="wasm_js" --remap-path-prefix={ROOT}=. --remap-path-prefix={cargo_home}=/cargo')
    run("cargo", "build", "--locked", "--release", "--target", "wasm32-unknown-unknown", "-p", "ethornell-web", "-j", os.environ.get("ETHORNELL_BUILD_JOBS", "6"), env=environment)
    bindgen = os.environ.get("WASM_BINDGEN", "wasm-bindgen")
    if subprocess.check_output([bindgen, "--version"]).decode().strip() != "wasm-bindgen 0.2.130":
        raise ValueError("ETHORNELL_WASM_BINDGEN_VERSION_MISMATCH")
    run(bindgen, str(ROOT / "target/wasm32-unknown-unknown/release/ethornell_web.wasm"), "--target", "web", "--out-dir", str(ROOT / "build/retrom/pkg"))
    run("npm", "run", "test:host", "--prefix", "platform/retrom")
    esbuild = str(ROOT / "platform/retrom/node_modules/.bin/esbuild")
    for entry, name in [("bridge.mjs", "ethornell.mjs"), ("worker.mjs", "ethornell-worker.mjs")]:
        run(esbuild, "platform/retrom/" + entry, "--bundle", "--format=esm", "--target=es2022", "--legal-comments=inline", "--minify", "--banner:js=/*! Ethornell: MPL-2.0; see THIRD-PARTY-NOTICES.md and corresponding source. */", f"--outfile={output / name}")
    shutil.copyfile(ROOT / "platform/wasm/audio-worklet.js", output / "audio-worklet.mjs")
    shutil.copyfile(ROOT / "build/retrom/pkg/ethornell_web_bg.wasm", output / "ethornell_bg.wasm")
    shutil.copyfile(font, output / "font.ttf")
    shutil.copyfile(font_license, output / "FONT-OFL.txt")
    shutil.copyfile(ROOT / "LICENSE-MPL-2.0", output / "LICENSE-MPL-2.0")
    packages = dependency_graph()
    write_notices(output / "THIRD-PARTY-NOTICES.md", packages)
    archive(output / "ethornell-source.tar.gz", packages)
    if tree_digest() != source:
        raise ValueError("ETHORNELL_SOURCE_CHANGED_DURING_BUILD")
    descriptor = {"schemaVersion": 1, "kind": "RETROM_CORE_CANDIDATE_V1", "coreId": "ethornell",
                  "repository": "https://github.com/retrom-project/ethornell-rs", "branch": git("branch", "--show-current"),
                  "commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain")),
                  "sourceTreeSha256": source, "adapterAbi": "ethornell-host-v1", "files": [
                      {"filename": p.name, "sizeBytes": p.stat().st_size, "sha256": digest(p)} for p in sorted(output.iterdir())]}
    (output / "retrom-core-candidate.json").write_text(json.dumps(descriptor, sort_keys=True, indent=2) + "\n")


if __name__ == "__main__":
    main()
