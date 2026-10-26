# Concurrency test cases

Each case needs an outer test timeout so a regression fails instead of hanging.

| Area | Case to test |
|---|---|
| Engine owner task | Many callers send frames at once: replies match requests, order is preserved, the queue stays bounded. Drop a caller mid-request: the owner survives and the next frame still works. |
| Refused frame | A refusal arrives before mutation: no setpoint is written, the degraded event is published, the next valid frame is accepted after re-prepare. |
| SSE lag | A slow client falls behind: trend samples are dropped, refusal and degraded events still arrive, `seq` shows the gap, and a fast client is not slowed. |
| SSE disconnect | Closing the client ends its task and releases its channel slot; a reconnect with `Last-Event-ID` resumes or gets a fresh snapshot. |
| Agent deadline | The whole repair loop stops at the absolute deadline and at the attempt cap; a stalled stream trips the per-chunk timeout; a 429 waits at least `Retry-After`. |
| BOPTEST client | A timeout on `/advance` is not retried; the next call re-reads state. Two concurrent steps for one test case are impossible by construction. |
| Cancellation | Cancel before admission, mid-I/O and after completion; completion racing a deadline yields exactly one outcome and releases permits. |
| Spawned tasks | Drop the caller of an operation blocked on a controlled event and observe whether the child still runs; assert permit recovery and the join outcome, not just that the timeout returned. A panicking task is observed and reported. |
| Shutdown | Admit work, begin shutdown, try to admit more (rejected), finish some work, hold other work past the deadline, then assert tasks and resources are cleaned up and BACnet `stop()` ran. |

A `JoinHandle` does not cancel on drop, a `JoinSet` aborts its tasks on drop, and `spawn_blocking` work needs a cooperative stop flag. Check these against the tokio version in `Cargo.lock`.

<!-- Adapted from yet-more-skills/rust-async-concurrency (MIT OR Apache-2.0) -->
