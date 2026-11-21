#!/usr/bin/env python3
"""Repeat the native materials probe and preserve comparable raw measurements."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time

TIMINGS = ("submit_median_us", "submit_p95_us", "completed_median_us", "completed_p95_us")
COUNTERS = ("captures", "cache_hits", "cache_misses", "blur_passes", "draw_calls", "fallbacks", "evictions")


def validate_record(record):
    if not isinstance(record, dict) or record.get("schema_version") != 1:
        raise ValueError("expected materials benchmark schema_version 1")
    if not isinstance(record.get("renderer"), str) or not record["renderer"]:
        raise ValueError("benchmark must identify its actual graphics renderer")
    rows = record.get("results")
    if not isinstance(rows, list) or not rows:
        raise ValueError("benchmark returned no measured scenarios")
    keys = set()
    for row in rows:
        key = row.get("material"), row.get("case")
        if not isinstance(key[0], str) or key[1] not in ("cached", "invalidated") or key in keys:
            raise ValueError("benchmark contains invalid or duplicate scenario identities")
        keys.add(key)
        for field in TIMINGS:
            value = row.get(field)
            if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                raise ValueError(f"invalid timing field {field}")
        for field in ("texture_bytes", "peak_texture_bytes"):
            if type(row.get(field)) is not int or row[field] < 0:
                raise ValueError(f"invalid storage field {field}")
        counters = row.get("stats_delta", {})
        for field in COUNTERS:
            if type(counters.get(field)) is not int or counters[field] < 0:
                raise ValueError(f"missing or invalid counter {field}")
        if row["texture_bytes"] > row["peak_texture_bytes"]:
            raise ValueError("live effect storage exceeds reported peak storage")
    if any((name, "cached") not in keys or (name, "invalidated") not in keys for name, _ in keys):
        raise ValueError("each material needs both cached and invalidated measurements")
    return record


def summarize(records):
    baseline = records[0]
    identity = ("renderer", "quality", "iterations", "warmup", "capabilities", "size", "clock")
    for record in records[1:]:
        if any(record.get(field) != baseline.get(field) for field in identity):
            raise ValueError("renderer, capability or workload settings changed between repetitions")
    groups = {}
    for record in records:
        seen = set()
        for row in record["results"]:
            key = row["material"], row["case"]
            groups.setdefault(key, []).append(row)
            seen.add(key)
        if seen != {(row["material"], row["case"]) for row in baseline["results"]}:
            raise ValueError("measured scenarios changed between repetitions")
    rows = []
    for (material, case), samples in groups.items():
        rows.append({
            "material": material, "case": case, "repetitions": len(samples),
            **{field: statistics.median(row[field] for row in samples) for field in TIMINGS},
            "texture_bytes_max": max(row["texture_bytes"] for row in samples),
            "peak_texture_bytes_max": max(row["peak_texture_bytes"] for row in samples),
            "fallback_repetitions": sum(row["stats_delta"]["fallbacks"] > 0 for row in samples),
            "stats_delta_by_repetition": [row["stats_delta"] for row in samples],
        })
    return {field: baseline.get(field) for field in identity} | {"results": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path, help="Built materials example; build time is excluded")
    parser.add_argument("--output", required=True, type=Path, help="New directory outside the Git checkout")
    parser.add_argument("--iterations", type=int, default=120)
    parser.add_argument("--warmup", type=int, default=12)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--quality", choices=("disabled", "low", "balanced", "high"), default="balanced")
    parser.add_argument("--timeout", type=int, default=300, help="Maximum seconds for each bounded probe process")
    parser.add_argument("--label", default="", help="Machine/driver/power-mode annotation; not a performance claim")
    args = parser.parse_args()
    if not (1 <= args.iterations <= 2000 and 0 <= args.warmup <= 200
            and 1 <= args.repeats <= 20 and 1 <= args.timeout <= 3600):
        parser.error("iterations 1..2000, warmup 0..200, repeats 1..20, timeout 1..3600")
    binary = args.binary.resolve(strict=True)
    if not binary.is_file():
        parser.error("--binary must name an executable file")
    output = args.output.resolve()
    repository = Path(__file__).resolve().parents[1]
    if output.is_relative_to(repository):
        parser.error("--output must be outside the repository so measurements do not enter source history")
    output.mkdir(parents=True, exist_ok=False)
    command = [str(binary), "--bench", "--iterations", str(args.iterations), "--warmup", str(args.warmup),
               "--quality", args.quality]
    run_metadata = {
        "schema_version": 1, "started_utc": datetime.now(timezone.utc).isoformat(),
        "machine": platform.machine(), "platform": platform.platform(), "label": args.label,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "command": command,
        "repeats": args.repeats,
        "interpretation": "CPU submit and synchronized frame wall time; not GPU timestamp duration or whole-process memory",
    }
    (output / "run.json").write_text(json.dumps(run_metadata, indent=2) + "\n", encoding="utf-8")
    records = []
    try:
        for repeat in range(args.repeats):
            print(f"Measuring repetition {repeat + 1}/{args.repeats}", file=sys.stderr, flush=True)
            start = time.monotonic()
            result = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout, check=False)
            (output / f"run-{repeat + 1:02d}.stderr.txt").write_text(result.stderr, encoding="utf-8")
            (output / f"run-{repeat + 1:02d}.json").write_text(result.stdout, encoding="utf-8")
            if result.returncode:
                raise RuntimeError(f"probe exited {result.returncode}; see saved stderr")
            record = validate_record(json.loads(result.stdout))
            record["process_wall_seconds"] = time.monotonic() - start
            records.append(record)
        report = {"schema_version": 1, "run": run_metadata, "summary": summarize(records)}
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        for row in report["summary"]["results"]:
            print(f"{row['material']:16} {row['case']:12} "
                  f"submit {row['submit_median_us']:9.2f} us  completed {row['completed_median_us']:9.2f} us  "
                  f"effects {row['peak_texture_bytes_max'] / 1048576:6.2f} MiB  "
                  f"fallback runs {row['fallback_repetitions']}/{row['repetitions']}")
        print(f"Raw measurements and summary: {output}")
    except (ValueError, OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"Materials measurement failed: {error}\nPartial evidence remains in {output}\n")


if __name__ == "__main__":
    main()
