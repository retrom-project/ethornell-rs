"""Collect the selected browser graph, exact source locations and license texts."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
EXTRA = ROOT / ".github/rpg-runtime/licenses"


def dependency_graph():
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version=1",
        "--filter-platform", "wasm32-unknown-unknown"], cwd=ROOT))
    tree = subprocess.check_output([
        "cargo", "tree", "--locked", "-p", "ethornell-web", "--target",
        "wasm32-unknown-unknown", "--prefix", "none", "--format", "{p}"], cwd=ROOT).decode()
    selected = {tuple(line.removesuffix(" (*)").split()[:2]) for line in tree.splitlines()}
    return sorted((p for p in metadata["packages"] if (p["name"], "v" + p["version"]) in selected),
                  key=lambda p: (p["name"], p["version"]))


def write_notices(output, packages):
    sections = [(ROOT / "RETROM-NOTICES.md").read_text()]
    texts = {}
    sources = json.loads((EXTRA / "sources.json").read_text())
    for item in sources:
        data = (EXTRA / item["filename"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise ValueError("ETHORNELL_LICENSE_HASH_MISMATCH: " + item["filename"])
        texts["external/" + item["filename"]] = data.decode()
    sections.append("## Locked browser dependencies (normal and build graph)\n\n"
                    "These are selected build inputs, not a claim that host build tools are linked into Wasm.\n")
    for package in packages:
        folder = Path(package["manifest_path"]).parent
        label = package["name"] + " " + package["version"]
        local = package["source"] is None
        license_name = package["license"] or ("MPL-2.0; FFmpeg-derived portions LGPL-2.1-or-later"
                       if package["name"] == "na_mpeg2_decoder" else "MPL-2.0" if local else None)
        if not license_name:
            raise ValueError("ETHORNELL_LICENSE_UNDECLARED: " + label)
        source = (f'https://crates.io/api/v1/crates/{package["name"]}/{package["version"]}/download'
                  if (package["source"] or "").startswith("registry+") else package["source"] or "this source archive")
        sections.append(f"### {label}\n\nLicense: {license_name}\n\nSource: {source}\n")
        licenses = [p for p in folder.rglob("*") if p.is_file() and
                    p.name.upper().startswith(("LICENSE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")) and
                    not any(part in (".git", "node_modules", "target") for part in p.parts)]
        if package.get("license_file"):
            licenses.append(folder / package["license_file"])
        if not licenses:
            if local or license_name == "MPL-2.0":
                licenses = [ROOT / "LICENSE-MPL-2.0"]
            elif not any(item["filename"].startswith(package["name"] + "-") for item in sources):
                raise ValueError("ETHORNELL_LICENSE_TEXT_MISSING: " + label)
        for path in sorted(set(licenses)):
            data = path.read_text()
            key = next((key for key, value in texts.items() if value == data), None)
            if key is None:
                key = label + "/" + path.relative_to(folder).as_posix() if path.is_relative_to(folder) else path.name
                texts[key] = data
            sections.append("License text: " + key + "\n")
        if license_name == "MPL-2.0":
            sections.append("Copyright notices are retained in the accompanying complete dependency source.\n")
    for name, data in sorted(texts.items()):
        sections.append(f"## License text: {name}\n\n```text\n{data.rstrip()}\n```\n")
    output.write_text("\n".join(sections))
