# Orbix Handoff

Hand a task plan to Orbix Core and it executes locally on the free
on-device model. You (a frontier model or any tool) do the planning
turn; Orbix does the forty execution turns. This document is the whole
contract - it is deliberately small and stable.

## Endpoint

```
POST http://127.0.0.1:51231/handoff
Authorization: Bearer <token>        (or x-api-key: <token>)
Content-Type: application/json
```

The endpoint is off by default. The user turns it on in
Orbix Settings > Models > "Expose local model endpoint", which also
shows the token.

## The spec

```json
{
  "title": "Competitor scan",
  "goal": "Summarise what our top 3 competitors shipped this quarter",
  "tone": "dry, factual",
  "deliverable": "doc",
  "steps": [
    { "kind": "tool", "instruction": "Fetch the changelog",
      "tool": "web_fetch", "args": { "url": "https://example.com/changelog" } },
    { "kind": "action", "instruction": "ls ~/Documents/research" },
    { "kind": "prompt", "instruction": "Summarise the findings so far in 5 bullets" }
  ]
}
```

| Field | Required | Meaning |
|---|---|---|
| `title` | yes | 1-120 chars. Names the run and any deliverable. |
| `goal` | no | One sentence of intent; injected as context into every prompt step. |
| `tone` | no | Style hint for prompt steps. |
| `steps` | yes | 1-50 entries, executed in order. Each step sees the previous steps' outputs. |
| `deliverable` | no | `doc` (branded .docx in ~/Documents/Orbix Documents), `panel` (a live panel in My UI), or `none` (default). |

### Step kinds

| kind | What runs | Fields |
|---|---|---|
| `prompt` | The local model, with prior step outputs as context. | `instruction` = the prompt |
| `action` | A shell command on the user's Mac. | `instruction` = the command |
| `tool` | A built-in Orbix tool. | `tool` = `shell_exec` \| `file_read` \| `file_write` \| `file_edit` \| `file_list` \| `web_fetch`, `args` = its arguments, `instruction` = human label |

## Responses

Default: an SSE stream (`text/event-stream`), one JSON object per
`data:` line.

```
data: {"event":"accepted","run_id":"...","workflow_id":"...","steps":3}
data: {"event":"step","index":0,"label":"Fetch the changelog","status":"running"}
data: {"event":"step","index":0,"label":"Fetch the changelog","status":"done"}
data: {"event":"done","status":"completed","result":"...","steps":[...],"deliverable":{"kind":"doc","path":"..."}}
```

Fire-and-forget: append `?wait=false` to get an immediate
`202 {"accepted":true,"run_id":"...","workflow_id":"..."}`; progress is
then visible in the Orbix app's Run surfaces.

Errors: `400 {"accepted":false,"error":"<what to fix>"}` for an invalid
spec, `401` for a missing or wrong token.

## Guarantees

- Runs are durable: a killed app resumes the run at the step it was on.
- Every run is visible and stoppable in the Orbix app.
- Nothing in the spec or its outputs leaves the Mac unless a step
  itself calls out (e.g. `web_fetch`).

## Telling a model to emit this

A single instruction suffices:

> Emit an Orbix handoff: a JSON object with title, goal, steps
> (kind: prompt | action | tool) per docs/orbix-handoff.schema.json.
> Prefer prompt steps for reasoning and tool steps for I/O.

The JSON Schema next to this file validates the envelope.
