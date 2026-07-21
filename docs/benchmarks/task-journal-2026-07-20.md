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

Environment: Linux 6.12.74, Rust/Cargo 1.96.0, `/tmp` on tmpfs. Cutover base
`4f13bf3`; measurements refreshed on 2026-07-21. Results are local engineering
measurements, not a hardware-independent performance contract. These results
are from the release run after durable-receipt latency and sync-call
instrumentation were added.

## Results

| Store/workload | Wall time | p50 | p95 | max | Physical frame bytes |
|---|---:|---:|---:|---:|---:|
| Legacy model, 16 updates, 4 MiB | 303.12 ms | 17.22 ms | 24.37 ms | 26.00 ms | 64.0 MiB |
| Legacy model, 64 updates, 4 MiB | 897.27 ms | 13.49 ms | 17.27 ms | 20.19 ms | 256.0 MiB |
| Legacy model, 256 updates, 4 MiB | 3,621.61 ms | 13.69 ms | 19.48 ms | 22.31 ms | 1.0 GiB |
| Journal, 10,002 updates, 64 KiB | 118.20 ms | 24.58 ms durable | 49.78 ms durable | 54.56 ms durable | 384,085 B |
| Journal, 10,002 updates, 1 MiB | 119.71 ms | 28.88 ms durable | 52.17 ms durable | 56.39 ms durable | 384,085 B |
| Journal, 10,002 updates, 4 MiB | 125.86 ms | 30.24 ms durable | 60.40 ms durable | 64.89 ms durable | 384,085 B |

The bounded legacy samples are linear at about 14–20 ms and 4 MiB serialized per
update after warmup. Extrapolating that measured slope to 10,002 updates gives
about 140–200 seconds and 39.1 GiB written. The complete 4 MiB journal case is
126 ms, roughly 1,100 times faster than that projection, while its append
frames add 384,085 bytes for 372,861 logical output bytes.

Every journal size produced 40 durable batches, 40 commit-publication events,
and 40 coalesced terminal-append events. The instrumented workload observed 82
durability sync calls: the new artifact directory is anchored and synced, then
each batch syncs artifact and Task-journal files. The final same-Task durability
barrier, deliberately submitted behind the complete flood, took 54.53 ms at 64
KiB, 56.34 ms at 1 MiB, and 64.82 ms at 4 MiB. Shutdown drain of an identical
flood took 83.10 ms, 71.90 ms, and 114.28 ms, with restart proving every admitted byte.
Separately, production Stop—including the durable `stopping` transition and
Agent cancellation dispatch—took 2.35–2.72 ms. Protocol publication is covered
by the runtime contract suite rather than this disabled-notifier timing fixture. Peak retained
same-Task/global queue payload stayed between 563,730 B and 629,778 B, below
the 2 MiB per-Task bound.

Compaction took 5.09 ms, 16.47 ms, and 51.70 ms respectively. Restart replay
after compaction took 43.06 ms, 52.68 ms, and 73.96 ms while also recovering 16
unrelated Tasks with 256 KiB histories. All 372,861 output bytes replayed
exactly once, and the Task revision remained 1. The 4 MiB state root used
8,790,808 bytes before compaction and 8,784,828 bytes afterward, including the
4 MiB unrelated-history fixture.

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
0.0043 ms p50 and 0.0057–0.0075 ms p95. “Physical frame bytes” is the exact retained
file-byte growth (or bytes passed to the legacy replacement writes), not a
kernel/block-device counter. Sync counts come from instrumentation around each
exercised durability call; individual sync-call latency is not separately
instrumented. Scheduler
tests separately prove bounded per-Task/global byte admission, reserved control
capacity, same-Task barrier ordering, shutdown unblocking, and round-robin
fairness.
