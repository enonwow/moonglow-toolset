"""Compact T1-T3 summaries; no raw-event expansion or third-party dependencies."""
import argparse
import json
from pathlib import Path


def slope(samples, field):
    """OLS MiB/min after minute five; observed drift, not a leak diagnosis."""
    points = [(r["elapsed_s"] / 60, r["memory"][field] / 2**20)
              for r in samples if r["elapsed_s"] >= 300]
    if len(points) < 3:
        return None
    x = sum(p[0] for p in points) / len(points)
    y = sum(p[1] for p in points) / len(points)
    denominator = sum((a - x)**2 for a, _ in points)
    return sum((a - x) * (b - y) for a, b in points) / denominator if denominator else None


def summarize(path):
    data = json.loads(path.read_text(encoding="utf-8-sig"))
    samples = [json.loads(line) for line in path.with_suffix(".minutes.jsonl")
               .read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    duration = data["duration_s"]
    counts = data["metrics"]["counts"]
    memory = {}
    for field in ("private_bytes", "working_set_bytes"):
        values = [s["memory"][field] / 2**20 for s in samples]
        memory[field] = dict(first_mib=values[0], last_mib=values[-1],
                             min_mib=min(values), max_mib=max(values),
                             slope_after_5min_mib_per_min=slope(samples, field))
    bounds = {}
    for label, get in {
        "objects": lambda s: s["core"]["objects"],
        "retained_events": lambda s: s["core"]["events"]["retained"],
        "seen_event_ids": lambda s: s["core"]["events"]["seen_game_sequences"],
        "queue": lambda s: s["core"]["queue"],
        "results": lambda s: s["core"]["results"],
        "drafts": lambda s: s["core"]["assistant"]["drafts"],
        "sending_drafts": lambda s: s["core"]["assistant"]["sending"],
        "memory_turns": lambda s: s["core"]["assistant"]["knowledge"]["memory"]["turns"],
        "cards": lambda s: s["core"]["assistant"]["knowledge"]["npc_cards"],
        "notes": lambda s: s["core"]["assistant"]["knowledge"]["notes"],
        "cached_looks": lambda s: sum(a["looks"]["player_looks"] for a in s["ui"]["areas"]),
        "preview_extras": lambda s: sum(a["looks"]["extras"] for a in s["ui"]["areas"]),
        "pictures": lambda s: s["ui"]["pictures"],
        "focus_rows": lambda s: sum(s["ui"]["focus_rows"]),
        "persisted_bytes": lambda s: s["files"]["logical_bytes"],
    }.items():
        values = [get(s) for s in samples]
        bounds[label] = dict(first=values[0], last=values[-1], maximum=max(values))
    return dict(source=str(path), duration_s=duration,
                cpu_one_core_percent=data["cpu_one_core_percent"],
                ui_hz=counts.get("frames", 0) / duration,
                summed_scene_redraw_hz=counts.get("pictures", 0) / duration,
                startup=data["startup"], memory=memory, bounds=bounds,
                stages=data["metrics"]["timings"],
                final_query_probe=data["last_sample"]["query_probe"],
                input=data["metadata"]["input"],
                visual_peaks={k:v for k,v in counts.items() if k.startswith("max:")},
                replay_cycles=counts.get("replay_cycles", 0),
                error=data["error"],
                note="Minute cache maxima may miss transient peaks. Private-byte slope is not exclusive Rust heap ownership or proof against all leaks.")


def prefix(path, minutes):
    """Matched complete minute intervals; never average quantiles into a p95."""
    rows = [json.loads(line) for line in path.with_suffix(".minutes.jsonl")
            .read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    rows = [r for r in rows if r["interval_s"] >= 59][:minutes]
    if len(rows) != minutes:
        raise ValueError(f"Need {minutes} complete minute intervals")
    seconds = sum(r["interval_s"] for r in rows)
    stages = {}
    for name in sorted({n for r in rows for n in r["metrics"]["timings"]}):
        values = [r["metrics"]["timings"][name] for r in rows
                  if name in r["metrics"]["timings"]]
        count = sum(v["n"] for v in values)
        stages[name] = dict(n=count,
                           mean_ms=sum(v["mean_ms"] * v["n"] for v in values) / count,
                           max_ms=max(v["max_ms"] for v in values),
                           minute_p95_range_ms=[min(v["p95_ms"] for v in values),
                                                max(v["p95_ms"] for v in values)])
    return dict(source=str(path), complete_minutes=minutes, interval_s=seconds,
                elapsed_s=rows[-1]["elapsed_s"],
                cpu_one_core_percent=sum(r["cpu_one_core_percent"] * r["interval_s"]
                                         for r in rows) / seconds,
                ui_hz=sum(r["metrics"]["counts"].get("frames", 0) for r in rows) / seconds,
                summed_scene_redraw_hz=sum(r["metrics"]["counts"].get("pictures", 0)
                                          for r in rows) / seconds,
                stages=stages,
                note="Exact weighted means from complete intervals; p95 range is per-minute, not a pooled percentile.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("result", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--first-minutes", type=int)
    args = parser.parse_args()
    result = prefix(args.result, args.first_minutes) if args.first_minutes else summarize(args.result)
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf8")
    else:
        print(text, end="")
