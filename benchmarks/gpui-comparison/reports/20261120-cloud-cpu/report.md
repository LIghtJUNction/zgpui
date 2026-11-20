# Fixed-metric layout measurements

Sources: zgpui `1d796d1fc63808dc099ed7b10c92e75835ae2814`, GPUI `a246423a3b24a08e0d0c8574eab3a9de422747e0` plus recorded visibility-only patch.

Linux CPU 0; 7 paired rounds; 5000 individual samples per process. No samples removed.

## Operation latency

GPUI/zgpui ratios above 1 mean the zgpui implementation took less time on this specific task. A 95% interval crossing 1 does not resolve a difference. These are per-case intervals, not simultaneous proof across every capability.

| Nodes | Task | zgpui p50/p95/p99 (ns) | GPUI p50/p95/p99 (ns) | GPUI/zgpui paired mean [95%] |
|---:|---|---:|---:|---:|
| 3 | fresh | 162/187/264 | 2426/2497/5712 | 14.00 [11.76, 16.27] |
| 3 | geometry-scan | 34/45/47 | 37/47/49 | 0.89 [0.54, 1.37] |
| 3 | resize | 116/132/157 | 1198/1229/1957 | 10.63 [9.80, 11.49] |
| 3 | retained | 116/140/187 | 198/218/298 | 1.68 [1.34, 2.18] |
| 21 | fresh | 1544/1658/2509 | 14729/17003/94380 | 8.66 [6.24, 10.54] |
| 21 | geometry-scan | 47/63/87 | 45/63/75 | 0.78 [0.58, 0.99] |
| 21 | resize | 927/962/1506 | 6203/6364/53899 | 6.31 [5.43, 7.11] |
| 21 | retained | 925/949/1489 | 1083/1126/1456 | 1.10 [0.97, 1.25] |
| 73 | fresh | 5623/5746/30073 | 62344/147592/373978 | 13.20 [11.80, 15.00] |
| 73 | geometry-scan | 99/159/201 | 92/145/173 | 0.78 [0.53, 1.08] |
| 73 | resize | 3734/3765/8045 | 20459/37466/126145 | 5.71 [5.30, 6.11] |
| 73 | retained | 3743/3791/29485 | 3285/3391/23071 | 0.79 [0.62, 0.94] |
| 241 | fresh | 19341/34223/109283 | 170802/349867/758105 | 8.41 [6.87, 9.44] |
| 241 | geometry-scan | 238/353/413 | 216/348/379 | 0.89 [0.81, 0.97] |
| 241 | resize | 13076/13846/85094 | 62770/146007/340020 | 4.97 [4.64, 5.34] |
| 241 | retained | 13088/15907/91008 | 10866/11637/73129 | 0.81 [0.76, 0.85] |

## CPU, throughput and memory

Batch CPU uses Linux `/proc/thread-self/schedstat` run time when available; it includes the common sampling bookkeeping. Allocation counters run in a separate executable. Process RSS includes the test context and shared executable, and is not the library's exclusive memory.

| Nodes | Task | Backend | Reported batch CPU ns/op | Ops/s | Alloc calls/op | Requested bytes/op | Peak child RSS KiB |
|---:|---|---|---:|---:|---:|---:|---:|
| 3 | fresh | gpui | 3192.6 | 343504 | 21.00 | 26572.0 | 67304 |
| 3 | fresh | zgpui | unresolved [5/7 zero deltas] | 4667468 | 5.00 | 277.0 | 67304 |
| 3 | geometry-scan | zgpui | unresolved [7/7 zero deltas] | 16778242 | 0.00 | 0.0 | 67304 |
| 3 | geometry-scan | gpui | unresolved [6/7 zero deltas] | 21431331 | 0.00 | 0.0 | 67304 |
| 3 | resize | zgpui | unresolved [6/7 zero deltas] | 7504464 | 1.00 | 72.0 | 67304 |
| 3 | resize | gpui | 1603.8 | 706629 | 7.00 | 2016.0 | 67304 |
| 3 | retained | gpui | unresolved [6/7 zero deltas] | 3872756 | 2.00 | 16.0 | 67304 |
| 3 | retained | zgpui | unresolved [6/7 zero deltas] | 6612476 | 1.00 | 72.0 | 67304 |
| 21 | fresh | zgpui | 2263.7 | 427210 | 23.00 | 2008.0 | 67304 |
| 21 | fresh | gpui | 18397.9 | 54115 | 54.00 | 80788.0 | 67304 |
| 21 | geometry-scan | zgpui | unresolved [6/7 zero deltas] | 14249341 | 0.00 | 0.0 | 67304 |
| 21 | geometry-scan | gpui | unresolved [7/7 zero deltas] | 19708572 | 0.00 | 0.0 | 67304 |
| 21 | resize | zgpui | 1297.6 | 789334 | 1.00 | 504.0 | 67304 |
| 21 | resize | gpui | 7997.1 | 126713 | 19.00 | 5208.0 | 67304 |
| 21 | retained | gpui | 1371.0 | 771308 | 5.00 | 160.0 | 67304 |
| 21 | retained | zgpui | 802.4 | 843061 | 1.00 | 504.0 | 67304 |
| 73 | fresh | zgpui | 7099.4 | 151357 | 75.00 | 7032.0 | 67304 |
| 73 | fresh | gpui | 83191.8 | 11365 | 94.00 | 405428.0 | 67304 |
| 73 | geometry-scan | zgpui | unresolved [5/7 zero deltas] | 6373086 | 0.00 | 0.0 | 67304 |
| 73 | geometry-scan | gpui | unresolved [7/7 zero deltas] | 8760100 | 0.00 | 0.0 | 67304 |
| 73 | resize | gpui | 24850.3 | 39991 | 35.00 | 27480.0 | 67304 |
| 73 | resize | zgpui | 3996.6 | 227898 | 1.00 | 1752.0 | 67304 |
| 73 | retained | zgpui | 4754.6 | 183020 | 1.00 | 1752.0 | 67304 |
| 73 | retained | gpui | 4000.3 | 241021 | 9.00 | 576.0 | 67304 |
| 241 | fresh | zgpui | 23188.3 | 38451 | 243.00 | 23293.0 | 67304 |
| 241 | fresh | gpui | 206786.3 | 4691 | 161.00 | 1022700.0 | 67304 |
| 241 | geometry-scan | zgpui | 412.7 [3/7 zero deltas] | 3393667 | 0.00 | 0.0 | 67304 |
| 241 | geometry-scan | gpui | unresolved [5/7 zero deltas] | 3862561 | 0.00 | 0.0 | 67304 |
| 241 | resize | gpui | 79247.0 | 12322 | 63.00 | 113184.0 | 67304 |
| 241 | resize | zgpui | 16798.4 | 61166 | 1.00 | 5784.0 | 67304 |
| 241 | retained | zgpui | 15999.3 | 60840 | 1.00 | 5784.0 | 67304 |
| 241 | retained | gpui | 13593.5 | 75620 | 16.00 | 1920.0 | 67304 |

## Counter quality and environment

The scheduler CPU counter returned a zero batch delta in 69 of 224 timing processes; 0 counter readings were unavailable. Zero deltas remain in the raw data and medians. An unresolved median is not a claim of zero CPU cost, and rounds with zero deltas are shown explicitly.

Raw child peak RSS ranged from 11904 to 124296 KiB. The per-round distribution below keeps process/collector effects visible; a repeated grouped median does not establish equal library memory use. This runner retains earlier samples in its parent process, and this gate does not isolate collector/fork effects from child RSS. Allocation-call and requested-byte counts come from the separate instrumented executable.

| Process round | Child RSS min / median / max (KiB) |
|---:|---:|
| 0 | 11904/13676/16364 |
| 1 | 30444/30444/30444 |
| 2 | 48788/48788/48788 |
| 3 | 67304/67304/67304 |
| 4 | 85760/85760/85760 |
| 5 | 104208/104208/104208 |
| 6 | 124296/124296/124296 |

The selected CPU's `/proc/stat` observation interval contained 348 steal ticks out of 2521 total observed ticks (13.80%). These are whole-interval counters, not individual-operation CPU time. The [Linux kernel field definitions](https://docs.kernel.org/filesystems/proc.html#miscellaneous-kernel-statistics-in-proc-stat) describe the steal field as involuntary wait. Paired process order and retained tails help describe this observed environment; the intervals do not certify performance on an idle physical machine.

## Scope

Fresh includes tree construction, layout, complete geometry reads and destruction. Retained preserves each framework's existing tree and leaves GPUI caching enabled. Resize alternates equal-share widths. Geometry-scan compares zgpui button geometry with an explicit linear adapter over GPUI Bounds; it is not GPUI input dispatch.

Native application startup, actual GPU frames, text shaping, glyph pixels, accessibility, platform input and layouts outside zgpui's current component/size limits are not established by these measurements. Fixture setup appears in summary.json as a harness diagnostic and must not be presented as native startup.

All per-operation samples and per-process load/context-switch records are retained next to this report. The paired interval resamples independent process rounds rather than treating every adjacent operation as an independent hardware experiment.
