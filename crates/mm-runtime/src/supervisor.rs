//! The supervisory loop: each tick, run the verified sequences in order on the latest readings,
//! clamp and gate their outputs, and decide what BOPTEST receives. The engine has no fail-safe;
//! this is it. A sequence that cannot step (bad reading, refused frame, stale data) writes
//! nothing new: its points hold their last good values or go back to the baseline controller.

use std::collections::BTreeMap;

use mm_bridge::WireValue;
use serde::Serialize;

use crate::plant::{Measurements, Plant, PlantError, Writes};
use crate::sequence::{RunningSequence, StepFailure};
use crate::wiring::{OutputBinding, TickOutputs};

/// What a point gets when its sequence cannot produce a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailSafe {
    /// Keep sending the last good value (if there is one; otherwise release).
    HoldLast,
    /// Hand the point back to BOPTEST's baseline controller.
    Release,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    /// The readings did not move forward in time; no sequence ran.
    Stale {
        time: f64,
    },
    Degraded {
        sequence: String,
        reason: StepFailure,
    },
    Clamped {
        point: String,
        value: f64,
        sent: f64,
    },
    /// The output was not a finite number (or not a number at all).
    Rejected {
        point: String,
    },
    Held {
        point: String,
        value: f64,
    },
    Released {
        point: String,
    },
}

/// Everything that happened in one tick, for the UI, the BACnet mirror and the run record.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TickReport {
    pub time: f64,
    pub outputs: TickOutputs,
    pub writes: Writes,
    pub events: Vec<Event>,
}

pub struct Supervisor {
    sequences: Vec<RunningSequence>,
    bindings: Vec<(String, OutputBinding)>,
    policy: FailSafe,
    last_time: Option<f64>,
    last_good: BTreeMap<String, f64>,
}

impl Supervisor {
    /// `sequences` run in the given order; `bindings` pair a sequence name with an output.
    pub fn new(
        sequences: Vec<RunningSequence>,
        bindings: Vec<(String, OutputBinding)>,
        policy: FailSafe,
    ) -> Self {
        Self {
            sequences,
            bindings,
            policy,
            last_time: None,
            last_good: BTreeMap::new(),
        }
    }

    pub fn tick(&mut self, readings: &Measurements) -> TickReport {
        let mut events = Vec::new();
        let mut outputs = TickOutputs::new();
        let time = readings.get("time").copied().unwrap_or(f64::NAN);
        let fresh = time.is_finite() && self.last_time.is_none_or(|last| time > last);
        if fresh {
            self.last_time = Some(time);
            for seq in &mut self.sequences {
                match seq.step(time, readings, &outputs) {
                    Ok(values) => {
                        outputs.insert(seq.name.clone(), values);
                    }
                    Err(reason) => events.push(Event::Degraded {
                        sequence: seq.name.clone(),
                        reason,
                    }),
                }
            }
        } else {
            events.push(Event::Stale { time });
        }

        let mut writes = Writes::new();
        for (sequence, binding) in &self.bindings {
            let value = outputs.get(sequence).and_then(|o| o.get(&binding.output));
            let gate_open = match &binding.gate {
                None => Some(true),
                Some(gate) => match gate.eval(readings, &outputs) {
                    Ok(WireValue::Boolean(b)) => Some(b),
                    _ => None,
                },
            };
            let point = &binding.point;
            match (value, gate_open) {
                // A closed gate is a decision, not a failure: hand the point back.
                (_, Some(false)) => {
                    release(&mut writes, point);
                    self.last_good.remove(point);
                    events.push(Event::Released {
                        point: point.clone(),
                    });
                }
                (Some(WireValue::Real(x)), Some(true)) if x.is_finite() => {
                    let sent = x.clamp(binding.min, binding.max);
                    if sent != *x {
                        events.push(Event::Clamped {
                            point: point.clone(),
                            value: *x,
                            sent,
                        });
                    }
                    send(&mut writes, point, sent);
                    self.last_good.insert(point.clone(), sent);
                }
                (Some(_), Some(true)) => {
                    events.push(Event::Rejected {
                        point: point.clone(),
                    });
                    self.fail_safe(&mut writes, &mut events, point);
                }
                // No value this tick, or the gate could not be evaluated.
                _ => self.fail_safe(&mut writes, &mut events, point),
            }
        }
        TickReport {
            time,
            outputs,
            writes,
            events,
        }
    }

    fn fail_safe(&self, writes: &mut Writes, events: &mut Vec<Event>, point: &str) {
        match (self.policy, self.last_good.get(point)) {
            (FailSafe::HoldLast, Some(&value)) => {
                send(writes, point, value);
                events.push(Event::Held {
                    point: point.to_owned(),
                    value,
                });
            }
            _ => {
                release(writes, point);
                events.push(Event::Released {
                    point: point.to_owned(),
                });
            }
        }
    }

    /// Runs up to `steps` ticks: tick on the current readings, send the writes, advance.
    /// `on_tick` sees every report (stream it to the UI). Stops early when the scenario ends.
    pub async fn run<P: Plant>(
        &mut self,
        plant: &mut P,
        first: Measurements,
        steps: usize,
        mut on_tick: impl FnMut(&TickReport),
    ) -> Result<usize, PlantError> {
        let mut readings = first;
        for done in 0..steps {
            let report = self.tick(&readings);
            on_tick(&report);
            readings = match plant.advance(&report.writes).await {
                Ok(next) => next,
                Err(PlantError::Ended) => return Ok(done + 1),
                Err(e) => return Err(e),
            };
        }
        Ok(steps)
    }
}

fn send(writes: &mut Writes, point: &str, value: f64) {
    writes.insert(format!("{point}_u"), value);
    writes.insert(format!("{point}_activate"), 1.0);
}

fn release(writes: &mut Writes, point: &str) {
    writes.insert(format!("{point}_activate"), 0.0);
}
