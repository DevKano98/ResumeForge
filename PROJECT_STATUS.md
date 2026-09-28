# ResumeForge project status

**Updated:** 28 September 2026  
**Version:** `0.1.0`  
**Overall state:** The 27 planned build steps are implemented. Local unit and integration checks, the release build, and a packaged-server startup smoke test passed. The full real-account acceptance run remains open because GitHub CLI is not signed in on this machine.

This document records what is in the repository today. [README.md](README.md) is the shorter run guide, [MASTER_BUILD_PROMPT.md](MASTER_BUILD_PROMPT.md) is the original specification and definition of done, and [HANDOVER_CODEX.md](HANDOVER_CODEX.md) contains the development history.

## Product flow

1. Save a LaTeX master resume in Setup, using a starter or previewing an adaptation of an existing document. The server checks required content macros and compiles the master before saving it.
2. Connect GitHub CLI and sync repositories. The app shallow-clones repositories, scans their files for secrets, then indexes evidence from their README and common manifest files.
3. Paste a job description and optional instructions. A single-worker queue runs JD analysis, project ranking, Antigravity content generation, evidence validation, LaTeX rendering, and PDF compilation.
4. Watch the Canvas event stream, inspect accepted or rejected evidence, download PDF and `.tex` files, regenerate a linked version, or delete an application.

All application data is stored locally. Antigravity and GitHub CLI are external command-line dependencies; the app does not accept account passwords.

## Implemented build areas

| Planned steps | Built behavior | Main code |
| --- | --- | --- |
| 1–5: foundation, database, templates | Rust/Axum server, SQLite migration, local data layout, master-template validation, SHA-256 provenance and version snapshots | `src/main.rs`, `src/db/`, `migrations/001_initial.sql`, `src/routes/templates.rs` |
| 6–11: application API, AI process, events, first UI | Resume CRUD and artifacts, Antigravity capability probe through a PTY, structured error outcomes, persisted/broadcast Canvas events, WebSocket replay, static frontend | `src/routes/`, `src/services/pty_runner.rs`, `src/services/event_bus.rs`, `frontend/` |
| 12–14: LaTeX and page control | Escaped structured content is inserted into four master macros. Tectonic compiles and PDF page count/density are checked. Up to three attempts remove weak bullets; a fourth may apply `\tighten`. A one-page sparse result is labeled `ready_sparse` | `src/services/latex.rs`, `src/services/pdf.rs`, `src/services/render_loop.rs` |
| 15: queue and recovery | `POST /api/resumes` queues work; one consumer processes jobs serially. Startup marks interrupted work `cancelled`. Active generation cannot be deleted | `src/services/generation_queue.rs`, `src/db/mod.rs`, `src/routes/resumes.rs` |
| 16–19: GitHub and knowledge store | GitHub CLI status, remote repository listing, shallow clone and SHA-based incremental sync, gitleaks plus assignment-pattern filtering, README/manifest evidence with file and line references | `src/services/github.rs`, `github_indexer.rs`, `secret_scanner.rs`, `project_search.rs`, `src/db/projects.rs`, `repositories.rs` |
| 20–22: full generation pipeline | Heuristic JD analysis, project ranking, Antigravity JSON generation with one repair attempt for malformed output, evidence-ID validation, render/compile loop, saved artifacts and live events | `src/services/project_search.rs`, `antigravity.rs`, `resume_generator.rs` |
| 23–25: versioning and errors | Regeneration links to `parent_resume_id`; template adaptation returns preview/diff before a separate save; stage-specific failure status and detail are stored and surfaced | `src/routes/resumes.rs`, `src/services/template_adapt.rs`, `src/services/generation_queue.rs` |
| 26: frontend | Create, Setup, GitHub, History, and resume-detail pages; Canvas graph/event feed, evidence review, downloads, regeneration, and error display | `frontend/*.html`, `frontend/js/`, `frontend/css/app.css` |
| 27: local package | Release executable plus frontend, starter templates, README and `run.ps1`. Launcher waits for health before opening localhost. Existing private data is not copied | `scripts/package.ps1`, `dist/ResumeForge/` |

### Database and local files

The SQLite migration defines `settings`, `repositories`, `projects`, `project_evidence`, `master_templates`, `resumes`, `resume_projects`, and `generation_events`. Foreign keys cascade evidence and event cleanup where appropriate. Resume rows store status, error stage/detail, version parent, artifact paths, master hash, page count, density, and compact-mode flag.

The default working-directory layout is:

```text
data/
  resumeforge.db                  Local SQLite database, created at startup
  master/resume.tex               Active master, created when saved in Setup
  master/master-history/          Version snapshots
  repo-cache/<repository-id>/     App-owned shallow clones
  applications/<resume-id>/       Job, input, evidence, JSON, LaTeX and PDF artifacts
  templates/starter/              Shipped classic and modern starter templates
  temp/                           Temporary compile/scan work
```

The build bundle contains only the starter part of `data/`. Its database and user content are created later on first use. The bundle smoke-test database was removed after testing.

### HTTP and live interfaces

| Area | Routes |
| --- | --- |
| Health and capabilities | `GET /api/health`, `GET /api/system/status`, `GET /api/system/antigravity-probe` |
| GitHub | `GET /api/github/status`, `POST /api/github/sync` |
| Indexed projects | `GET /api/projects`, `GET /api/projects/{id}` |
| Master template | `GET/POST /api/template`, `GET /api/template/pdf`, `POST /api/template/adapt`, `GET /api/master/history`, `GET /api/master/history/{id}`, `GET /api/templates/starters` |
| Applications | `GET/POST /api/resumes`, `GET/DELETE /api/resumes/{id}`, `POST /api/resumes/{id}/regenerate` |
| Application details | `GET /api/resumes/{id}/events`, `/evidence`, `/pdf`, `/tex`; `GET /ws/resumes/{id}/live` |

The frontend is served from `frontend/` at `http://127.0.0.1:3000/` by default. `PORT` can override the port. The app currently binds to loopback in `src/main.rs`.

### Generation stages and status handling

The queue advances through `queued`, `analysing_job`, `retrieving_projects`, `generating_content`, `validating_evidence`, `rendering_latex`, `compiling_pdf`, and `checking_pages`, ending at `ready` or `ready_sparse` when successful. Failures have specific stored statuses, including `github_error`, AI empty/denied/timeout/hung/invalid-JSON/error variants, `latex_error`, `page_limit_error`, `validation_error`, and `cancelled`. The row also records the failing stage and detail.

Every application has persisted Canvas events with ordered sequence numbers. The WebSocket sends saved events and then live broadcasts. The detail page can also read events over HTTP. AI output is parsed as structured JSON; project bullet evidence IDs must match the selected repository evidence. Rejected claims are recorded in `evidence.json`. Experience facts are taken from the user's master resume and are not repository-cited.

## Verification performed on 28 September 2026

| Check | Result and scope |
| --- | --- |
| `cargo fmt --check`, `cargo test --lib`, `cargo build` | Passed. 15 library tests passed, including queue serialization, secret filtering, evidence rejection, template adaptation, and adversarial LaTeX escaping/compilation. |
| `step11_test` | Passed static-file serving and WebSocket event delivery. |
| `step14_test` | Passed normal/sparse one-page classification and attempt-four `\tighten` Canvas flow. |
| `step15_test` | Passed queued HTTP requests, serial worker, failure status, artifacts, and startup recovery. |
| `step19_test` | Passed against a local Git repository: clone, gitleaks scan, evidence extraction, unchanged-SHA skip, and incremental sync. |
| `step22_test` | Passed an end-to-end generation with a real Antigravity call, persisted Canvas events, evidence-grounded one-page PDF, and `ready_sparse` outcome. |
| `step24_test` | Passed adapted-template preview, LaTeX compile, separate save, and version tracking. |
| Frontend JavaScript | `node --check` passed for all six JavaScript files. |
| Release package | `scripts/package.ps1` completed; `dist/ResumeForge/resumeforge.exe` served HTTP 200 from `/api/health` and `/` when launched from its bundle. |

The release compiler encountered transient Windows file locks on initial attempts. `scripts/package.ps1` now builds with one job and retries transient failures; the final release build passed. This is a packaging-environment issue observed during verification, not a failing Rust test.

## Remaining acceptance checks and limitations

1. **Real GitHub account sync is unverified.** `gh auth status` still reports no signed-in account. The user must complete `gh auth login` and `gh auth setup-git` in a terminal, then sync at least one actual repository through the GitHub page. Do not send credentials to the app or chat.
2. **The full user-data run has not been performed.** The tests use controlled master/JD/repository fixtures. The original definition of done also calls for an actual master resume, real job description, live account sync, download/regenerate/delete checks, and a forced timeout observation in the UI. These should be run with the user's own data after GitHub sign-in.
3. **Repository evidence scope is deliberately narrow.** Indexing reads a root README plus common root manifests (`Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `requirements.txt`). It does not claim full source-code understanding or proof that every generated statement is true. Evidence-ID validation confirms linkage to indexed lines; users should review final wording.
4. **External tools remain prerequisites.** Running the bundle does not install or bundle `gh`, `git`, `gitleaks`, `agy`, or Tectonic, and Tectonic needs its LaTeX bundle cached. Setup reports capability failures.
5. **Release package is local Windows packaging.** The checked `run.ps1` starts the executable and opens localhost. Cross-platform distribution and installer signing are outside this build.

These open checks mean the project is **implemented and locally verified**, while full real-account acceptance is **pending**. No claim of error-free operation for every repository, template, or job description has been established.

## How to run and verify

From the repository root:

```powershell
cargo run --bin resumeforge
# Then open http://127.0.0.1:3000/
```

Or run `dist/ResumeForge/run.ps1` from the packaged directory. On a new machine, install the external CLI tools, sign in through their own CLI flows, and use Setup to save a master resume before generating.

Useful repeat checks:

```powershell
cargo test --lib
cargo run --bin step19_test
cargo run --bin step22_test
cargo run --bin step24_test
```

`step19_test` needs gitleaks; `step22_test` makes a real Antigravity call. To rebuild the package, move or rename the existing `dist/ResumeForge` first because the packaging script intentionally refuses to overwrite it.
