# Compact block-graph format (v0 proposal)

The LLM emits this JSON through structured output; a deterministic emitter expands it to CXF JSON-LD; the Open Control Engine (via `cxf-verify` or `oce-api`) verifies it. Files: schema `docs/block-graph.schema.json` (71 lines), a Python prototype of the converter, emitter and validator (`mm_blockgraph.py`) is kept in local research notes as a porting reference for `mm-graph`.

## 1. What shaped the format

- **Meta structured-output limits** (`docs/model-api.md` section 2): strict subset means a plain-object root, no `oneOf`/`allOf`, `anyOf` only below the root, `additionalProperties:false` and *every* property listed in `required` on every object, depth <= 10, <= 1000 enum values in total. `pattern`, `minItems`/`maxItems` and schema-valued `additionalProperties` are undocumented, so the schema uses none of them. Consequences: `params` is an array of `{name, value}` (no open map); edges are strings (no 2-tuples); absent data is `[]` or `""`; everything else (id syntax, edge syntax, arity, types) is checked by the deterministic validator in section 6.
- **Everything derivable from the palette is left out.** Port names and port types per class, parameter value types (double / boolean / integer), node typing and IRIs all come from the engine catalog (`block-census.md` section 3). For all 1,115 library block instances the CXF port lists equal the catalog's declaration-order names, so the LLM never lists ports on a block.
- **Token budget.** The recommended wire form is a median ~430 tokens (max ~1,200) for library-size rules, against ~2,700 (max ~9,200) for raw CXF.

## 2. The format by example (AHU-0027, stale override)

```json
{"name":"ahu_0027",
 "inputs":[{"name":"override_active","type":"bool","unit":"bool","description":"Manual override active flag"}],
 "outputs":[{"name":"yFault","type":"bool","description":"True once override_active has been held ..."}],
 "blocks":[
  {"id":"staleOvr","class":"Logical.TrueDelay","params":[{"name":"delayTime","value":86400.0},{"name":"delayOnInit","value":true}]},
  {"id":"persist", "class":"Logical.TrueDelay","params":[{"name":"delayTime","value":3600.0}, {"name":"delayOnInit","value":true}]}],
 "connections":["override_active -> staleOvr.u","staleOvr.y -> persist.u","persist.y -> yFault"]}
```

Addressing: a bare token is a boundary input (as source) or boundary output (as destination); `id.port` is a block port. Input/output names and block ids contain no dots and are unique across all three.

## 3. JSON Schema (draft 2020-12, strict-subset safe)

`docs/block-graph.schema.json`, reproduced in full (checked: `check_schema` passes; all 137 converted library rules validate; no `oneOf`/`allOf`; every object closed with all properties required):

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "MechanicalMuseBlockGraph",
  "description": "Compact CDL block graph. A deterministic emitter expands it to CXF JSON-LD; the Open Control Engine verifies it.",
  "type": "object",
  "additionalProperties": false,
  "required": ["name", "inputs", "outputs", "blocks", "connections"],
  "properties": {
    "name": {"type": "string", "description": "snake_case name of the sequence, e.g. stale_override"},
    "inputs": {"type": "array", "description": "Boundary inputs (sensor/command/setpoint points). Names are fixed by the task.",
               "items": {"$ref": "#/$defs/input"}},
    "outputs": {"type": "array", "description": "Boundary outputs. Names are fixed by the task. Each is the target of exactly one connection.",
                "items": {"$ref": "#/$defs/output"}},
    "blocks": {"type": "array", "description": "CDL block instances.", "items": {"$ref": "#/$defs/block"}},
    "connections": {
      "type": "array",
      "description": "Edges written '<src> -> <dst>'. src is an input name or '<blockId>.<outPort>'. dst is an output name or '<blockId>.<inPort>'. Every block input and every output has exactly one incoming edge; an output port may fan out to many.",
      "items": {"type": "string"}
    }
  },
  "$defs": {
    "signalType": {"type": "string", "enum": ["bool", "real", "int"]},
    "input": {
      "type": "object", "additionalProperties": false, "required": ["name", "type", "unit", "description"],
      "properties": {
        "name": {"type": "string", "description": "snake_case, no dots"},
        "type": {"$ref": "#/$defs/signalType"},
        "unit": {"type": "string", "description": "degC, %, Pa, kPa, L/s, kW, s, 1 or bool"},
        "description": {"type": "string"}
      }
    },
    "output": {
      "type": "object", "additionalProperties": false, "required": ["name", "type", "description"],
      "properties": {
        "name": {"type": "string", "description": "e.g. yFault; no dots"},
        "type": {"$ref": "#/$defs/signalType"},
        "description": {"type": "string"}
      }
    },
    "block": {
      "type": "object", "additionalProperties": false, "required": ["id", "class", "params"],
      "properties": {
        "id": {"type": "string", "description": "camelCase instance id; unique across inputs, outputs and blocks; no dots"},
        "class": {
          "type": "string", "description": "CDL class path below Buildings.Controls.OBC.CDL",
          "enum": [
            "Logical.And", "Logical.TrueDelay", "Reals.GreaterThreshold", "Reals.Sources.Constant", "Reals.Subtract",
            "Reals.Switch", "Logical.Not", "Reals.LessThreshold", "Integers.Sources.Constant", "Logical.Or",
            "Reals.MultiplyByParameter", "Reals.Abs", "Integers.Equal", "Reals.Multiply", "Conversions.BooleanToReal",
            "Conversions.BooleanToInteger", "Reals.Divide", "Reals.Greater", "Reals.MovingAverage", "Reals.Add",
            "Reals.Line", "Integers.Switch", "Discrete.Sampler", "Reals.Max", "Logical.Switch",
            "Reals.AddParameter", "Reals.Min", "Logical.Latch", "Logical.Sources.Constant", "Conversions.IntegerToReal",
            "Logical.Timer", "Discrete.UnitDelay", "Reals.Less", "Logical.Edge", "Reals.PIDWithReset",
            "Reals.Hysteresis", "Reals.PID", "Logical.TrueFalseHold", "Logical.Change", "Integers.GreaterThreshold",
            "Logical.Pre", "Discrete.TriggeredSampler", "Integers.Add", "Logical.Xor", "Reals.Limiter",
            "Logical.Sources.SampleTrigger", "Integers.Change", "Integers.Multiply", "Logical.Nor", "Integers.LessThreshold",
            "Integers.LessEqualThreshold", "Integers.AddParameter", "Discrete.FirstOrderHold"
          ]
        },
        "params": {"type": "array", "description": "Parameters to set; omit defaults. [] if none.", "items": {"$ref": "#/$defs/param"}}
      }
    },
    "param": {
      "type": "object", "additionalProperties": false, "required": ["name", "value"],
      "properties": {
        "name": {"type": "string", "description": "CDL parameter name, e.g. delayTime"},
        "value": {"anyOf": [{"type": "number"}, {"type": "boolean"}, {"type": "string"}]}
      }
    }
  }
}
```

**Design notes**
- **Class enum (53).** The 62 census classes, minus the 8 width-driven ones (`Logical.MultiAnd/MultiOr`, `Reals.MultiMax/MultiMin/MultiSum/MatrixGain`, `Routing.*ScalarReplicator`; they need `nin` plus `u1..u<nin>` port synthesis) and `Utilities.Assert`; ordered by combined frequency. It covers 100% of library block instances and 96.2% of G36-fixture instances. **Fallback** = rebuild the enum from the 105 authorable classes in the pinned catalog (width-driven and positional `TimeTable` classes excluded; well inside the 1000-value cap) and put those classes' ports in the prompt palette; `mm-graph`'s validator already accepts any authorable class. I avoided an `anyOf [enum, free string]` per block because the free-string branch would void the grammar constraint.
- **Interface is locked by the task.** For the benchmark the point list and output list come from the card (`points`, `outputs`); the validator requires `inputs`/`outputs` names and types to equal them exactly. `unit` and `description` exist only so the model states what it thinks each point is; the emitter drops them (CXF v1 has no annotations).
- **`value`** accepts number, boolean, string. Real parameters accept JSON integers (emitter writes `86400.0`); the string branch is only for enum parameters (`Reals.PID*.controllerType`).
- **Edge order.** Write edges in dataflow order (inputs first). CXF groups edges by source; order within a source's fan-out list is preserved, order between sources is irrelevant.
- **Hidden convention to state in the system prompt (or default in the emitter):** every one of the 177 library `Logical.TrueDelay` blocks sets `delayOnInit = true`, a deliberate deviation from the CDL default `false` (startup conservatism; cards explain it only in their Deviations section). It is also 177 of the 203 graph parameters that the cards' `params` map does not expose, so a model working from the card alone cannot discover it; vectors that start with an already-true input depend on it.
- **Name hygiene:** `cxf-verify` resolves a vector's input name by `.name`/`#name` suffix and errors on ambiguity, so boundary names must not equal block port names (`u`, `y`, `u1`, `u2`, ...). The validator should enforce a minimum length of 2 and a port-name blacklist.
- **Not in v0 (cheap later wins):** inline literals as edge sources to avoid `Constant` blocks (39 library, 149 G36 instances), eliding port names on single-port blocks, width-driven classes.

## 4. Two library rules converted by script (round-trip proven)

`scripts/mm_blockgraph.py::cxf_to_compact` reads the CXF; `compact_to_cxf` regenerates it from the compact JSON plus the palette. Wire form below (pretty-printed here, minified in the size measurements).

**AHU-0027** (2 blocks): raw `rule.cxf.jsonld` 4,182 B (13 nodes) -> wire 650 B (6.4x smaller) -> the JSON shown in section 2.

**AHU-0005** (3 blocks, two real inputs): raw 5,316 B (17 nodes) -> wire 766 B (6.9x).

```json
{"name":"ahu_0005",
 "inputs":[{"name":"mat","type":"real","unit":"°C","description":"Mixed air temperature (after OA and RA mix)"},
           {"name":"sat","type":"real","unit":"°C","description":"Supply (discharge) air temperature"}],
 "outputs":[{"name":"yFault","type":"bool","description":"True while SAT has stayed more than mat_sat_gap_threshold below MAT for at least alarm_delay"}],
 "blocks":[{"id":"gap",   "class":"Reals.Subtract",        "params":[]},
           {"id":"gapBig","class":"Reals.GreaterThreshold","params":[{"name":"t","value":3.0}]},
           {"id":"persist","class":"Logical.TrueDelay",    "params":[{"name":"delayTime","value":1800.0},{"name":"delayOnInit","value":true}]}],
 "connections":["mat -> gap.u1","sat -> gap.u2","gap.y -> gapBig.u","gapBig.y -> persist.u","persist.y -> yFault"]}
```

Port order matters (`Subtract` is `u1 - u2`, so `gap = mat - sat`); the palette line `Reals.Subtract(u1:R,u2:R)->(y:R)` must make that explicit to the model.

**Result over all 137 rules** (`scripts/final_sizes.py`, `scripts/rt_semantic.py`):
- 137/137 regenerate a semantically identical graph (same `@id` set, same node bodies; a singleton object equals a singleton array; doubles compared numerically).
- 130/137 are exactly equal to the library file after JSON parsing, including node order. The 7 others differ cosmetically: node order (AHU-0023, CHW-0002, HW-0006, HW-0008), root `S231:hasOutput` written as a single object instead of an array (HP-0005, RTU-0009), and the literal `"0.30"` vs `"0.3"` (RTU-0007).
- Not yet checked: engine `content_id` equality (needs `cxf-verify` built at the pinned engine; see `benchmark-split.md`). The 130 exact matches should be identical; the 7 are the ones to try first.

## 5. What the emitter must supply

| Item | Rule (library dialect, as `tools/authoring/cxfgen.py` writes it) |
|---|---|
| `@context` | `{"S231": "http://data.ashrae.org/S231P#", "base": "<ns>"}` |
| Namespace / root IRI | `<ns>` = `urn:mechanical-muse:<name>#` (library uses `urn:cxf-library:<id>#`); root `@id` = `<ns><name>`, `S231:label` = `name` |
| Root node | `@type S231:Block`; `containsBlock` (blocks in order), `hasInput` (inputs), `hasOutput` (outputs) |
| Block node | `@type` = `<ns>Buildings.Controls.OBC.CDL.<class>`; `S231:label` = id; `hasParameter` (in given order), `hasInput` / `hasOutput` = palette port names in declaration order (object if one, array if several) |
| Port nodes | per block port and per boundary connector: `@type S231:{Real,Boolean,Integer}{Input,Output}` + `S231:isOfDataType {"@id": "S231:Real"/"Boolean"/"Integer"}` (kind from the palette, or `signalType` for boundary) |
| Parameter nodes | `<block>.<param>` with `S231:value {"@value": "...", "@type": xsd}`; xsd from the palette param kind: Real -> `xsd:double` written as Python `repr(float)`, Integer -> `xsd:integer`, Boolean -> `xsd:boolean` (`true`/`false`) |
| Enum parameters | not exercised by the library dialect; G36 fixtures use a raw string value plus `isOfDataType` of the enum class (`...Types.SimpleController.PI`); needs a fixture-backed emitter branch before `Reals.PID*` are allowed |
| Connections | `S231:isConnectedTo` on the source node, destination ref(s) grouped by source |
| Defaults | parameters not listed are left to the engine default; `Required` parameters must be listed (validator) |
| Dropped | `unit`, `description` (no annotations in CXF v1), card metadata, vectors |
| Not supplied by anyone yet | width-driven port synthesis (`u1..u<nin>`), literal-source sugar |

## 6. Pre-engine validator (palette-level, prototype `validate_compact`)

Checks: ids/names unique and dot-free; every class in the palette; every parameter known to the class and every `Required` parameter present; every edge endpoint exists; endpoint types match (`Real`/`Boolean`/`Integer`); every block input and every output driven exactly once. 0 findings on all 137 converted library rules. Seeded mutation test over the 137 rules (`scripts/mutation.py`): deleting an edge, adding a second driver, an unknown class, and dropping a required parameter are each flagged in 137/137 mutants; a boundary-type swap in 35/35 mutants where one was possible. The engine's own `composite/<rule-id>: ` diagnostics (with node IRI as subject) are the second line of defense for the repair loop.

## 7. Size: raw CXF vs compact JSON, all 137 rules

Variants (all measured as UTF-8 bytes; tokens ~ bytes/4, no tokenizer was run):
- **raw**: `rule.cxf.jsonld` as stored (2-space pretty-printed).
- **raw-min**: the same JSON minified.
- **dense**: structure only, `params` as an object, edges as `[src,dst]` pairs, no unit/description (not strict-schema safe).
- **wire**: recommended form above without unit/description (`wire-lean`) and with them (`wire`, descriptions taken from the points dictionary and card outputs, so a plausible upper bound for what the model writes).

| Variant | Median B | p90 B | Max B | Median tokens | Max tokens | Total (137 rules) |
|---|---|---|---|---|---|---|
| raw (on disk) | 10,831 | 18,174 | 36,859 (HW-0011) | 2,708 | 9,215 | 1,665,681 B |
| raw-min | 7,770 | 13,191 | 27,153 | 1,943 | 6,788 | 1,204,945 B |
| dense | 997 | 1,564 | 3,800 | 249 | 950 | 147,743 B |
| wire-lean | 1,222 | 1,871 | 4,322 | 306 | 1,080 | 179,456 B |
| **wire (recommended)** | **1,717** | **2,593** | **4,829** (HW-0011) | **429** | **1,207** | 241,938 B |

Per-rule reduction ratios:

| Ratio | Median | Mean | Min (rule) | Max (rule) |
|---|---|---|---|---|
| raw / wire | **6.56x** | 6.80x | 4.51x (HX-0001) | **9.74x** (AHU-0022) |
| raw-min / wire | 4.76x | 4.93x | 3.22x (HX-0001) | 7.11x (AHU-0022) |
| raw / wire-lean | 9.05x | 9.16x | 7.13x (HX-0001) | 11.03x (VAV-0007) |
| raw / dense | 11.31x | 11.22x | 8.72x (HX-0001) | 13.16x (VAV-0009) |

Aggregate: 1.67 MB of CXF becomes 242 kB of wire JSON (6.9x; 5.0x against minified CXF). The unit/description fields are about 26% of the wire bytes; dropping them from the model's output and joining them from the task's point list brings the median to ~306 tokens. The worst case for the model is a 28-block, 38-connection rule (HW-0011) at ~1,200 output tokens. Caveat: bytes/4 is a rough proxy; confirm with Meta's tokenizer (JSON-LD IRIs and punctuation-heavy text usually tokenize worse than prose, so the real raw-CXF token count is probably higher, not lower).

## 8. Open questions

- Does Meta's decoder accept `anyOf` of three primitives for `value`? If not, use `{"value": number, "text": string}` or encode booleans as strings and let the palette coerce.
- Should the model emit `unit`/`description` at all? They cost ~26% of output bytes and CXF drops them.
- Enum-valued parameters and width-driven blocks are the two places where the emitter needs more than the palette (G36-dialect fixtures are the reference).
