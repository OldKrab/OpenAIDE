# Driver flight recorder

`scripts/driver-profiler.mjs` is a standalone Linux collector for the local Driver.
It attaches to the running Web Shell without restarting Driver and follows the
Driver systemd cgroup across restarts. Run it as a separate user service, outside
the Driver cgroup, using a stable checkout and Node 24 or later. Commands invoked
from linked Task worktrees resolve the primary checkout's capture directory.

Capture includes one-second process/thread CPU counters (with parent pids), resident memory, I/O
counters, thread states and kernel wait channels, machine resource pressure,
allowlisted App Server/Web Shell lifecycle logs with correlation identifiers,
and 100 Hz Web Shell JavaScript CPU samples in 30-second `.cpuprofile` segments.
It enables the Web Shell's Node inspector on loopback port 9229 and verifies the
target PID before starting CPU capture. The inspector exposes process control to
local users; keep that port local. Do not run another profiler on that inspector
while this collector owns its CPU capture.

The rolling window is ten minutes. Segments overlapping its boundary remain until
the next rotation; snapshots filter log/resource records to the exact window.
Each JSONL segment is capped at 8 MiB; capacity loss is reported in service logs
and `status.json`. Log bootstrap reads at most 1 MiB per source and reports omitted
history. Source log rotation is detected on the next sample; unconsumed records
in the old file can be lost at that boundary. Running capture saves CPU profiles
every 30 seconds; the snapshot command asks the collector to flush its current
profile first. Its manifest records whether that flush succeeded.

Artifacts live in ignored Driver state, under
`diagnostics/driver-profiler`, with private directory/file permissions. Capture
excludes raw protocol payloads, prompts, Chat, command lines, source URLs/paths,
and arbitrary error text. Existing product logs retain their independent rotation
policy; the collector does not change or delete them.

## Investigate a slow flow

Freeze evidence immediately before investigating so retention cannot erase it:

```bash
node scripts/driver-profiler.mjs snapshot
node scripts/driver-profiler.mjs status
systemctl --user status openaide-driver-profiler.service --no-pager
journalctl --user -u openaide-driver-profiler.service -n 30 --no-pager
```

The snapshot command prints the saved directory. Snapshots are retained outside
the rolling capture until explicitly removed. Check the manifest timestamp,
coverage flags, and service errors before claiming capture is complete. Follow
Task/request/connection identifiers through lifecycle records, then compare waits
with per-process CPU/I/O changes and Web Shell CPU samples at the same wall time.
CPU profile `captureStartedAt`/`captureEndedAt` supply wall-clock alignment. Standard
DevTools accepts these `.cpuprofile` files; source URLs are intentionally blank.

Coverage limits: native Rust/Agent stack sampling is unavailable with the current
unprivileged Linux perf policy; resource counters and product timings still cover
those processes. This collector cannot see rendering, browser main-thread stalls,
or network time before a request reaches the Driver. Browser-side profiling must
be configured in the affected browser separately. This capture cannot reconstruct
events that occurred before it started, except retained lifecycle log history.
