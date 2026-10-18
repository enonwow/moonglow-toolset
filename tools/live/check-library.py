#!/usr/bin/env python3
"""CI guard for the Live dependency boundary and optional build feature.

Use --check-features to also type-check all targets in both feature variants.
"""
import argparse
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check-features", action="store_true")
args = parser.parse_args()
for features in [[], ["--features", "build"]]:
    result = subprocess.run(
        ["cargo", "+1.98.0", "tree", "-p", "mg-live", "-e", "normal",
         "--no-default-features", "--prefix", "none", *features],
        check=True, capture_output=True, text=True,
    )
    forbidden = {"wgpu", "mg-render", "mg-preview", "mg-area"}
    if not features:
        forbidden |= {"mg-module", "mg-resman", "mg-tlk", "zip"}
    found = set(re.findall(r"^([^ ]+) v", result.stdout, re.MULTILINE)) & forbidden
    if found:
        raise SystemExit(f"Live runtime has unexpected dependencies: {sorted(found)}")
    if args.check_features:
        subprocess.run(
            ["cargo", "+1.98.0", "check", "-p", "mg-live", "--all-targets",
             "--no-default-features", "-j", "2", *features], check=True,
        )
print("Live library dependency/feature guard passed")
