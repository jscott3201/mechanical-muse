# Results log

Every measured result and every adjustment made because of one, newest first, one line each.
Append as results come in; never edit an old line (add a correction below it instead). The
detail and the caveats live in `docs/runtime-results.md` (BOPTEST runs) and
`docs/benchmark-results.md` (model runs); records are in `fixtures/` or the gitignored `runs/`.

Changes are against BOPTEST's own controllers over the same window unless stated. "Window" is
BOPTEST simulation time; Spark is `muse-spark-1.3` unless stated.

| When | Kind | What | Result | Where |
| --- | --- | --- | --- | --- |
| 2026-10-28 | Feature | Muse Spark explains a run's events and its own verified drafts | fault demo: 5 event spans → 3 items, accurate; recorded copies committed | `fixtures/explanations/` |
| 2026-10-28 | Rehearsal | `make demo` hero flow, recorded model | works; fixed live-without-key, draft-to-run path, fault shading, axis labels | demo-script |
| 2026-10-27 | Feature | Muse Spark explains each reference graph in plain words | 4 graphs, 4–5 parts each, every named block exists | `fixtures/explanations/` |
| 2026-10-27 | BOPTEST | Static pressure v2 (airflow-based zone requests) + CHW, day 185 noon, 10 h | energy −24.1 %, thermal discomfort −42.1 %, CO2 discomfort +17.4 %; no fail-safe event | runtime-results; `fixtures/replays/day185-noon-pressure.jsonl` |
| 2026-10-27 | Spark | Zone requests v2, high, with vectors, 3 runs | first try 0/3 (all miss the exact 85 % damper release), after repair 3/3 | benchmark-results |
| 2026-10-27 | Adjustment | Zone requests v1 → v2: request from the airflow shortfall (90/70/50 % of setpoint for 120 s) | why: BOPTEST dampers 0.57 → 0.77 over 3 h at 50 Pa while airflow fell to 55 % | `specs/zone_pressure_requests/spec.md` |
| 2026-10-27 | Probe | BOPTEST zones at 50 Pa duct pressure (direct, no sequences) | airflow 55–71 % of setpoint, dampers ≤ 0.77, CO2 rising | runtime-results |
| 2026-10-27 | BOPTEST | Static pressure v1 + CHW, day 185 noon, 10 h | energy −33.6 %, thermal +8.9 %, CO2 +51.4 %; trimmed to 50 Pa, no zone ever asked | runtime-results |
| 2026-10-27 | Spark | Zone requests v1 / static pressure reset, 3 runs each | zone 2/3 first try, 3/3 after repair; reset 3/3 first try | benchmark-results |
| 2026-10-27 | BOPTEST | Baseline, day 185 noon, 10 h (Modal) | `ener_tot` 0.3255, same as the local baseline | `fixtures/replays/day185-noon-baseline.jsonl` |
| 2026-10-27 | Spark | Plant reset, live sessions with the loop rule in the prompt | 3/3 verified, 0 of 9 drafts looped | benchmark-results |
| 2026-10-27 | Adjustment | Prompt rule 9: name the two blocks that break a loop (UnitDelay, Pre) | every first draft had looped before it | `crates/mm-agent/src/prompt.rs` |
| 2026-10-27 | BOPTEST | Live Spark draft of the plant reset, day 185 noon, 10 h (Modal) | writes and KPIs identical to the hand-built reference: energy −4.7 % | runtime-results; `fixtures/replays/day185-noon-live-spark-draft.jsonl` |
| 2026-10-27 | Spark | Plant reset, live sessions with loop-aware repair messages | 3/3 verified (was 0/2 before the change) | benchmark-results |
| 2026-10-27 | Adjustment | Repair message names an algebraic loop as a block chain and the fix; shows inputs at a failing instant | 5 of 10 attempts had been refused as loops | `crates/mm-agent/src/feedback.rs` |
| 2026-10-27 | BOPTEST | Fault demo: floor 1 supply air NaN for 5 steps (Modal) | degraded, handed back to BOPTEST, recovered at step 25 | runtime-results; `fixtures/replays/day186-midnight-fault-demo.jsonl` |
| 2026-10-27 | Spark | Plant reset, benchmark, vector feedback, 3 runs | 2/3 after repair (was 1/12 with validator-only feedback) | benchmark-results |
| 2026-10-27 | BOPTEST | CHW reset with AHU requests, day 185 noon, 10 h (local) | energy −4.7 %, chiller energy −14.2 %, thermal +2.0 % | runtime-results |
| 2026-10-26 | Spark | Held-out library fault rules (37, tier A), validator-only feedback | 35–36 of 37 verified within 3 attempts; constant-false 0 | benchmark-results |
| 2026-10-26 | Spark | AHU chilled-water requests, 3 runs | 3/3 first try | benchmark-results |
| 2026-10-26 | BOPTEST | First live run: CHW reset with requests fixed at 0, 4 h (local) | energy −5.4 %, chiller energy −18 %, thermal no worse | runtime-results |
