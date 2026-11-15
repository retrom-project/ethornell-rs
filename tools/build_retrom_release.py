"""Build and verify immutable browser assets and their complete rebuildable source."""
import json
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
from build_retrom_candidate import ROOT, digest, git, main as build_candidate

ASSETS = ("ethornell.mjs", "ethornell-worker.mjs", "ethornell_bg.wasm", "audio-worklet.mjs", "font.ttf",
          "LICENSE-MPL-2.0", "FONT-OFL.txt", "THIRD-PARTY-NOTICES.md")
REPOSITORY = "https://github.com/retrom-project/ethornell-rs"
BASELINE = "7cfc2fba8261e59c68095ab1ddc4f1f3534c27d8"


def verify(output):
    metadata = json.loads((output / "rpg-runtime-release.json").read_text())
    expected = set(ASSETS) | {"ethornell-source.tar.gz", "rpg-runtime-release.json"}
    if set(p.name for p in output.iterdir()) != expected:
        raise ValueError("ETHORNELL_RELEASE_FILE_SET_INVALID")
    if (metadata["schemaVersion"] != 1 or metadata["repository"] != REPOSITORY or
            not re.fullmatch(r"retrom-core-g7cfc2fba8261-r[1-9][0-9]*(-rc\.[1-9][0-9]*)?", metadata["tag"]) or
            metadata["adapterAbi"] != "ethornell-host-v1" or metadata["sourceCommits"] != {"engine": BASELINE}):
        raise ValueError("ETHORNELL_RELEASE_IDENTITY_INVALID")
    rows = metadata["files"] + [metadata["sourceArchive"]]
    if len(rows) != len(ASSETS) + 1 or {r["filename"] for r in rows} != expected - {"rpg-runtime-release.json"}:
        raise ValueError("ETHORNELL_RELEASE_MANIFEST_INVALID")
    for row in rows:
        path = output / row["filename"]
        if path.stat().st_size != row["sizeBytes"] or digest(path) != row["sha256"]:
            raise ValueError("ETHORNELL_RELEASE_HASH_MISMATCH: " + row["filename"])
    with tarfile.open(output / "ethornell-source.tar.gz") as archive:
        names = archive.getnames()
        if len(names) != len(set(names)) or any(".." in Path(name).parts or name.startswith("/") or
                not name.startswith("ethornell/") or "assets/fonts/default.ttf" in name for name in names):
            raise ValueError("ETHORNELL_SOURCE_ARCHIVE_INVALID")
        identity = json.load(archive.extractfile("ethornell/.retrom-source.json"))
        if identity["commit"] != metadata["commit"] or hashlib.sha256(json.dumps(identity["rows"], sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest() != metadata["sourceTreeSha256"]:
            raise ValueError("ETHORNELL_SOURCE_IDENTITY_MISMATCH")
        for required in ("Cargo.lock", ".cargo/config.toml", "rust-toolchain.toml", "RETROM.md", "tools/build_retrom_candidate.py"):
            if "ethornell/" + required not in names:
                raise ValueError("ETHORNELL_REBUILD_INPUT_MISSING: " + required)
    print(json.dumps({"verified": True, "tag": metadata["tag"], "commit": metadata["commit"],
                      "sourceArchiveSha256": metadata["sourceArchive"]["sha256"]}))


def main():
    output = Path(sys.argv[1]).resolve()
    if sys.argv[2:] == ["--verify"]:
        verify(output)
        return
    tag = sys.argv[2]
    if git("status", "--porcelain"):
        raise ValueError("ETHORNELL_CLEAN_SOURCE_REQUIRED")
    if not re.fullmatch(r"retrom-core-g7cfc2fba8261-r[1-9][0-9]*(-rc\.[1-9][0-9]*)?", tag):
        raise ValueError("ETHORNELL_RELEASE_TAG_INVALID")
    subprocess.run(["git", "merge-base", "--is-ancestor", BASELINE, "HEAD"], cwd=ROOT, check=True)
    if "--check" not in sys.argv:
        reference = os.environ.get("RETROM_RELEASE_TAG_REF", "refs/tags/" + tag)
        if reference not in ("refs/tags/" + tag, "refs/retrom-release/" + tag):
            raise ValueError("ETHORNELL_RELEASE_TAG_REF_INVALID")
        if git("cat-file", "-t", reference) != "tag" or git("rev-list", "-n", "1", reference) != git("rev-parse", "HEAD"):
            raise ValueError("ETHORNELL_ANNOTATED_TAG_REQUIRED")
        subprocess.run(["git", "merge-base", "--is-ancestor", "HEAD", "retrom/g7cfc2fba8261"], cwd=ROOT, check=True)
    sys.argv = [sys.argv[0], str(output)]
    build_candidate()
    candidate = json.loads((output / "retrom-core-candidate.json").read_text())
    (output / "retrom-core-candidate.json").unlink()
    row = lambda name: {"filename": name, "sizeBytes": (output / name).stat().st_size, "sha256": digest(output / name)}
    metadata = {"schemaVersion": 1, "repository": REPOSITORY, "tag": tag, "commit": candidate["commit"],
                "adapterAbi": candidate["adapterAbi"], "sourceCommits": {"engine": BASELINE},
                "sourceTreeSha256": candidate["sourceTreeSha256"], "files": [row(name) for name in ASSETS],
                "sourceArchive": row("ethornell-source.tar.gz"),
                "build": {"rust": "1.100.0", "wasmBindgen": "0.2.130", "node": "24.22.1", "esbuild": "0.27.0"}}
    (output / "rpg-runtime-release.json").write_text(json.dumps(metadata, sort_keys=True, indent=2) + "\n")
    verify(output)


if __name__ == "__main__":
    main()
