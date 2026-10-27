# Runtime results

Live runs of verified sequences against BOPTEST. Each entry states the setup, what was seen, and
what it does not show. Run records (JSON) stay local in the gitignored `runs/`; the numbers
below are copied from them.

## 2026-10-27: a Muse Spark draft running the plant, on BOPTEST hosted on Modal

**Setup.** The plant reset graph that Muse Spark drafted in the recorded authoring session
(`fixtures/recordings/chw_plant_reset.json`: refused by the engine, then 8 of 10 scenarios, then
10 of 10; content id `cxf:fnv1a128:61dcdb7e…`) ran in place of our hand-built graph, with the
three verified air handler request sequences feeding it. Before loading it, `mm-server`
re-checked its interface, ran the validator and replayed its vectors again. Same window as the
reference run below: day 185 from 12:00, 300 steps of 120 s, one-day warm-up. BOPTEST v0.9.0
ran on Modal (native x86), reached from `mm-server` on a laptop.

**Result.** All 300 setpoint writes were identical to the hand-built reference graph's run of
the same window (the local run of this morning, record `fixtures/replays/day185-noon-requests.jsonl`),
and so were the BOPTEST KPIs (`ener_tot` 0.3101, `cost_tot` 0.0213, `tdis_tot` 0.1564). No
fail-safe event fired. Record: `fixtures/replays/day185-noon-spark-draft.jsonl`.

**What it does not show.** Identical behaviour on this window, where the draft and the reference
receive the same inputs; not equivalence in general (the vectors are the stronger evidence of
that, and they are conformance tests, not proof). The draft came from a recorded session: the
model call happened earlier the same day; the checks and the run were live.

**Fault, live.** A 60-step run from day 186 00:00 on the same draft, with floor 1's supply air
temperature replaced by NaN for steps 20 to 24 (an injected test fault). On each faulted step the
reading showed as no data, the floor 1 request sequence and the plant reset were degraded, and
both chilled-water setpoints were handed back to BOPTEST's own controllers; from step 25 the
reset wrote again. Record: `fixtures/replays/day186-midnight-fault-demo.jsonl`.

### BOPTEST on Modal: timings

BOPTEST's four services (web, worker, redis, minio) in one x86 container on Modal (4 cores,
16 GiB), reached through a Modal tunnel. Same test case and window as the local runs.

| Step | Local Docker on Apple silicon (x86 emulated) | Modal (native x86) |
| --- | --- | --- |
| select | not timed separately | 34 s |
| initialize, day 185 noon, one-day warm-up | 939 to 1,555 s | 170 s |
| one 120 s step (advance) | 0.84 to 9.4 s | 0.24 s, tunnel round trip included |

The first readings after initialize matched the local run to every digit, as did the whole
300-step run above. Cost at Modal's list prices is about $0.32 per hour for this size. Notes: the
minio release BOPTEST names is no longer published as a binary or public image, so the image
builds it from source at that release tag; the deployment files are not in the repository yet.

## 2026-10-27: plant reset with air handler requests, 10 hours

Setup as above, with the request side wired: three instances of the verified
`specs/ahu_chw_requests` sequence (one per floor's air handler) run before the plant reset each
tick, and the reset reads the sum of their requests. Each instance reads its floor's supply air
temperature, the supply air setpoint BOPTEST reports in effect (`floorN_TSupAirSet_u`), and the
measured cooling valve position. Window: day 185 (Monday) 12:00 to 22:00 after a 1-day warm-up,
300 steps of 120 s. Command: `MM_LIVE_STEPS=300 MM_LIVE_START=16027200 cargo test -p mm-runtime
--test live_boptest -- --ignored`, and again with `MM_LIVE_BASELINE=1`.

| Time | Requests (sum) | TSupSet (K) | Measured CHW supply (K) | dpSet (Pa) | Measured dp (Pa) | Valves (floors 1-3) |
| --- | --- | --- | --- | --- | --- | --- |
| 12:00 | 0 | 278.71 | 278.73 | 239,125 | 239,206 | 0.40, 0.40, 0.48 |
| 14:30 | 0 | 282.15 | 280.65 | 227,647 | 231,179 | 0.48, 0.48, 0.57 |
| 16:30 | 0 | 282.15 | 282.02 | 95,650 | 100,982 | 0.69, 0.69, 0.78 |
| 16:52 | 3 | 282.15 | 282.08 | 95,650 | 31,978 | 0.95, 0.95, 1.00 |
| 16:56 | 3 | 282.15 | 282.09 | 98,520 | 36,154 | 0.99, 0.98, 1.00 |
| 17:00 | 3 | 282.15 | 282.10 | 101,389 | 43,918 | 0.96, 0.95, 1.00 |
| 17:06 | 1 | 282.15 | 282.11 | 95,650 | 65,593 | 0.84, 0.84, 0.94 |
| 17:30 | 0 | 282.15 | 282.14 | 95,650 | 100,063 | 0.69, 0.69, 0.78 |
| 21:00 | 0 | 278.71 | 282.15 | 239,125 | 95,470 | 0.68, 0.69, 0.77 |

What it shows:

- The whole chain ran live for 300 steps with no fail-safe event: three request sequences and
  the reset, all loaded only from CXF with a passing replay receipt.
- The reset responded to requests. It trimmed through the afternoon (supply water at its warm
  limit by 14:30, pressure at its floor by 16:30). At about 16:50 the measured loop pressure fell
  well below its setpoint (to about 32 kPa; the cause inside BOPTEST was not investigated) and
  the cooling valves opened past 95 %. Each air handler's latch set, giving 3 requests; with 2
  ignored, the reset gained 1 % per update and the pressure setpoint rose in two steps. When the
  valves eased under 85 % the latches cleared and the reset trimmed back to its floor, where the
  plant then held, valves near 70 % and supply air on setpoint.
- At 21:00 occupancy ended, the plant enable dropped, and both setpoints returned to design at
  once, as the spec says.
- From 12:00 to 16:00 the requests stayed at 0 (supply air within 0.6 K of setpoint, valves
  under 72 %), so that part matches a run with the request count fixed at 0.

Against the baseline over the same 10 hours (the two runs agree to within 0.3 kW of chiller
power, out of about 520 kW, until the reset's first trim at 12:20):

| Over the 10 h window | Baseline | Plant reset with requests | Change |
| --- | --- | --- | --- |
| BOPTEST `ener_tot` (kWh/m², whole building) | 0.3255 | 0.3101 | −4.7% |
| BOPTEST `cost_tot` | 0.0227 | 0.0213 | −6.2% |
| BOPTEST `emis_tot` | 0.0930 | 0.0879 | −5.6% |
| BOPTEST `tdis_tot` (thermal discomfort, K·h/zone) | 0.1534 | 0.1564 | +2.0% |
| BOPTEST `idis_tot` (CO2 discomfort) | 1463.0 | 1463.0 | no change |
| Chiller energy (kWh, from 120 s power samples) | 4,710 | 4,042 | −14.2% |
| CHW pump energy (kWh, same method) | 2,300 | 2,352 | +2.2% |

What it does not show:

- One summer afternoon and evening. Thermal discomfort rose 2 % (about 0.003 K·h per zone over
  10 hours), a small cost that a longer comparison should track.
- The pressure half again saved no pump energy: warmer supply water needs more flow, and pump
  energy rose slightly. The saving is at the chillers.
- The large CO2 discomfort total comes from the case itself; it is the same in both runs.

## 2026-10-26: chilled-water plant reset, first live run

Setup: BOPTEST v0.9.0 (`compose.yaml`, local Docker on Apple silicon, x86 worker emulated),
`multizone_office_complex_air`. Start at 05:30 on day 185 (a Monday) after a 1-day warm-up
under baseline control; 120 steps of 120 s (4 simulated hours). The sequence is the
`specs/chw_plant_reset` reference graph, loaded only after its 10 vector scenarios replayed
green (content id checked at load). Bindings: request count constant 0 (the AHU request
sequence is not wired yet), plant enable = any floor's AHU occupancy flag, staging constant
false. Outputs clamped to 278.15..288.15 K and 0..478,250 Pa, fail-safe policy release.
Command: `MM_LIVE_STEPS=120 cargo test -p mm-runtime --test live_boptest -- --ignored`.

| Time | TSupSet (K) | Measured CHW supply (K) | dpSet (Pa) | Measured dp (Pa) | Chiller power (kW) | CHW pump power (kW) |
| --- | --- | --- | --- | --- | --- | --- |
| 05:30 | 278.71 | 282.28 | 239,125 | 67,185 | 1,082 | 300 |
| 05:54 | 278.85 | 282.18 | 239,125 | 267,017 | 1,559 | 448 |
| 06:30 | 279.95 | 280.81 | 239,125 | 246,229 | 1,043 | 270 |
| 07:06 | 280.91 | 280.60 | 239,125 | 243,486 | 617 | 255 |
| 07:54 | 282.15 | 281.17 | 239,125 | 238,877 | 433 | 255 |
| 08:30 | 282.15 | 281.69 | 193,213 | 201,237 | 439 | 248 |
| 09:28 | 282.15 | 282.03 | 130,084 | 136,239 | 462 | 238 |

What it shows:

- The loop ran all 120 steps with no fail-safe event (no stale data, refused frame, rejected or
  held value). Every tick wrote both setpoints with `_activate` = 1.
- The sequence behaved as its spec says on the live clock: setpoints held at design for the
  start delay plus one update period; the first trim (278.8476 K) is the same value the offline
  test and the spec vectors give; the supply temperature reached its warm limit after 25
  updates, and only then did the pressure setpoint start to fall.
- BOPTEST followed the writes: measured supply temperature and loop pressure track the
  setpoints with a lag of minutes.
- The floor AHU occupancy flag was already 1 at 05:30 (the case documentation gives 06:00 as
  the start of occupied hours; the AHUs start early). Day 184, the case's peak cooling day, is
  a Sunday and stays unoccupied in the morning.

Against the baseline (same window, no writes: `MM_LIVE_BASELINE=1`). BOPTEST is deterministic
here: both runs match to the decimal until the reset's first trim, so the differences below come
from the reset alone.

| Over the 4 h window | Baseline | Plant reset | Change |
| --- | --- | --- | --- |
| BOPTEST `ener_tot` (kWh/m², whole building) | 0.2911 | 0.2754 | −5.4% |
| BOPTEST `cost_tot` | 0.0171 | 0.0157 | −8% |
| BOPTEST `emis_tot` | 0.0758 | 0.0706 | −7% |
| BOPTEST `tdis_tot` (thermal discomfort, K·h/zone) | 0.6823 | 0.6812 | no worse |
| BOPTEST `idis_tot` (CO2 discomfort) | 7.646 | 7.659 | +0.2% |
| Chiller energy (kWh, from 120 s power samples) | 3,439 | 2,810 | −18% |
| CHW pump energy (kWh, same method) | 1,091 | 1,101 | +1% |

At 09:28 the baseline held 278.88 K and 239,495 Pa; the reset held 282.03 K and 136,239 Pa.

What it does not show:

- One 4-hour summer morning, not a season. The saving comes from warmer supply water at the
  chillers; the lower pressure setpoint did not reduce pump power here (pump power hardly moved
  in either run), so the pressure half of the reset shows no benefit yet.
- With zero requests the reset only trims, whatever the air handlers need. Comfort held over
  this window, but a hotter afternoon could need the response half. That needs the AHU request
  sequence wired as an upstream sequence, then the comparison repeated.

Speed: initialize took 939 to 1,555 s with a 1-day warm-up (over BOPTEST's default 20 min
worker-reply timeout, hence the compose override). Steps on this summer morning took
0.84 s each (101 s for 120 steps), against 9.4 s each in a January spike: step cost depends
strongly on the period simulated, so time a demo window before relying on it.
