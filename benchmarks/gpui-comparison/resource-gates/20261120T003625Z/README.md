# CPU baseline recovery stopped by the memory reserve gate

**Outcome: interrupted compilation; no geometry result and no performance
samples.** This record describes one bounded recovery attempt on
2026-11-20. It does not establish a benchmark winner or a completed GPUI build.

## Fixed input and result

| Item | Observation |
| --- | --- |
| CPU source checkpoint | `83369dbcea2ca7bca943a050dc2fa06869723f37` |
| Active branch checkpoint | `892fce4ceb152bc0b0564543930e1ebf08161e6e` |
| GPUI source | `a246423a3b24a08e0d0c8574eab3a9de422747e0` |
| zgpui library baseline | `1d796d1fc63808dc099ed7b10c92e75835ae2814` |
| Start | 2026-11-20 00:36:25 UTC |
| First sample below reserve | 2026-11-20 00:36:58.667031 UTC |
| Service runtime | 33.263 seconds |
| Unit memory peak | 848,125,952 bytes: about 808.8 MiB |
| Unit swap use | 0 bytes |
| Unit CPU time | 23.749 seconds |
| Global available memory at stop | 3,091,791,872 bytes: about 2.88 GiB |
| Geometry verification | Not reached |
| Performance sampling / GPU execution | Not started |

Checkpoint `892fce4` adds prepared headless source and its scope documentation.
It changes no CPU source or CPU lock. That headless module remains uncompiled;
this attempt used only the existing CPU fixture's `build.py --timing-only`.
The CPU lock SHA-256 stayed
`23dcd455e3326f9836ea9da927d3014376db6656e56228f55e6421b5ae7ff490`.

## Mechanism and observed protection

The fresh startup check saw 5,696,774,144 bytes of global `MemAvailable`
(about 5.31 GiB). Before invoking Cargo, the worker read its actual cgroup
controls and verified:

- `memory.high=1610612736`: 1.5 GiB.
- `memory.max=2147483648`: 2 GiB.
- `memory.swap.max=0`.
- `cpu.max=100000 100000`: one CPU quota.
- Niceness 19 and one Cargo job.

All build and temporary paths were on the development machine's `/home`
Btrfs filesystem. No build directory used the `/tmp` memory filesystem. The
existing cache was reused; compilation flags and the dependency lock were not
changed.

A supervisor sampled global available memory once per second and stopped only
this owned systemd unit when the first sample fell below the 3 GiB reserve.
The unit also had a 780-second lifetime limit. A sampling guard detects a
threshold crossing; it cannot prevent unrelated allocations between samples.

The final sample recorded no cgroup OOM event or OOM kill. The service was
stopped with SIGTERM through systemd. systemd reported the requested stop as
`Result=success` and its controller returned zero; **those values do not mean
that Cargo or the geometry gate succeeded**. The receipt explicitly records
`resource_guard_stopped` and no successful build.

At 00:38:00.867662 UTC, a read-only check found no process in the owned cgroup,
the cgroup no longer existed, and the unit was inactive/dead. Global available
memory was then 2,181,795,840 bytes (about 2.03 GiB). These measurements do not
identify which process or owner caused the global memory change.

The dependency cache increased from 526,172 KiB to 642,500 KiB, about 627.4 MiB,
and was retained. No timing executable or worker success record was produced.
There was no automatic restart and no new workflow.

## Evidence

- [Machine-readable receipt](receipt.json)
- [Startup observation](preflight.json)
- [Actual cgroup controls read inside the unit](actual-limits.json)
- [All resource samples](raw-samples.jsonl)
- [Native service and compiler log](unit-stderr.txt)
- [Original supervisor result](supervisor-result.json)
- [Stopped process/cgroup confirmation](stopped-confirmation.json)
- [Read-only cloud environment inventory](cloud-readonly.json)

The cloud inventory found no `rustc`, `cargo`, `rustup`, or `clang` in
PATH and no Cargo registry/source/checkouts cache. That workspace therefore
cannot directly resume this cached build. Provisioning and dependency transfer
would be separate work; neither was started, and cloud CPU measurements would
not be substituted for measurements on the development machine.

## Next execution gate

Wait for an explicitly coordinated idle window. Recheck global available
memory and pressure, verify a private hard-limited unit, and retain the reserve
watcher. Finish CPU compilation and the independent geometry oracle before
recording performance. High-contention resource samples in this directory are
operational evidence, not benchmark performance data. The separate GPU lock and
real rendering work remain later gates.
