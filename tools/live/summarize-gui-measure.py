"""Validate native replay controls and print comparable measurements (no timing thresholds)."""
import argparse
import json
from pathlib import Path

PROFILES = ("debug", "devopt", "release")
CASES = ("static", "reports-only", "quiet", "reports", "idle", "moving", "effects-markers", "effects-full",
         "hidden", "minimized", "panel", "log", "chat-loaded", "log-loaded", "cues", "speech")


def read(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def validate(d):
    assert not d["heap_accounting"], "Allocation tracking is a separate experiment"
    assert d["duration_s"] >= 5 and d["cpu_one_core_percent"] >= 0
    assert d["ui"]["n"] == d["frames"]
    assert d["native_frames"] >= 0 and d["queued_commands"] == 0
    assert d["objects"] == 219 and d["crowd"] == 218
    case = d["case"]
    if case in ("static", "hidden", "minimized"):
        assert d["area_pictures"] == 0, f"{case} control unexpectedly redraws the area"
    if case == "hidden":
        assert d["report_reads"] == 0, "Hidden area still queries reports"
    if case == "minimized":
        assert d["minimized_os"], "Window was not minimized"
    if case == "effects-full":
        assert d["census"]["looks"]["effect_models"] == 16
    if case.endswith("-loaded"):
        assert d["census"]["event_query_count"] == 500
    if case == "cues":
        assert d["transient_max"] >= 1
    if case == "speech":
        assert d["speech_max"] >= 1
    if case not in ("static", "quiet", "hidden", "minimized"):
        assert d["report_reads"] == d["reports"], "Every injected report must be observed"
    assert d["binary_sha256"] and d["fixture_sha256"] and d["module_sha256"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2] / "target/test-output/live-measure")
    parser.add_argument("--partial", action="store_true", help="Print available samples without enforcing complete coverage")
    args = parser.parse_args()
    rows = {}
    for profile in PROFILES:
        for case in CASES:
            path = args.root / f"{profile}-{case}.json"
            if not path.exists() and args.partial:
                continue
            d = read(path)
            if not args.partial:
                validate(d)
                assert d["profile"].lower() == profile and d["case"] == case
            rows[profile, case] = d
    if not args.partial:
        for key in ("fixture_sha256", "module_sha256"):
            assert len({d[key] for d in rows.values()}) == 1, f"Mixed replay inputs: {key}"
        for profile in PROFILES:
            assert len({d["binary_sha256"] for (p, _), d in rows.items() if p == profile}) == 1, f"Mixed binaries: {profile}"
    print("Cells: CPU % of one core / UI passes/s / 3D redraws/s.\n")
    print("| Case | Debug | DevOpt | Release |")
    print("| --- | ---: | ---: | ---: |")
    for case in CASES:
        cells = []
        for profile in PROFILES:
            d = rows.get((profile, case))
            cells.append(f"{d['cpu_one_core_percent']:.2f}% / {d['fps']:.2f} / {d['area_picture_hz']:.2f}" if d else "pending")
        print(f"| {case} | " + " | ".join(cells) + " |")
    if not args.partial:
        print(f"\nValidated {len(rows)} native samples; no speed threshold imposed.")


if __name__ == "__main__":
    main()
