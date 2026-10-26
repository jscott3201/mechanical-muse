---
name: rust-async-concurrency
description: "Guides tokio lifecycles in mm-server, mm-agent and mm-runtime: cancellation, timeouts, bounded channels, SSE fan-out, shutdown. Use when touching async code or chasing hangs."
license: MIT OR Apache-2.0
metadata:
  short-description: "Tokio lifecycles, cancellation, SSE fan-out, shutdown"
---
# Async and Concurrency

Map the lifecycle before changing synchronization: who admits work, owns it, cancels it and observes completion, and what survives if the caller vanishes at each `.await`. A timeout is not a rollback; the peer may already have acted.

## Ownership rules for this workspace
- One owner task per `Engine` (it needs `&mut`). Other tasks reach it over a bounded `mpsc` with a `oneshot` reply; never hold `Arc<Mutex<Engine>>` across an await. Authoring and verification engines stay separate from the runtime engine.
- SSE fan-out in `mm-server`: bounded `broadcast` or per-client bounded queues with an explicit lag policy. On lag, drop only trend samples; never drop refusal, degraded or verification events. A closed client ends its stream task.
- BOPTEST `/advance` is not idempotent: one request in flight per test case, never auto-retried. After a timeout treat the outcome as unknown and re-read state first. One writer per BOPTEST container.
- Agent loop in `mm-agent`: one absolute deadline for the whole repair loop plus an attempt cap; per-chunk timeouts on streamed reqwest bodies (a single `.timeout()` caps the whole stream); retry only 429, 500 and 503, honoring `Retry-After` with jitter.
- Meta rate limits are per team and shared by every worker (Spark 3,000 RPM and 4M TPM; contributor 150 RPM). Keep the concurrency cap in one place, inside the `ModelClient` implementation.
- BACnet server: call `stop()` before dropping it. `Drop` is not async shutdown.

## Cancellation and scheduling
- Check every `select!` arm for cancel safety. Never race `prepare_frame` plus `execute_frame`, or a half-read HTTP body, against a cancel arm; select on whole units of work.
- Dropping a `JoinHandle` detaches its task, dropping a `JoinSet` aborts its tasks, and a started `spawn_blocking` task cannot be aborted. Keep an owner, cancel cooperatively and observe the join result.
- Keep `std` mutex guards short and never across `.await`; keep blocking and CPU work off the async path; keep a fixed lock order.
- Bound queues as well as active work (a semaphore does not bound the waiters) and define overload behavior.

## Shutdown and tests
- Seal admission, drain admitted work to a deadline, signal cancellation, join owned tasks, then stop BACnet and close the streams.
- Engine time is BOPTEST simulation seconds. Use `tokio::time::pause` only for wall-clock deadlines, and give every async test an outer timeout.
- Test cases: [concurrency cases](references/cases.md)

<!-- Adapted from yet-more-skills/rust-async-concurrency (MIT OR Apache-2.0) -->
