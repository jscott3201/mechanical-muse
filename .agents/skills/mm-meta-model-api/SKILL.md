---
name: mm-meta-model-api
description: "Guides mm-agent calls to the Meta Model API (Responses): structured output, reasoning carry-over, tools, retries, fixtures. Use when changing prompts, schemas, ModelClient or the repair loop."
license: MIT OR Apache-2.0
metadata:
  short-description: "Meta Model API: Responses, schemas, retries, fixtures"
---
`POST https://api.meta.ai/v1/responses` with `Authorization: Bearer $MODEL_API_KEY`, read from the environment by `mm-server` only (Muse Code's own `META_API_KEY` is a different variable). Models: `muse-spark-1.3`; `-contributor` (trains on inputs, 150 RPM) only for public library data. Depth: `docs/model-api.md`.

**Request rules**
- Flat `text.format {type:"json_schema", name, schema, strict:true}` (nested `response_format` is Chat Completions and a 400 here). Strict needs `additionalProperties:false` and every property in `required`; no `oneOf`/`allOf`; `anyOf` below the root; depth 10, 5,000 properties, 1,000 enum values, no `$ref` cycles. `pattern`, `minimum`, `minItems` are undocumented: re-validate client-side.
- Set `store:false` explicitly (default is true). Reasoning across tool turns: stateless (`store:false` + `include:["reasoning.encrypted_content"]`, replay output items verbatim as raw `serde_json::Value`) or `previous_response_id`; both in one request is a 400.
- `tool_choice` accepts only `"auto"`, so handle plain-text replies. `tool_search` cannot be combined with `text.format`. Tool `arguments` are JSON strings: decode, then validate. Unsupported (400): effort `none`, `truncation:"auto"`, `stop`, `logprobs`.
- Walk `output[]`, not `output_text`; check `status` (`incomplete`) and refusals before parsing. Reasoning tokens spend `max_output_tokens`: leave headroom.
- PDF input: text from the first 100 pages, page images from the first 50. Inline up to 50 MB: use the Files API above ~30 MB (base64 growth unclear).

**Retries and limits** (per team, all workers share Spark 3,000 RPM)
- Retry 429 (honor `Retry-After`), 500, 503: backoff from 0.5-1 s, doubling, 0-1 s jitter, 3-5 tries. Other 4xx: fix the request. 504: stream, background mode, or lower effort.
- Stream long calls with a per-chunk timeout, not a whole-request one. Keep instructions, tools and schema first and stable (prefix cache).

**Unknowns to probe with an `#[ignore]` live test** (record outcomes in `docs/model-api.md` section 10)
- Does `text.format` work with function tools in one request? Until proven: tool turns without it, then a final call with it.
- Max output tokens per model (undocumented; plan for <= 131,072). Whether the `params[].value` anyOf is accepted (`mm-block-graph-cxf`).

**Code**
- `ModelClient` trait in `mm-agent`; Spark, Wonder and Glimmer implement it, wire types stay inside each impl. Glimmer runs on Modal behind an OpenAI-compatible endpoint (stack and URL pending in `docs/DECISIONS.md`), probably Chat Completions with other schema and reasoning semantics: check its routes.
- Offline tests use scrubbed recorded fixtures behind the trait; live tests are `#[ignore]` and env-gated. Never log headers or request bodies; fixtures hold no key or file id.
