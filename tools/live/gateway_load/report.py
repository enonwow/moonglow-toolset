"""Summaries of a load run: percentiles, windows that skip warm-up and outages, CPU/RSS series."""
import math


def pct(values, p):
    """Nearest-rank percentile; None for an empty list."""
    if not values:
        return None
    v = sorted(values)
    return v[min(len(v) - 1, max(0, math.ceil(p / 100 * len(v)) - 1))]


def summary(values):
    if not values:
        return {"n": 0}
    return {"n": len(values), "p50": round(pct(values, 50), 1), "p95": round(pct(values, 95), 1),
            "p99": round(pct(values, 99), 1), "max": round(max(values), 1), "mean": round(sum(values) / len(values), 1)}


def inside(t, windows):
    return any(a <= t < b for a, b in windows)


def steady(rows, start, end, skip):
    """Sample rows [t, value, ...] inside [start, end) and outside every `skip` window."""
    return [r for r in rows if start <= r[0] < end and not inside(r[0], skip)]


def rates(series, key, skip, start, end):
    """Per-interval rate of a monotonic counter (reset-tolerant), skipping intervals in `skip`.

    series: list of dicts with "t" and the counter at series[i][key] (None when unavailable).
    Returns [(t_mid, rate_per_second)].
    """
    out = []
    for a, b in zip(series, series[1:]):
        if a.get(key) is None or b.get(key) is None or b["t"] <= a["t"]:
            continue
        if b["t"] < start or a["t"] >= end or inside(a["t"], skip) or inside(b["t"], skip):
            continue
        delta = b[key] - a[key]
        if delta < 0:  # the process restarted: its counter began again at zero
            delta = b[key]
        out.append(((a["t"] + b["t"]) / 2, delta / (b["t"] - a["t"])))
    return out


def slope(points):
    """Least-squares slope of (x, y) per x unit; 0 for fewer than two points."""
    n = len(points)
    if n < 2:
        return 0.0
    mx, my = sum(p[0] for p in points) / n, sum(p[1] for p in points) / n
    den = sum((p[0] - mx) ** 2 for p in points)
    return sum((p[0] - mx) * (p[1] - my) for p in points) / den if den else 0.0


def flat(values, tolerance=0.10):
    """True when the mean of the last fifth stays within +-tolerance of the mean of the first fifth."""
    if len(values) < 10:
        return None
    k = max(1, len(values) // 5)
    first, last = sum(values[:k]) / k, sum(values[-k:]) / k
    return abs(last - first) <= tolerance * first
