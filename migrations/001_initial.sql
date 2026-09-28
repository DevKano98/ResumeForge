-- ResumeForge initial schema
-- Foreign keys must be enabled per-connection: PRAGMA foreign_keys = ON;

CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE repositories (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    github_repo_id      TEXT NOT NULL UNIQUE,
    owner               TEXT NOT NULL,
    name                TEXT NOT NULL,
    description         TEXT,
    url                 TEXT NOT NULL,
    homepage_url        TEXT,
    visibility          TEXT NOT NULL CHECK (visibility IN ('public','private')),
    default_branch      TEXT NOT NULL DEFAULT 'main',
    latest_commit_sha   TEXT,
    local_clone_path    TEXT,
    pushed_at           TEXT,
    indexed_at          TEXT,
    last_secret_scan_at TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE projects (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    repository_id       INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    summary             TEXT,
    technologies_json   TEXT NOT NULL DEFAULT '[]',
    features_json       TEXT NOT NULL DEFAULT '[]',
    architecture_json   TEXT NOT NULL DEFAULT '[]',
    resume_points_json  TEXT NOT NULL DEFAULT '[]',
    confidence          REAL NOT NULL DEFAULT 0.0,
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE project_evidence (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id   INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    claim        TEXT NOT NULL,
    source_file  TEXT NOT NULL,
    line_start   INTEGER,
    line_end     INTEGER,
    commit_sha   TEXT,
    confidence   REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE master_templates (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    content_hash        TEXT NOT NULL,
    file_path           TEXT NOT NULL,
    is_starter_template INTEGER NOT NULL DEFAULT 0,
    adapted_from_paste  INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    superseded_at       TEXT
);

CREATE TABLE resumes (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    company              TEXT,
    role                 TEXT,
    job_description      TEXT NOT NULL,
    extra_instructions   TEXT,
    status               TEXT NOT NULL DEFAULT 'queued' CHECK (status IN (
        'queued','analysing_job','retrieving_projects','generating_content',
        'validating_evidence','rendering_latex','compiling_pdf','checking_pages',
        'ready','ready_sparse','github_error','ai_empty_response','ai_tool_denied','ai_timeout',
        'ai_hung','ai_invalid_json','ai_error','latex_error','page_limit_error',
        'validation_error','cancelled'
    )),
    error_stage          TEXT,
    error_detail         TEXT,
    parent_resume_id     INTEGER REFERENCES resumes(id) ON DELETE SET NULL,
    pdf_path             TEXT,
    tex_path             TEXT,
    master_sha256        TEXT,
    page_count           INTEGER,
    content_density_pct  REAL,
    compact_mode_applied INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at         TEXT
);

CREATE TABLE resume_projects (
    resume_id        INTEGER NOT NULL REFERENCES resumes(id) ON DELETE CASCADE,
    project_id       INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    relevance_score  REAL NOT NULL DEFAULT 0.0,
    selected         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (resume_id, project_id)
);

CREATE TABLE generation_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    resume_id     INTEGER NOT NULL REFERENCES resumes(id) ON DELETE CASCADE,
    seq           INTEGER NOT NULL,
    ts            TEXT NOT NULL DEFAULT (datetime('now')),
    node          TEXT NOT NULL,
    event_type    TEXT NOT NULL CHECK (event_type IN (
        'command_started','command_output','thinking','tool_call',
        'artifact_produced','node_done','node_error'
    )),
    payload_json  TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_generation_events_resume ON generation_events(resume_id, seq);
CREATE INDEX idx_resumes_status ON resumes(status);
CREATE INDEX idx_resume_projects_project ON resume_projects(project_id);
CREATE INDEX idx_projects_repository ON projects(repository_id);
