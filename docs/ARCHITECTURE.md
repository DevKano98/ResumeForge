# ResumeForge Architecture & Contributor Guide

## 1. System Overview & Core Philosophy
ResumeForge is a local-first, privacy-preserving desktop engine for Windows that tailors evidence-grounded, single-page LaTeX resumes to specific job postings.

Unlike naive AI resume wrappers that hallucinate metrics or invent experiences, ResumeForge enforces a strict **evidence invariant**:
> **Truth Invariant:** The model may only select, rephrase, or emphasize facts that are strictly grounded in either the candidate's verified master resume or commits/benchmarks extracted from local indexed GitHub repositories. Any invented claim, metric, employer, or ungrounded technology is deterministically removed before rendering.

The application runs entirely on the user's machine, consisting of an Axum-based Rust backend on `127.0.0.1:3000` and a responsive, vanilla HTML/CSS/JS frontend with an interactive SVG orchestration canvas.

---

## 2. Five-Stage Tailoring Pipeline

When a user submits a job description, the request enters a single-receiver generation queue that processes runs sequentially:

```
[1. Job Analysis] ──> [2. Project Retrieval] ──> [3. Content Generation] ──> [4. Evidence Validator] ──> [5. Render & Fit]
```

1. **Reading the Job Description (`jd_analysis`):**
   - Parses requirements, target company, target role, core technical skills, and domain concepts.
2. **Finding Your Best Projects (`project_retrieval`):**
   - Scans indexed repository metadata and evidence claims stored in local SQLite (`projects` and `project_evidence` tables).
   - Computes weighted skill overlap and ranks the top projects relevant to the posting.
3. **Writing Your Resume (`content_generation`):**
   - Calls the Antigravity CLI (`agy`) via a headless pseudo-terminal (PTY) runner.
   - Supplies the candidate's master facts, JD requirements, and ranked projects with numbered evidence IDs.
   - Enforces a strict JSON response schema (`GeneratedResumeContent`). Arbitrary LaTeX from the model is forbidden.
4. **Checking Every Claim Is True (`evidence_validation`):**
   - Executes deterministic truth guards against the generated draft.
   - Rejects unverified metrics, absent employers, unreferenced evidence IDs, or unapproved skills.
5. **Fitting It to One Page (`render_compile`):**
   - Injects sanitized content into the active LaTeX master template (`\ResumeSummary`, `\ResumeSkills`, `\ResumeWorkHistory`, `\ResumeProjects`).
   - Runs Tectonic in an iterative compaction loop (up to 4 attempts) dropping lowest-scoring bullets until the document fits on exactly 1 page.
   - Applies the `\tighten` macro if content density is low (`ready_sparse`).

---

## 3. Anti-Hallucination Truth Guards

The validation layer (`src/services/resume_generator.rs`) acts as a zero-trust gate between the AI model and the LaTeX compiler:

- **Summary Guard:** Every number or percentage in the summary must exist in the candidate's master facts. The summary must not name employers or titles not present in the master.
- **Experience Guard:** Every company name, job title, and employment date range must match an entry from the candidate's verified master resume. Any metric or percentage in a bullet must appear in that master entry's bullets.
- **Project Guard:** Any number or percentage in a project bullet must appear verbatim in the text of a cited evidence claim. Bullets citing non-existent or foreign evidence IDs are rejected.
- **Technology Vocabulary Guard:** A global vocabulary is built from master skills and technologies of indexed projects. Any vocabulary term appearing in a project bullet must be part of that project's cited evidence; in an experience bullet, it must exist in that master entry.
- **Skills Guard:** The output skills list is limited to the union of master known skills and ranked project technologies. Unauthorized skills are stripped and logged in `evidence.json`.

---

## 4. Data Flow & Event Schema

All inter-stage state and progress updates are mediated by an in-memory, thread-safe broadcast event bus (`src/services/event_bus.rs`) and persisted to the SQLite `generation_events` table.

### Event Schema
Every event follows a normalized JSON structure:
```json
{
  "resume_id": 1,
  "node": "content_generation",
  "seq": 8,
  "ts": "2026-09-28T09:00:08Z",
  "type": "step_update",
  "payload": {
    "delta": "Drafting achievements for RaftConsensus..."
  }
}
```

- **Node Keys:** `jd_analysis`, `project_retrieval`, `content_generation`, `evidence_validation`, `render_compile`.
- **Event Types:** `command_started`, `step_update`, `command_output`, `artifact_produced`, `node_warning`, `node_done`, `node_error`, `result`.
- **Coalescing:** High-frequency PTY text chunks are buffered and flushed as single events either every ~250ms or when exceeding ~200 characters.

Clients connect via WebSocket at `/ws/resumes/:id/live`. If a connection drops, the frontend automatically falls back to polling `/api/resumes/:id/events`.

---

## 5. Security & Isolation Model

ResumeForge operates with a strict desktop security model:

1. **Loopback Binding & DNS Rebinding Protection:**
   - The Axum server binds exclusively to `127.0.0.1`.
   - Incoming requests must provide a `Host` header matching `localhost` or `127.0.0.1` on the active port. Other hosts are rejected with HTTP 400.
2. **Strict Origin Enforcement:**
   - All state-modifying requests (`POST`, `PUT`, `DELETE`) and WebSocket handshakes require an `Origin` matching `http://127.0.0.1:<PORT>` or `http://localhost:<PORT>`, rejecting cross-origin attempts with HTTP 403.
3. **Payload Bounds:**
   - Request bodies are strictly capped (job descriptions: 64 KB; master templates: 512 KB) to prevent memory exhaustion.
4. **Secret Scanning (Fail-Closed):**
   - GitHub repositories are indexed locally. Every file is scanned with Gitleaks before ingestion.
   - If Gitleaks is missing or fails, indexing refuses to proceed. Secrets, tokens, and private keys are never stored in SQLite or passed to the AI.
5. **No Credential Logging:**
   - API tokens, passwords, and raw prompt transcripts are never logged at `INFO` level and never returned in API payloads.

---

## 6. Directory & Storage Layout

At runtime, local user data is kept isolated in the `data/` directory:
- `data/resumeforge.db`: SQLite database in WAL mode with enforced foreign keys.
- `data/master/resume.tex`: Active master LaTeX resume template.
- `data/templates/starter/`: Bundled classic and modern starter templates.
- `data/applications/:id/`: Per-generation artifacts (compiled PDF, `.tex` source, `evidence.json`, and input context).
- `tools/bin/`: Portable local copies of `gh`, `gitleaks`, and `tectonic`.
