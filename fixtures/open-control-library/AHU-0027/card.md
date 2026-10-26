---
schema: cxf-library/fault-card/v1
id: AHU-0027
name: Manual override detected on critical control point
equipment: ahu
status: verified
phase: 2
method: rule
severity: 4
category: EXCESS_CONSUMPTION
confidence: LOW
estimation_method: QUALITATIVE_ONLY
source:
  - "HVAC FDD Reference v1.0 §9, AHU-0027"
  - "PNNL RetuningOpps A30"
  - "PNNL-27338"
g36: null
clusters: []
suppresses: []
suppressed_by: []
related: [AHU-0018]
playbooks: [after-hours-operation]
operating_states: all
preconditions: "override_active is mapped from the BAS — a BACnet priority-array occupancy at priorities 1-8, or the vendor's operator-override flag — on a control point that matters. The host must ensure the flag reflects operator or manual overrides only; automation modes (optimal start, demand response, smoke control) writing at their own priorities are not overrides and must not raise it."
points:
  - override_active
outputs:
  - name: yFault
    description: True once override_active has been held continuously for max_override_duration plus alarm_delay
params:
  max_override_duration:
    default: 86400.0
    unit: s
    description: Continuous override duration beyond which the override counts as stale (24 h)
    cxf: staleOvr.delayTime
  alarm_delay:
    default: 3600.0
    unit: s
    description: Further persistence required after the duration limit before the alarm asserts
    cxf: persist.delayTime
energy_impact:
  affected_subsystem: Depends on the overridden point — a setpoint, damper, valve, or equipment enable
  savings_range: Context-dependent; roughly 10% of buildings carry at least one stale override
  climate_sensitivity: Depends on the overridden point
  runtime_estimation: "Qualitative — no per-fault model; use the Energy Impact Reference §4.4 framework once the overridden point is identified"
emissions:
  scope: "1|2"
  method: QUALITATIVE_EMISSIONS
verified:
  engine_rev: 41e997f
  content_id: "cxf:fnv1a128:3104e499f03189fbd6a10f3c3a9e1243"
  date: 2026-10-05
---

## Description

An override has been active on a control point for longer than any legitimate
troubleshooting session. Overrides are a normal service tool — hold the valve
open, watch the response, release it — but the release is the step that gets
skipped. What was a ten-minute diagnostic becomes the building's permanent
control strategy, invisible to anyone reading the sequence, and it stays that
way until someone audits the priority arrays. Around 10% of buildings carry at
least one; they are a standard retro-commissioning finding (PNNL RetuningOpps
A30, PNNL-27338). The cost is entirely a function of what was overridden — a
heating valve held at 100% is expensive, a nuisance alarm limit is not — hence
info severity. The value of the rule is that it turns an invisible condition
into a work order.

## Detection Logic

```
yFault = override_active
     sustained continuously for max_override_duration
     and then held a further alarm_delay
```

Block graph (`rule.cxf.jsonld`):

![AHU-0027 block graph](diagram.svg)

Two delays in series measure the duration rather than reading it: `staleOvr`
turns true only after `override_active` has been continuously true for
`max_override_duration` (24 h), and `persist` requires a further `alarm_delay`
(1 h) before `yFault` asserts — 25 h total for an override that is never
released. Any release, however brief, drops both timers to false and discards
the accumulated duration, so the alarm only ever describes one continuous
override. An override released and re-applied restarts the full 25 h.

## Possible Diagnoses

1. Temporary override forgotten by the operator after troubleshooting
2. Override set during commissioning and never removed — often predating the
   current operations staff
3. Override set to mask an underlying equipment problem (a failed sensor, a
   stuck actuator), in which case releasing it will expose the real fault

## Energy Impact

EXCESS_CONSUMPTION, LOW confidence, QUALITATIVE_ONLY. There is no per-fault
energy model: the waste depends on which point is overridden and to what value,
spanning from zero (an override matching what the sequence would command) to the
full cost of a disabled control loop. Quantify it after identifying the point,
using the Energy Impact Reference §4.4 framework. Prevalence ~10% of buildings
(PNNL RetuningOpps A30); no PNNL EEM maps to this fault.

## Emissions Impact

Scope 1 or 2, QUALITATIVE_EMISSIONS, LOW confidence, basis N/A. Which scope
applies follows the overridden point: a hot-water valve, boiler enable, or
gas-fired stage drives on-site combustion (scope 1); a fan, chiller, pump, or
electric-heat command drives purchased electricity (scope 2); a setpoint or
damper can drive both. No magnitude range until the point is known.

## Deviations

- The reference's host-computed `override_duration` point is dropped: the
  duration is measured in-rule with a `TrueDelay` chain, so the rule deploys
  against a raw BAS flag with no host-side state machine and no derived point
  whose reset semantics would have to be specified and trusted. What is
  implemented — continuous override, any release resets — matches the
  reference's intent for a stale override.
- The trade-off is that the duration does not survive a controller restart:
  with `delayOnInit = true` an override already active for a week re-times from
  the restart and alarms 25 h later. That is the conservative direction; a host
  keeping its own override history can raise the fault sooner.
- Two delays instead of one: the reference's `AlarmDelay` is separate from
  `max_override_duration`, so we keep both parameters independently tunable
  even though a single 90000 s delay would behave identically at the default
  values. A site that wants a 4 h override limit changes one parameter.
- The reference tags this fault for AHU, RTU, VAV, CHW, and HW. This is the
  AHU-family instance; the block graph is equipment-agnostic and each sibling
  family binds `override_active` from its own point dictionary.

## Notes

Step 2 of the after-hours-operation playbook — release stuck overrides in the
BACnet priority array — is this fault's remote fix verbatim, which is why it
shares that playbook. The two also co-occur: AHU-0018 treats an active
override as justification for after-hours fan operation, so a forgotten
override silences FC-052 while this rule flags the reason it is silent.

Take diagnosis 3 seriously before releasing anything. If the override was
masking a failed sensor or a stuck actuator, releasing it puts the equipment
back under a control loop that cannot work — trend the affected point for a
shift after release.
