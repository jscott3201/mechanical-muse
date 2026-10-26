# Claims to avoid, and what to write instead

Prior work to credit, not erase: LLM-generated CDL with human review (Wan et al., arXiv 2509.14623, Sept 2025; up to 83% on five control modules); LLM plus simulation checks for HVAC control code (Jiang et al., Buildings, 2026, private project); LLM-writes, tools-verify loops for PLC and DCS code (LLM4PLC, Agents4PLC, SemaPLC, Spec2Control). Before Nov 4 re-search "CDL" + LLM and "Guideline 36" + LLM, since this area moves fast.

| Do not write | Write instead |
|---|---|
| No one has used LLMs to write building control code or CDL | Earlier work relied on compilation, human review or private data; we replay test vectors through a deterministic engine |
| First LLM + verification loop for control logic | An LLM-writes, engine-verifies loop for building control sequences, with an open benchmark |
| Verifiably correct, proven, guaranteed, safe to deploy, commissioning-grade | Passes X of Y library test vectors (conformance on those vectors only) |
| Guideline 36 compliant, ASHRAE certified | G36-style sequence; the CDL subset the engine executes; 231P is a proposed standard |
| Endorsed by, or built with, LBNL, ASHRAE or IBPSA | Builds on open tooling around CDL and CXF; no affiliation |
| Beats other models, humans, or the 83% / 97.1% in prior papers; state of the art | Numbers from our harness only, with the baseline beside them; prior criteria differ |
| Muse Spark is best at HVAC control | `muse-spark-1.3` at effort E passes X of Y on split Z; other models only if run under identical prompts |
| Replaces controls engineers; autonomous building control | An authoring assistant under verification; a person reviews before any deployment |
| Field-validated, tested in real buildings | Closed loop on the BOPTEST simulator (`multizone_office_complex_air`) and a simulated BACnet device |
| We built the engine, BACnet stack or BOPTEST | Pre-existing, pinned, listed in the README table |
| Spec-derived vectors | Only if derived from the spec text; library vectors are author-written scenarios, say "library test vectors" |
| Ambiguity finder proves or finds all ambiguities; novel technique | Candidate ambiguities from a heuristic with a known blind spot (models agreeing on one wrong reading); report precision and recall on seeded cases |
| No vendor offers this | We found no vendor product that does this (limited search) |

Novelty without "first" (safest for video and pitch): "Mechanical Muse measures, in the open and with a deterministic engine, how often an LLM's building control sequence matches its written spec on replayed test vectors, and shows where independent drafts disagree about what the spec means." Drop the last clause unless that feature ships.

Also:
- "Open" holds only if the repo, vectors and benchmark data are public.
- Derived signals (fan status, plant enable, cooling-loop signal) are labelled derived, never presented as BOPTEST measurements.
- Say "a CDL subset": the engine executes the 136 elementary classes in its catalog, with no G36 composite blocks.
- Name the exact model id, reasoning effort and engine rev next to every number.
