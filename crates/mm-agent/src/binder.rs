//! Muse Spark proposes how an engineer's sequence connects to the emulated building: which
//! BOPTEST measurement feeds each input and which BOPTEST setpoint each output writes. Model
//! output and only a proposal: `mm-server` checks every point against the BOPTEST point list
//! and the engineer confirms the binding before any run uses it.

use serde_json::{Value, json};

use crate::client::{Effort, ModelClient, ModelError, ModelRequest, OutputSchema, Usage};
use crate::prompt::TaskSpec;

const INSTRUCTIONS: &str = "You connect a verified building control sequence to an emulated office building \
(BOPTEST multizone_office_complex_air: three floors, one air handler per floor, five zones per floor). You are \
given the sequence's interface and the building's point list. Reply with one JSON object that follows the \
point_binding schema.

Rules:
1. floors: the floors (1, 2, 3) whose air handler this sequence should run on; usually all three for an \
air-handler sequence.
2. Point names: copy them exactly from the list. Where a point exists once per floor, write it with {floor} in \
place of the floor number (floor{floor}_reaAHU_TSup_y). Weather points have no floor.
3. inputs: one entry per sequence input. kind is measurement (only for a real input: read from a measurement \
point), above (required for a boolean input fed from the building: true while a measurement is above threshold, \
for example threshold 0.5 on a 0/1 status; give the threshold in the point's own unit) or constant (a fixed value; use it only when the building has no fitting point, and say so in notes). Use \
empty point and 0 threshold where they do not apply; value is only for constant.
4. outputs: one entry per sequence output. point is the input point the output writes (its name without the \
_u or _activate suffix), or empty when the output is for display only or the building has no such setpoint. \
min and max clamp what is written: they must include every value the sequence can output (round min down and \
max up, so a value the sequence sends exactly is never clamped), stay within the point's listed range, and stay \
within sensible limits for the equipment. measured is the measurement that shows the result (for a supply air temperature setpoint, the supply \
air temperature), or empty.
5. Units must match: the building uses SI units (K, Pa, fractions). If an input's unit differs from the point's, \
say so in notes.
6. notes: short plain sentences on anything the engineer should check, such as a stand-in signal (occupancy \
used as an enable) or a constant.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["floors", "inputs", "outputs", "notes"],
        "properties": {
            "floors": {"type": "array", "items": {"type": "integer"}},
            "inputs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["input", "kind", "point", "threshold", "value"],
                    "properties": {
                        "input": {"type": "string"},
                        "kind": {"type": "string", "enum": ["measurement", "above", "constant"]},
                        "point": {"type": "string"},
                        "threshold": {"type": "number"},
                        "value": {"anyOf": [{"type": "number"}, {"type": "boolean"}]}
                    }
                }
            },
            "outputs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["output", "point", "measured", "min", "max"],
                    "properties": {
                        "output": {"type": "string"},
                        "point": {"type": "string"},
                        "measured": {"type": "string"},
                        "min": {"type": "number"},
                        "max": {"type": "number"}
                    }
                }
            },
            "notes": {"type": "array", "items": {"type": "string"}}
        }
    })
}

/// The request for `task`, given the building's point list as text (one point per line).
pub fn bind_request(model: &str, task: &TaskSpec, points: &str) -> ModelRequest {
    let interface = json!({
        "name": task.name,
        "description": task.description,
        "inputs": task.inputs,
        "outputs": task.outputs,
    });
    let text = format!(
        "Sequence:\n{}\n\nBuilding points (name | kind | unit | range | description):\n{points}",
        serde_json::to_string_pretty(&interface).unwrap_or_default()
    );
    ModelRequest {
        tools: Vec::new(),
        model: model.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        input: vec![json!({"role": "user", "content": [{"type": "input_text", "text": text}]})],
        schema: Some(OutputSchema {
            name: "point_binding".into(),
            schema: schema(),
        }),
        effort: Some(Effort::Medium),
        max_output_tokens: 16_000,
    }
}

/// Asks `client` for a binding; returns the reply's JSON (checked by the caller) and its cost.
/// With `problems` from an earlier reply (`earlier`), asks once more with them listed.
pub async fn propose_binding<C: ModelClient>(
    client: &C,
    model: &str,
    task: &TaskSpec,
    points: &str,
    retry: Option<(&Value, &[String])>,
) -> Result<(Value, Usage), ModelError> {
    let mut request = bind_request(model, task, points);
    if let Some((earlier, problems)) = retry {
        request.input.push(json!({"role": "assistant", "content": [{"type": "output_text", "text": earlier.to_string()}]}));
        request.input.push(json!({"role": "user", "content": [{"type": "input_text", "text": format!(
            "The building's checks refused this binding:\n- {}\nReply with the whole binding again, fixed.",
            problems.join("\n- ")
        )}]}));
    }
    let reply = client.complete(&request).await?;
    let value = serde_json::from_str(&reply.text).map_err(|e| ModelError::Malformed {
        message: format!("the binding is not JSON: {e}"),
    })?;
    Ok((value, reply.usage))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_request_carries_the_interface_and_the_points() {
        let task: TaskSpec = serde_json::from_value(json!({
            "name": "sat", "description": "d",
            "inputs": [{"name": "oat", "type": "real", "unit": "K", "meaning": "Outdoor air"}],
            "outputs": [{"name": "sp", "type": "real", "unit": "K", "meaning": "Setpoint"}]
        }))
        .unwrap();
        let r = bind_request(
            "muse-spark-1.3",
            &task,
            "weaSta_reaWeaTDryBul_y | measurement | K",
        );
        let text = r.input[0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"oat\"") && text.contains("weaSta_reaWeaTDryBul_y"));
        assert_eq!(r.schema.unwrap().name, "point_binding");
    }
}
