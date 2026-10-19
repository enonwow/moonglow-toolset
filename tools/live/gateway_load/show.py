"""Print the key numbers of one or more runs as a markdown table: python show.py LABEL [LABEL...]"""
import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parents[3] / "target" / "test-output" / "gateway-load"


def cell(d, *keys):
    for k in keys:
        d = d.get(k) if isinstance(d, dict) else None
    return "-" if d is None else d


def p(d):
    return "-" if not d or not d.get("n") else "%s / %s / %s" % (d["p50"], d["p95"], d["p99"])


def main(labels):
    print("| run | stage | area push ms p50/p95/p99 | chat event ms | command RTT ms | RTT minus game wait | agent wake ms | gw cores | RSS MB first-last, slope/min | redis cores |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for label in labels:
        s = json.loads((OUT / label / "summary.json").read_text())
        for st in s["stages"]:
            rss = st.get("gateway_rss_mb") or {}
            print("| %s | %s | %s | %s | %s | %s | %s | %s (max %s, throttled %s s/s) | %s -> %s, %s | %s |" % (
                label, st["name"], p(st["area_push_ms"]), p(st["chat_event_ms"]), p(st["command_rtt_ms"]),
                p(st["command_rtt_minus_game_wait_ms"]), p(st["agent_wake_ms"]), st["gateway_cpu_cores_mean"],
                st["gateway_cpu_cores_max_5s"], st["gateway_throttled_s_per_s"], rss.get("min", "-"), rss.get("last", "-"),
                rss.get("slope_mb_per_min", "-"), st["redis_cpu_cores_mean"]))
        c = s["commands"]
        print("\n%s: commands sent %s, final %s, unknown %s, unknown_local %s, rejected %s, lost %s, duplicate results %s, duplicate events %s; events %s; client lag max %.0f ms; log panics %s\n" % (
            label, c["sent"], c["final"], c["unknown"], c["unknown_local"], c["rejected"], c["lost"], c["duplicate_results"],
            c["duplicate_events"], s["events"], s["client_lag_max_ms"], s["gateway_log_errors"]))


main(sys.argv[1:])
