# ResumeForge — Architecture v3 Addendum: Agent Orchestration Canvas

Builds on v2. Nothing here changes the pipeline's logic — it changes what the
user can *see* while it runs: a live graph of agents/stages, the actual
commands each one issues, streamed output, and generated artifacts (LaTeX
diffs, JSON, PDFs) appearing as they're produced, rather than a status word
that changes every few seconds.

This is the same shape ADK, LangGraph Studio, and AutoGen Studio all converge
on: a workflow of typed agent nodes (sequential / parallel / loop), each
emitting structured events onto a shared bus that a UI subscribes to. You
don't need their frameworks — you need their *event model*, implemented in
~300 lines of Rust.

---

## 1. Reframe the pipeline as an agent graph

Every v2 pipeline stage becomes a node with a declared type, borrowing ADK's
vocabulary because it maps cleanly onto what you already have:

```text
ResumeGenerationWorkflow  (SequentialAgent)
│
├── JDAnalyzerAgent            (deterministic — Rust regex/heuristics)
├── ProjectRetrievalAgent      (deterministic — keyword/tech-overlap ranking)
├── ContentGenerationAgent     (LLM-backed — the actual agy call)
├── EvidenceValidatorAgent     (deterministic — accept/reject bullets)
└── RenderCompileLoopAgent     (LoopAgent, max 4 iterations)
    ├── LatexRenderAgent       (deterministic — placeholder injection)
    ├── CompileAgent           (deterministic — Tectonic)
    └── PageCheckAgent         (deterministic — pass / shorten / tighten)
```

The only node that's a "real" LLM agent is `ContentGenerationAgent`. The rest
are deterministic Rust — and that's fine. In ADK terms they're workflow/tool
agents, not LLM agents, but they still belong on the canvas: seeing "Project
Retrieval Agent: ranked 34 projects, top match TurfNet (0.91)" appear live is
exactly as valuable as watching the LLM call, and it means the canvas isn't a
black box for 90% of the pipeline and only "alive" for one step.

**Design rule:** every node, LLM-backed or not, emits the same event shape.
The UI never needs to know which nodes are "real" agents.

---

## 2. Normalized event schema

Don't couple the canvas to `agy`'s raw `stream-json` shape directly — that
format has already changed across versions in ways outside your control
(Section 2 of v2). Put a thin adapter in front of it that translates whatever
`agy` actually emits into your own stable schema. Every node — deterministic
or LLM — emits only this:

```json
{
  "resume_id": 48,
  "node": "content_generation",
  "seq": 17,
  "ts": "2026-09-27T20:46:03Z",
  "type": "command_started | command_output | thinking | tool_call
          | artifact_produced | node_done | node_error",
  "payload": { }
}
```

Examples per type:

```json
{ "type": "command_started",
  "payload": { "command": "agy -p <redacted prompt> --output-format stream-json --non-interactive" } }

{ "type": "command_output",
  "payload": { "chunk": "Analyzing job description for core skills..." } }

{ "type": "artifact_produced",
  "payload": { "kind": "latex_diff", "before": "...", "after": "..." } }

{ "type": "artifact_produced",
  "payload": { "kind": "json", "content": { "skills": ["Java","PostgreSQL"] } } }

{ "type": "node_done",
  "payload": { "summary": "4 relevant projects found" } }
```

`payload.command` is always redacted of secrets/tokens before it's ever
constructed, not before display — never build the display string from
something that could contain a raw credential.

This is also where you defend against `agy` reliability issues from v2: if
the PTY reader hits `ai_empty_response` or `ai_hung`, that becomes a normal
`node_error` event on the canvas — "Content Generation Agent: no response
after 30s, retrying" — instead of an opaque hang the user just has to trust.
The orchestration layer turns your existing failure classification into
something visible, which is a genuine upgrade, not just decoration.

---

## 3. Transport: event bus + WebSocket, with replay

```text
generation_queue worker (v2, Section 9)
   │
   ├── writes every event to generation_events (already in v2 schema)
   └── publishes every event onto a per-resume tokio::sync::broadcast channel
                    │
                    ▼
        GET /ws/resumes/:id/live   (Axum WebSocket upgrade)
                    │
                    ▼
              Canvas UI (browser)
```

**Reconnect / refresh handling:** on WS connect, first replay everything
already in `generation_events` for that resume_id (so refreshing mid-generation
doesn't lose history), then subscribe to the live broadcast channel for new
events. This is why v2 already having a `generation_events` table pays off
here — you're not building persistence and live streaming as two separate
things, the live channel is just "the tail of the same log."

One broadcast channel per in-flight generation, torn down when the resume
reaches a terminal status. Since v2 already serializes generation through a
single-worker queue, you never have more than one channel actively producing
events at a time — keeps this simple for V1.

---

## 4. The Canvas UI

Vanilla JS + inline SVG (matches your existing stack — no React, no canvas
library needed since the graph is fixed-layout, not force-directed).

```text
+---------------------------------------------------------------+
|  Microsoft . Software Engineer Intern              [*] Live   |
+---------------------------------------------------------------+
|                                                                 |
|   oJD Analyzer -> oProject      -> oContent    -> oEvidence     |
|    v done          Retrieval        Generation      Validator  |
|                     v done          o running       o waiting  |
|                                        |                        |
|                                        v                        |
|                              +------------------+               |
|                              | oRender >oCompile |               |
|                              |       >oPageCheck | (loop x2)     |
|                              +------------------+               |
|                                                                 |
+---------------------------------------------------------------+
| Content Generation Agent                          [collapse]  |
| $ agy -p "<prompt>" --output-format stream-json --non-inter... |
|                                                                 |
| > Analyzing job description for core skills...                |
| > Matched TurfNet, TrustSplit, Government Project              |
| > Drafting summary...                                          |
| > { "summary": "Backend-focused SWE intern with..." }          |
|                                                                 |
+---------------------------------------------------------------+
```

- Node states: `idle` (hollow circle) -> `running` (pulsing) -> `done` (filled
  check) -> `error` (filled X, red). SVG + CSS keyframe animation, no JS
  animation loop needed.
- Edges animate (dashed line, moving offset) only while the upstream node is
  `running`, to show data "in flight" toward the next node.
- Clicking a node expands its console panel below the graph — raw
  `command_output` chunks stream in as they arrive (WS message -> append to a
  scroll region), same pattern as a terminal.
- `artifact_produced` events with `kind: "latex_diff"` render as a compact
  unified diff (+/- lines, monospace, red/green) directly in the console —
  this is the "seeing code being made" part. `kind: "json"` renders as
  syntax-highlighted collapsed/expandable JSON.
- The `RenderCompileLoopAgent` box visually shows iteration count ("loop x2 of
  4") so a Stage-A/B shortening retry (v2 Section 7) is legible as "trying
  again," not a confusing reappearance of an already-"done" node.

---

## 5. What each node concretely surfaces

| Node | command_started | command_output examples | artifact_produced |
|---|---|---|---|
| JD Analyzer | *(none, pure Rust)* | "Extracted 6 core skills, 3 secondary" | JD analysis JSON |
| Project Retrieval | *(none, pure Rust)* | "Ranked 34 projects" | ranked list JSON |
| Content Generation | actual `agy` invocation (redacted) | streamed reasoning/text chunks | resume-content JSON |
| Evidence Validator | *(none, pure Rust)* | "2 claims rejected: no evidence" | accepted/rejected JSON |
| LaTeX Render | *(none, pure Rust)* | "Injected into 4 placeholders" | before/after `.tex` diff |
| Compile | `tectonic -X compile resume.tex` | raw tectonic log tail | — |
| Page Check | *(none, pure Rust)* | "2 pages — shortening (attempt 2/4)" | page count |

Every row exists whether or not an LLM is involved — that's what makes the
canvas feel alive for the full run, not just during the one network call.

---

## 6. Additions to v2's schema / API

`generation_events` (v2) needs two more columns to carry the richer payload:
```text
event_type   TEXT   -- 'command_started' | 'command_output' | 'artifact_produced' | ...
payload_json TEXT   -- the JSON blob shown above
```
(`stage`/`detail` from v2 can be derived from these rather than kept
separately — collapse into one table.)

New API surface:

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/ws/resumes/:id/live` | WebSocket upgrade; replays history then streams |
| GET | `/api/resumes/:id/events` | Non-WS fallback: full event log as JSON (already in v2) |

No new tables beyond the two columns above — the orchestration layer is a
view/transport concern on top of data v2 already persists, which is the
point of designing it this way.

---

## 7. Scope check — what's genuinely V1 vs. what to defer

Be honest about what this adds: a WebSocket endpoint, a broadcast-channel
plumbing layer, an event-normalization adapter, and a non-trivial piece of
frontend (SVG graph + live console + diff rendering). That's a real chunk of
work, comparable in size to the LaTeX pipeline itself.

**Keep in V1:** the event bus + WebSocket + linear node graph with live
console text. This is what makes the system trustworthy and debuggable, and
it directly reuses the `stream-json` parsing you already need for the
Antigravity wrapper — you're not building parallel infrastructure.

**Push to V1.1:** the diff-rendering polish (syntax highlighting, collapsible
JSON trees), edge animations, and anything about making the canvas visually
impressive rather than functionally transparent. A plain scrolling log under
each node is enough to prove the concept; the animated SVG graph is the part
that's fun to build but adds the least functional value per hour.

---

## 8. Dev sequence insertion point

Add after v2 step 9 (Antigravity wrapper with failure classification) and
before step 20 (full pipeline wired end to end):

```text
9.  Antigravity wrapper (PTY + stream-json + failure classification)  [v2]
9a. Event bus: tokio::sync::broadcast per resume_id                   [v3]
9b. Adapter: agy stream-json -> normalized event schema                [v3]
9c. generation_events schema extended (event_type, payload_json)      [v3]
9d. WebSocket endpoint + replay-then-subscribe                        [v3]
9e. Minimal canvas UI: static node list + live console, no SVG yet    [v3]
...
20. Full pipeline wired end to end — now every node emits events,
    canvas shows the whole run live, not just Antigravity's part
...
26. SVG graph polish, diff rendering, edge animation (V1.1)           [v3]
```

This keeps the orchestration layer additive rather than a rewrite: it plugs
into the queue worker and the Antigravity wrapper you were already building,
and the "boring" version (plain log, no graph) is a small, real milestone you
hit on the way to the polished one — not an all-or-nothing feature.
