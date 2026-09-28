# Verification Spikes & Integration Tests

The binaries in this directory (`src/bin/*.rs`) are throwaway verification spikes and step-by-step milestone tests used during the sequential build of ResumeForge (Section 12 of `MASTER_BUILD_PROMPT.md`).

- `pty_spike.rs`: Step 1 PTY subprocess spike verifying `agy` stream-json output and Windows ConPTY non-blocking reads.
- `db_spike.rs`: Step 3 SQLite connection pool, foreign keys enforcement, and migration verification.
- `status_test.rs`: Step 4 system health status route checks.
- `template_test.rs`: Step 5 starter templates, master `.tex` saving, snapshot history, and compilation validation.
- `tighten_spike.rs`: Verification of `\tighten` macro functional reality (compressing multi-page content to 1 page).
- `density_check.rs`: Verification of `pdf::estimate_content_density` heuristics on realistic full vs sparse documents.
- `step6_test.rs`: Step 6 Tectonic compilation, PDF metrics inspection, and HTTP PDF/LaTeX serving.
- `step7_test.rs`: Step 7 Resume history CRUD, artifact generation, and permanent deletion (DB + disk directory).
- `step9_test.rs`: Step 9 ContentGenerationAgent prompt generation, 3-run schema consistency verification, and evidence ID citations.
- `step10_test.rs`: Step 10 Event bus, SQLite persistence, and WebSocket replay-then-subscribe integration test.
- `step11_test.rs`: Step 11 Minimal canvas UI files serving and WebSocket event stream delivery test.
- `step12_test.rs`: Step 12 Deterministic LaTeX renderer placeholder injection and compilation test.
- `step13_test.rs`: Step 13 Stage A content-shortening and one-page validation loop test.
- `step14_test.rs`: Step 14 Stage B (\tighten) macro compaction and ready_sparse detection test.

**Note:** These binaries are verification harnesses and not part of the production application runtime. The primary application entry point is `src/main.rs`.
