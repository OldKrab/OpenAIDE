# Task Journal Incident Benchmark — 2026-07-20

This benchmark models the recorded slow Driver workload: a Task with 4 MiB of
Chat receives 10,000 Tool updates carrying 372,861 terminal-output bytes. The
legacy path models the observed normalization failure by upserting an unchanged
Tool row, which still reads, serializes, and atomically replaces the complete
message history. The journal path persists every terminal byte as typed append
data without changing the Task snapshot revision.

Run it with:

```sh
cargo test -p openaide-app-server --release --test task_storage_benchmark -- --ignored --nocapture
```

Environment: Linux 6.12.74, Rust/Cargo 1.96.0, `/tmp` on tmpfs. Base commit
`8d31a413abb62e867ae24f2fea1df249770cc80c`. Results are local engineering
measurements, not a hardware-independent performance contract.

## Results

| Store/workload | Wall time | p50 | p95 | max | Estimated serialized bytes |
|---|---:|---:|---:|---:|---:|
| Legacy, 16 updates | 156.72 ms | 9.22 ms | 10.92 ms | 16.25 ms | 64 MiB |
| Legacy, 64 updates | 593.20 ms | 9.17 ms | 9.85 ms | 10.23 ms | 256 MiB |
| Legacy, 256 updates | 2,328.34 ms | 9.06 ms | 9.29 ms | 10.25 ms | 1 GiB |
| Journal, 10,000 updates | 57.72 ms | 0.0042 ms admission | 0.0073 ms admission | 0.0433 ms admission | 373 KiB output |

The bounded legacy samples are linear at about 9.1 ms and 4 MiB serialized per
update. Extrapolating that measured slope to 10,000 updates gives about 91
seconds and 39.1 GiB serialized. The complete journal case is 57.72 ms, about
1,575 times faster than that time projection, and avoids roughly 100,000 times
the payload serialization.

The journal produced 40 durable batches. The final durability barrier took
0.20 ms, restart replay took 20.91 ms, all 372,861 output bytes replayed exactly
once, and the Task revision remained 1. Final on-disk size, including the 4 MiB
baseline, framing, Task journal, and artifact journal, was 4,579,434 bytes.

## Interpretation And Limits

The comparison intentionally bounds the legacy run: running all 10,000 updates
would rewrite about 39 GiB to demonstrate an already-linear cost. The legacy
disk-size column is not bytes written because atomic replacement reclaims the
old file; estimated serialized bytes exposes that write amplification.

The benchmark includes JSON serialization, checksums, file syncs, scheduling,
the final barrier, exact replay, and directory-size measurement. It does not yet
instrument kernel-level bytes written or individual sync-call latency, and its
barrier measurement occurs after one producer has submitted the flood. Scheduler
tests separately prove bounded per-Task/global data admission, reserved control
capacity, same-Task barrier ordering, and round-robin fairness. A later Target
soak should add concurrent cancellation and slow-filesystem latency distributions
before production cutover.
