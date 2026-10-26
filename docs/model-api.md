# Meta Model API cheatsheet for Mechanical Muse (Rust + reqwest)

Compiled 2026-10-26 from dev.meta.ai docs (paraphrased). Doc base: https://dev.meta.ai/docs/
Legend: **[DOC]** stated in the docs. **[SECONDARY]** third-party (LiteLLM/Vercel/press). **[UNVERIFIED]** not documented or inferred; test before relying on it. No live API calls were made (no key), so every wire shape below comes from the docs, not from a tested request.

---

## 1. Auth, base URL, which endpoint to use

Sources: https://dev.meta.ai/docs/overview, /authentication, /sdks, /protocols, /protocols/responses, /protocols/chat-completions, /protocols/messages, /quickstart

- Base URL: `https://api.meta.ai/v1` [DOC]. The Anthropic SDK is configured with `https://api.meta.ai` (it appends `/v1/messages`) [DOC].
- Auth: `Authorization: Bearer $MODEL_API_KEY` plus `Content-Type: application/json` [DOC]. Keys start with `LLM_`. The key is shown once at creation. Keep it server-side; never ship it to a browser [DOC].
- Messages endpoint also tolerates `x-api-key` and `anthropic-version`, but they are optional and Bearer works [DOC: create-message page].
- Unauthenticated `GET https://api.meta.ai/v1/status` reports overall and per-model availability [DOC; the page's own example sends a Bearer header, which conflicts with its "no key needed" claim, so send it or not, it should not matter] https://dev.meta.ai/docs/api-reference/status

| Endpoint | Path | Notes |
|---|---|---|
| Responses | `POST /v1/responses` | Docs call it the recommended default for new work and agents |
| Chat Completions | `POST /v1/chat/completions` | Drop-in for OpenAI chat code; Spark text models only |
| Messages | `POST /v1/messages` | Anthropic wire format; stateless; Spark text models only |

**Recommendation: Responses.** Reasons, all [DOC]:
- Reasoning continuity across tool turns, either server-side (`previous_response_id`) or stateless (`store:false` + `include:["reasoning.encrypted_content"]`, then replay the returned `reasoning` items). Chat Completions cannot carry reasoning across turns for external keys; each turn reasons from scratch.
- Structured output via `text.format`, hosted `tool_search`, hosted `web_search`, file inputs (`input_file`), `background:true`, `/v1/responses/input_tokens` for counting.
- It is the only endpoint family with fine-grained streaming events for function-call arguments (`response.function_call_arguments.delta/.done`) and clean failure events (`response.failed`, `error`).

When to pick Messages instead [DOC + inference]: it is the only protocol that accepts `tool_choice: {"type":"any"}` (force some tool) and `none`; Responses and Chat Completions accept only `"auto"`. It also supports `output_config.format` (json_schema) and `tool_search`. Cost: stateless, PDF `document` block source shapes are undocumented, and the response `content` array starts with a `redacted_thinking` block. Only Spark text models are served there.

Model availability by endpoint [DOC models + protocol pages]: Chat Completions and Messages are documented as serving the Muse Spark text models. `muse-wonder-1.0` is listed as a Model API model (Standard mode only, "preview" on the models page). Its endpoint coverage is **[UNVERIFIED]**; assume Responses works and probe Chat Completions before depending on it.

---

## 2. Structured output

Sources: https://dev.meta.ai/docs/structured-output, /cookbook/structured-output, /api-reference/responses/schemas, /api-reference/chat-completions/schemas, /api-reference/messages/schemas, /protocols/messages

### Request shapes

| Protocol | Field | Shape |
|---|---|---|
| Responses | `text.format` | `{"type":"json_schema","name":"...","schema":{...},"strict":bool?,"description":"..."?}` (flat, no nested `json_schema` key) |
| Chat Completions | `response_format` | `{"type":"json_schema","json_schema":{"name":"...","schema":{...},"strict":bool?,"description":"..."?}}` (nested) |
| Messages | `output_config.format` | `{"type":"json_schema","schema":{...}}` (no `name`; limits for this path not documented) |

- `name`: letters, digits, `_`, `-`, max 64 chars [DOC].
- Cross-use is a hard error: sending `response_format` to Responses, or `text.format` to Chat Completions, returns 400 [DOC].
- A legacy `{"type":"json_object"}` mode exists on both; it only guarantees syntactically valid JSON and needs a prompt instruction. Docs recommend `json_schema` instead [DOC].
- Result location: Responses has the JSON string in the `output[]` `message` item's `output_text.text`; Chat Completions has it in `choices[0].message.content`. Both are strings that you `serde_json::from_str`.

### Strict mode [DOC]
- `strict` defaults to **false** everywhere (`response_format.json_schema.strict`, `text.format.strict`, `tools[].function.strict`, `tools[].strict`).
- Decoding is schema-constrained regardless of `strict`. The flag only controls whether the server additionally validates your schema against the strict subset and 400s on violations. Tool-call arguments are not guaranteed valid even with `strict:true`, so validate them yourself.
- Strict subset rules: root must be a plain object (no root-level `anyOf`/`oneOf`/`allOf`/`enum`/`not`); `allOf` and `oneOf` unsupported anywhere; `anyOf` allowed below root; every object has `additionalProperties:false`; every object's `required` lists all of its `properties` keys. The same rules apply inside `$defs`/`definitions`.

### Limits [DOC; apply to `text.format`, `response_format` and function-tool `parameters`; violations return 400]

| Limit | Value |
|---|---|
| Nesting depth | 10 levels |
| Total properties | 5,000 across the whole schema |
| Total string length | 120,000 chars (property names, definition names, enum values, const values) |
| Enum values | 1,000 total across all enums (raisable per app) |
| Large string enums | an enum over 250 values is also capped at 15,000 combined characters |
| Expanded size | rejected above 200,000 nodes after `$ref` inlining |
| Recursion | any `$ref` cycle returns 400; flatten to fixed depth |

Features:
- `$ref`/`$defs`/`definitions`: allowed; refs are inlined for the size check; non-recursive only.
- `anyOf`: OK below root. `oneOf`/`allOf`: not in strict mode; behavior in non-strict mode is not described (the doc says non-strict schemas are "accepted and normalized") **[UNVERIFIED]**.
- `enum`, `const`, `description`, `required`, `additionalProperties:false`: documented as in use.
- `minimum`/`maximum`, `pattern`, `format`, `minItems`/`maxItems`, `default`, `prefixItems`, type arrays like `["string","null"]`, schema-valued `additionalProperties`: **not mentioned** in the docs. Design for `anyOf:[{...},{"type":"null"}]` for nullable fields and re-validate bounds client-side (e.g. with the `jsonschema` crate). Do not assume numeric/string bounds are enforced.

### Failure modes
- Output is documented as guaranteed to conform, with the schema validated before decoding [DOC]. But the page does not describe refusals, truncation, or interaction with reasoning **[UNVERIFIED]**. Defensive handling to build:
  - Truncation: reasoning tokens share the output budget. If `max_output_tokens`/`max_completion_tokens` is too low you get a cut-off or empty result: Chat `finish_reason:"length"`, Responses `status:"incomplete"` (the `incomplete_details.reason` values are not listed in the docs). The video page notes an empty transcript with `finish_reason:length` when the budget is under about 4,000 [DOC]. Always check status before parsing.
  - Refusal: Chat message has a `refusal` field (string or null) and finish reasons include `content_filter`; Responses has `response.refusal.delta/.done` stream events (the non-streaming refusal content-part shape is not in the docs **[UNVERIFIED]**); Messages `stop_reason` can be `refusal`. A 400 with code `content_policy_violation` is also possible.
  - Schema errors: 400 `invalid_request_error`, `param` usually points at `text`/`response_format`.
- Incompatibilities [DOC]: `tool_search` cannot be combined with `text.format` json_schema (400 in both hosted and client mode). Whether `text.format` can be combined with ordinary function tools in the same request is **not documented [UNVERIFIED]**.
  - Safe pattern: tool loop turns with `tools` and no `text.format`; then one final call with `text.format` and no tools (re-sending the history). Costs one extra call and probably breaks the prompt-cache prefix, since the tools array changes (inference).

---

## 3. Tool calling

Sources: https://dev.meta.ai/docs/tool-calling, /tool-search, /cookbook/tool-function-calling, /protocols/messages, /search-grounding

### Definitions [DOC]
- Responses (flat): `{"type":"function","name":"...","description":"...","parameters":{JSON Schema},"strict":false}`.
- Chat Completions (nested): `{"type":"function","function":{"name","description","parameters","strict"}}`.
- Messages: `{"type":"custom"?,"name","description","input_schema",...}` with optional `strict` and `defer_loading`.
- `type:"custom"` freeform-text tools exist on Responses only (400 on Chat Completions).
- Use `tools`, not the deprecated `functions`/`function_call`.
- Names must match `^[a-zA-Z0-9_.-]+$` with at most one dot (two or more dots return 400). Custom tools are exempt. If `parameters` omits `type:"object"` the API adds it. `tool_search` is a reserved name. With `web_search` enabled, names `browser.search`, `browser.open`, `browser.find` are reserved.
- No maximum number of function tools is documented. Use `defer_loading` + `tool_search` for large sets.

### Parallel calls
- `parallel_tool_calls` defaults to true; set false for one call per turn (Responses and Chat). Messages uses `tool_choice.disable_parallel_tool_use` [DOC].
- `max_tool_calls` caps built-in (hosted) tool calls only, not your function tools. Cap your own loop with a `max_turns`.

### Returning results [DOC]
- Responses: output contains `function_call` items (`id`, `call_id`, `name`, `arguments` as a **JSON string**, `status`). You send back `{"type":"function_call_output","call_id":"...","output":"<string>"}`. `call_id` must be 1-64 chars and match the call. Either chain with `previous_response_id` or replay history yourself (see section 9b). With `previous_response_id`, result ordering is not enforced; when building history manually keep results in call order. Only the latest assistant turn is validated against the current `tools`.
- Chat Completions: `finish_reason:"tool_calls"`, `message.tool_calls[]` (each with `id`, `function.name`, `function.arguments` string). Append the **entire** assistant message, then one `{"role":"tool","tool_call_id","content"}` per call. Omitting the assistant message returns 400.
- Messages: `stop_reason:"tool_use"`, return `tool_result` blocks in the next user message (Anthropic convention).

### tool_choice
- Responses and Chat Completions: only `"auto"` (default). `"none"`, `"required"`, and named functions return 400 [DOC]. You cannot force a tool call; always handle plain-text replies.
- Messages: `auto`, `any`, `none` accepted; named `{type:"tool"}` returns 400 [DOC].

### Streaming tool arguments [DOC]
- Responses: `response.output_item.added` (function_call, `in_progress`), `response.function_call_arguments.delta` chunks, `.done`, then `response.output_item.done` (`completed`). Do not discard an item before it is `completed`.
- Chat Completions: accumulate `tool_calls` deltas by `index`; set `stream_options:{"include_usage":true}` for a final usage chunk (an interrupted stream may omit usage).
- Messages: standard Anthropic SSE events.

### Tool search / deferred tools [DOC]
- Responses and Messages only (not Chat Completions); any tool-capable model.
- Add `{"type":"tool_search"}` to `tools`; set `defer_loading:true` on functions to defer (optionally grouped in `{"type":"namespace","name","description","tools":[...]}`; set `defer_loading` on the functions, not the namespace). Hosted mode needs at least one deferred tool; `defer_loading` without `tool_search` is a 400; only one `tool_search` per request.
- Client mode: `{"type":"tool_search","execution":"client","description":"...","parameters":{...}}`; the model emits a `tool_search_call` with a `call_id` and stops; you answer with a `tool_search_output` echoing that `call_id` and a `tools` array that defines what becomes callable. Changing the loaded set later breaks cache from that point.
- Output items to model in your enums: `tool_search_call`, `tool_search_output`, then `function_call` (may carry a `namespace` field).
- Incompatible with `text.format` json_schema and with `reasoning.effort:"none"` (400).
- Tips: namespace descriptions should be clear and high level; fewer than 10 functions per namespace.

---

## 4. PDF and image input

Sources: https://dev.meta.ai/docs/file-handling, /image-understanding, /api-reference/files, /protocols/messages

**Two ways to attach a PDF** [DOC]:
1. Inline (nothing stored): `file_data` as a base64 data URL, or `file_url` (public http/https; the server fetches it). Limit 50 MB (50,000,000 bytes). Whether that limit is measured before or after base64 expansion is not stated **[UNVERIFIED]**; for PDFs over about 35 MB use the Files API.
2. Files API: `POST /v1/files` multipart, `purpose=user_data`, `file=@...` (put `purpose` and `expires_after` parts before `file`, and `file` last, for lower latency). Returns `{"id":"file-...","object":"file","bytes","created_at","filename","purpose","status":"uploaded"}`. Up to 1 GiB per file; team storage 100 GiB; uploads fail with 400 at the quota. Default: no expiry. Set `expires_after[anchor]=created_at` and `expires_after[seconds]=3600..2592000` (1 hour to 30 days); both fields are required or you get 400. `GET /v1/files`, `GET /v1/files/{id}`, `DELETE /v1/files/{id}` for management.

Content-part shapes:
- Responses: `{"type":"input_file","filename":"doc.pdf","file_data":"data:application/pdf;base64,<B64>"}`, or `{"type":"input_file","file_url":"https://..."}`, or `{"type":"input_file","file_id":"file-..."}`.
- Chat Completions: `{"type":"file","file":{"filename":"doc.pdf","file_data":"data:application/pdf;base64,<B64>"}}`. A `file_id` form is not documented for Chat.
- Messages: `document` blocks with source `base64`/`url`/`file` exist, but PDF-specific shapes are not documented **[UNVERIFIED]**.

**PDF limits** [DOC]: text is extracted from the first 100 pages (later pages contribute no text); page images are kept for the first 50 pages only and those count against the 50-image per-request cap. So PDFs of 50 pages or fewer are fully covered (text + visuals). Split long PDFs before sending. Token cost per page is not documented; use `POST /v1/responses/input_tokens` to measure.

**Images** [DOC]:
- Responses: `{"type":"input_image","image_url":"https://... or data:image/png;base64,..."}` (a plain string) or `{"type":"input_image","file_id":"file-..."}`.
- Chat Completions: `{"type":"image_url","image_url":{"url":"..."}}` (object).
- Images must be in `user`-role messages; other roles are ignored.
- Formats: png, jpeg/jpg, gif, webp, x-icon. Max 50 images per request (400 beyond). 50 MB inline per image. Corrupt images return 400 `invalid_request_error` naming the image.
- **No `detail` option is documented** (no low/high/auto). Token cost scales with resolution: about 3 tokens for a 1x1, roughly 1,300-1,500 for a ~1280 px image, and about 110 tokens of fixed overhead for a short text request. The model may resize.
- Other input types on the same models: `video/mp4`, `audio/mpeg`, `audio/wav`. Audio quality on Spark 1.3 is flagged as degraded; Meta suggests 1.2 for audio.

---

## 5. Reasoning controls

Source: https://dev.meta.ai/docs/reasoning, /protocols/responses, /protocols/chat-completions, /protocols/messages, /api-reference/*/schemas

- Responses: `"reasoning":{"effort":"high","summary":"auto"}`. Chat Completions: top-level `"reasoning_effort":"high"`. Messages: `thinking:{"type":"adaptive"}` plus `output_config.effort` (`low|medium|high|xhigh`; the schema page also lists `max`). `thinking.type:"disabled"` is a 400; `enabled` with `budget_tokens` (>=1024, < max_tokens) is accepted but does not set depth.
- Values: `none` (a **400** on Muse Spark and Wonder, since they always reason), `minimal`, `low`, `medium`, `high`, `xhigh`, `max`.
- `max`: supported on `muse-spark-1.3` and `muse-wonder-1.0`; **not** on `muse-spark-1.3-contributor`. On the Model API, `max` does not select "Muse Wonder Full" (that is Muse Code only).
- Omit the parameter and the model picks its own level; no per-model default is documented. A secondary source (LiteLLM) lists only `minimal..xhigh` for Spark 1.3 and describes `max` as not yet available; the official reasoning page says `max` is supported, so test it **[UNVERIFIED]**.
- Raw chain of thought is never returned or streamed. Responses `summary` (`auto|concise|detailed`) populates `reasoning` item `summary[]` (can be empty `[]`); streaming events `response.reasoning_summary_text.delta/.done`. Chat Completions has `reasoning_content` but external callers get it redacted/empty.
- Across turns:
  - Responses, server-managed: `previous_response_id`. It carries turns only; `instructions`, `tools`, and `reasoning.effort` must be re-sent each call.
  - Responses, stateless (the docs' recommended mode): `store:false` + `include:["reasoning.encrypted_content"]`, then pass the returned `reasoning` items back in `input`. Replayed `reasoning` input items need a `summary` array (`[]` if empty) and must be followed by an assistant message or `function_call` before the next user/system/developer message, else 400. An intermediate assistant text before a tool call should carry `phase:"commentary"`.
  - **Cannot mix**: `previous_response_id` with `include:["reasoning.encrypted_content"]` returns 400 (`param:"include"`).
  - Chat Completions: not carried. Messages: replay `thinking`/`redacted_thinking` blocks.
- Token accounting: reasoning tokens are billed as output tokens and count against `max_output_tokens` / `max_completion_tokens`. Reported in `usage.output_tokens_details.reasoning_tokens` (Responses), `usage.completion_tokens_details.reasoning_tokens` (Chat), `usage.output_tokens_details.thinking_tokens` (Messages). The `reasoning_tokens` figure excludes user-visible preamble text. At `max`, give a generous output cap.
- Logprobs are unsupported on all Muse Spark models.

---

## 6. Context and output limits

Sources: https://dev.meta.ai/docs/models, /protocols/responses, /protocols/chat-completions, /quickstart, https://vercel.com/ai-gateway/models/muse-spark-1.3

- Context window: **1,048,576 tokens** for `muse-spark-1.3`, `muse-spark-1.3-contributor`, `muse-wonder-1.0` (and Spark 1.1/1.2) [DOC]. Modalities in: text, image, video, audio, PDF; out: text.
- Max output tokens: **not documented** on the models, pricing, quickstart, or API-reference pages [DOC gap]. The Responses page only says input + `max_output_tokens` must fit the window and that exceeding the "model maximum" is a 400. A Vercel AI Gateway page (secondary) lists 1,048,576 as the max output. Meta's own OpenCode sample config uses an output limit of 131,072, a client-side figure [DOC quickstart]. Plan around <=131,072 and treat anything higher as **[UNVERIFIED]**.
- Parameter names/minimums: Responses `max_output_tokens` (min 16); Chat Completions `max_completion_tokens` (`max_tokens` is a deprecated alias; `max_output_tokens` is rejected); Messages `max_tokens` (required).
- No long-context price premium [DOC pricing].
- `truncation`: the Responses feature page says only `"disabled"` is accepted (`"auto"` is rejected), so the server never trims history. You must compact it yourself. The schema also lists `context_management:[{"type":"compaction","compact_threshold":int}]`; its semantics are undocumented **[UNVERIFIED]**.
- Background responses (`background:true`): body capped at 1 MiB (413). Cannot be combined with `stream:true` on creation.

---

## 7. Rate limits, pricing, contributor mode

Sources: https://dev.meta.ai/docs/pricing-rate-limits, /models, /muse-code/subscriptions, /reasoning

Prices per 1M tokens, USD [DOC]:

| Model | Input | Cached input | Output |
|---|---|---|---|
| `muse-spark-1.3` (Standard) | $1.25 | $0.15 | $4.25 |
| `muse-spark-1.3-contributor` | $0.10 | $0.002 | $0.20 |
| `muse-wonder-1.0` (Standard only) | $4.00 | $0.20 | $20.00 |

- Contributor variant of Wonder: not available on pay-as-you-go. No long-context premium. Reasoning tokens bill as output. Web search grounding: $2.50 per 1,000 queries on top of tokens.
- Worked example, 100k input tokens + 10k output tokens (uncached): Spark standard about $0.168; contributor about $0.012; Wonder about $0.60.

Rate limits are **per team, per model**, not per key [DOC]:

| Model | RPM | TPM |
|---|---|---|
| `muse-spark-1.3` | 3,000 | 4,000,000 |
| `muse-wonder-1.0` | 3,000 | 4,000,000 |
| `muse-spark-1.3-contributor` | 150 | 3,000,000 |

- LiteLLM's launch post (secondary) says contributor is 100 RPM; the official page says 150. Trust the official number, and expect that it may change.
- Background-response submissions are capped at 600 per minute per team.
- Headers on successful token-based responses: `x-ratelimit-limit-tokens`, `x-ratelimit-remaining-tokens`, `x-ratelimit-limit-requests`, `x-ratelimit-remaining-requests`.
- Tier upgrades, batch discounts, free credits: not documented.
- `cached_tokens` shows prefix-cache hits (automatic; no minimum prefix length documented; optional `prompt_cache_key` and Responses-only `prompt_cache_retention: "in_memory"|"24h"`) https://dev.meta.ai/docs/prompt-caching.

**What "contributor" means for data** [DOC models page; a Muse Code page; press as SECONDARY]:
- Contributor is a cheaper model ID (`*-contributor`); in exchange, your content may be used to train future Meta models. The Muse Code page says the covered content is prompts, code, files and context you send, plus the model's responses; the terms of service govern the detail (https://dev.meta.ai/docs/muse-code/subscriptions). Press coverage (TechCrunch, Sep 2026) says prompts and outputs.
- Standard mode is the default; content is not used for training.
- On pay-as-you-go you opt in by choosing the contributor model ID. The "toggle off any time" behavior documented in Muse Code applies to Muse Code, not to the model ID choice.
- For Mechanical Muse: do not route user-uploaded PDFs or anything private through `-contributor`. It is fine for synthetic/demo data. The hackathon repo must be public, so keep API keys and any uploaded-file IDs out of it.

---

## 8. Errors and retries

Source: https://dev.meta.ai/docs/error-handling, /pricing-rate-limits

Error body (OpenAI-style): `{"error":{"message","type","param","code"}}`. `message` is the most reliable. `type`: `invalid_request_error`, `authentication_error`, `rate_limit_error`, `server_error`, `billing_error`. `code` is often null on validation errors; named codes include `invalid_api_key`, `model_not_found`, `file_not_found`, `rate_limit_exceeded`, `payload_too_large`, `gateway_timeout`, `server_shutting_down`, `service_overloaded`, `backend_unavailable`, `content_policy_violation`, `billing_not_configured`. Messages endpoint uses the Anthropic envelope (`overloaded_error` for 503, `api_error` for other 5xx, `permission_error` for 403, `not_found_error` for 404).

| Status | Meaning | Retry |
|---|---|---|
| 400 | bad params, oversized/undecodable media, malformed conversation, schema violation, content policy | No |
| 401 | missing/invalid/revoked key | No |
| 402 | billing | No |
| 403 | key lacks access | No |
| 404 | model/file not found | No (a 503 on model lookup means retry) |
| 413 | background body >1 MiB or `store=true` payload too large to persist | No (unchanged) |
| 429 | per-team RPM/TPM | **Yes**, honor `Retry-After` |
| 500 | server error (includes model-output parse failures) | **Yes** |
| 503 | overloaded/backend unavailable/shutting down | **Yes**; `Retry-After` 60 s for overloaded/unavailable, a few seconds for shutting down |
| 504 | non-streaming server time limit (duration not stated) | Usually not the same request; stream, use `background:true`, shrink input, or lower effort |

- Backoff guidance: exponential, start 0.5-1 s, double each retry, add 0-1 s jitter, 3-5 attempts, never retry 4xx other than 429. Wait at least `Retry-After`.
- Streaming (Responses): failures arrive as an SSE event, not an HTTP status: `response.failed` (status failed with `error`) or `error` (`code`, `message`). On `server_shutting_down` retry the whole request. A dropped/truncated stream can produce an empty turn; treat as transport failure and re-run [cookbook interleaved page]. The first event must arrive within a server limit; after that there is no time limit.
- Newly created stored responses can briefly 404 on GET or 400 on a chained `previous_response_id`; retry with short backoff.
- No request-id header is documented.
- Exceptions to the JSON envelope: unknown-path 404 has an empty body; `GET /v1/files` with a bad `order` returns `text/plain`.

---

## 9. Raw HTTP examples (hand-written from documented shapes; not run)

Set `export MODEL_API_KEY=LLM_...` first. Helper for all: `-H "Authorization: Bearer $MODEL_API_KEY" -H "Content-Type: application/json"`.

### 9a. Structured output (Responses, strict schema)

```bash
curl -sS https://api.meta.ai/v1/responses \
  -H "Authorization: Bearer $MODEL_API_KEY" \
  -H "Content-Type: application/json" \
  -d @- <<'JSON'
{
  "model": "muse-spark-1.3",
  "instructions": "Return one idea card for the user's prompt.",
  "input": [
    {"role": "user", "content": [{"type": "input_text", "text": "Idea seed: a clockwork heron that tunes itself to rain."}]}
  ],
  "reasoning": {"effort": "medium"},
  "max_output_tokens": 4000,
  "store": false,
  "text": {
    "format": {
      "type": "json_schema",
      "name": "idea_card",
      "description": "A single structured idea card.",
      "strict": true,
      "schema": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "title": {"type": "string"},
          "mood": {"type": "string", "enum": ["calm", "tense", "playful", "eerie"]},
          "tags": {"type": "array", "items": {"type": "string"}},
          "score": {"type": "number"},
          "notes": {"anyOf": [{"type": "string"}, {"type": "null"}]}
        },
        "required": ["title", "mood", "tags", "score", "notes"]
      }
    }
  }
}
JSON
```

Parse: find the `output[]` item with `"type":"message"`; take its `content[]` entry of `"type":"output_text"`; `serde_json::from_str(&text)`. First check top-level `status == "completed"` (else it may be `incomplete`) and look for a refusal part.

Chat Completions equivalent (differences only): URL `/v1/chat/completions`; `"messages":[{"role":"developer","content":"..."},{"role":"user","content":"..."}]`; `"reasoning_effort":"medium"`; `"max_completion_tokens":4000`; `"response_format":{"type":"json_schema","json_schema":{"name":"idea_card","strict":true,"schema":{...same...}}}`; read `choices[0].message.content` and check `finish_reason` is `stop` and `message.refusal` is null.

### 9b. Tool-calling turn plus tool result (Responses, stateless replay)

Turn 1 request:

```json
{
  "model": "muse-spark-1.3",
  "instructions": "You may call tools. Use them before answering.",
  "input": [
    {"role": "user", "content": [{"type": "input_text", "text": "What is the weather in Charlotte?"}]}
  ],
  "tools": [
    {
      "type": "function",
      "name": "get_weather",
      "description": "Return current weather for a city.",
      "strict": true,
      "parameters": {
        "type": "object",
        "additionalProperties": false,
        "properties": {"city": {"type": "string"}},
        "required": ["city"]
      }
    }
  ],
  "parallel_tool_calls": true,
  "reasoning": {"effort": "low"},
  "max_output_tokens": 4000,
  "store": false,
  "include": ["reasoning.encrypted_content"]
}
```

Expected response `output` (shape per docs; ids and ciphertext are illustrative):

```json
[
  {"id": "rs_abc", "type": "reasoning", "summary": [], "encrypted_content": "<opaque>"},
  {"id": "fc_abc", "type": "function_call", "status": "completed", "name": "get_weather", "call_id": "call_abc123", "arguments": "{\"city\":\"Charlotte\"}"}
]
```

Turn 2 request: same `model`, `instructions`, `tools`, `reasoning`, `store:false`, `include`; `input` is the original user message, then the **prior output items verbatim**, then your result(s):

```json
{
  "model": "muse-spark-1.3",
  "instructions": "You may call tools. Use them before answering.",
  "input": [
    {"role": "user", "content": [{"type": "input_text", "text": "What is the weather in Charlotte?"}]},
    {"id": "rs_abc", "type": "reasoning", "summary": [], "encrypted_content": "<opaque>"},
    {"id": "fc_abc", "type": "function_call", "status": "completed", "name": "get_weather", "call_id": "call_abc123", "arguments": "{\"city\":\"Charlotte\"}"},
    {"type": "function_call_output", "call_id": "call_abc123", "output": "{\"temp_c\": 18, \"sky\": \"clear\"}"}
  ],
  "tools": [
    {
      "type": "function",
      "name": "get_weather",
      "description": "Return current weather for a city.",
      "strict": true,
      "parameters": {
        "type": "object",
        "additionalProperties": false,
        "properties": {"city": {"type": "string"}},
        "required": ["city"]
      }
    }
  ],
  "reasoning": {"effort": "low"},
  "max_output_tokens": 4000,
  "store": false,
  "include": ["reasoning.encrypted_content"]
}
```

Loop until `output` has no `function_call` items. Every `function_call` needs a matching `function_call_output` (same `call_id`); keep results in call order.

Server-managed variant (documented alternative): drop `store:false` and `include`; turn 2 body is `{"model":..., "previous_response_id":"resp_...","input":[{"type":"function_call_output","call_id":"call_abc123","output":"..."}],"instructions":...,"tools":[...],"reasoning":{...}}`. Re-send `instructions`, `tools`, `reasoning.effort`. Stored responses persist server-side (`store` defaults to true).

Chat Completions variant:

```json
{
  "model": "muse-spark-1.3",
  "messages": [
    {"role": "user", "content": "What is the weather in Charlotte?"},
    {"role": "assistant", "content": null, "tool_calls": [{"id": "call_abc123", "type": "function", "function": {"name": "get_weather", "arguments": "{\"city\":\"Charlotte\"}"}}]},
    {"role": "tool", "tool_call_id": "call_abc123", "content": "{\"temp_c\": 18, \"sky\": \"clear\"}"}
  ],
  "tools": [
    {"type": "function", "function": {"name": "get_weather", "description": "Return current weather for a city.", "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}
  ]
}
```

### 9c. PDF plus prompt (Responses)

Inline base64:

```json
{
  "model": "muse-spark-1.3",
  "input": [
    {
      "role": "user",
      "content": [
        {"type": "input_file", "filename": "paper.pdf", "file_data": "data:application/pdf;base64,<BASE64_OF_PDF>"},
        {"type": "input_text", "text": "Summarize the three main claims in this paper."}
      ]
    }
  ],
  "reasoning": {"effort": "medium"},
  "max_output_tokens": 4000,
  "store": false
}
```

Files API route (better for big PDFs, reuse across calls, and keeps request bodies small):

```bash
curl -sS https://api.meta.ai/v1/files \
  -H "Authorization: Bearer $MODEL_API_KEY" \
  -F purpose=user_data \
  -F "expires_after[anchor]=created_at" \
  -F "expires_after[seconds]=86400" \
  -F "file=@paper.pdf"
# -> {"id":"file-...", ...}; then use {"type":"input_file","file_id":"file-..."} in the content array
```

Chat Completions PDF part: `{"type":"file","file":{"filename":"paper.pdf","file_data":"data:application/pdf;base64,<BASE64_OF_PDF>"}}` inside a user message's `content` array alongside `{"type":"text","text":"..."}`.

Token check before sending: `POST /v1/responses/input_tokens` with the same `model` and `input` returns `{"object":"response.input_tokens","input_tokens":N}`.

---

## 10. Gotchas for Rust and reqwest, plus uncertainty ledger

### Rust / reqwest
- Cargo: `reqwest = { version = "0.12", default-features = false, features = ["json","stream","multipart","rustls-tls"] }`, plus `base64`, `serde`, `serde_json`, and an SSE parser (`eventsource-stream` or hand-rolled on `bytes_stream()`). Use `.bearer_auth(key)`.
- Model `output[]` as `Vec<serde_json::Value>` or a tagged enum with a catch-all variant. Item types seen: `message`, `reasoning`, `function_call`, `tool_search_call`, `tool_search_output`, `web_search_call`. For stateless replay, store the raw `Value` of each item and re-send it unchanged; typed round-tripping will drop fields (`encrypted_content`, `id`, `status`, `namespace`).
- `function_call.arguments` and `function_call_output.output` are strings holding JSON: double-decode on the way in, double-encode on the way out. Always validate arguments (strict does not guarantee them).
- Do not use `output_text` convenience field alone; it can be null. Walk `output[].content[]`.
- Use `skip_serializing_if = "Option::is_none"` on every optional request field. The server 400s on many unsupported params and unknown `include` values; never send `stop`, `n>1`, `logprobs`, `logit_bias`, `prediction`, `truncation:"auto"`, `reasoning.effort:"none"`, `verbosity`, or `web_search_options`. Messages rejects unknown top-level fields and `stop_sequences`/`top_k`.
- Do not set both `temperature` and `top_p`. Leave defaults (1.0) per Meta's advice. `top_p:0` is a 400 on Chat.
- `store` defaults to **true**: responses are persisted server-side. Set `store:false` explicitly if you replay state yourself or handle private content.
- Timeouts: non-streaming requests have an undocumented server time limit that yields 504. With high reasoning effort, use `stream:true` (or `background:true` with polling; body <=1 MiB, no streaming at creation). A single `reqwest` `.timeout()` caps the whole request including body streaming, so for SSE prefer `read_timeout` (check your reqwest version) or `tokio::time::timeout` per chunk. Allow minutes for the first event on large inputs, since uncached long prompts delay it.
- Parse errors defensively: JSON envelope for most 4xx/5xx, but empty-body 404 for unknown paths and `text/plain` for some `/v1/files` 400s. Branch on status and `Content-Type`, then fall back to the raw body text.
- Honor `Retry-After` (seconds). Read `x-ratelimit-remaining-tokens/requests` to throttle proactively. RPM/TPM are per team, shared by every key, so a hackathon team shares one pool. Contributor is only 150 RPM.
- Multipart upload: `reqwest::multipart::Form` with `text("purpose","user_data")`, then `expires_after[anchor]` / `expires_after[seconds]` text parts, then `file` last (docs say this order lowers latency).
- Base64 inflates payload by about 4/3. A 40 MB PDF becomes about 53 MB; prefer the Files API above roughly 30 MB.
- SSE: Chat Completions streams end with `[DONE]`; the Responses retrieval stream (`GET /v1/responses/{id}?stream=true`) emits lifecycle events only, supports `starting_after`, and ends with `[DONE]`. Handle `response.completed`, `response.incomplete`, `response.failed`, and `error`. Reasoning text is never streamed; only summaries if requested.
- With `previous_response_id`, `usage` covers only the new turn; with hosted tools (web_search), `usage.input_tokens` is cumulative over iterations and overstates context. Use the count endpoint to measure context.
- A model switch means a recount: tokenization is model-specific. Comparing the three models means three separate count calls.
- Function names: at most one dot; avoid names `tool_search`, `browser.*` when web search is on.
- Anthropic SDK-style responses begin with a `redacted_thinking` block; never read `content[0]` as text.
- Prompt caching is automatic prefix caching: keep system prompt, tools, and schema first and stable; put user content last. Do not put timestamps in the prefix. Optional `prompt_cache_key` helps routing at volume.

### Comparison-run notes for the three models
- `muse-spark-1.3` vs `-contributor`: same capabilities except no `max` effort on contributor, a 150 RPM team cap, and training-data use. `muse-wonder-1.0` has no contributor variant on pay-as-you-go, costs about 3.2x input and 4.7x output of Spark standard, and supports `max`.
- Wonder's page lists features (sub-agents, parallel tool calls, computer use) that go beyond Spark. Whether anything beyond the shared Responses surface applies to API callers is **[UNVERIFIED]**.

### Uncertainty ledger (undocumented or contradictory; verify with a live call)
1. Max output tokens per model (undocumented; secondary says 1,048,576; sample client config uses 131,072).
2. Whether `text.format` json_schema works in the same request as function tools, and how it behaves with `reasoning` at high/max effort.
3. Refusal and incomplete shapes for structured output on Responses (`incomplete_details.reason` values; refusal content part).
4. Support for JSON Schema keywords beyond the documented ones (minimum/maximum, pattern, format, minItems/maxItems, type arrays, default); non-strict `oneOf`/`allOf` behavior.
5. `strict:true` on `output_config.format` for Messages, and the Messages limits.
6. PDF `document` block shape on Messages; whether the 50 MB inline limit counts base64 expansion.
7. Which endpoints serve `muse-wonder-1.0` besides Responses.
8. Whether `max` effort actually works on `muse-spark-1.3` (official page says yes; a LiteLLM post says not yet).
9. Contributor RPM: official 150, LiteLLM post 100.
10. Replaying output items verbatim (including `id`/`status` fields) as `input` items is the expected pattern but docs only say structural validation is stricter on hand-built arrays.
11. Server time limit before 504, and whether the status endpoint needs auth.
12. `truncation` enum (schema lists `auto`; feature page says only `disabled` is accepted) and what `context_management` compaction does.
13. Image `detail` control: none documented.
14. Contributor data-use scope and retention for pay-as-you-go: governed by the Meta Model API terms, which were not read.

### Source index
- https://dev.meta.ai/docs/overview, /models, /quickstart, /authentication, /sdks, /pricing-rate-limits, /error-handling
- https://dev.meta.ai/docs/structured-output, /tool-calling, /tool-search, /search-grounding, /file-handling, /image-understanding, /reasoning, /token-counting, /prompt-caching, /video-understanding
- https://dev.meta.ai/docs/protocols, /protocols/responses, /protocols/chat-completions, /protocols/messages
- https://dev.meta.ai/docs/api-reference/responses/schemas, /api-reference/chat-completions/schemas, /api-reference/messages/schemas, /api-reference/messages/create-message, /api-reference/models/schemas, /api-reference/status
- https://dev.meta.ai/docs/cookbook/structured-output, /cookbook/tool-function-calling, /cookbook/basic-agent-loop, /cookbook/interleaved-reasoning-tool-use, /cookbook/alert-fatigue-copilot
- https://dev.meta.ai/docs/muse-code/subscriptions, https://dev.meta.ai/models/muse-wonder
- Secondary: https://docs.litellm.ai/blog/muse_spark_1_3, https://vercel.com/ai-gateway/models/muse-spark-1.3, https://techcrunch.com/2026/09/03/meta-is-paying-to-peek-at-how-you-use-their-latest-ai-model/, https://wersm.com/meta-muse-spark-contributor-pricing-training-data/
