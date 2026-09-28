# MASTER BUILD PROMPT — ResumeForge

Paste this entire file as the system/task prompt to a coding agent (Claude
Code, Antigravity, Cursor, or a human developer) to build ResumeForge from
scratch. It is self-contained: schema, API, file layout, state machine, and
sequencing are all specified below. The two supporting documents in `docs/`
(`architecture-v2-hardened.md`, `architecture-v3-orchestration.md`) contain
the reasoning behind each decision if you need to resolve an ambiguity — this
file is the executable summary of both.

---

## 0. What you are building

ResumeForge is a **local-first resume generation system**. It runs entirely
on the user's laptop (a Rust/Axum server on localhost) and does one thing:

```text
Master LaTeX Resume + GitHub Project Knowledge + Job Description
  + Optional Instructions
      ↓  (agent pipeline, see Section 5)
New Tailored LaTeX Resume → PDF, guaranteed exactly 1 page
      ↓
Saved as an immutable, versioned resume record
      ↓
Preview / Download / Regenerate / Delete, watched live on a canvas UI
```

Non-negotiable design principles, in priority order:

1. **Rust owns everything deterministic**: files, database, GitHub, LaTeX
   rendering, PDF compilation, page counting, deletion. The AI never touches
   the filesystem, never runs shell commands, never controls document layout.
2. **The AI (Antigravity CLI, `agy`) only reasons and rewrites text.** It
   receives pre-assembled context and returns structured content. It does not
   discover projects, does not decide file structure, does not choose fonts
   or margins.
3. **Every generated resume is immutable** once created. Regeneration creates
   a new version; nothing is ever overwritten in place.
4. **Every claim on a resume must trace to evidence** from the user's own
   repositories. Unevidenced claims are rejected before rendering, not caught
   after the fact.
5. **The `agy` CLI is currently unreliable in non-TTY (subprocess) contexts**
   — confirmed reports of silent zero-byte output and indefinite hangs when
   piped rather than attached to a terminal. Build the integration assuming
   this, not hoping around it. See Section 6.
6. **Nothing here needs a cloud backend, accounts, React, a vector database,
   Docker, or an agent framework.** See Section 11 for the full exclusion list.

---

## 1. Technology stack (do not substitute without a stated reason)

| Layer | Technology |
|---|---|
| Frontend | HTML + CSS + vanilla JavaScript (no framework, no build step) |
| Backend | Rust, Axum, Tokio |
| Database | SQLite via SQLx (bundled, no separate DB server) |
| Serialization | Serde / serde_json |
| GitHub access | `gh` CLI for metadata/auth, `git` shallow clone for content |
| Secret scanning | `gitleaks` binary, invoked as a subprocess |
| AI | Google Antigravity CLI (`agy`), spawned through a PTY |
| PTY handling | `portable-pty` or `pty-process` crate |
| Resume source | LaTeX |
| PDF compiler | Tectonic, with a pre-fetched local bundle |
| Live UI transport | Axum WebSocket upgrade + `tokio::sync::broadcast` |
| Logs | `tracing` + `tracing-subscriber` |

---

## 2. File structure

Create exactly this layout:

```text
resumeforge/
├── Cargo.toml
├── migrations/
│   └── 001_initial.sql
├── src/
│   ├── main.rs                     # boot, startup recovery sweep, Axum router
│   ├── config.rs
│   ├── state.rs                    # AppState: db pool, queue handle, broadcast registry
│   ├── errors.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── system.rs                # health checks incl. antigravity probe, tectonic bundle
│   │   ├── github.rs
│   │   ├── templates.rs             # master resume CRUD + Tier-2 adaptation
│   │   ├── projects.rs
│   │   ├── resumes.rs               # CRUD, regenerate, delete, pdf/tex download
│   │   └── live.rs                  # WebSocket endpoint, event replay + subscribe
│   ├── services/
│   │   ├── mod.rs
│   │   ├── command_runner.rs        # generic subprocess helper (gh, git, gitleaks, tectonic)
│   │   ├── pty_runner.rs            # PTY spawn + stream reader for agy specifically
│   │   ├── antigravity.rs           # prompt construction, failure classification
│   │   ├── event_bus.rs             # normalized event schema, broadcast + persist
│   │   ├── github.rs
│   │   ├── github_indexer.rs        # shallow clone, SHA diffing, manifest parsing
│   │   ├── secret_scanner.rs        # gitleaks wrapper
│   │   ├── project_search.rs        # ranking: keyword/tech overlap
│   │   ├── latex.rs                 # placeholder injection, tighten macro application
│   │   ├── pdf.rs                   # page count + content-density estimate
│   │   ├── resume_generator.rs      # orchestrates the full agent pipeline (Section 5)
│   │   └── generation_queue.rs      # single-worker mpsc queue
│   ├── db/
│   │   ├── mod.rs
│   │   ├── repositories.rs
│   │   ├── projects.rs
│   │   └── resumes.rs
│   └── models/
│       ├── mod.rs
│       ├── github.rs
│       ├── project.rs
│       ├── resume.rs
│       └── generation.rs            # event types, node types, status enum
├── frontend/
│   ├── index.html                   # Dashboard / Create Resume
│   ├── setup.html                   # First-run system health + master resume
│   ├── github.html
│   ├── history.html
│   ├── resume.html                  # Individual resume page + live canvas
│   ├── css/app.css
│   ├── js/
│   │   ├── api.js
│   │   ├── create.js
│   │   ├── history.js
│   │   ├── github.js
│   │   ├── setup.js
│   │   └── canvas.js                # SVG node graph + WebSocket console (Section 7)
│   └── assets/
└── data/
    ├── resumeforge.db
    ├── master/
    │   ├── resume.tex
    │   └── master-history/          # timestamped snapshot on every master edit
    ├── templates/starter/           # 1-2 shipped templates with placeholder macros
    ├── projects/
    ├── repo-cache/                  # shallow git clones, one dir per repo
    ├── temp/
    └── applications/
        └── <resume-id>/
            ├── job.txt
            ├── instructions.txt
            ├── input-context.json
            ├── resume.json
            ├── resume.tex
            ├── resume.pdf
            ├── master-snapshot.tex
            └── evidence.json        # { accepted: [...], rejected: [...] }
```

---

## 3. Database schema

Use exactly the schema in `migrations/001_initial.sql` (included in this
buildkit, already written — do not redesign it). Tables: `settings`,
`repositories`, `projects`, `project_evidence`, `master_templates`,
`resumes`, `resume_projects`, `generation_events`. Key points an
implementer must respect:

- `resumes.status` is an enum-as-TEXT with these values only:
  `queued, analysing_job, retrieving_projects, generating_content,
  validating_evidence, rendering_latex, compiling_pdf, checking_pages,
  ready, ready_sparse, github_error, ai_empty_response, ai_tool_denied, ai_timeout, ai_hung,
  ai_invalid_json, ai_error, latex_error, page_limit_error,
  validation_error, cancelled`.
- `resumes` also has `error_stage`, `error_detail`, `master_sha256`,
  `content_density_pct`, `compact_mode_applied`, `parent_resume_id`
  (self-referential, nullable, for regenerate lineage).
- `generation_events` is the single source of truth for both the persisted
  history log and the live canvas feed: `resume_id, seq, ts, node, event_type,
  payload_json`. Never store event history anywhere else.
- `resume_projects` and `generation_events` cascade-delete when their parent
  `resumes` row is deleted. Verify this with `PRAGMA foreign_keys = ON;` set
  at connection time — SQLite does not enforce FKs by default.
- `master_templates` tracks every master resume version:
  `content_hash, file_path, is_starter_template, adapted_from_paste,
  created_at, superseded_at`.

---

## 4. State machine

```text
queued → analysing_job → retrieving_projects → generating_content
       → validating_evidence → rendering_latex → compiling_pdf
       → checking_pages → [ready | ready_sparse]
```

`checking_pages` can loop back to `rendering_latex` up to 4 times total
(3 content-shortening attempts + 1 `\tighten`-macro attempt) before settling
into `page_limit_error`. See Section 5 step 5 for the exact shortening rule.

**Startup recovery (implement in `main.rs`, before the Axum server binds):**
```sql
UPDATE resumes SET status = 'cancelled', error_stage = 'server_restart'
WHERE status NOT IN ('ready','ready_sparse','github_error','ai_empty_response',
  'ai_tool_denied','ai_timeout','ai_hung','ai_invalid_json','ai_error','latex_error',
  'page_limit_error','validation_error','cancelled');
```
This guarantees no resume is left polling forever after a crash.

**Concurrency:** exactly one generation runs at a time, via a single-consumer
`tokio::mpsc` queue in `generation_queue.rs`. `POST /api/resumes` only ever
inserts a row and pushes an ID onto the channel — it must return immediately.

---

## 5. The agent pipeline (implement as `resume_generator.rs`)

Each stage is a node that emits events (Section 7) regardless of whether it
calls the AI:

1. **JDAnalyzerAgent** (pure Rust, no AI call) — extract role, core skills,
   secondary skills, concepts from the pasted job description via keyword
   heuristics. If company/role fields were left blank in the UI, infer them
   here rather than making them mandatory inputs.
2. **ProjectRetrievalAgent** (pure Rust) — rank indexed projects by keyword
   overlap + technology overlap + feature overlap against the JD analysis.
   No embeddings in V1. Return a ranked list with scores.
3. **ContentGenerationAgent** (the one real AI call) — build a prompt from:
   master resume facts + JD analysis + top-N ranked projects + their evidence
   + optional user instructions + resume constraints (no fabrication, cite
   evidence IDs per bullet). Explicitly include in the prompt template that the
   environment has no tool access and the model must never attempt tool use,
   only returning the requested structured JSON directly. Call `agy` per
   Section 6. Expect structured JSON back adhering strictly to the categorized skills schema:
   `{ summary: string, skills: { languages: string[], frameworks_and_tools: string[], core_concepts: string[] }, experience: [{title, company, date_range, location, bullets: string[]}], projects: [{project_id: number, name: string, bullets: [{text: string, evidence_ids: number[]}]}], achievements: string[] }`.
   No LaTeX from the model at this stage.
4. **EvidenceValidatorAgent** (pure Rust) — in the `projects` section, every bullet must cite
   `evidence_ids` that exist in `project_evidence` for that project. Bullets
   without valid evidence are dropped and logged to `evidence.json`'s
   `rejected[]` array with a reason, not silently discarded. Note: `experience` bullets
   are strictly exempt from `evidence_ids` validation (they originate from the user's own asserted
   master resume facts and work history, not cloned repository evidence).
5. **RenderCompileLoopAgent** — a loop of up to 4 iterations:
   a. `LatexRenderAgent` (pure Rust): inject validated content into the
      master template's placeholder macros (`\ResumeSummary{}`,
      `\ResumeSkills{}`, etc.) — never touch styling commands.
   b. `CompileAgent` (pure Rust): run `tectonic -X compile resume.tex`.
   c. `PageCheckAgent` (pure Rust): count pages.
      - 1 page, adequately filled → `ready`.
      - 1 page, under ~60% content density → `ready_sparse`.
      - >1 page, attempts 1-3 → drop the weakest-relevance bullets first
        (preserve strongest JD matches, never fabricate; on tie or near-tie
        relevance scores between an evidence-grounded project bullet and a
        self-asserted experience bullet, experience bullets are dropped first
        because project bullets carry repo-verified evidence backing), go back to (a).
      - >1 page, attempt 4, and the master template defines a `\tighten`
        macro → apply it once, go back to (a), mark
        `compact_mode_applied = true`.
      - >1 page after attempt 4 → `page_limit_error`; keep the last
        known-good ≤1-page LaTeX (if any) alongside the failing one.

---

## 6. Antigravity CLI integration — build this exactly as specified

`agy` headless mode has confirmed reliability problems specifically in
non-TTY (subprocess) contexts: silent zero-byte stdout with exit code 0 in
some versions, indefinite hangs in others. A plain `tokio::process::Command`
with piped stdio will hit this. Build `pty_runner.rs` as follows:

1. Spawn `agy -p "<prompt>" --output-format stream-json --print-timeout <duration>`
   attached to a PTY master/slave pair (via `portable-pty`), not `Stdio::piped()`. Note: do NOT
   pass `--non-interactive` (that flag does not exist in `agy`; `-p` / `--print`
   is itself the non-interactive mode). Passing `--print-timeout` enables the CLI
   to enforce its own exit rather than relying solely on Rust-side kill logic.
   Separate timeout budgets explicitly:
   - Headless capability probe (`/api/system/antigravity-probe`): 30s.
   - ContentGenerationAgent (`services/antigravity.rs`): 90–120s (configurable, default 120s; repair re-prompt 60s), since real generation prompts with full master facts and ranked projects require significantly more token generation time than a 1-word probe.
2. Windows ConPTY non-EOF behavior: on Windows, the ConPTY pseudoconsole pipe
   does not emit EOF (`Ok(0)`) when the spawned child exits. If a reader loop
   naively blocks on `reader.read()`, it hangs indefinitely. Reading MUST be
   performed on a background task/thread with an MPSC channel, while
   concurrently polling `child.try_wait()`. Once process exit is detected,
   drain output for an additional ~300–500ms before concluding. Keep this
   `try_wait()`/drain pattern as a robust backstop even when `--print-timeout` is set.
3. Stream reader line tolerance & multi-line retry: configure PTY column width to 8192
   as a first line of defense against terminal line-wrapping. In `pty_runner.rs`,
   the line reader must retry on JSON parse failure by accumulating additional lines
   (up to a reasonable cap, e.g. 10 lines) before giving up and logging as a non-JSON
   diagnostic line (such as `jetski: no output produced — a tool required...`).
   Skip and log (do NOT fail on) any line or buffer that does not parse as valid JSON.
4. Classify outcomes explicitly — these are first-class states, not generic
   errors:
   - `ai_empty_response` — process exits 0, zero bytes or empty string returned,
     with NO `denied_actions` field.
   - `ai_tool_denied` — triggered when the result event's `denied_actions` array
     is non-empty, regardless of whether response text is also empty. This must
     NOT trigger the generic repair-reprompt retry logic (retrying an identical
     prompt will likely be denied again). Instead: log the specific denied
     action name to `generation_events` as a `node_error`, and surface it distinctly
     on the canvas ("Content Generation Agent: attempted <action>, denied by permission system").
   - `ai_timeout` — no output within the configured window.
   - `ai_hung` — process still alive well past expected latency; kill it.
   - `ai_invalid_json` — output received but doesn't parse as valid JSON or fails semantic
     schema validation after exactly one repair re-prompt ("return ONLY valid JSON matching schema,
     no prose, no code fences"). This single repair re-prompt is the strict retry ceiling; if the
     repair attempt also fails, it is immediately terminal `ai_invalid_json` with no further automatic
     retries. The worst-case wall-clock ceiling for `generating_content` is strictly bounded at
     180s (120s initial content-gen timeout + 60s repair re-prompt timeout), guaranteeing the stage never hangs.
   - `ai_error` — nonzero exit code or stderr content.
5. Do not trust `--json-schema` enforcement as guaranteed even if the flag is
   accepted — validate the response shape yourself regardless.
6. Redact secrets/tokens from any command string before it is ever
   constructed for logging/display, not as a display-time filter.
7. On boot, run a trivial probe prompt ("reply with the single word OK")
   through this exact code path and surface the result on the Setup/System
   Health screen as its own row. Raise the probe timeout to 30s minimum (a
   simple 1-word answer takes ~15s due to CLI startup and model thinking tokens;
   shorter timeouts trigger false-positive `ai_timeout` failures). Do not report
   "Antigravity: authenticated" from `agy auth status` alone; confirm it actually
   responds headlessly.
8. Never call `--dangerously-skip-permissions` / `--yolo`. The AI has no
   filesystem or shell access in this architecture — it only returns text.
   Headless `-p` mode automatically denies unauthorized tool calls by default,
   providing a hard security barrier.

---

## 7. Live orchestration canvas

Every node in Section 5 (AI-backed or pure Rust) emits the same event shape,
published to a `tokio::sync::broadcast` channel keyed by `resume_id` and
simultaneously persisted to `generation_events`:

```json
{
  "resume_id": 48, "node": "content_generation", "seq": 17,
  "ts": "2026-09-27T20:46:03Z",
  "type": "command_started | command_output | artifact_produced | node_done | node_error",
  "payload": { }
}
```

### Event Mapping (`agy` stream-json → Canvas Schema)
| `agy` Event Field | Condition / Subfield | Normalized Canvas Event (`type` & `payload`) |
|---|---|---|
| `event: "init"` | `init.conversation_id`, `init.tools` | `type: "command_started"`, `payload: { "conversation_id": "...", "tools_count": ... }` |
| `event: "step_update"` | `step_update.text_delta` present | `type: "command_output"`, `payload: { "chunk": coalesced_deltas }` (Coalesced: buffered and flushed every ~250ms or when buffer exceeds ~200 chars) |
| `event: "step_update"` | `step_update.state == "DONE"` | `type: "command_output"`, `payload: { "turn_done": true, "duration": ..., "usage": ... }` |
| `event: "result"` | `result.status == "SUCCESS"` | `type: "artifact_produced"`, `payload: { "kind": "json", "content": result.response }` |
| Process exit ≠ 0 / timeout | Failure detected by runner | `type: "node_error"`, `payload: { "stage": "ai_error", "detail": "..." }` |


- `GET /ws/resumes/:id/live` — on connect, replay all existing
  `generation_events` rows for that resume, then subscribe to the live
  broadcast channel for new ones. This means a page refresh mid-generation
  never loses history.
- The frontend (`canvas.js`) renders a fixed-layout SVG graph matching the
  pipeline shape in Section 5 (linear, with a visible loop box for
  render/compile/page-check), animates node state (idle → running → done /
  error), and shows a scrolling console per node fed by `command_output` and
  `artifact_produced` events (render `latex_diff` payloads as a unified
  diff, `json` payloads as collapsible text).
- V1 scope: the event bus, WebSocket, and a plain per-node scrolling log are
  required. Animated SVG polish and syntax-highlighted diffs are acceptable
  to ship as a fast-follow — do not let them block the functional version.

---

## 8. GitHub indexing

- Auth via `gh auth login` (never store credentials yourself); check with
  `gh auth status`.
- List repos: `gh repo list <user> --limit 1000 --json name,nameWithOwner,
  description,isPrivate,url,homepageUrl,languages,pushedAt`.
- Content access is via **shallow clone**, not per-file API calls:
  `git clone --depth 1 <url>` into `data/repo-cache/<repo-id>/` on first
  index; on re-sync, compare GitHub's `latest_commit_sha` to the clone's
  `git rev-parse HEAD` — skip if equal, else `git fetch --depth 1 && git
  reset --hard origin/<default-branch>`.
- Before any file content is read into a project-knowledge object, run
  `gitleaks detect --source <clone-path> --no-git -r report.json` against
  the clone; anything flagged is excluded from the manifest. Keep a
  secondary regex pass (`API_KEY=`, `SECRET=`, etc.) as a cheap backstop,
  never as the only defense.
- Parse `README.md`, `package.json`, `Cargo.toml`, `requirements.txt`,
  `pyproject.toml`, `go.mod`, `Dockerfile`, etc. from the local clone to
  build the project-knowledge JSON object (Section 16 shape from the
  original blueprint — technologies, features with evidence file
  references, architecture notes).
- Ignore always: `.env*, *.pem, *.key, *.p12, *.crt, credentials.*,
  secrets/, node_modules/, target/, dist/, build/, .git/, vendor/`, binaries,
  large files.

---

## 9. LaTeX template strategy

- **Tier 1 (fully supported):** ship 1-2 starter templates in
  `data/templates/starter/`, already containing the placeholder macros
  (`\ResumeSummary{}`, `\ResumeSkills{}`, section injection points). This is
  the default path.
- **Tier 2 (best-effort):** if a user pastes arbitrary LaTeX, run a one-time
  AI-assisted migration that wraps existing content in the required macros
  **without touching any visual/styling command**, show the user a diff,
  require explicit approval before saving as master. Mark
  `master_templates.adapted_from_paste = true`.
- Every master edit writes a timestamped copy to `data/master/master-history/`
  and inserts a new `master_templates` row superseding the previous one.

---

## 10. API surface

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/api/system/status` | CLI/system health (gh, agy, tectonic, sqlite) |
| GET | `/api/system/antigravity-probe` | On-demand headless capability probe |
| GET | `/api/github/status` | GitHub auth state |
| POST | `/api/github/sync` | Sync repos (shallow clone + reindex changed) |
| GET | `/api/projects` | List indexed projects |
| GET | `/api/projects/:id` | Project detail incl. evidence |
| GET / POST | `/api/template` | Fetch / save master LaTeX |
| POST | `/api/template/adapt` | Tier-2 AI-assisted template migration |
| GET | `/api/master/history` | List master template versions |
| POST | `/api/resumes` | Enqueue a generation, returns immediately |
| GET | `/api/resumes` | History list |
| GET | `/api/resumes/:id` | Resume metadata |
| GET | `/api/resumes/:id/events` | Full event log (non-WS fallback) |
| GET | `/api/resumes/:id/pdf` | Download PDF |
| GET | `/api/resumes/:id/tex` | Download LaTeX |
| POST | `/api/resumes/:id/regenerate` | New version, parent-linked |
| DELETE | `/api/resumes/:id` | Permanent delete (row + `data/applications/<id>/`) |
| GET | `/ws/resumes/:id/live` | WebSocket: replay + live event stream |

---

## 11. Explicitly out of scope for V1

No accounts. No cloud backend. No Supabase. No React. No vector database
(embeddings-based ranking is future work). No Docker requirement. No
browser extension. No job-site scraping. No automated applications. No
RAG framework. No LangChain. No agent-framework dependency (ADK vocabulary
is used conceptually, not as a library). No hosting. No mid-generation
cancellation (only pre-start cancellation from the queue).

---

## 12. Build order (follow in sequence — do not skip ahead)

1. **Spike first, standalone, before any Axum code**: confirm you can
   reliably get text back from `agy -p ...` through a PTY in a bare Rust
   binary on the target machine/OS. This is the go/no-go gate for the whole
   project — if it doesn't work, stop and fix the PTY approach before
   building anything on top of it.
2. Axum project skeleton, serves the static `frontend/` files.
3. SQLite + `migrations/001_initial.sql` (already written, in this buildkit).
4. System health route incl. Antigravity probe + Tectonic bundle status.
5. Master `.tex` upload/paste/save, starter templates, master-history
   snapshots.
6. Tectonic compilation + PDF serving (pre-fetch the bundle so first compile
   doesn't require internet).
7. Resume history CRUD with dummy content (no AI yet) — prove the DB/API
   shape before adding the hard part.
8. Permanent delete, verified to cascade `resume_projects` and
   `generation_events`.
9. Wrap the Section-1 PTY spike into `services/antigravity.rs` properly,
   with full failure classification (Section 6).
10. Event bus (`tokio::sync::broadcast` per resume_id) + `generation_events`
    persistence + WebSocket endpoint with replay-then-subscribe.
11. Minimal canvas UI: static node list + live scrolling console, no SVG
    graph yet — prove the transport before polishing the visual.
12. Deterministic LaTeX renderer (placeholder injection).
13. One-page validation, content-shortening attempts only (Stage A).
14. `\tighten`-macro attempt (Stage B) + sparse-page detection.
15. Single-worker generation queue + startup recovery sweep.
16. GitHub CLI auth detection.
17. Shallow-clone indexing + incremental SHA-based sync.
18. `gitleaks`-based secret filtering.
19. Project analysis → local project-knowledge store.
20. JD analysis.
21. Project ranking (keyword/tech overlap).
22. Wire retrieval → Antigravity → evidence validation → render → compile →
    page check as the full pipeline, end to end, with every node emitting
    events to the canvas.
23. Regenerate / version lineage (`parent_resume_id`).
24. Tier-2 template adaptation flow.
25. Error-state polish across all failure branches (Section 4's status list).
26. Frontend polish; SVG graph animation, diff rendering, edge animation
    (acceptable as fast-follow per Section 7).
27. Package for local startup (single binary + `data/` directory, README
    with `gh auth login` / `agy auth login` prerequisites).

---

## 13. Definition of done

V1 is complete when this exact run works, unattended, start to finish:

```text
Start ResumeForge → localhost opens → System Health shows every check green,
including the Antigravity headless probe (not just "installed") →
GitHub connected, at least one repo indexed via clone + gitleaks scan →
Master LaTeX exists (starter template, or a Tier-2 adapted + approved one) →
Paste a real job description, optional instruction →
Generate → exactly one generation in flight, queue enforced →
Canvas shows every node's live status and console output, not a bare spinner →
Antigravity call survives a PTY spawn; a forced timeout produces a clean
  ai_timeout event on the canvas, not a hang →
Evidence validated; any rejected claims are visible on the resume detail page →
LaTeX renders, Tectonic compiles from the cached bundle →
Page count checked; shortening/tightening applied if needed; sparse pages
  flagged as ready_sparse rather than presented as normal →
Resume saved with master_sha256 provenance →
Kill the server mid-generation on a second resume; on restart it shows
  'cancelled', never stuck polling forever →
Open the resume, download PDF and .tex, regenerate to create v2, delete v1
  permanently (row + files both gone) →
Everything above happened without a single silent failure: every non-happy
  path produced a specific, visible status and message.
```

Build to this checklist, not to "it worked once."
