#!/usr/bin/env python3
"""Validate a release tag against Cargo metadata before any release work."""

import argparse
import os
from pathlib import Path
import tomllib


def metadata(manifest: Path, ref_type: str, tag: str) -> tuple[str, str]:
    if ref_type != "tag":
        raise ValueError("select a version tag when dispatching the release workflow")
    package = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
    name, version = package["name"], package["version"]
    if not isinstance(name, str) or not isinstance(version, str):
        raise ValueError("package name and version must be explicit strings")
    if any(c in name + version for c in "\r\n"):
        raise ValueError("package name and version must be single-line values")
    if tag != f"v{version}":
        raise ValueError(f"tag {tag!r} does not match manifest version v{version}")
    if package.get("publish") is False:
        raise ValueError("the package disables registry publication")
    return name, version


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path("Cargo.toml"))
    parser.add_argument("--ref-type", default=os.environ.get("GITHUB_REF_TYPE", ""))
    parser.add_argument("--tag", default=os.environ.get("GITHUB_REF_NAME", ""))
    args = parser.parse_args()
    try:
        name, version = metadata(args.manifest, args.ref_type, args.tag)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a", encoding="utf-8") as stream:
            stream.write(f"name={name}\nversion={version}\n")
    print(f"Validated {name} {version} at tag {args.tag}")


if __name__ == "__main__":
    main()
