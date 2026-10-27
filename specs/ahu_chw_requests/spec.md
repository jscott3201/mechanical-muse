# Air handler chilled-water requests

A sequence for one air handler that tells the chilled-water plant how hard it is working. It
sends reset requests (0 to 3), which feed the plant reset (`specs/chw_plant_reset`, summed over
the air handlers), and a plant request (0 or 1), which says the air handler needs the chillers.

Written in our own words from the behaviour of the Modelica Buildings Library's multizone air
handler plant-request block (`Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.mo`
at commit `a131864e`, the copy vendored by open-control-engine), read as elementary block
semantics. Only the chilled-water half is covered: the BOPTEST air handlers have no hot-water
coil. Nothing here is copied from the library or from any standard.

## Interface

| Point | Type | Meaning |
| --- | --- | --- |
| `TAirSup` | real input, K | Measured supply air temperature |
| `TAirSupSet` | real input, K | Supply air temperature setpoint |
| `uCooCoi` | real input, 0 to 1 | Cooling coil valve position |
| `yChiWatResReq` | integer output | Chilled-water reset requests, 0 to 3 |
| `yChiPlaReq` | integer output | Chiller plant request, 0 or 1 |

## Parameters

| Name | Value | Meaning |
| --- | --- | --- |
| 3-request error | 3 K | Supply air this far above setpoint, for long enough, sends 3 requests |
| 2-request error | 2 K | The same for 2 requests |
| temperature release band | 0.1 K | A temperature condition, once on, stays on until the error is no longer above its threshold minus this band |
| persistence | 120 s | How long a temperature condition must last before it counts |
| valve open | 0.95 | A valve above this sets the valve latches |
| valve reset | 0.85 | A valve below this clears the reset-request latch |
| valve closed | 0.10 | A valve below this clears the plant-request latch |
| valve release band | 0.05 | The valve conditions, once on, stay on until the valve crosses back past their threshold by this band |

## Behaviour

The temperature error is supply air minus setpoint (positive when the air is too warm).

1. **3 requests** when the error has been above 3 K for 120 s. Once on, the condition holds
   while the error stays above 2.9 K; at 2.9 K or less it ends at once.
2. **2 requests** when the error has been above 2 K for 120 s (holding while above 1.9 K).
3. **1 request** while the valve latch is set. It sets when the valve goes above 0.95 and
   clears when the valve goes below 0.85. Between those it keeps its last state, so a valve that
   eases from 0.97 to 0.88 keeps requesting, and one that rises from 0.84 to 0.93 does not.
4. **0** otherwise. The highest applicable count wins: 3 over 2 over 1.
5. **Plant request** 1 while a second latch is set: it sets with the valve above 0.95, as in 3,
   but clears only when the valve goes below 0.10.

Edge instants, stated in the task because the prose above leaves them open:
- The thresholds are strict: an error of exactly 3 K is not above 3 K; a valve at exactly 0.95
  does not set a latch; exactly 0.85 does not clear the reset latch; exactly 0.10 does not clear
  the plant latch.
- A temperature condition that drops out restarts its 120 s count when it returns.
- A temperature condition already met at the first frame counts as having lasted: the requests
  appear at once.
- The latches act when a condition turns on. If the valve jumps in one frame from below 0.85 to
  above 0.95, the clear condition has ended and the latch sets. A valve already above 0.95 at
  the first frame starts with both latches set.

## Deviations from the library block

- Chilled-water outputs only (no heating coil inputs or hot-water outputs).
- The library reads the commanded valve position; on BOPTEST the runtime feeds the measured
  valve position, which is what the plant exposes.
- Time is the engine's host tick (60 s steps in the vectors); inputs that change between frames
  take effect at the next frame.

## Validation (done before any model sees this spec)

- `reference.py` is an independent model of the behaviour (stdlib Python, no engine code). Its
  self-check compares it with checkpoints worked out by hand, and it writes `vectors.json`: 12
  scenarios covering steady state, 3 and 2 requests, exact thresholds and release bands, the
  persistence timer and its restart, a condition true at the start, the valve latch through
  partial closing and reopening, the latch boundaries, the plant request's separate clear, a
  clear and set in the same frame, and temperature requests overriding the valve request.
  Supply temperatures are chosen so every error is exact in binary floating point.
- `graph.json` is a hand-built block graph (18 blocks) mirroring the library block's structure.
  `crates/mm-bridge/tests/specs.rs` checks that it passes every scenario on the pinned engine,
  and that each of these mistakes fails at least one: wrong 3- or 2-request threshold, ">= 3 K"
  instead of "> 3 K", no release band on either temperature condition, 60 s persistence, the
  delay applied to a condition true at the start, 2 checked before 3, the valve request without
  the latch, the valve opening at 0.90, the reset latch clearing at 0.90, the plant latch
  clearing with the reset latch, the plant latch clearing at 0.15, and the error reversed.
