#!/usr/bin/env python3
# SPDX-FileCopyrightText: Thomas Lothian
# SPDX-License-Identifier: GPL-3.0-or-later
"""Keep the locked Cargo dependency inventory and source-release SBOM current."""

import argparse
import json
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs" / "dependency-inventory.json"


def build_inventory() -> dict:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    if manifest["package"]["license"] != "GPL-3.0-or-later":
        raise ValueError("AVID Core package license must be GPL-3.0-or-later")
    if not (ROOT / "LICENSE").is_file():
        raise ValueError("The GPLv3 LICENSE file is missing")
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--locked", "--format-version", "1"],
            cwd=ROOT,
            text=True,
        )
    )
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    checksums = {(item["name"], item["version"]): item.get("checksum") for item in lock["package"]}
    packages = []
    for item in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if not item.get("license"):
            raise ValueError(f"Missing license metadata: {item['name']} {item['version']}")
        packages.append(
            {
                "name": item["name"],
                "version": item["version"],
                "license": item["license"],
                "source": item.get("source") or "this repository",
                "checksumSha256": checksums.get((item["name"], item["version"])),
            }
        )
    return {"schemaVersion": 1, "generatedFrom": "Cargo.lock and cargo metadata --locked", "packages": packages}


def cyclonedx(inventory: dict) -> dict:
    components = []
    for item in inventory["packages"]:
        component = {
            "type": "library",
            "name": item["name"],
            "version": item["version"],
            "purl": f"pkg:cargo/{item['name']}@{item['version']}",
            "licenses": [{"expression": item["license"]}],
        }
        if item["checksumSha256"]:
            component["hashes"] = [{"alg": "SHA-256", "content": item["checksumSha256"]}]
        components.append(component)
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {"component": {"type": "library", "name": "avid-core", "version": next(x["version"] for x in inventory["packages"] if x["name"] == "avid-core")}},
        "components": components,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true")
    group.add_argument("--write", action="store_true")
    group.add_argument("--sbom", type=Path)
    args = parser.parse_args()
    inventory = build_inventory()
    serialized = json.dumps(inventory, indent=2, sort_keys=True) + "\n"
    if args.check:
        if not INVENTORY.exists() or INVENTORY.read_text(encoding="utf-8") != serialized:
            raise SystemExit("Dependency inventory is stale; run script/dependency_inventory.py --write")
    elif args.write:
        INVENTORY.parent.mkdir(parents=True, exist_ok=True)
        INVENTORY.write_text(serialized, encoding="utf-8")
    else:
        args.sbom.parent.mkdir(parents=True, exist_ok=True)
        args.sbom.write_text(json.dumps(cyclonedx(inventory), indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
