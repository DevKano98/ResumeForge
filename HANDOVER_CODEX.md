# ResumeForge â€” Complete Project Status & Codex Handover Guide

> **Current Status as of September 28, 2026**:
> Steps 1 through 27 are implemented and verified locally, including the release bundle startup smoke test.
> **Remaining live prerequisite**: Sign in with `gh auth login` and run `gh auth setup-git`, then verify account sync and the full Definition of Done.

---

## 1. Executive Summary & Vision

### What We Are Building
**ResumeForge** is an open-source, local-first, privacy-respecting resume tailoring engine written in pure Rust.

```text
Pasted Job Description
         â”‚
         â–¼
[ 1. JDAnalyzerAgent ] (pure Rust keyword heuristics)
         â”‚
         â–¼
[ 2. ProjectRetrievalAgent ] (pure Rust project ranking against local repo index)
         â”‚
         â–¼
[ 3. ContentGenerationAgent ] (Antigravity CLI `agy -p` in headless PTY; returns strict JSON)
         â”‚
         â–¼
[ 4. EvidenceValidatorAgent ] (pure Rust; validates project bullet evidence IDs against repo cache)
         â”‚
         â–¼
[ 5. RenderCompileLoopAgent ] (up to 4 iterations: pure Rust LaTeX injection + Tectonic compile)
   â”œâ”€â”€ Stage A (Attempts 1-3): Drop weakest-relevance bullets (experience dropped before projects)
   â”œâ”€â”€ Stage B (Attempt 4): Inject \tighten macro (compact margins to 0.4in)
   â””â”€â”€ Page Check: 1 page & >=60% -> ready; 1 page & <60% -> ready_sparse; >1 page -> page_limit_error
         â”‚
         â–¼
Ready PDF & Provenance-Tracked LaTeX (.tex)
```

### Core Architecture Principles
1. **Local-First & Headless:** No cloud accounts, no Supabase, no external telemetry. All indexing, compilation, and storage happens locally.
2. **Zero Hallucination / Evidence Grounding:** AI cannot invent facts. Project bullets **must cite evidence IDs** mapped to line ranges in locally cloned repositories.
3. **Evidence Asymmetry:** Project bullets are strictly validated against repository evidence; experience bullets are user-asserted facts from work history and are exempt from evidence IDs.
4. **Single-Worker Concurrency:** Only 1 generation can run at a time via a single-consumer queue (`tokio::sync::mpsc`). `POST /api/resumes` only inserts a record and enqueues.
5. **Deterministic LaTeX Rendering:** AI NEVER emits LaTeX directly. AI produces structured JSON; Rust injects text into placeholder macros (`\ResumeSummary`, `\ResumeSkills`, `\ResumeExperience`, `\ResumeProjects`) without touching any document geometry or styling.
6. **Live Orchestration Canvas:** Every agent node emits normalized Canvas events published to an in-memory broadcast bus (`tokio::sync::broadcast`) and simultaneously persisted to SQLite (`generation_events`). A minimal vanilla HTML/CSS/JS frontend connects via WebSocket (`/ws/resumes/:id/live`) with replay-then-subscribe.

---

## 2. Tech Stack & Environment

- **Language & Runtime:** Rust (2021 Edition), Axum 0.8, Tokio 1.x (full features).
- **Database:** SQLite with SQLx 0.8 (`runtime-tokio`, `sqlite`, `migrate`, `chrono`).
  - Connection pool configuration: `.foreign_keys(true)`, `.journal_mode(SqliteJournalMode::Wal)`, `.busy_timeout(5s)`.
  - Migrations automatically run on startup via embedded `sqlx::migrate!()`.
- **LaTeX Engine:** Tectonic (XeTeX-based standalone compiler) in `%USERPROFILE%\AppData\Local\agy\bin` or PATH.
- **AI CLI:** Google Antigravity CLI (`agy`) attached to a headless PTY via `portable-pty`.
- **PDF Inspection:** `lopdf 0.45` for page count and density estimation.
- **Frontend:** Vanilla HTML5, modern CSS3, vanilla ES6 JavaScript. No Node.js, no React, no bundler.
- **Operating System:** Windows 10/11 x86_64 (`w64devkit` mingw toolchain).

---

## 3. Directory Layout

```text
resumeforge-buildkit/
â”œâ”€â”€ migrations/
â”‚   â””â”€â”€ 001_initial.sql          # Canonical SQLite schema (tables, foreign keys, cascades)
â”œâ”€â”€ data/
â”‚   â”œâ”€â”€ templates/starter/       # Starter templates (classic.tex, modern.tex with \Resume... macros)
â”‚   â”œâ”€â”€ master/                  # Current master resume.tex and master-history/ snapshots
â”‚   â”œâ”€â”€ repo-cache/              # Cloned shallow git repositories
â”‚   â””â”€â”€ temp/                    # Isolated compile directories (compile_<uuid>/, cleaned on exit)
â”œâ”€â”€ frontend/
â”‚   â”œâ”€â”€ resume.html              # Canvas UI (SVG pipeline graph & scrolling terminal)
â”‚   â”œâ”€â”€ css/app.css              # Terminal theme styling
â”‚   â””â”€â”€ js/
â”‚       â”œâ”€â”€ api.js               # REST & WebSocket client helpers
â”‚       â””â”€â”€ canvas.js            # Live event stream listener & terminal renderer
â”œâ”€â”€ src/
â”‚   â”œâ”€â”€ main.rs                  # Server startup, DB init, migration execution, static routing
â”‚   â”œâ”€â”€ lib.rs                   # Library crate exposing all models, routes, services
â”‚   â”œâ”€â”€ config.rs                # App configuration (paths, ports)
â”‚   â”œâ”€â”€ state.rs                 # Shared AppState (SqlitePool, Config, EventBus, ProbeCache)
â”‚   â”œâ”€â”€ errors.rs                # Unified Axum AppError and status mapping
â”‚   â”œâ”€â”€ db/
â”‚   â”‚   â”œâ”€â”€ mod.rs               # SqlitePool factory with WAL, FKs, and 5s busy timeout
â”‚   â”‚   â”œâ”€â”€ resumes.rs           # Resume CRUD queries
â”‚   â”‚   â””â”€â”€ templates.rs         # Master templates & snapshot tracking
â”‚   â”œâ”€â”€ models/
â”‚   â”‚   â”œâ”€â”€ generation.rs        # ContentGenerationInput, GeneratedResumeContent, JdAnalysis
â”‚   â”‚   â”œâ”€â”€ resume.rs            # DB models for Resume, ResumeDetail, Status enums
â”‚   â”‚   â””â”€â”€ template.rs          # MasterTemplate, StarterTemplateInfo
â”‚   â”œâ”€â”€ routes/
â”‚   â”‚   â”œâ”€â”€ mod.rs               # Axum Router assembly
â”‚   â”‚   â”œâ”€â”€ system.rs            # GET /api/system/status & /api/system/antigravity-probe
â”‚   â”‚   â”œâ”€â”€ templates.rs         # GET/POST /api/template, starter templates
â”‚   â”‚   â”œâ”€â”€ resumes.rs           # Resume CRUD, PDF & .tex downloads
â”‚   â”‚   â””â”€â”€ ws.rs                # GET /ws/resumes/:id/live (WebSocket replay-then-subscribe)
â”‚   â”œâ”€â”€ services/
â”‚   â”‚   â”œâ”€â”€ command_runner.rs    # Safe async subprocess runner
â”‚   â”‚   â”œâ”€â”€ pty_runner.rs        # ConPTY subprocess spawner for agy stream-json
â”‚   â”‚   â”œâ”€â”€ antigravity.rs       # ContentGenerationAgent prompt steering & repair re-prompt
â”‚   â”‚   â”œâ”€â”€ event_bus.rs         # Broadcast bus, AtomicU64 seq counters, delta coalescing
â”‚   â”‚   â”œâ”€â”€ latex.rs             # Unconditional escaping, balanced-brace macro replacement
â”‚   â”‚   â”œâ”€â”€ pdf.rs               # Page counting & content density inspection
â”‚   â”‚   â”œâ”€â”€ render_loop.rs       # RenderCompileLoopAgent (Stage A shortening + Stage B \tighten)
â”‚   â”‚   â”œâ”€â”€ generation_queue.rs  # Single-consumer queue and placeholder generation worker
â”‚   â”‚   â”œâ”€â”€ github.rs            # (To implement in Step 16)
â”‚   â”‚   â”œâ”€â”€ github_indexer.rs    # (To implement in Step 17)
â”‚   â”‚   â”œâ”€â”€ secret_scanner.rs    # (To implement in Step 18)
â”‚   â”‚   â””â”€â”€ project_search.rs    # (To implement in Step 19-21)
â”‚   â””â”€â”€ bin/                     # Standalone verification harnesses (Steps 1â€“14)
```

---

## 4. Completed Milestones (Steps 1 to 15)

### Step 1: PTY Subprocess Spawning (`pty_runner.rs`)
- Spawns `agy -p "<prompt>" --output-format stream-json --print-timeout <duration>` via `portable-pty`.
- ConPTY configured with `cols: 8192` and a multi-line retry buffer up to 10 lines to prevent long stream JSON lines from breaking.
- Strict security: Never uses `--dangerously-skip-permissions` or `--yolo`. Headless mode automatically denies unauthorized actions.
- Full failure classification: `ai_tool_denied`, `ai_empty_response`, `ai_timeout`, `ai_hung`, `ai_invalid_json`, `ai_error`.

### Step 2: Axum Web Server & Static UI
- Static serving of `frontend/` files without heavy web dependencies.

### Step 3: SQLite Connection Pool & Migrations (`001_initial.sql`)
- Connection pool with `.foreign_keys(true)` on `SqliteConnectOptions`.
- Foreign key cascade deletions strictly verified for `resume_projects` and `generation_events`.
- `.busy_timeout(5s)` prevents intermittent `SQLITE_BUSY` errors during concurrent streaming writes.

### Step 4: System Health & Headless Capability Probe
- `GET /api/system/status` returns cached health snapshot (<100ms) for SQLite, GitHub CLI, Tectonic, and Antigravity.
- `GET /api/system/antigravity-probe` runs live capability probe with dedicated 30s timeout budget.

### Step 5: Master LaTeX Templates & Snapshots
- Starter templates: `classic.tex` and `modern.tex`.
- In-memory validation + dry-run Tectonic compile in `data/temp/` before saving master.
- Preamble snapshots stored in `data/master/master-history/`.
- Verified `\tighten` macro effectively compresses 2-page documents down to 1 page.

### Step 6: Tectonic Compilation & PDF Serving
- `compile_latex_content`: Isolated `compile_<uuid>` directories automatically removed after compile.
- PDF and LaTeX download endpoints: `GET /api/resumes/:id/pdf` and `GET /api/resumes/:id/tex`.
- `estimate_content_density` heuristics calibrated (60% threshold for sparse documents).

### Step 7 & 8: Resume CRUD & Permanent Cascading Deletion
- `POST /api/resumes` creates record with real `master_sha256` file hash.
- `DELETE /api/resumes/:id` deletes DB row (cascading children) and permanently deletes the application folder on disk.

### Step 9: ContentGenerationAgent & Pinned Schema
- Categorized skills schema pinned:
  ```json
  "skills": {
    "languages": ["Rust", "C++"],
    "frameworks_and_tools": ["Tokio", "SQLite"],
    "core_concepts": ["Distributed Systems"]
  }
  ```
- Strict semantic validation: exactly 1 targeted repair re-prompt on invalid structure.
- Hard timeout budget: 120s generation + 60s repair = 180s worst-case ceiling.
- Verified across 3 consecutive real `agy` runs with 100% schema compliance and evidence citations.

### Step 10 & 11: Live Event Bus & Minimal Canvas UI
- Per-resume `AtomicU64` sequence counters initialized from SQLite `COALESCE(MAX(seq), 0)`.
- Concurrent emission verified: 20 simultaneous threads producing strictly monotonic `seq: 1..=20` with zero duplicates.
- Stream coalescing: Consecutive `text_delta` tokens buffered and flushed every ~250ms or ~200 characters (yielding an **87.6% event reduction**).
- WebSocket `GET /ws/resumes/:id/live`: Replays existing SQLite events, then subscribes to live broadcast.
- Canvas UI (`canvas.js`): Terminal status shutoff and capped exponential backoff (max 15s).

### Step 12: Deterministic LaTeX Renderer (`latex.rs`)
- **Unconditional Escaping:** Strictly escapes all 10 LaTeX special characters (`\`, `&`, `%`, `$`, `#`, `_`, `{`, `}`, `~`, `^`) without heuristics.
- **Balanced-Brace Macro Replacer:** Scans only after `\begin{document}`, strictly preserving preamble `\newcommand{\ResumeSkills}[1]{#1}`.
- Injects content into `\ResumeSummary`, `\ResumeSkills`, `\ResumeExperience`, `\ResumeProjects`.
- Full-pipeline adversarial verification: `{key: value}`, command neutralization `\input{secret.tex}`, and back-to-back specials all compile cleanly to 1 page with intact visual text.

### Step 13: One-Page Validation & Stage A Shortening (`render_loop.rs`)
- Relevance scoring against JD keywords.
- **Singleton Protection:** Never drops the only bullet of an entry.
- **Experience vs. Project Tie-Breaker Rule:** Experience bullets are dropped before project bullets on equal or near-equal scores because project bullets carry repo-verified evidence backing. Documented in Section 5 step 5c.
- Stage A shortens up to 3 attempts, emitting `command_output`, `artifact_produced` (dropped bullet), and `node_done`.

### Step 14: Stage B (`\tighten`) Compaction & Sparse-Page Detection
- **4-Iteration State Machine:**
  - Attempts 1..=3: Content shortening without altering geometry.
  - Attempt 4: Injects `\tighten` right after `\begin{document}` to compact margins to `0.4in`.
  - If still $>1$ page after Attempt 4 $\to$ `page_limit_error`.
- **Status Classification:**
  - 1 page, density $\ge 60\% \to$ `FinalResumeStatus::Ready`.
  - 1 page, density $< 60\% \to$ `FinalResumeStatus::ReadySparse`.
- Live WebSocket stream verified: captures Attempt 4 `\tighten` and `node_done` with `compact_mode_applied: true`.

---

### Step 15: Single-Worker Generation Queue & Startup Recovery Sweep
- `src/db/mod.rs` runs the startup recovery sweep during database initialization, before Axum binds.
- `AppState` starts one `tokio::sync::mpsc::channel(128)` consumer. `POST /api/resumes` inserts a `queued` row and enqueues its ID without compiling in the request.
- The worker serially processes the generation pipeline, persists ready/error states, and emits Canvas events. Queued deletion is supported; deletion of an active generation returns HTTP 409.
- `step15_test` verifies three queued HTTP requests, serial processing, PDF artifacts, failure status, and restart recovery. `step7_test` was updated for asynchronous generation.

---

## 5. Steps 16–27 and Remaining Verification

- GitHub auth detection, repository discovery, shallow clone and incremental sync, gitleaks filtering, and README/manifest evidence indexing are implemented. `step19_test` passes with a local Git repository, including SHA skip and changed-commit sync.
- JD analysis, project ranking, Antigravity content generation, evidence validation, LaTeX rendering, and Canvas events are wired into the single-worker queue. `step22_test` passed with a real Antigravity call and a one-page `ready_sparse` PDF.
- Regeneration creates linked `parent_resume_id` records. Template adaptation offers preview/diff and requires a separate save. `step24_test` passes.
- Frontend pages and the release packaging script are present. Unit tests, debug build, `step11_test`, `step14_test`, `step15_test`, `step19_test`, `step22_test`, and `step24_test` passed on September 28. `scripts/package.ps1` built the release bundle; its executable served health and the home page from the packaged directory. The smoke-test database was removed afterward so the bundle remains clean.
- `gh auth status` currently reports no authenticated account. Do not claim live GitHub sync or the full Definition of Done passed until the user signs in using the CLI and the actual account sync is tested. See `README.md` for run and package instructions.

---

## 6. How to Run & Verify the Codebase

All build tools and compilers are local to this machine. Ensure PATH includes `w64devkit` and `agy`:

```powershell
$env:PATH = "$env:USERPROFILE\w64devkit\bin;$env:USERPROFILE\AppData\Local\agy\bin;" + $env:PATH

# Run all unit tests (including all 3 full-pipeline LaTeX adversarial tests)
cargo test --lib

# Run any specific step verification binary:
cargo run --bin step12_test   # Deterministic LaTeX rendering & compilation
cargo run --bin step13_test   # Stage A shortening + live WebSocket events
cargo run --bin step14_test   # Stage B \tighten + ready_sparse classification
cargo run --bin step15_test   # Serial queue, artifacts, error status, and startup recovery
cargo run --bin step19_test   # Local Git clone, gitleaks scan, evidence index, incremental sync
cargo run --bin step22_test   # End-to-end generation using Antigravity
cargo run --bin step24_test   # Adapted template preview and save

# Start the main server
cargo run --bin resumeforge
```

The above local tests passed on September 28, 2026. Live account sync remains unverified pending `gh auth login`.

