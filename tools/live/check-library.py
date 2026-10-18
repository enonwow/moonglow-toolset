#!/usr/bin/env python3
"""CI guard: the Live runtime must not pull in rendering crates."""
import re
import subprocess

result = subprocess.run(
    ["cargo", "+1.98.0", "tree", "-p", "mg-live", "-e", "normal", "--prefix", "none"],
    check=True, capture_output=True, text=True,
)
forbidden = {"wgpu", "mg-render", "mg-preview", "mg-area"}
found = set(re.findall(r"^([^ ]+) v", result.stdout, re.MULTILINE)) & forbidden
if found:
    raise SystemExit(f"Live runtime has rendering dependencies: {sorted(found)}")
print("Live runtime dependency guard passed")
