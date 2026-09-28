# ResumeForge — Architecture v2 (Hardened)

This revises the original blueprint. The shape is unchanged — Rust owns every
deterministic thing, Antigravity only reasons/rewrites text, LaTeX macros gate
all styling — but every point that v1 quietly assumed would "just work" now
has an explicit failure mode and a designed recovery path. Changes from v1 are
marked **[v2]**.

---

## 1. Technology Stack

| Layer | Technology |
|---|---|
| Frontend | HTML + CSS + Vanilla JavaScript |
| Backend | Rust |
| HTTP framework | Axum |
| Async runtime | Tokio |
| Database | SQLite (SQLx, bundled) |
| Serialization | Serde / serde_json |
| GitHub access | GitHub CLI `gh` + shallow git clone **[v2]** |
| AI | Google Antigravity CLI `agy`, spawned via PTY **[v2]** |
| Resume source | LaTeX |
| PDF compiler | Tectonic, with pre-fetched local bundle **[v2]** |
| PDF inspection | Rust PDF library (page count + rendered content height **[v2]**) |
| Secret scanning | `gitleaks` against cached clone, not hand-rolled regex **[v2]** |
| PTY handling | `portable-pty` or `pty-process` crate **[v2, new]** |
| File storage | Local filesystem |
| Logs | tracing + tracing-subscriber |

---

## 2. The Antigravity Integration Layer **[v2 — this is the load-bearing fix]**

Current reports on `agy` headless mode show it is unreliable specifically in
non-TTY subprocess contexts — the exact context a Rust `tokio::process::Command`
spawn creates by default. Observed failure modes across versions: silent
zero-byte stdout with exit code 0, and indefinite hangs. The same commands work
fine attached to a real terminal. So the wrapper is not "call the binary,
parse stdout" — it's a small subsystem:

```text
services/antigravity.rs
│
├── spawn via PTY (portable-pty), never plain Stdio::piped()
├── write prompt to pty master
├── read stream-json events, line by line, with a read-timeout per line
├── concatenate/extract final text block
├── strip markdown fences, attempt JSON parse
│     └── on parse failure: single repair re-prompt ("return ONLY valid JSON")
├── classify failure explicitly:
│     ai_empty_response   (0 bytes, exit 0)
│     ai_timeout          (no output within N seconds)
│     ai_hung             (process alive past 2x expected latency → kill -9)
│     ai_invalid_json     (parse failed twice)
│     ai_error            (nonzero exit / stderr content)
└── never trust --json-schema enforcement as guaranteed;
    treat it as a hint, validate the shape yourself regardless
```

**Startup capability probe** (part of system health, Section 8): on boot, run
a trivial prompt ("reply with the single word OK") through the exact same PTY
path used in production. If it doesn't come back correctly within a few
seconds, Antigravity shows **✗ not responding** in Settings — before the user
ever pastes a job description, not after.

**Concurrency:** exactly one `agy` invocation in flight at a time, enforced by
a single-worker queue (Section 9), both because local resource use should stay
predictable and because headless OAuth sessions have reported daily quota
limits meant for interactive use.

---

## 3. Screens

Unchanged from v1 (Dashboard, Setup, GitHub, History, Resume detail), with
two additions:

- **Setup / System Health** gets a row for **Antigravity capability probe**
  (not just "installed" / "authenticated" but "responded to test prompt: ✓/✗")
  and a row for **Tectonic bundle** ("✓ cached locally" / "⚠ will download on
  first compile").
- **Resume detail page** gets a small **provenance panel**: claims omitted for
  lack of evidence, and — if the page came from Stage-B shortening (Section 7)
  — a note that compact spacing was applied.

---

## 4. File Structure

```text
resumeforge/
├── Cargo.toml / Cargo.lock
├── migrations/
├── src/
│   ├── main.rs              # includes startup recovery sweep [v2]
│   ├── config.rs
│   ├── state.rs
│   ├── errors.rs
│   ├── routes/ {system,github,templates,projects,resumes}.rs
│   ├── services/
│   │   ├── command_runner.rs
│   │   ├── pty_runner.rs            # [v2, new] PTY spawn + stream-json reader
│   │   ├── github.rs
│   │   ├── github_indexer.rs        # now drives shallow clone, not per-file API [v2]
│   │   ├── secret_scanner.rs        # wraps gitleaks binary [v2]
│   │   ├── project_search.rs
│   │   ├── antigravity.rs           # failure classification per Section 2 [v2]
│   │   ├── latex.rs
│   │   ├── pdf.rs                   # page count + content-density estimate [v2]
│   │   ├── resume_generator.rs      # single-worker queue owner [v2]
│   │   └── generation_queue.rs      # [v2, new]
│   ├── db/ {repositories,projects,resumes}.rs
│   └── models/ {github,project,resume,generation}.rs
├── frontend/  (unchanged structure)
└── data/
    ├── resumeforge.db
    ├── master/
    │   ├── resume.tex
    │   └── master-history/           # [v2, new] snapshot on every master edit
    ├── templates/starter/             # [v2, new] 1-2 shipped templates w/ macros
    ├── projects/
    ├── repo-cache/                    # [v2] shallow git clones, not loose files
    ├── temp/
    └── applications/
        ├── 000001/
        │   ├── job.txt
        │   ├── instructions.txt
        │   ├── input-context.json
        │   ├── resume.json
        │   ├── resume.tex
        │   ├── resume.pdf
        │   ├── master-snapshot.tex     # [v2, new] provenance
        │   ├── evidence.json           # now includes rejected[] with reasons [v2]
        │   └── metadata.json
        └── 000002/
```

---

## 5. Database

### `settings` — unchanged

### `repositories` — unchanged, plus:
```text
local_clone_path       -- data/repo-cache/<id>
last_secret_scan_at
```

### `projects` — unchanged

### `project_evidence` — unchanged

### `resumes` — extended **[v2]**
```text
id
company
role
job_description
extra_instructions
status                 -- see Section 6, now includes 'ready_sparse', 'cancelled'
error_stage            -- [v2, new] 'ai_timeout' | 'latex_error' | ...
error_detail           -- [v2, new] raw message / log excerpt
parent_resume_id
pdf_path
tex_path
master_sha256          -- [v2, new] provenance for reproducibility
page_count
content_density_pct    -- [v2, new] rough fill estimate, flags sparse pages
compact_mode_applied   -- [v2, new] bool, Stage-B shortening used
created_at
completed_at
```

### `resume_projects` — unchanged (ensure ON DELETE CASCADE **[v2]**)

### `generation_events` **[v2, new table]**
Replaces trying to cram a timeline into one status column.
```text
id
resume_id
stage                  -- 'analysing_job' | 'compiling_pdf' | ...
occurred_at
detail                 -- free text, e.g. "2 claims rejected: no evidence"
```

### `master_templates` **[v2, new table]**
```text
id
content_hash
file_path
is_starter_template     -- bool
adapted_from_paste      -- bool, true if AI-migrated (Section 8)
created_at
superseded_at
```

---

## 6. Resume Generation State Machine **[v2]**

```text
queued → analysing_job → retrieving_projects → generating_content
       → validating_evidence → rendering_latex → compiling_pdf
       → checking_pages → [ready | ready_sparse]
```

Failure states: `github_error`, `ai_empty_response`, `ai_timeout`, `ai_hung`,
`ai_invalid_json`, `ai_error`, `latex_error`, `page_limit_error`,
`validation_error`, `cancelled`.

**Startup recovery sweep**, run before Axum binds:
```sql
UPDATE resumes SET status = 'cancelled', error_stage = 'server_restart'
WHERE status NOT IN (
  'ready','ready_sparse','github_error','ai_empty_response','ai_timeout',
  'ai_hung','ai_invalid_json','ai_error','latex_error','page_limit_error',
  'validation_error','cancelled'
);
```
This guarantees no resume is stuck polling forever after a crash or restart.

---

## 7. Exact-Page Rule, Two Stages **[v2]**

**Stage A (attempts 1–3):** content-only shortening, exactly as v1 — drop
weakest-relevance bullets first, preserve strongest JD matches, no
fabrication, no layout changes.

**Stage B (attempt 4, opt-in per template):** if the master template defines
a `\tighten` macro (fixed, pre-tested spacing reduction — never AI-authored),
apply it once as a deterministic last resort before failing. Record
`compact_mode_applied = true`.

**Underflow check:** after any successful ≤1-page compile, estimate content
density (line/bullet count vs. template's typical fill). Below ~60%, set
`status = ready_sparse` instead of `ready` so History shows a soft warning
rather than presenting a half-empty page as a normal success.

**Final failure:** `page_limit_error`, but keep the last known-good ≤1-page
attempt (if one existed at a lower content level) alongside the failing one,
so the user has something to fall back to manually.

---

## 8. LaTeX Template Strategy, Tiered **[v2]**

- **Tier 1 (fully supported):** 1–2 starter templates shipped in
  `data/templates/starter/`, already containing `\ResumeSummary{}`,
  `\ResumeSkills{}`, etc. This is the default and the only path guaranteed to
  work end-to-end on day one.
- **Tier 2 (best-effort migration):** user pastes their own LaTeX → one-time
  AI-assisted wrap step: Antigravity is asked to insert the placeholder macros
  around existing content **without touching any visual command** (fonts,
  margins, packages, colors). The diff is shown to the user for approval
  before it's saved as master. `master_templates.adapted_from_paste = true`
  is stored so the UI can show a "custom template — verify formatting after
  first generation" notice.
- Every master edit writes a timestamped copy into `data/master/master-history/`
  and a new `master_templates` row (superseding the previous one), so master
  resume changes are versioned the same way generated resumes are.

---

## 9. Generation Queue & Concurrency **[v2]**

```text
POST /api/resumes
   ↓
insert row, status = queued
   ↓
push resume_id onto bounded mpsc channel
   ↓
single consumer task (the only thing allowed to call antigravity.rs)
   ↓
processes one resume fully before taking the next
```

This bounds Antigravity calls to one at a time (protecting against quota
limits and making PTY-spawn failures easier to diagnose — no interleaved
output from concurrent `agy` processes), and gives you one obvious place to
add cancellation later (drop from queue if still `queued`; can't cancel mid-flight
in v1, which is an acceptable limitation to state explicitly rather than
half-build).

---

## 10. GitHub Indexing **[v2 — mechanism made explicit]**

- First sync: `git clone --depth 1 <repo-url>` into
  `data/repo-cache/<repo-id>/`.
- Re-sync: compare `latest_commit_sha` from `gh repo list` against the local
  clone's `git rev-parse HEAD`. If equal, skip entirely. If different:
  `git fetch --depth 1 && git reset --hard origin/<default-branch>`.
- **Before any file content is read into the project-knowledge object**, run
  `gitleaks detect --source <clone-path> --no-git -r report.json` against the
  clone. Anything it flags is excluded from the manifest/README/source
  analysis, not just pattern-matched inline. This replaces hand-rolled
  `API_KEY=` regex scanning as the primary defense — the regex list stays as
  a cheap second pass, not the only one.
- Manifest parsing (`package.json`, `Cargo.toml`, `README.md`, etc.) reads
  from the local clone, so it's fast and offline once cloned.

---

## 11. Evidence & Provenance Visibility **[v2]**

`evidence.json` per application now records both sides:
```json
{
  "accepted": [
    { "text": "...", "project_id": 14, "evidence_ids": [81, 82] }
  ],
  "rejected": [
    { "text": "Improved API latency by 72%", "reason": "no_matching_evidence" }
  ]
}
```
The resume detail page shows a one-line summary ("2 claims omitted — view")
instead of a resume that's mysteriously thinner than expected with no
explanation.

---

## 12. Error Surfacing **[v2]**

Every terminal failure state writes `error_stage` + `error_detail` on the
`resumes` row, and a full timeline in `generation_events`. The frontend's
generation-progress view (Section in v1 — unchanged in spirit) reads the
event log directly instead of inferring history from a single status field:

```text
✓ Job analysed
✓ 4 relevant projects found
✓ Resume content generated (2 claims omitted — no evidence)
✗ LaTeX compilation failed — line 84, undefined control sequence
  [ View Log ]
```

---

## 13. Tectonic Offline Readiness **[v2]**

Pre-fetch and cache Tectonic's default bundle during setup
(`tectonic -X bundle` local cache), and add it as its own row in System
Health: "LaTeX package bundle: ✓ cached" vs. "⚠ will download on first
compile." Prevents a silent failure the first time someone compiles without
internet.

---

## 14. API

Unchanged from v1's table, plus:

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/api/system/antigravity-probe` | Run the capability probe on demand |
| GET | `/api/resumes/:id/events` | Full generation event timeline |
| POST | `/api/template/adapt` | Kick off Tier-2 AI-assisted template migration |
| GET | `/api/master/history` | List master template versions |

---

## 15. Development Sequence **[v2 — reordered]**

The key change from v1: **the riskiest unknown moves to the front**, so you
find out early whether the architecture's central assumption holds, instead
of discovering it after steps 1–7 are already built on top of it.

1. **Spike: PTY-wrapped `agy` call, standalone binary, no Axum yet.** Confirm
   you can reliably get text back from a non-interactive subprocess call on
   your actual machine/OS. This is the go/no-go gate for the whole plan.
2. Create Rust/Axum project, serve HTML UI.
3. SQLite + migrations (full v2 schema from the start, including
   `generation_events`, `error_stage`, `master_templates`).
4. Settings/system-health checks, including the Antigravity probe and
   Tectonic bundle status.
5. Master `.tex` upload/paste/save, with starter templates (Tier 1) and
   master-history snapshots.
6. Tectonic compilation + PDF serving, with pre-fetched bundle.
7. Resume history CRUD without AI (dummy content).
8. Permanent delete, with cascade to `resume_projects` and `generation_events`.
9. Wrap the Section-1 PTY spike into `services/antigravity.rs` proper, with
   full failure classification.
10. Deterministic LaTeX renderer (placeholder injection).
11. One-page validation, Stage A only.
12. Stage B (`\tighten` macro) + sparse-page detection.
13. Single-worker generation queue + startup recovery sweep.
14. GitHub CLI auth detection.
15. Shallow-clone indexing strategy + incremental SHA sync.
16. `gitleaks`-based secret filtering.
17. Project analysis → local project knowledge store.
18. JD analysis.
19. Project ranking (keyword/tech-overlap, no embeddings yet).
20. Connect retrieval → Antigravity, full pipeline end to end.
21. Evidence validation + rejected-claims logging.
22. Regenerate/version relationships.
23. Generation progress UI reading `generation_events`.
24. Tier-2 template adaptation flow.
25. Logs/error recovery polish.
26. Frontend polish.
27. Packaging for local startup.

---

## 16. V1 Definition of Done **[v2, tightened]**

Same end-to-end scenario as before, with two additions that make "done" mean
something more trustworthy than "worked once":

```text
Start ResumeForge
   ↓
System Health shows Antigravity capability probe passing (not just "installed")
   ↓
GitHub connected, projects indexed via clone + gitleaks-scanned
   ↓
Master LaTeX exists (starter template or adapted-and-approved custom one)
   ↓
Paste Microsoft JD, optional instruction
   ↓
Generate → queued (only generation in flight)
   ↓
Full event timeline visible during generation, not just a spinner
   ↓
Antigravity call survives a PTY spawn, times out cleanly if it doesn't
   ↓
Evidence validated, rejections logged and visible
   ↓
LaTeX rendered, Tectonic compiles from cached bundle
   ↓
Page count checked; Stage A/B shortening applied if needed; sparse pages flagged
   ↓
Resume saved, provenance (master_sha256) recorded
   ↓
Kill the server mid-generation on a second resume → on restart it shows
  'cancelled', not stuck at 'compiling_pdf' forever
   ↓
Open preview, download PDF/.tex, regenerate → v2, delete v1 permanently
```

---

## 17. Still Deliberately Out of Scope

Unchanged from v1: no accounts, no cloud backend, no React, no vector DB, no
Docker requirement, no browser extension, no job-site scraping, no automated
applications, no RAG framework, no LangChain, no agent swarm, no hosting.
Embeddings-based project ranking and mid-generation cancellation are explicit
future work, not silently missing pieces.
