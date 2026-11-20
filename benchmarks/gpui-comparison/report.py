#!/usr/bin/env python3
"""Summarize retained raw samples and draw a publication-ready vector figure."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import statistics


def quantile(values, probability):
    ordered = sorted(values)
    return ordered[max(0, min(len(ordered) - 1, math.ceil(probability * len(ordered)) - 1))]


def paired_interval(ratios):
    logarithms = [math.log(value) for value in ratios]
    randomizer = random.Random(20112026)
    samples = [math.exp(statistics.mean(randomizer.choices(logarithms, k=len(logarithms)))) for _ in range(5000)]
    return {"point": math.exp(statistics.mean(logarithms)), "lower_95": quantile(samples, 0.025),
            "upper_95": quantile(samples, 0.975), "method": "paired-round geometric mean ratio; bootstrap rounds, 5000 draws"}


def summarize(runs, allocation_runs):
    groups = {}
    for run in runs:
        measure = run["measurement"]
        key = (measure["rows"], measure["workload"])
        groups.setdefault(key, {}).setdefault(measure["backend"], []).append(run)
    allocations = {(run["measurement"]["rows"], run["measurement"]["workload"], run["measurement"]["backend"]):
                   run["measurement"] for run in allocation_runs}
    results = []
    for (side, workload), backends in sorted(groups.items()):
        if set(backends) != {"zgpui", "gpui"}:
            raise ValueError("every result needs both implementations")
        row = {"side": side, "leaves": side * side, "nodes": 1 + side + side * side, "workload": workload, "backends": {}}
        round_means = {}
        for backend, records in backends.items():
            samples = [value for run in records for value in run["measurement"]["samples_ns"]]
            means = {run["round"]: statistics.mean(run["measurement"]["samples_ns"]) for run in records}
            round_means[backend] = means
            cpu = [run["measurement"]["batch_thread_cpu_ns"] / len(run["measurement"]["samples_ns"])
                   for run in records if run["measurement"].get("batch_thread_cpu_ns") is not None]
            allocation = allocations[(side, workload, backend)]
            totals = allocation["allocation_totals"]
            allocation_count = len(allocation["samples_ns"])
            row["backends"][backend] = {
                "sample_count": len(samples), "round_count": len(records), "mean_ns": statistics.mean(samples),
                "p50_ns": quantile(samples, 0.5), "p95_ns": quantile(samples, 0.95), "p99_ns": quantile(samples, 0.99),
                "throughput_per_second": 1e9 / statistics.mean(samples),
                "median_batch_thread_cpu_ns_per_operation": statistics.median(cpu) if cpu else None,
                "batch_thread_cpu_available_rounds": len(cpu),
                "batch_thread_cpu_zero_delta_rounds": sum(
                    run["measurement"].get("batch_thread_cpu_ns") == 0 for run in records),
                "median_fixture_setup_ns": statistics.median(run["measurement"]["fixture_setup_ns"] for run in records),
                "median_peak_process_rss_kib": statistics.median(run["peak_process_rss_kib"] for run in records),
                "median_involuntary_context_switches": statistics.median(run["involuntary_context_switches"] for run in records),
                "allocations_per_operation": totals["allocations"] / allocation_count,
                "reallocations_per_operation": totals["reallocations"] / allocation_count,
                "requested_bytes_per_operation": totals["allocated_bytes"] / allocation_count,
                "peak_additional_requested_bytes": totals["peak_additional_requested_bytes"],
                "round_mean_ns": means,
            }
        if set(round_means["gpui"]) != set(round_means["zgpui"]):
            raise ValueError("the paired process rounds are incomplete")
        ratios = [round_means["gpui"][index] / round_means["zgpui"][index] for index in sorted(round_means["gpui"])]
        row["gpui_over_zgpui_time"] = paired_interval(ratios)
        results.append(row)
    return results


def measurement_quality(runs, metadata):
    cpu = [run["measurement"].get("batch_thread_cpu_ns") for run in runs]
    rss = [run["peak_process_rss_kib"] for run in runs]
    rounds = []
    for number in sorted({run["round"] for run in runs}):
        values = [run["peak_process_rss_kib"] for run in runs if run["round"] == number]
        rounds.append({"round": number, "minimum_kib": min(values),
                       "median_kib": statistics.median(values), "maximum_kib": max(values)})
    before = metadata.get("host_start", {}).get("cpu_stat")
    after = metadata.get("host_end", {}).get("cpu_stat")
    cpu_stat = None
    if before and after:
        first, last = before.split(), after.split()
        if first[0] == last[0] and len(first) >= 9 and len(last) >= 9:
            changes = [int(end) - int(start) for start, end in zip(first[1:9], last[1:9])]
            total = sum(changes)
            cpu_stat = {"cpu": first[0], "fields": ["user", "nice", "system", "idle", "iowait", "irq", "softirq", "steal"],
                        "deltas_ticks": changes, "total_ticks": total,
                        "steal_fraction": changes[7] / total if total > 0 and min(changes) >= 0 else None,
                        "scope": "Whole selected-CPU observation interval; not per-operation CPU time."}
    return {"timing_invocations": len(runs), "batch_cpu_unavailable_invocations": cpu.count(None),
            "batch_cpu_zero_delta_invocations": cpu.count(0),
            "rss_minimum_kib": min(rss), "rss_maximum_kib": max(rss), "rss_by_round": rounds,
            "selected_cpu_stat": cpu_stat,
            "reporter_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "limitations": ["A zero scheduler-counter delta does not establish zero CPU cost; all zero values are retained.",
                            "Child peak RSS includes process startup and fixture context; it does not isolate library-owned memory.",
                            "Paired intervals describe this run, not a noise-free machine or another platform."]}


def markdown(rows, metadata, quality):
    lines = ["# Fixed-metric layout measurements", "",
             f"Sources: zgpui `{metadata['sources']['zgpui_baseline_sha']}`, GPUI `{metadata['sources']['gpui_upstream_sha']}` plus recorded visibility-only patch.", "",
             f"Linux CPU {metadata['cpu']}; {metadata['rounds']} paired rounds; {metadata['samples']} individual samples per process. No samples removed.", "",
             "## Operation latency", "",
             "GPUI/zgpui ratios above 1 mean the zgpui implementation took less time on this specific task. A 95% interval crossing 1 does not resolve a difference. These are per-case intervals, not simultaneous proof across every capability.", "",
             "| Nodes | Task | zgpui p50/p95/p99 (ns) | GPUI p50/p95/p99 (ns) | GPUI/zgpui paired mean [95%] |",
             "|---:|---|---:|---:|---:|"]
    for row in rows:
        z, g, ratio = row["backends"]["zgpui"], row["backends"]["gpui"], row["gpui_over_zgpui_time"]
        lines.append(f"| {row['nodes']} | {row['workload']} | {z['p50_ns']:.0f}/{z['p95_ns']:.0f}/{z['p99_ns']:.0f} | {g['p50_ns']:.0f}/{g['p95_ns']:.0f}/{g['p99_ns']:.0f} | {ratio['point']:.2f} [{ratio['lower_95']:.2f}, {ratio['upper_95']:.2f}] |")
    lines += ["", "## CPU, throughput and memory", "",
              "Batch CPU uses Linux `/proc/thread-self/schedstat` run time when available; it includes the common sampling bookkeeping. Allocation counters run in a separate executable. Process RSS includes the test context and shared executable, and is not the library's exclusive memory.", "",
              "| Nodes | Task | Backend | Reported batch CPU ns/op | Ops/s | Alloc calls/op | Requested bytes/op | Peak child RSS KiB |",
              "|---:|---|---|---:|---:|---:|---:|---:|"]
    for row in rows:
        for backend, value in row["backends"].items():
            cpu = value["median_batch_thread_cpu_ns_per_operation"]
            zero = value["batch_thread_cpu_zero_delta_rounds"]
            cpu_text = "unavailable" if cpu is None else ("unresolved" if cpu == 0 else f"{cpu:.1f}")
            if zero:
                cpu_text += f" [{zero}/{value['round_count']} zero deltas]"
            lines.append(f"| {row['nodes']} | {row['workload']} | {backend} | {cpu_text} | {value['throughput_per_second']:.0f} | {value['allocations_per_operation']:.2f} | {value['requested_bytes_per_operation']:.1f} | {value['median_peak_process_rss_kib']:.0f} |")
    lines += ["", "## Counter quality and environment", "",
              f"The scheduler CPU counter returned a zero batch delta in {quality['batch_cpu_zero_delta_invocations']} of {quality['timing_invocations']} timing processes; {quality['batch_cpu_unavailable_invocations']} counter readings were unavailable. Zero deltas remain in the raw data and medians. An unresolved median is not a claim of zero CPU cost, and rounds with zero deltas are shown explicitly.", "",
              f"Raw child peak RSS ranged from {quality['rss_minimum_kib']} to {quality['rss_maximum_kib']} KiB. The per-round distribution below keeps process/collector effects visible; a repeated grouped median does not establish equal library memory use. This runner retains earlier samples in its parent process, and this gate does not isolate collector/fork effects from child RSS. Allocation-call and requested-byte counts come from the separate instrumented executable.", "",
              "| Process round | Child RSS min / median / max (KiB) |", "|---:|---:|"]
    for value in quality["rss_by_round"]:
        lines.append(f"| {value['round']} | {value['minimum_kib']}/{value['median_kib']:.0f}/{value['maximum_kib']} |")
    stat = quality["selected_cpu_stat"]
    if stat and stat["steal_fraction"] is not None:
        lines += ["", f"The selected CPU's `/proc/stat` observation interval contained {stat['deltas_ticks'][7]} steal ticks out of {stat['total_ticks']} total observed ticks ({stat['steal_fraction']:.2%}). These are whole-interval counters, not individual-operation CPU time. The [Linux kernel field definitions](https://docs.kernel.org/filesystems/proc.html#miscellaneous-kernel-statistics-in-proc-stat) describe the steal field as involuntary wait. Paired process order and retained tails help describe this observed environment; the intervals do not certify performance on an idle physical machine."]
    lines += ["", "## Scope", "",
              "Fresh includes tree construction, layout, complete geometry reads and destruction. Retained preserves each framework's existing tree and leaves GPUI caching enabled. Resize alternates equal-share widths. Geometry-scan compares zgpui button geometry with an explicit linear adapter over GPUI Bounds; it is not GPUI input dispatch.", "",
              "Native application startup, actual GPU frames, text shaping, glyph pixels, accessibility, platform input and layouts outside zgpui's current component/size limits are not established by these measurements. Fixture setup appears in summary.json as a harness diagnostic and must not be presented as native startup.", "",
              "All per-operation samples and per-process load/context-switch records are retained next to this report. The paired interval resamples independent process rounds rather than treating every adjacent operation as an independent hardware experiment.", ""]
    return "\n".join(lines)


def plot(rows, path):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 10, "axes.spines.top": False,
                         "axes.spines.right": False, "svg.fonttype": "none"})
    figure, axes = plt.subplots(2, 2, figsize=(12.5, 8), layout="constrained")
    for axis, workload in zip(axes.flat, ("fresh", "retained", "resize", "geometry-scan")):
        selected = sorted([row for row in rows if row["workload"] == workload], key=lambda row: row["nodes"])
        for backend, color, marker in (("zgpui", "#1a73e8", "o"), ("gpui", "#5f6368", "s")):
            axis.plot([row["nodes"] for row in selected], [row["backends"][backend]["p50_ns"] / 1000 for row in selected],
                      color=color, marker=marker, linewidth=1.8, label=backend)
            axis.plot([row["nodes"] for row in selected], [row["backends"][backend]["p99_ns"] / 1000 for row in selected],
                      color=color, linestyle=":", alpha=0.7, linewidth=1.3)
        axis.set_yscale("log")
        axis.set_title(workload + (" · host geometry adapter" if workload == "geometry-scan" else ""), loc="left")
        axis.set_xlabel("Total nodes")
        axis.set_ylabel("Operation wall time (µs, log scale)")
        axis.grid(axis="y", color="#e8eaed", linewidth=0.7)
        axis.legend(frameon=False)
    figure.suptitle("Fixed-metric layout core · solid p50 / dotted p99\nSame source-pinned binary, CPU and release profile · no GPU frame claim", fontsize=14)
    figure.savefig(path, metadata={"Creator": "zgpui / GPUI comparison; Matplotlib"})
    plt.close(figure)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--no-plot", action="store_true")
    args = parser.parse_args()
    metadata = json.loads((args.directory / "metadata.json").read_text())
    runs = json.loads((args.directory / "runs.json").read_text())
    results = summarize(runs,
                        json.loads((args.directory / "allocations.json").read_text()))
    quality = measurement_quality(runs, metadata)
    (args.directory / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    (args.directory / "quality.json").write_text(json.dumps(quality, indent=2) + "\n")
    (args.directory / "report.md").write_text(markdown(results, metadata, quality))
    if not args.no_plot:
        plot(results, args.directory / "comparison.svg")
    print(json.dumps({"summary": str(args.directory / "summary.json"), "report": str(args.directory / "report.md")}))


if __name__ == "__main__":
    main()
