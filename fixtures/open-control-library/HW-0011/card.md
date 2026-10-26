---
schema: cxf-library/fault-card/v1
id: HW-0011
name: Hot-water temperature-control hunting
equipment: hw
status: verified
phase: 2
method: rule
severity: 3
category: EFFICIENCY_LOSS
confidence: MEDIUM
estimation_method: QUALITATIVE_ONLY
source:
  - "NIST, Automatically Detecting Faulty Regulation in HVAC Controls (2013), pp. 412 and 416-419 — oscillatory regulated/actuating variables, setpoint allowance, reset exclusions, and field-tuned parameters"
  - "LBNL Simulated Boiler Plant dataset inventory, PDF pp.4-8 — an explicitly faulted boiler-supply PI controller and one-minute HWS/setpoint/status/gas channels; no native normalized firing-rate percentage"
  - "Library precedent VFD-0004 — verified dual rolling-mean crossing-count plus rolling-deviation topology and its tick-coupled count contract"
g36: null
clusters: []
suppresses: []
suppressed_by: []
related: [HW-0001, HW-0010, HW-0012]
playbooks: [hot-water-plant-faults]
operating_states: "one boiler, or one semantically continuous capacity-weighted aggregate, in settled normal automatic temperature control"
preconditions: "Use actual normalized firing feedback where available. A firing command or rated-input-normalized gas-power signal is a disclosed proxy; never average percentages across unequal boilers. Boiler stage identity and count must remain stable for the full evaluation window. hws_temp and hws_temp_sp must be the same controlled loop/header in the same units, with a stable final active target. Exclude startup, purge/light-off, minimum-fire cycling that is normal for the appliance, setpoint reset, warm-up, lead/lag transfer, stage change, tuning tests, real load steps, and safety/current/fuel/demand/emissions limits, then restart the warm-up. The host must report NO_EVAL for at least the first evaluation_window after every valid-state entry. Use a fixed tick in [28.6 s, 300 s) at defaults, 60 s recommended; irregular or change-of-value sampling invalidates the crossing counts. Firing-rate and temperature signals must be fresh, aligned, and resolved below the configured materiality bands."
points:
  - boiler_firing_rate
  - hws_temp
  - hws_temp_sp
outputs:
  - name: yFault
    description: True after material firing-rate hunting and unstable temperature regulation overlap continuously for alarm_persistence
  - name: yFiringRateHunting
    description: Immediate diagnostic; true when rolling firing-rate mean crossings and rolling MAD both exceed their strict limits
  - name: yTemperatureUnstable
    description: Immediate diagnostic; true when temperature-error mean crossings and rolling MAE from setpoint both exceed their strict limits
params:
  evaluation_window:
    default: 1800.0
    unit: s
    description: "ADOPTED_TUNABLE 30-minute hydronic evidence window. All six rolling means must change together."
    cxf: [muFiring.delta, firingCrossRate.delta, firingMad.delta, muTemperatureError.delta, temperatureCrossRate.delta, temperatureMae.delta]
  max_crossings_per_window:
    default: 6.0
    unit: "1"
    description: "ADOPTED_TUNABLE strict maximum for both lanes. Six clears and seven faults; one physical cycle normally contributes two mean crossings."
    cxf: [firingCrossHigh.t, temperatureCrossHigh.t]
  min_firing_rate_mad:
    default: 7.5
    unit: "%"
    description: "ADOPTED_TUNABLE rolling mean absolute firing-rate deviation in percentage points. For a square wave only, 7.5 MAD points correspond to the brief's 15-point peak-to-peak excursion."
    cxf: firingAmplitudeHigh.t
  temperature_allowance:
    default: 1.5
    unit: K
    description: "ADOPTED_TUNABLE strict rolling mean absolute temperature error from setpoint, not an instantaneous +/-1.5 K band."
    cxf: temperatureBandHigh.t
  count_scale:
    default: 30.0
    unit: "1"
    description: "DERIVED as evaluation_window / fixed tick = 1800/60. Both event-count multipliers must be retuned together whenever window or cadence changes."
    cxf: [firingCrossCount.k, temperatureCrossCount.k]
  alarm_persistence:
    default: 300.0
    unit: s
    description: "ADOPTED_TUNABLE five-minute continuous overlap after the already 30-minute rolling evidence."
    cxf: persist.delayTime
energy_impact:
  affected_subsystem: Boiler modulation, hot-water temperature control, distribution, and burner/staging wear
  savings_range: "Context-dependent; oscillation can add fuel, distribution loss, purge/cycling loss, and mechanical wear"
  climate_sensitivity: heating-dominant
  runtime_estimation: "QUALITATIVE_ONLY. Instability hours do not determine avoidable fuel without a measured counterfactual and aligned fuel/load data."
emissions:
  scope: "1"
  method: QUALITATIVE_EMISSIONS
verified:
  engine_rev: 41e997f
  content_id: "cxf:fnv1a128:8ffe23a0fcbd7cff7187ccd1d65a5c7f"
  date: 2026-10-05
---

## Description

This rule looks for a plant that is repeatedly moving its heat input and its
controlled water temperature without settling. It requires both actuator-side
and process-side evidence, rejecting harmless modulation with stable water and
temperature disturbance behind a steady burner. The two diagnostic outputs
show which half is present before the combined finding matures.

## Detection Logic

```text
firing_mean  = MovingAverage(boiler_firing_rate, evaluation_window)
firing_count = MovingAverage(Change(boiler_firing_rate > firing_mean), window)
               * count_scale
firing_mad   = MovingAverage(abs(boiler_firing_rate - firing_mean), window)
yFiringRateHunting = firing_count > max_crossings_per_window
                   AND firing_mad > min_firing_rate_mad

temperature_error = hws_temp - hws_temp_sp
temperature_mean  = MovingAverage(temperature_error, evaluation_window)
temperature_count = MovingAverage(Change(temperature_error > temperature_mean), window)
                    * count_scale
temperature_mae   = MovingAverage(abs(temperature_error), window)
yTemperatureUnstable = temperature_count > max_crossings_per_window
                     AND temperature_mae > temperature_allowance

yFault = TrueDelay(yFiringRateHunting AND yTemperatureUnstable,
                   alarm_persistence)
```

Block graph (`rule.cxf.jsonld`):

![HW-0011 block graph](diagram.svg)

Each `Change` pulse is converted Boolean -> Integer -> Real before its moving
average. `count_scale` converts a one-tick pulse area back to an event count.
Diagnostics age out with their trailing windows; the final delay drops on the
tick either diagnostic clears and uses `delayOnInit=true`.

## Possible Diagnoses

1. Temperature-loop gain too high or integral time too short for plant delay.
2. Burner minimum-fire limit interacting with plant load or stage sequencing.
3. Noisy, quantized, biased, poorly placed, or intermittently stale HWS sensor.
4. Competing header, boiler-local, mixing-valve, and supervisory controllers.
5. Lead/lag transfers or an invalid aggregate firing signal admitted by host.
6. A real load/setpoint transition or safety/application limit not excluded.

## Energy Impact

The finding is qualitative. Hunting can add fuel and distribution loss through
overshoot, increase purge/light-off loss if modulation becomes cycling, and add
wear. The rule carries neither fuel nor useful-load data and cannot assign a
savings percentage from its rolling statistics.

## Emissions Impact

Scope-1 emissions may rise when hunting increases boiler fuel use, purge, or
light-off loss. The graph has no fuel channel, so it cannot quantify that change
or claim a portable emissions benefit.

## Deviations

- Mean crossings replace literal direction reversals, following VFD-0004 and
  VAV-0005. Periodic cycles normally produce two of either, but drift with
  ripple can differ; parameter names state the implemented statistic.
- The brief's 15-point excursion becomes 7.5 MAD points only for a square wave.
  A sine requires about 23.6 points peak-to-peak to reach MAD 7.5.
- `temperature_allowance` is rolling MAE, not repeated instantaneous exits
  from a +/-1.5 K band. A +/-1.5 K square is equality-clear; a sinusoid needs
  amplitude above about 2.36 K. The mean-crossing lane intentionally admits a
  one-sided oscillatory offset, pinned in vectors.
- This is an application of NIST regulation concepts, not its two-sided CUSUM.
- Event counts are tick-coupled. The 64-checkpoint ring requires
  `dt >= 1800/63 = 28.6 s`, while strict `count > 6` requires `dt < 300 s`.
  Sixty seconds is the exercised cadence; COV and sub-tick reversals can hide
  events.
- Partial-window divisors can extrapolate a warm-up burst, so the first 1800 s
  and every excluded discontinuity are host NO_EVAL despite raw outputs.
- No simulation FPR/TPR is claimed. EnergyPlus lacks realistic burner PI
  dynamics and no local LBNL dataset copy was available for a faithful replay.

## Notes

If only `yFiringRateHunting` is true, inspect modulation feedback, burner
limits, and staging before retuning. If only `yTemperatureUnstable` is true,
look for a sensor, load, flow, or competing controller. When both are true,
first prove the host did not admit a legitimate transition.
