#!/usr/bin/env python3
"""Format criterion benchmark results as a markdown table.

Usage:
    # Run benchmarks with JSON output and format:
    cargo criterion -p rpc-bench --bench rpc_bench --message-format=json 2>/dev/null | python3 benches/rpc/format-results.py

    # From a saved file:
    cargo criterion -p rpc-bench --bench rpc_bench --message-format=json 2>/dev/null > /tmp/bench.json
    python3 benches/rpc/format-results.py /tmp/bench.json

    # Filter to specific benchmarks:
    python3 benches/rpc/format-results.py /tmp/bench.json "unary/logs"

    # Or via task:
    task bench:fmt

Parses cargo-criterion's JSON output (--message-format=json) and produces a
clean markdown table with timing, throughput, and change-vs-baseline. Also
handles criterion's human-readable text output as a fallback.

Adapted from buffa's benchmarks/charts/generate.py.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


def parse_criterion_json(text: str) -> list[dict]:
    """Parse cargo-criterion JSON output (--message-format=json).

    Each line is a JSON object. We extract 'benchmark-complete' messages
    which contain the benchmark id, timing statistics, throughput, and
    change-vs-baseline.
    """
    results: list[dict] = []

    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        if msg.get("reason") != "benchmark-complete":
            continue

        entry: dict = {"name": msg["id"]}

        typical = msg.get("typical") or msg.get("slope") or {}
        if typical:
            ns = typical["estimate"]
            if ns >= 1_000_000:
                entry["time_median"] = ns / 1_000_000
                entry["time_unit"] = "ms"
            else:
                entry["time_median"] = ns / 1_000
                entry["time_unit"] = "µs"

        throughput_list = msg.get("throughput")
        if throughput_list and typical:
            tp = throughput_list[0]
            bytes_per_iter = tp["per_iteration"]
            ns_per_iter = typical["estimate"]
            if tp.get("unit") == "bytes":
                mib_per_s = bytes_per_iter / ns_per_iter * 1e9 / 1_048_576
                entry["throughput"] = mib_per_s
                entry["throughput_unit"] = "MiB/s"
            elif tp.get("unit") == "elements":
                elem_per_s = bytes_per_iter / ns_per_iter * 1e9
                if elem_per_s >= 1000:
                    entry["throughput"] = elem_per_s / 1000
                    entry["throughput_unit"] = "Kelem/s"
                else:
                    entry["throughput"] = elem_per_s
                    entry["throughput_unit"] = "elem/s"

        change = msg.get("change")
        if change:
            mean = change.get("mean") or change.get("median") or {}
            if mean:
                pct = (mean["estimate"] - 1) * 100
                entry["change"] = f"{pct:+.2f}%"
                ci = mean.get("confidence_interval")
                if ci:
                    lo = (ci["lower_bound"] - 1) * 100
                    hi = (ci["upper_bound"] - 1) * 100
                    # Significant if CI doesn't cross zero
                    entry["significant"] = (lo > 0 and hi > 0) or (lo < 0 and hi < 0)

        results.append(entry)

    return results


def parse_criterion_text(text: str) -> list[dict]:
    """Parse criterion's human-readable text output as fallback.

    Handles both inline names (name  time: [...]) and wrapped names
    (name on line 1, time: [...] on line 2).
    """
    results: list[dict] = []
    pending_name: str | None = None

    for line in text.splitlines():
        # Inline: "name  time: [low unit median unit high unit]"
        m = re.match(
            r"^(\S+)\s+time:\s+\[([\d.]+)\s+([µm]s)\s+([\d.]+)\s+[µm]s\s+([\d.]+)\s+[µm]s\]",
            line,
        )
        if m:
            pending_name = None
            results.append({
                "name": m.group(1),
                "time_median": float(m.group(4)),
                "time_unit": m.group(3),
            })
            continue

        # Wrapped name on its own line
        m = re.match(r"^(\S+/\S+)\s*$", line)
        if m:
            pending_name = m.group(1)
            continue

        # Continuation time: line after wrapped name
        if pending_name:
            m = re.match(
                r"^\s+time:\s+\[([\d.]+)\s+([µm]s)\s+([\d.]+)\s+[µm]s\s+([\d.]+)\s+[µm]s\]",
                line,
            )
            if m:
                results.append({
                    "name": pending_name,
                    "time_median": float(m.group(3)),
                    "time_unit": m.group(2),
                })
                pending_name = None
                continue
            # Not a time line — clear pending
            if line.strip():
                pending_name = None

        if not results:
            continue

        entry = results[-1]

        # Throughput
        m = re.search(
            r"thrpt:\s+\[[\d.]+ \S+\s+([\d.]+) (\S+)\s+[\d.]+ \S+\]", line
        )
        if m:
            val = float(m.group(1))
            unit = m.group(2)
            if unit == "GiB/s":
                val *= 1024
                unit = "MiB/s"
            entry["throughput"] = val
            entry["throughput_unit"] = unit

        # Change
        m = re.search(
            r"change:\s+\[[\S]+\s+([\S]+)\s+[\S]+\]\s+\(p\s*=\s*([\d.]+)", line
        )
        if m:
            entry["change"] = m.group(1)
            entry["significant"] = float(m.group(2)) < 0.05

    return results


def parse_auto(text: str) -> list[dict]:
    """Auto-detect format and parse."""
    for line in text.splitlines():
        stripped = line.strip()
        if stripped:
            if stripped.startswith("{"):
                return parse_criterion_json(text)
            return parse_criterion_text(text)
    return []


def format_table(results: list[dict], filter_pattern: str | None = None) -> str:
    """Format results as a markdown table."""
    if filter_pattern:
        results = [r for r in results if filter_pattern in r["name"]]

    if not results:
        return "No matching benchmarks found."

    has_throughput = any("throughput" in r for r in results)
    has_change = any("change" in r for r in results)

    cols = ["Benchmark", "Median"]
    if has_throughput:
        cols.append("Throughput")
    if has_change:
        cols.append("vs Baseline")

    header = "| " + " | ".join(cols) + " |"
    # Right-align numeric columns
    alignments = [":---"] + ["---:"] * (len(cols) - 1)
    sep = "| " + " | ".join(alignments) + " |"

    rows: list[str] = []
    for r in results:
        median = f"{r['time_median']:.2f} {r['time_unit']}"
        cells = [f"`{r['name']}`", median]

        if has_throughput:
            if "throughput" in r:
                cells.append(f"{r['throughput']:.1f} {r['throughput_unit']}")
            else:
                cells.append("—")

        if has_change:
            if "change" in r:
                change = r["change"]
                sig = r.get("significant", False)
                if sig:
                    try:
                        val = float(change.rstrip("%"))
                        if val < -2:
                            change += " ✅"
                        elif val > 2:
                            change += " ⚠️"
                    except ValueError:
                        pass
                cells.append(change)
            else:
                cells.append("—")

        rows.append("| " + " | ".join(cells) + " |")

    return header + "\n" + sep + "\n" + "\n".join(rows)


def main() -> None:
    if len(sys.argv) >= 2 and not sys.argv[1].startswith("-"):
        text = Path(sys.argv[1]).read_text()
    else:
        text = sys.stdin.read()

    filter_pattern = sys.argv[2] if len(sys.argv) >= 3 else None

    results = parse_auto(text)
    if not results:
        print("No benchmark results found in input.", file=sys.stderr)
        sys.exit(1)

    print(format_table(results, filter_pattern))


if __name__ == "__main__":
    main()
