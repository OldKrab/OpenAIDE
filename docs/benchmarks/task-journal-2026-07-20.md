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
measurements, not a hardware-independent performance contract. The table shows
the observed range across repeated valid release runs because host contention
changed wall latency while byte, batch, sync, revision, and event counts stayed
identical.

## Results

| Store/workload | Wall time | p50 | p95 | max | Physical frame bytes |
|---|---:|---:|---:|---:|---:|
| Legacy model, 16 updates, 4 MiB | 282.25–528.91 ms | 16.93–31.43 ms | 17.60–34.56 ms | 18.37–42.89 ms | 64.0 MiB |
| Legacy model, 64 updates, 4 MiB | 893.61–2,000.18 ms | 13.34–30.29 ms | 15.90–36.14 ms | 20.33–45.20 ms | 256.0 MiB |
| Legacy model, 256 updates, 4 MiB | 3,631.25–7,883.55 ms | 13.79–30.08 ms | 15.38–36.64 ms | 22.34–44.15 ms | 1.0 GiB |
| Journal, 10,002 updates, 64 KiB | 96.33–396.26 ms | 0.0043 ms admission | 0.0053–0.0061 ms admission | 0.0907–0.1284 ms admission | 384,085 B |
| Journal, 10,002 updates, 1 MiB | 93.56–400.15 ms | 0.0034–0.0044 ms admission | 0.0044–0.0058 ms admission | 0.0590–0.1281 ms admission | 384,085 B |
| Journal, 10,002 updates, 4 MiB | 123.87–447.95 ms | 0.0034–0.0043 ms admission | 0.0043–0.0057 ms admission | 0.0533–0.1525 ms admission | 384,085 B |

The bounded legacy samples are linear at about 14–30 ms and 4 MiB serialized per
update after warmup. Extrapolating that measured slope to 10,002 updates gives
about 140–300 seconds and 39.1 GiB written. The complete 4 MiB journal case is
124–448 ms, roughly 300–2,400 times faster than that projection, while its append
frames add 384,085 bytes for 372,861 logical output bytes.

Every journal size produced 40 durable batches, 40 commit-publication events,
and 40 coalesced terminal-append events. The workload performed 82 durability
sync calls: the new artifact directory is anchored and synced, then each batch
syncs artifact and Task-journal files. The final same-Task durability barrier,
deliberately submitted behind the complete flood, took 35.42–335.27 ms at 64 KiB,
32.70–340.90 ms at 1 MiB, and 77.86–388.72 ms at 4 MiB. Shutdown drain of an identical flood
took 77.23–366.82 ms, 74.85–371.15 ms, and 94.60–384.13 ms, with restart proving every admitted byte.
Separately, production Stop—including the durable `stopping` transition and
Agent cancellation dispatch—took 2.07–10.52 ms. Protocol publication is covered
by the runtime contract suite rather than this disabled-notifier timing fixture. Peak retained
same-Task/global queue payload stayed between 382,227 B and 1,126,645 B, below
the 2 MiB per-Task bound.

Compaction took 4.12–16.42 ms, 24.27–27.98 ms, and 50.46–74.28 ms respectively.
Restart replay after compaction took 21.74–53.28 ms, 26.59–30.94 ms, and
43.30–53.44 ms while also recovering 16
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
measurement at three Task sizes plus 16 unrelated historical Tasks. “Physical frame bytes” is the exact retained
file-byte growth (or bytes passed to the legacy replacement writes), not a
kernel/block-device counter. The sync count follows the exercised durability
calls; individual sync-call latency is not separately instrumented. Scheduler
tests separately prove bounded per-Task/global byte admission, reserved control
capacity, same-Task barrier ordering, shutdown unblocking, and round-robin
fairness.
