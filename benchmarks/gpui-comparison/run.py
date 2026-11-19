#!/usr/bin/env python3
"""Record randomized, paired, single-CPU samples without changing machine settings."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent


def write(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def text_file(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return None


def capture(arguments):
    result = subprocess.run(arguments, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def checksum(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def host_snapshot(cpu):
    root = Path(f"/sys/devices/system/cpu/cpu{cpu}")
    return {"load_average": os.getloadavg(), "monotonic_ns": time.monotonic_ns(),
            "cpu_stat": next((line for line in Path("/proc/stat").read_text().splitlines()
                              if line.startswith(f"cpu{cpu} ")), None),
            "frequency_khz": text_file(root / "cpufreq/scaling_cur_freq"),
            "governor": text_file(root / "cpufreq/scaling_governor"),
            "siblings": text_file(root / "topology/thread_siblings_list"),
            "core_type": text_file(root / "topology/core_type")}


def execute(binary, arguments, output, cpu, priority):
    command = ["nice", "-n", str(priority), "taskset", "--cpu-list", str(cpu), str(binary), *arguments]
    before = host_snapshot(cpu)
    started = time.perf_counter_ns()
    with output.open("w") as stream, output.with_suffix(".stderr").open("w") as error:
        process = subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=error)
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
    elapsed = time.perf_counter_ns() - started
    result = {"argv": command, "returncode": process.returncode, "elapsed_process_ns": elapsed,
              "process_cpu_seconds": usage.ru_utime + usage.ru_stime, "user_cpu_seconds": usage.ru_utime,
              "kernel_cpu_seconds": usage.ru_stime, "peak_process_rss_kib": usage.ru_maxrss,
              "voluntary_context_switches": usage.ru_nvcsw, "involuntary_context_switches": usage.ru_nivcsw,
              "before": before, "after": host_snapshot(cpu), "raw_file": output.name}
    if process.returncode:
        write(output.with_suffix(".failure.json"), result)
        raise RuntimeError(f"benchmark failed: {output}; {output.with_suffix('.stderr')}")
    result["measurement"] = json.loads(output.read_text())
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cpu", type=int, required=True)
    parser.add_argument("--rounds", type=int, default=7)
    parser.add_argument("--samples", type=int, default=5000)
    parser.add_argument("--seed", type=int, default=20261120)
    parser.add_argument("--nice", type=int, default=10)
    parser.add_argument("--binary", type=Path, default=ROOT / ".cache/bin/timing")
    parser.add_argument("--allocation-binary", type=Path, default=ROOT / ".cache/bin/allocations")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "linux" or args.cpu not in os.sched_getaffinity(0):
        raise SystemExit("this runner requires a permitted Linux CPU; the Rust fixture itself is portable")
    if args.rounds < 3 or args.samples < 100 or not 0 <= args.nice <= 19:
        raise SystemExit("use at least 3 paired rounds, 100 samples, and a normal/reduced scheduler priority")
    for tool in ("nice", "taskset"):
        if not shutil.which(tool):
            raise SystemExit(f"missing {tool}")
    for binary in (args.binary, args.allocation_binary):
        if not binary.is_file():
            raise SystemExit(f"missing prepared binary: {binary}")
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    sources = json.loads((ROOT / "sources.json").read_text())
    metadata = {"schema": 1, "created_at": datetime.now(timezone.utc).isoformat(), "sources": sources,
                "harness_commit": capture(["git", "rev-parse", "HEAD"]),
                "rustc": capture(["rustc", "+1.100.0", "--version", "--verbose"]),
                "uname": list(os.uname()), "cpuinfo": Path("/proc/cpuinfo").read_text(),
                "cpu": args.cpu, "nice": args.nice, "rounds": args.rounds, "samples": args.samples, "seed": args.seed,
                "lock_sha256": checksum(ROOT / "Cargo.lock"), "timing_binary_sha256": checksum(args.binary),
                "allocation_binary_sha256": checksum(args.allocation_binary),
                "scope": "Fixed-metric layout core; geometry-scan uses an explicit linear adapter for GPUI bounds. No window startup, text shaping, GPU pixels or event dispatch comparison.",
                "timing": "Individual operation wall latency; separate executable for allocation counting. Process CPU/RSS include fixture setup, warmup and JSON output.",
                "startup": "fixture_setup_ns is harness fixture setup, not a production application's native startup.",
                "features": "GPUI upstream default features plus test-support; no disabled production algorithms.",
                "host_start": host_snapshot(args.cpu)}
    metadata["scheduler_stats_enabled"] = text_file("/proc/sys/kernel/sched_schedstats")
    metadata["harness_files_sha256"] = {
        str(path.relative_to(ROOT)): checksum(path)
        for pattern in ("*.py", "*.toml", "src/*.rs")
        for path in sorted(ROOT.glob(pattern))
    }
    write(output / "metadata.json", metadata)
    for kind, binary in [("timing", args.binary), ("allocations", args.allocation_binary)]:
        verification = execute(binary, ["verify"], output / f"verify-{kind}.json", args.cpu, args.nice)
        if not verification["measurement"].get("verified"):
            raise RuntimeError("fixture parity verification did not pass")
    randomizer = random.Random(args.seed)
    cases = [(side, workload) for side in (1, 4, 8, 15)
             for workload in ("fresh", "retained", "resize", "geometry-scan")]
    records = []
    for round_number in range(args.rounds):
        randomizer.shuffle(cases)
        for side, workload in cases:
            backends = ["zgpui", "gpui"]
            randomizer.shuffle(backends)
            pair = []
            for backend in backends:
                filename = f"r{round_number:02}-{side:02}-{workload}-{backend}.json"
                result = execute(args.binary, [backend, workload, str(side), str(args.samples)],
                                 output / filename, args.cpu, args.nice)
                result["round"] = round_number
                pair.append(result)
            if pair[0]["measurement"]["checksum"] != pair[1]["measurement"]["checksum"]:
                raise RuntimeError(f"output parity changed during timing: {side}/{workload}")
            records.extend(pair)
        write(output / "runs.json", records)
        print(json.dumps({"completed_round": round_number + 1, "total_rounds": args.rounds}), flush=True)
    allocations = []
    for side, workload in cases:
        for backend in ("zgpui", "gpui"):
            filename = f"alloc-{side:02}-{workload}-{backend}.json"
            allocations.append(execute(args.allocation_binary, [backend, workload, str(side), "1024"],
                                       output / filename, args.cpu, args.nice))
    write(output / "allocations.json", allocations)
    metadata["host_end"] = host_snapshot(args.cpu)
    write(output / "metadata.json", metadata)
    print(json.dumps({"complete": True, "raw_directory": str(output), "paired_runs": len(records)}))


if __name__ == "__main__":
    main()
