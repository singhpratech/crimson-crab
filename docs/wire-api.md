# Anthropic API — Wire Reference (authoritative for this SDK)

Source: official Anthropic API documentation, captured 2026-07-16. **Do not guess shapes — model exactly what is here.** Where a field is not listed, prefer forward-compatible catch-alls over inventing fields.

Base URL: `https://api.anthropic.com`

## Required headers

| Header | Value |
|---|---|
| `content-type` | `application/json` |
| `x-api-key` | API key (`sk-ant-...`) |
| `anthropic-version` | `2023-06-01` |
| `anthropic-beta` | comma-separated beta flags, only when needed |

## POST /v1/messages

Request body (core fields):

```jsonc
{
  "model": "claude-opus-4-8",            // required
  "max_tokens": 16000,                    // required
  "messages": [                           // required; first must be user
    {"role": "user", "content": "hi"},   // content: string shorthand OR array of content blocks
    {"role": "assistant", "content": [{"type": "text", "text": "..."}]}
  ],
  "system": "..." ,                       // optional; string OR [{type:"text", text, cache_control?}]
  "metadata": {"user_id": "..."},        // optional
  "stop_sequences": ["..."],             // optional
  "stream": true,                          // optional, default false
  "thinking": {"type": "adaptive", "display": "summarized"},
  //   variants: {"type":"adaptive", "display"?: "summarized"|"omitted"}
  //             {"type":"enabled", "budget_tokens": N}   (older models only)
  //             {"type":"disabled"}
  "output_config": {
    "effort": "high",                     // low | medium | high | xhigh | max
    "format": {"type": "json_schema", "schema": { /* JSON Schema, additionalProperties:false */ }}
  },
  "tools": [ /* see Tools */ ],
  "tool_choice": {"type": "auto"},       // auto | any | none | {"type":"tool","name":"..."}
  //   any tool_choice may include "disable_parallel_tool_use": true
  "cache_control": {"type": "ephemeral"}  // optional top-level: auto-cache last cacheable block
}
```

Notes:
- `temperature`/`top_p`/`top_k` exist on older models but are REJECTED (400) on Opus 4.7/4.8, Sonnet 5, Fable 5. Include them as `Option` fields; do not set defaults.
- Consecutive same-role messages allowed (API merges). First message must be `user`.
- Assistant prefill (trailing assistant message) 400s on 4.6+ models — allowed by SDK, server enforces.

### Content blocks — request (`ContentBlockParam`)

| type | fields |
|---|---|
| `text` | `text: String`, `cache_control?` |
| `image` | `source`: `{type:"base64", media_type:"image/png"..., data}` \| `{type:"url", url}` \| `{type:"file", file_id}`; `cache_control?` |
| `document` | `source`: `{type:"base64", media_type:"application/pdf", data}` \| `{type:"url"→ url}` (as `url_pdf`? no — `{type:"url", url}`) \| `{type:"text", media_type:"text/plain", data}` \| `{type:"file", file_id}` \| `{type:"content", content:[blocks]}`; `title?`, `context?`, `citations?: {enabled: bool}`, `cache_control?` |
| `tool_use` | `id`, `name`, `input: object` (echoing assistant turn back) |
| `tool_result` | `tool_use_id`, `content`: string OR array of `text`/`image` blocks, `is_error?: bool`, `cache_control?` |
| `thinking` | `thinking: String`, `signature: String` (echo back unchanged) |
| `redacted_thinking` | `data: String` |
| `server_tool_use` / `web_search_tool_result` / etc. | echo verbatim when replaying history — keep raw-Value passthrough |

### Content blocks — response (`ContentBlock`)

| type | fields |
|---|---|
| `text` | `text`, `citations?: [...]` |
| `tool_use` | `id`, `name`, `input: object` |
| `thinking` | `thinking: String` (may be empty when display=omitted), `signature: String` |
| `redacted_thinking` | `data` |
| `server_tool_use` | `id`, `name`, `input` |
| `web_search_tool_result` | `tool_use_id`, `content`: array of results OR error object `{type:"web_search_tool_result_error", error_code}` |
| `fallback` | `from: {model}`, `to: {model}` (Fable 5 server-side fallbacks) |
| *(unknown)* | MUST deserialize to catch-all, never error |

### Response (Message)

```jsonc
{
  "id": "msg_01...",
  "type": "message",
  "role": "assistant",
  "model": "claude-opus-4-8",
  "content": [ /* ContentBlock[] */ ],
  "stop_reason": "end_turn",   // end_turn | max_tokens | stop_sequence | tool_use | pause_turn | refusal | model_context_window_exceeded | null (mid-stream)
  "stop_sequence": null,
  "stop_details": null,        // populated ONLY when stop_reason=="refusal": {"type":"refusal","category":"cyber"|"bio"|...|null,"explanation": "..."}
  "usage": {
    "input_tokens": 10,
    "output_tokens": 25,
    "cache_creation_input_tokens": 0,
    "cache_read_input_tokens": 0
    // forward-compat: may include service_tier, iterations, inference_geo, etc. — flatten extras
  },
  "container": null            // {"id": "..."} when code-execution used
}
```

Response header `request-id: req_...` — capture into errors and expose on responses if convenient.

### Tools

Custom tool definition:
```jsonc
{
  "name": "get_weather",
  "description": "Get current weather for a location",
  "input_schema": {"type":"object","properties":{"location":{"type":"string"}},"required":["location"]},
  "strict": true,              // optional; requires additionalProperties:false + required
  "cache_control": {"type":"ephemeral"}   // optional, on last tool to cache tool block
}
```
Server tools (pass through as raw JSON — do not model each): `{"type":"web_search_20260209","name":"web_search", ...}`, `{"type":"code_execution_20260120","name":"code_execution"}`, `{"type":"bash_20250124","name":"bash"}`, `{"type":"text_editor_20250728","name":"str_replace_based_edit_tool"}`, `{"type":"memory_20250818","name":"memory"}` etc.

Tool loop: response `stop_reason == "tool_use"` → execute → append assistant `content` verbatim, then a user message whose content is ALL `tool_result` blocks (one per `tool_use.id`) in a single message. `pause_turn` → re-send history with the paused assistant turn appended; server resumes.

### cache_control

`{"type": "ephemeral"}` or `{"type": "ephemeral", "ttl": "1h"}` (values `"5m"`/`"1h"`). Placeable on system text blocks, tool definitions, message content blocks. Max 4 breakpoints. Also valid as a top-level request field (auto-place on last cacheable block).

## Streaming (SSE)

Request: same body + `"stream": true`. Response: `text/event-stream`. Records separated by blank line; each has `event: <name>` and `data: <json>` lines. There may be `ping` events; unknown event types must be tolerated.

Event sequence for a simple text response:

```
event: message_start
data: {"type":"message_start","message":{"id":"msg_...","type":"message","role":"assistant","model":"...","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":25,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":12}}

event: message_stop
data: {"type":"message_stop"}
```

Delta types inside `content_block_delta.delta`:
- `text_delta` → `{"text": "..."}` (append to text block at `index`)
- `input_json_delta` → `{"partial_json": "..."}` (concatenate fragments; final concatenation parses as the tool_use `input` object; empty-input tools may emit `""` → treat as `{}`)
- `thinking_delta` → `{"thinking": "..."}`
- `signature_delta` → `{"signature": "..."}` (arrives before the thinking block's `content_block_stop`)
- `citations_delta` → `{"citation": {...}}` (append to text block citations)

Other stream events: `ping` (`{"type":"ping"}`), `error` (`{"type":"error","error":{"type":"overloaded_error","message":"..."}}`) — an in-stream error terminates the message; surface as an error item.

Accumulation contract: `message_start.message` is the base; content blocks assembled by index from start/delta/stop; `message_delta.delta.stop_reason/stop_sequence` merge into the message; `message_delta.usage` merges (fields are cumulative totals, replace not add); result must equal the non-streaming `Message` shape.

## Errors (non-2xx)

Envelope:
```json
{"type": "error", "error": {"type": "invalid_request_error", "message": "..."}, "request_id": "req_..."}
```

| HTTP | error.type | retryable |
|---|---|---|
| 400 | `invalid_request_error` | no |
| 401 | `authentication_error` | no |
| 403 | `permission_error` (also `billing_error`) | no |
| 404 | `not_found_error` | no |
| 413 | `request_too_large` | no |
| 429 | `rate_limit_error` | yes (honor `retry-after` seconds header) |
| 500 | `api_error` | yes |
| 529 | `overloaded_error` | yes |

Also retry: 408, 409, network/connection failures. Rate-limit headers: `retry-after`, `x-ratelimit-limit-*`, `x-ratelimit-remaining-*`.

## POST /v1/messages/count_tokens

Body: `model`, `messages`, optional `system`, `tools`, `thinking` (same shapes as /v1/messages; NO `max_tokens`). Response: `{"input_tokens": 2095}`.

## GET /v1/models and GET /v1/models/{id}

List response: `{"data": [ModelInfo...], "has_more": bool, "first_id": "...", "last_id": "..."}` — pagination via `?limit=&after_id=&before_id=`.
ModelInfo: `{"id": "claude-opus-4-8", "display_name": "Claude Opus 4.8", "created_at": "...", "type": "model", "max_input_tokens": 1000000, "max_tokens": 128000, "capabilities": { /* nested {supported: bool} tree — keep as serde_json::Value */ }}`.

## Message Batches — /v1/messages/batches

- `POST /v1/messages/batches` body: `{"requests": [{"custom_id": "r1", "params": { /* full non-streaming /v1/messages body */ }}]}`
- Response `MessageBatch`: `{"id":"msgbatch_...","type":"message_batch","processing_status":"in_progress"|"canceling"|"ended","request_counts":{"processing":N,"succeeded":N,"errored":N,"canceled":N,"expired":N},"created_at":...,"expires_at":...,"results_url": "...|null", ...}`
- `GET /v1/messages/batches/{id}` → MessageBatch (poll until `processing_status == "ended"`)
- `GET /v1/messages/batches` → paginated list (`data`, `has_more`, `first_id`, `last_id`; `?limit=&after_id=&before_id=`)
- `POST /v1/messages/batches/{id}/cancel` → MessageBatch
- `GET /v1/messages/batches/{id}/results` → **JSONL stream**, one result per line, ANY order:
  `{"custom_id":"r1","result":{"type":"succeeded","message":{/* Message */}}}`
  result.type ∈ `succeeded` | `errored` (`{"error": {envelope}}`) | `canceled` | `expired`.

## Beta flags of note (SDK just passes them; do not hard-code behavior)

`files-api-2025-04-14`, `task-budgets-2026-03-13`, `compact-2026-01-12`, `context-management-2025-06-27`, `fast-mode-2026-02-01` (+ top-level `"speed":"fast"`), `server-side-fallback-2026-06-01` (+ top-level `"fallbacks":[{"model":"..."}]`), `mcp-client-2025-11-20` (+ `mcp_servers` top-level). The request builder must therefore accept arbitrary extra top-level JSON fields (`extra_body: serde_json::Map`) so users can use new betas without an SDK release.

## Current model IDs (for `models_ids.rs` constants + docs)

`claude-fable-5`, `claude-mythos-5` (Mythos-class, restricted availability), `claude-opus-4-8`, `claude-opus-4-7`, `claude-opus-4-6`, `claude-sonnet-5`, `claude-sonnet-4-6`, `claude-haiku-4-5` (full: `claude-haiku-4-5-20251001`), legacy: `claude-opus-4-5`, `claude-sonnet-4-5`.

**Future-model contract:** the `model` field is an OPEN STRING everywhere in the SDK — never an enum. Constants are conveniences only; any model ID Anthropic ships tomorrow must work with zero SDK changes. Runtime capability discovery goes through GET /v1/models (`max_input_tokens`, `max_tokens`, `capabilities` tree kept as raw JSON precisely so new capability keys appear without an SDK release).
