# Task Journal Incident Benchmark — 2026-07-20

This benchmark models the recorded slow Driver workload: a Task with 4 MiB of
Chat receives exactly 10,002 Tool updates carrying 372,861 terminal-output bytes. The
legacy path models the observed normalization failure by upserting an unchanged
Tool row, which still reads, serializes, and atomically replaces the complete
message history. The journal path persists every terminal byte as typed append
data without changing the Task snapshot revision.

Run it with:

```sh
cargo test -p openaide-app-server --release --test task_storage_benchmark -- --ignored --nocapture
```

Environment: Linux 6.12.74, Rust/Cargo 1.96.0, `/tmp` on ext4. Cutover base
`4f13bf3`; measurements refreshed on 2026-07-22. Results are local engineering
measurements, not a hardware-independent performance contract. These results
are from the release run after durable-receipt latency and sync-call
instrumentation were added.

## Results

| Store/workload | Wall time | p50 | p95 | max | Physical frame bytes |
|---|---:|---:|---:|---:|---:|
| Legacy model, 16 updates, 4 MiB | 302.92 ms | 17.86 ms | 19.40 ms | 24.75 ms | 64.0 MiB |
| Legacy model, 64 updates, 4 MiB | 952.97 ms | 14.27 ms | 18.27 ms | 23.02 ms | 256.0 MiB |
| Legacy model, 256 updates, 4 MiB | 3,774.88 ms | 14.39 ms | 16.97 ms | 23.07 ms | 1.0 GiB |
| Journal, 10,002 updates, 64 KiB | 126.27 ms | 23.72 ms durable | 55.97 ms durable | 61.07 ms durable | 384,086 B |
| Journal, 10,002 updates, 1 MiB | 174.59 ms | 53.90 ms durable | 102.53 ms durable | 107.65 ms durable | 384,086 B |
| Journal, 10,002 updates, 4 MiB | 156.46 ms | 44.97 ms durable | 88.19 ms durable | 93.38 ms durable | 384,085 B |

The bounded legacy samples are linear at about 14–20 ms and 4 MiB serialized per
update after warmup. Extrapolating that measured slope to 10,002 updates gives
about 140–200 seconds and 39.1 GiB written. The complete 4 MiB journal case is
156 ms, roughly 900 times faster than that projection, while its append
frames add 384,085 bytes for 372,861 logical output bytes.

Every journal size produced 40 durable batches, 40 commit-publication events,
and 40 coalesced terminal-append events. The instrumented workload observed 82
durability sync calls: the new artifact directory is anchored and synced, then
each batch syncs artifact and Task-journal files. The final same-Task durability
barrier, deliberately submitted behind the complete flood, took 60.99 ms at 64
KiB, 107.56 ms at 1 MiB, and 93.29 ms at 4 MiB. Shutdown drain of an identical
flood took 116.33 ms, 98.56 ms, and 127.41 ms, with restart proving every admitted byte.
Separately, production Stop—including the durable `stopping` transition and
Agent cancellation dispatch—took 3.87–4.40 ms. Protocol publication is covered
by the runtime contract suite rather than this disabled-notifier timing fixture. Peak retained
same-Task/global queue payload stayed between 563,730 B and 794,898 B, below
the 2 MiB per-Task bound.

Compaction took 6.35 ms, 24.40 ms, and 54.09 ms respectively. Catalog-only
startup took 1.39 ms, 2.03 ms, and 1.69 ms while listing 16 unrelated Tasks with
256 KiB histories. First access to the selected Task then replayed only its Chat
in 0.51 ms, 13.59 ms, and 19.80 ms. First access to its Tool detail reconciled
only that artifact in 3.59 ms, 10.27 ms, and 3.92 ms. All 372,861 output bytes
replayed exactly once, and the Task revision remained 1. The 4 MiB state root
used 8,803,845 bytes before compaction and 8,797,865 bytes afterward, including
the 4 MiB unrelated-history fixture and catalog projections.

## Driver-State Startup Check

The production failure involved six Tasks, 8.49 MiB of Task journals, and 1,817
Tool-artifact journals totaling 78.98 MiB. The previous startup reached its
storage workers after 7,249 ms because it scanned every artifact. A bounded run
of the new binary against an isolated reflink copy of those exact bytes reached
the workers after 743 ms while creating the six missing catalogs. A second
startup against the same copy reached them after 6 ms. Both published handoff
within the existing five-second wrapper deadline; the real Driver root was not
opened by the benchmark.

## Target-State Startup Check

The real disposable Target contained 28 Tasks and 70.16 MiB of Task journals,
including individual 44.32 MiB and 15.47 MiB histories. Its first deployment
exposed a remaining eager path: restart recovery loaded every Task before
checking whether its catalog metadata required recovery, so the five-second web
handoff still timed out. After moving that eligibility check ahead of hydration,
the same Target completed web handoff in 65 ms. Opening the 44.32 MiB Task then
paid its replay cost on demand; the browser rendered its Chat and expanded a
saved Tool detail with no console errors or warnings.

## Interpretation And Limits

The comparison intentionally bounds the legacy run: running all 10,002 updates
would rewrite about 39 GiB to demonstrate an already-linear cost. The legacy
disk-size column is not bytes written because atomic replacement reclaims the
old file; estimated serialized bytes exposes that write amplification.

The legacy fixture is isolated from the cut-over `Store` facade and models the
old read, parse, serialize, sync, atomic-rename, and parent-sync cycle. The
benchmark includes JSON serialization, checksums, file syncs, scheduling, a
worst-position final barrier, an equivalent shutdown drain, production Stop,
compaction, exact replay,
publication counts, observed queue high-water marks, and directory-size
measurement at three Task sizes plus 16 unrelated historical Tasks. Journal
percentiles measure admission-to-durable-receipt latency; admission alone was
0.0043–0.0047 ms p50 and 0.0075–0.0085 ms p95. “Physical frame bytes” is the exact retained
file-byte growth (or bytes passed to the legacy replacement writes), not a
kernel/block-device counter. Sync counts come from instrumentation around each
exercised durability call; individual sync-call latency is not separately
instrumented. Scheduler
tests separately prove bounded per-Task/global byte admission, reserved control
capacity, same-Task barrier ordering, shutdown unblocking, and round-robin
fairness.
