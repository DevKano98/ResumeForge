use crate::config::Config;
use crate::db::resumes;
use crate::models::generation::{ContentGenerationInput, GeneratedResumeContent};
use crate::services::render_loop::{FinalResumeStatus, RenderLoopContext, RenderLoopOutcome};
use crate::services::{antigravity, event_bus, project_search, render_loop};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::fmt;

#[derive(Debug)]
pub struct GenerationFailure {
    pub status: &'static str,
    pub stage: &'static str,
    pub detail: String,
}

impl fmt::Display for GenerationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.stage, self.detail)
    }
}

fn failure(status: &'static str, stage: &'static str, err: impl fmt::Display) -> GenerationFailure {
    GenerationFailure {
        status,
        stage,
        detail: err.to_string(),
    }
}

async fn status(db: &SqlitePool, id: i64, value: &str) -> Result<(), GenerationFailure> {
    sqlx::query("UPDATE resumes SET status=? WHERE id=?")
        .bind(value)
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| failure("validation_error", "database", e))?;
    Ok(())
}

async fn event(
    db: &SqlitePool,
    bus: &event_bus::EventBus,
    id: i64,
    node: &str,
    kind: &str,
    payload: serde_json::Value,
) -> Result<(), GenerationFailure> {
    event_bus::emit_event(db, bus, id, node, kind, payload)
        .await
        .map_err(|e| failure("validation_error", "event_bus", e))?;
    Ok(())
}

pub async fn run(
    id: i64,
    config: &Config,
    db: &SqlitePool,
    bus: &event_bus::EventBus,
) -> Result<(), GenerationFailure> {
    let row = resumes::get_resume_by_id(db, id)
        .await
        .map_err(|e| failure("validation_error", "database", e))?
        .ok_or_else(|| failure("cancelled", "generation", "Resume was deleted"))?;
    let app_dir = config.data_dir.join("applications").join(id.to_string());
    tokio::fs::create_dir_all(&app_dir)
        .await
        .map_err(|e| failure("validation_error", "files", e))?;
    tokio::fs::write(app_dir.join("job.txt"), &row.job_description)
        .await
        .map_err(|e| failure("validation_error", "files", e))?;
    if let Some(instructions) = &row.extra_instructions {
        tokio::fs::write(app_dir.join("instructions.txt"), instructions)
            .await
            .map_err(|e| failure("validation_error", "files", e))?;
    }
    let master_path = config.data_dir.join("master").join("resume.tex");
    let master = tokio::fs::read_to_string(&master_path).await.map_err(|_| {
        failure(
            "validation_error",
            "master_template",
            "Save a master resume in Setup before generating",
        )
    })?;
    let master_sha = format!("{:x}", Sha256::digest(master.as_bytes()));
    tokio::fs::write(app_dir.join("master-snapshot.tex"), &master)
        .await
        .map_err(|e| failure("validation_error", "files", e))?;

    event(
        db,
        bus,
        id,
        "jd_analysis",
        "command_started",
        serde_json::json!({}),
    )
    .await?;
    let jd = project_search::analyse_job(
        &row.job_description,
        row.company.as_deref(),
        row.role.as_deref(),
    );
    status(db, id, "retrieving_projects").await?;
    event(
        db,
        bus,
        id,
        "jd_analysis",
        "node_done",
        serde_json::json!({"role": jd.target_role, "core_skills": jd.core_skills}),
    )
    .await?;

    event(
        db,
        bus,
        id,
        "project_retrieval",
        "command_started",
        serde_json::json!({}),
    )
    .await?;
    let ranked = project_search::rank_projects(db, &jd)
        .await
        .map_err(|e| failure("github_error", "project_retrieval", e))?;
    if ranked.is_empty() {
        return Err(failure(
            "github_error",
            "project_retrieval",
            "No indexed projects with evidence. Connect GitHub and sync repositories first",
        ));
    }
    for project in &ranked {
        sqlx::query("INSERT INTO resume_projects (resume_id, project_id, relevance_score, selected) VALUES (?, ?, ?, 1)")
            .bind(id).bind(project.input.project_id).bind(project.score).execute(db).await
            .map_err(|e| failure("github_error", "project_retrieval", e))?;
    }
    event(db, bus, id, "project_retrieval", "node_done", serde_json::json!({"projects": ranked.iter().map(|p| serde_json::json!({"id": p.input.project_id, "score": p.score})).collect::<Vec<_>>() })).await?;

    let master_facts = crate::services::master_parser::parse_master_facts(&master);
    let name = if !master_facts.candidate_name.is_empty() && master_facts.candidate_name != "Candidate" {
        master_facts.candidate_name.clone()
    } else {
        candidate_name(&master)
    };
    let input = ContentGenerationInput {
        candidate_name: name,
        master_facts: master_facts.clone(),
        jd_analysis: jd.clone(),
        ranked_projects: ranked.iter().map(|p| p.input.clone()).collect(),
        extra_instructions: row.extra_instructions.clone(),
    };
    tokio::fs::write(
        app_dir.join("input-context.json"),
        serde_json::to_vec_pretty(&input).unwrap(),
    )
    .await
    .map_err(|e| failure("validation_error", "files", e))?;
    status(db, id, "generating_content").await?;
    let actual_timeout = antigravity::get_content_gen_timeout_secs(0);
    event(
        db,
        bus,
        id,
        "content_generation",
        "command_started",
        serde_json::json!({"timeout_seconds": actual_timeout}),
    )
    .await?;
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel();
    let generation = antigravity::run_content_generation_streamed(&input, 0, Some(stream_tx));
    tokio::pin!(generation);
    let mut coalescer = event_bus::DeltaCoalescer::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut live_open = true;
    let generation_result = loop {
        tokio::select! {
            result = &mut generation => break result,
            raw = stream_rx.recv(), if live_open => {
                if let Some(raw) = raw {
                    emit_raw_ai_event(db, bus, id, &raw, &mut coalescer).await?;
                } else { live_open = false; }
            }
            _ = interval.tick() => {
                if let Some(chunk) = coalescer.flush() {
                    event(db, bus, id, "content_generation", "command_output", serde_json::json!({"chunk": chunk})).await?;
                }
            }
        }
    };
    while let Ok(raw) = stream_rx.try_recv() {
        emit_raw_ai_event(db, bus, id, &raw, &mut coalescer).await?;
    }
    if let Some(chunk) = coalescer.flush() {
        event(
            db,
            bus,
            id,
            "content_generation",
            "command_output",
            serde_json::json!({"chunk": chunk}),
        )
        .await?;
    }
    let (mut generated, ai_run) = generation_result.map_err(|err| match err {
        antigravity::ContentGenError::AiToolDenied(_) => {
            failure("ai_tool_denied", "content_generation", err)
        }
        antigravity::ContentGenError::AiEmptyResponse => {
            failure("ai_empty_response", "content_generation", err)
        }
        antigravity::ContentGenError::AiTimeout(_) => {
            failure("ai_timeout", "content_generation", err)
        }
        antigravity::ContentGenError::AiHung => failure("ai_hung", "content_generation", err),
        antigravity::ContentGenError::AiInvalidJson(_) => {
            failure("ai_invalid_json", "content_generation", err)
        }
        antigravity::ContentGenError::AiError(_) => failure("ai_error", "content_generation", err),
    })?;
    event(
        db,
        bus,
        id,
        "content_generation",
        "node_done",
        serde_json::json!({"elapsed_ms": ai_run.elapsed_ms}),
    )
    .await?;

    status(db, id, "validating_evidence").await?;
    event(
        db,
        bus,
        id,
        "evidence_validation",
        "command_started",
        serde_json::json!({}),
    )
    .await?;

    let db_tech_vocab: Vec<String> = sqlx::query_scalar::<_, String>("SELECT technologies_json FROM projects")
        .fetch_all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|json_str| serde_json::from_str::<Vec<String>>(&json_str).ok())
        .flatten()
        .collect();

    let mut evidence_report = validate_evidence(
        &mut generated,
        &input.ranked_projects,
        &input.master_facts,
        &db_tech_vocab,
    );

    let master_warnings = crate::services::master_parser::check_master_parsing_warnings(&master, &input.master_facts);
    if !master_warnings.is_empty() {
        for w in &master_warnings {
            event(
                db,
                bus,
                id,
                "evidence_validation",
                "node_warning",
                serde_json::json!({"warning": w}),
            )
            .await?;
        }
    }

    if let Some(obj) = evidence_report.as_object_mut() {
        obj.insert("warnings".into(), serde_json::json!(master_warnings));
    }

    tokio::fs::write(
        app_dir.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence_report).unwrap(),
    )
    .await
    .map_err(|e| failure("validation_error", "files", e))?;
    if generated.projects.is_empty() {
        return Err(failure(
            "validation_error",
            "evidence_validation",
            "No project bullets survived evidence validation",
        ));
    }
    event(db, bus, id, "evidence_validation", "node_done", serde_json::json!({"accepted": evidence_report["accepted"].as_array().map_or(0, Vec::len), "rejected": evidence_report["rejected"].as_array().map_or(0, Vec::len)})).await?;
    tokio::fs::write(
        app_dir.join("resume.json"),
        serde_json::to_vec_pretty(&generated).unwrap(),
    )
    .await
    .map_err(|e| failure("validation_error", "files", e))?;

    status(db, id, "rendering_latex").await?;
    event(
        db,
        bus,
        id,
        "render_compile",
        "command_started",
        serde_json::json!({}),
    )
    .await?;
    let pdf_file = app_dir.join("resume.pdf");
    let temp_dir = config.data_dir.join("temp").join(format!("render_{id}"));
    let ctx = RenderLoopContext {
        pool: db,
        bus,
        resume_id: id,
    };
    let keywords: Vec<String> = jd
        .core_skills
        .iter()
        .chain(jd.secondary_skills.iter())
        .cloned()
        .collect();
    let result = render_loop::run_render_compile_loop(
        &master,
        &generated,
        &keywords,
        &temp_dir,
        Some(&pdf_file),
        Some(&ctx),
    )
    .await
    .map_err(|e| failure("latex_error", "render_compile", e))?;
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    match result {
        RenderLoopOutcome::Success {
            final_latex,
            page_count,
            content_density_pct,
            compact_mode_applied,
            status: final_status,
            ..
        } => {
            event(db, bus, id, "render_compile", "artifact_produced", serde_json::json!({"kind": "latex_diff", "content": line_diff(&master, &final_latex)})).await?;
            let tex_file = app_dir.join("resume.tex");
            tokio::fs::write(&tex_file, final_latex)
                .await
                .map_err(|e| failure("latex_error", "render_compile", e))?;
            let status_name = if final_status == FinalResumeStatus::ReadySparse {
                "ready_sparse"
            } else {
                "ready"
            };
            resumes::update_resume_artifacts(
                db,
                id,
                status_name,
                &pdf_file.to_string_lossy(),
                &tex_file.to_string_lossy(),
                &master_sha,
                page_count as i64,
                content_density_pct as f64,
            )
            .await
            .map_err(|e| failure("validation_error", "database", e))?;
            sqlx::query("UPDATE resumes SET compact_mode_applied=? WHERE id=?")
                .bind(i64::from(compact_mode_applied))
                .bind(id)
                .execute(db)
                .await
                .map_err(|e| failure("validation_error", "database", e))?;
            Ok(())
        }
        RenderLoopOutcome::PageLimitError {
            last_failing_latex, ..
        } => {
            tokio::fs::write(app_dir.join("resume.tex"), last_failing_latex)
                .await
                .map_err(|e| failure("latex_error", "render_compile", e))?;
            Err(failure(
                "page_limit_error",
                "render_compile",
                "Resume exceeds one page after shortening and compaction",
            ))
        }
        RenderLoopOutcome::CompileError { error_message, .. } => {
            Err(failure("latex_error", "render_compile", error_message))
        }
    }
}

fn candidate_name(master: &str) -> String {
    let re = regex::Regex::new(r"\\Huge\s+\\textbf\{([^}]+)\}").unwrap();
    re.captures(master)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "Candidate".into())
}

fn line_diff(before: &str, after: &str) -> String {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let mut out = String::new();
    for index in 0..old.len().max(new.len()) {
        match (old.get(index), new.get(index)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                out.push_str("- ");
                out.push_str(a);
                out.push('\n');
                out.push_str("+ ");
                out.push_str(b);
                out.push('\n');
            }
            (Some(a), None) => {
                out.push_str("- ");
                out.push_str(a);
                out.push('\n');
            }
            (None, Some(b)) => {
                out.push_str("+ ");
                out.push_str(b);
                out.push('\n');
            }
            _ => {}
        }
    }
    out
}

async fn emit_raw_ai_event(
    db: &SqlitePool,
    bus: &event_bus::EventBus,
    id: i64,
    raw: &serde_json::Value,
    coalescer: &mut event_bus::DeltaCoalescer,
) -> Result<(), GenerationFailure> {
    if let Some(delta) = raw
        .pointer("/step_update/text_delta")
        .and_then(|v| v.as_str())
    {
        if let Some(chunk) = coalescer.push(delta) {
            event(
                db,
                bus,
                id,
                "content_generation",
                "command_output",
                serde_json::json!({"chunk": chunk}),
            )
            .await?;
        }
    } else {
        if let Some(chunk) = coalescer.flush() {
            event(
                db,
                bus,
                id,
                "content_generation",
                "command_output",
                serde_json::json!({"chunk": chunk}),
            )
            .await?;
        }
        if let Some((kind, payload)) = event_bus::map_agy_event_to_canvas(raw) {
            event(db, bus, id, "content_generation", kind, payload).await?;
        }
    }
    Ok(())
}

pub fn is_skill_allowed(
    candidate: &str,
    master_facts: &crate::models::generation::MasterFacts,
    ranked: &[crate::models::generation::RankedProjectInput],
) -> bool {
    let cand_clean = candidate.trim();
    if cand_clean.is_empty() {
        return false;
    }
    let cand_canon = crate::services::master_parser::canonicalize_skill(cand_clean);

    // 1. Master known_skills
    for skill in &master_facts.known_skills {
        if crate::services::master_parser::canonicalize_skill(skill) == cand_canon {
            return true;
        }
        if term_appears_in_text(cand_clean, skill) {
            return true;
        }
    }

    // 2. Technologies of selected/ranked projects
    for proj in ranked {
        for tech in &proj.technologies {
            if crate::services::master_parser::canonicalize_skill(tech) == cand_canon {
                return true;
            }
            if term_appears_in_text(cand_clean, tech) {
                return true;
            }
        }
        // Tech named in cited evidence
        for ev in &proj.evidence {
            if term_appears_in_text(cand_clean, &ev.claim) {
                return true;
            }
        }
    }

    false
}

pub fn term_appears_in_text(term: &str, text: &str) -> bool {
    let term_trimmed = term.trim();
    if term_trimmed.is_empty() {
        return false;
    }
    if term_trimmed.len() <= 2 || term_trimmed.eq_ignore_ascii_case("go") {
        let pattern = format!(r"\b{}\b", regex::escape(term_trimmed));
        if let Ok(re) = regex::Regex::new(&pattern) {
            re.is_match(text)
        } else {
            false
        }
    } else {
        let pattern = format!(r"(?i)\b{}\b", regex::escape(term_trimmed));
        if let Ok(re) = regex::Regex::new(&pattern) {
            re.is_match(text)
        } else {
            false
        }
    }
}

pub fn validate_evidence(
    content: &mut GeneratedResumeContent,
    ranked: &[crate::models::generation::RankedProjectInput],
    master_facts: &crate::models::generation::MasterFacts,
    extra_tech_vocab: &[String],
) -> serde_json::Value {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    // Build technology vocabulary: all master skills plus all project technologies in DB / ranked
    let mut tech_vocab = std::collections::HashSet::new();
    for skill in &master_facts.known_skills {
        let s = skill.trim();
        if !s.is_empty() {
            tech_vocab.insert(s.to_string());
        }
    }
    for proj in ranked {
        for tech in &proj.technologies {
            let t = tech.trim();
            if !t.is_empty() {
                tech_vocab.insert(t.to_string());
            }
        }
    }
    for tech in extra_tech_vocab {
        let t = tech.trim();
        if !t.is_empty() {
            tech_vocab.insert(t.to_string());
        }
    }

    // 1. Summary Guard
    // Every number must appear in master facts; must not name an employer or title absent from master.
    if !content.summary.is_empty() {
        let mut summary_valid = true;
        let mut master_facts_text = String::new();
        master_facts_text.push_str(&master_facts.candidate_name);
        master_facts_text.push(' ');
        if let Some(c) = &master_facts.contact {
            master_facts_text.push_str(c);
            master_facts_text.push(' ');
        }
        if let Some(s) = &master_facts.current_summary {
            master_facts_text.push_str(s);
            master_facts_text.push(' ');
        }
        for s in &master_facts.known_skills {
            master_facts_text.push_str(s);
            master_facts_text.push(' ');
        }
        for w in &master_facts.work_history {
            master_facts_text.push_str(&w.company);
            master_facts_text.push(' ');
            master_facts_text.push_str(&w.title);
            master_facts_text.push(' ');
            master_facts_text.push_str(&w.date_range);
            master_facts_text.push(' ');
            master_facts_text.push_str(&w.highlights.join(" "));
            master_facts_text.push(' ');
        }
        for ed in &master_facts.education {
            master_facts_text.push_str(&ed.institution);
            master_facts_text.push(' ');
            master_facts_text.push_str(&ed.degree);
            master_facts_text.push(' ');
            if let Some(dr) = &ed.date_range {
                master_facts_text.push_str(dr);
                master_facts_text.push(' ');
            }
            master_facts_text.push_str(&ed.highlights.join(" "));
            master_facts_text.push(' ');
        }

        // Rule A: Numbers in summary
        let metrics = crate::services::master_parser::extract_metrics(&content.summary);
        for metric in &metrics {
            if !crate::services::master_parser::metric_appears_in_source(metric, &master_facts_text) {
                rejected.push(serde_json::json!({
                    "section": "summary",
                    "item": metric.clone(),
                    "reason": format!("Summary contains number/metric '{}' not found in master facts", metric)
                }));
                summary_valid = false;
                break;
            }
        }

        // Rule B: Employer check
        if summary_valid {
            let re_employer = regex::Regex::new(r"\b(?:at|for)\s+([A-Z][A-Za-z0-9&.]+)").unwrap();
            for cap in re_employer.captures_iter(&content.summary) {
                if let Some(m) = cap.get(1) {
                    let named = m.as_str().trim();
                    let is_tech = is_skill_allowed(named, master_facts, ranked);
                    let matches_master = master_facts.work_history.iter().any(|w| {
                        let norm_m = crate::services::master_parser::normalize_str(named);
                        let norm_w = crate::services::master_parser::normalize_str(&w.company);
                        norm_w.contains(&norm_m) || norm_m.contains(&norm_w)
                    });
                    if !is_tech && !matches_master && !master_facts.work_history.is_empty() {
                        rejected.push(serde_json::json!({
                            "section": "summary",
                            "item": named.to_string(),
                            "reason": format!("Summary names employer '{}' absent from master resume facts", named)
                        }));
                        summary_valid = false;
                        break;
                    }
                }
            }
        }

        // Rule C: Title check
        if summary_valid {
            let re_title = regex::Regex::new(r"\b(?:as\s+(?:a|an)?\s*)([A-Z][a-zA-Z0-9]+)").unwrap();
            for cap in re_title.captures_iter(&content.summary) {
                if let Some(m) = cap.get(1) {
                    let named = m.as_str().trim();
                    let matches_master = master_facts.work_history.iter().any(|w| {
                        let norm_m = crate::services::master_parser::normalize_str(named);
                        let norm_w = crate::services::master_parser::normalize_str(&w.title);
                        norm_w.contains(&norm_m) || norm_m.contains(&norm_w)
                    }) || master_facts.current_summary.as_ref().map_or(false, |s| {
                        let norm_m = crate::services::master_parser::normalize_str(named);
                        let norm_s = crate::services::master_parser::normalize_str(s);
                        norm_s.contains(&norm_m)
                    });
                    if !matches_master && !master_facts.work_history.is_empty() {
                        rejected.push(serde_json::json!({
                            "section": "summary",
                            "item": named.to_string(),
                            "reason": format!("Summary names title '{}' absent from master resume facts", named)
                        }));
                        summary_valid = false;
                        break;
                    }
                }
            }
        }

        if !summary_valid {
            content.summary = master_facts.current_summary.clone().unwrap_or_default();
        } else {
            accepted.push(serde_json::json!({
                "section": "summary",
                "content": content.summary.clone()
            }));
        }
    }

    // 2. Skills Guard
    content.skills.languages.retain(|skill| {
        if is_skill_allowed(skill, master_facts, ranked) {
            accepted.push(serde_json::json!({
                "section": "skills",
                "category": "languages",
                "skill": skill.clone()
            }));
            true
        } else {
            rejected.push(serde_json::json!({
                "section": "skills",
                "category": "languages",
                "item": skill.clone(),
                "reason": format!("Skill '{}' does not appear in master resume known skills or project technologies/evidence", skill)
            }));
            false
        }
    });

    content.skills.frameworks_and_tools.retain(|skill| {
        if is_skill_allowed(skill, master_facts, ranked) {
            accepted.push(serde_json::json!({
                "section": "skills",
                "category": "frameworks_and_tools",
                "skill": skill.clone()
            }));
            true
        } else {
            rejected.push(serde_json::json!({
                "section": "skills",
                "category": "frameworks_and_tools",
                "item": skill.clone(),
                "reason": format!("Skill '{}' does not appear in master resume known skills or project technologies/evidence", skill)
            }));
            false
        }
    });

    content.skills.core_concepts.retain(|concept| {
        if is_skill_allowed(concept, master_facts, ranked) {
            accepted.push(serde_json::json!({
                "section": "skills",
                "category": "core_concepts",
                "skill": concept.clone()
            }));
            true
        } else {
            rejected.push(serde_json::json!({
                "section": "skills",
                "category": "core_concepts",
                "item": concept.clone(),
                "reason": format!("Skill/concept '{}' does not appear in master resume known skills or project technologies/evidence", concept)
            }));
            false
        }
    });

    // 3. Experience Guard
    if master_facts.work_history.is_empty() {
        if !content.experience.is_empty() {
            for exp in content.experience.drain(..) {
                rejected.push(serde_json::json!({
                    "section": "experience",
                    "item": exp.company,
                    "reason": "Master resume has no experience entries; experience section cannot be generated"
                }));
            }
        }
    } else {
        content.experience.retain_mut(|exp| {
            let matching_master = master_facts.work_history.iter().find(|m| {
                crate::services::master_parser::normalize_str(&exp.company)
                    == crate::services::master_parser::normalize_str(&m.company)
                    && crate::services::master_parser::normalize_str(&exp.title)
                        == crate::services::master_parser::normalize_str(&m.title)
                    && crate::services::master_parser::normalize_dates(&exp.date_range)
                        == crate::services::master_parser::normalize_dates(&m.date_range)
            });

            let Some(master_entry) = matching_master else {
                rejected.push(serde_json::json!({
                    "section": "experience",
                    "item": format!("{} at {} ({})", exp.title, exp.company, exp.date_range),
                    "reason": "Experience entry (company, title, date range) does not match any entry in master resume facts"
                }));
                return false;
            };

            let master_bullets_text = master_entry.highlights.join(" ");
            exp.bullets.retain(|bullet| {
                // Metric check
                let metrics = crate::services::master_parser::extract_metrics(bullet);
                for metric in &metrics {
                    if !crate::services::master_parser::metric_appears_in_source(metric, &master_bullets_text) {
                        rejected.push(serde_json::json!({
                            "section": "experience",
                            "company": exp.company.clone(),
                            "bullet": bullet.clone(),
                            "metric": metric.clone(),
                            "reason": format!(
                                "Experience bullet contains metric/number '{}' not found in master resume entry highlights",
                                metric
                            )
                        }));
                        return false;
                    }
                }

                // Technology vocabulary check
                for term in &tech_vocab {
                    if term_appears_in_text(term, bullet) {
                        let in_highlights = master_entry.highlights.iter().any(|h| term_appears_in_text(term, h));
                        let in_skills = master_facts.known_skills.iter().any(|s| {
                            crate::services::master_parser::canonicalize_skill(s) == crate::services::master_parser::canonicalize_skill(term)
                                || term_appears_in_text(term, s)
                        });
                        if !in_highlights && !in_skills {
                            rejected.push(serde_json::json!({
                                "section": "experience",
                                "company": exp.company.clone(),
                                "bullet": bullet.clone(),
                                "term": term.clone(),
                                "reason": format!(
                                    "Experience bullet contains technology vocabulary term '{}' not found in master entry highlights or master skills",
                                    term
                                )
                            }));
                            return false;
                        }
                    }
                }

                accepted.push(serde_json::json!({
                    "section": "experience",
                    "company": exp.company.clone(),
                    "bullet": bullet.clone(),
                }));
                true
            });

            !exp.bullets.is_empty()
        });
    }

    // 4. Project Guard
    content.projects.retain_mut(|project| {
        let source = ranked.iter().find(|p| p.project_id == project.project_id);
        let Some(source) = source else {
            rejected.push(serde_json::json!({
                "section": "projects",
                "project_id": project.project_id,
                "reason": "Project was not in ranked evidence"
            }));
            return false;
        };
        project.name = source.name.clone();
        project.bullets.retain(|bullet| {
            // Check evidence ID validity
            let valid_ids = !bullet.evidence_ids.is_empty()
                && bullet
                    .evidence_ids
                    .iter()
                    .all(|id| source.evidence.iter().any(|item| item.id == *id));

            if !valid_ids {
                rejected.push(serde_json::json!({
                    "section": "projects",
                    "project_id": project.project_id,
                    "text": bullet.text,
                    "evidence_ids": bullet.evidence_ids,
                    "reason": "Missing or foreign evidence ID"
                }));
                return false;
            }

            // Check metrics/numbers against cited evidence claims
            let cited_claims_text = source
                .evidence
                .iter()
                .filter(|e| bullet.evidence_ids.contains(&e.id))
                .map(|e| e.claim.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            let metrics = crate::services::master_parser::extract_metrics(&bullet.text);
            for metric in &metrics {
                if !crate::services::master_parser::metric_appears_in_source(metric, &cited_claims_text) {
                    rejected.push(serde_json::json!({
                        "section": "projects",
                        "project_id": project.project_id,
                        "bullet": bullet.text.clone(),
                        "metric": metric.clone(),
                        "evidence_ids": bullet.evidence_ids.clone(),
                        "reason": format!(
                            "Metric/number '{}' in project bullet does not appear in cited evidence claims",
                            metric
                        )
                    }));
                    return false;
                }
            }

            // Technology vocabulary check
            for term in &tech_vocab {
                if term_appears_in_text(term, &bullet.text) {
                    let in_tech = source.technologies.iter().any(|t| {
                        crate::services::master_parser::canonicalize_skill(t) == crate::services::master_parser::canonicalize_skill(term)
                            || term_appears_in_text(term, t)
                    });
                    let in_claims = term_appears_in_text(term, &cited_claims_text);
                    if !in_tech && !in_claims {
                        rejected.push(serde_json::json!({
                            "section": "projects",
                            "project_id": project.project_id,
                            "bullet": bullet.text.clone(),
                            "term": term.clone(),
                            "evidence_ids": bullet.evidence_ids.clone(),
                            "reason": format!(
                                "Technology vocabulary term '{}' in project bullet does not appear in project technologies or cited evidence claims",
                                term
                            )
                        }));
                        return false;
                    }
                }
            }

            accepted.push(serde_json::json!({
                "section": "projects",
                "project_id": project.project_id,
                "text": bullet.text.clone(),
                "evidence_ids": bullet.evidence_ids.clone()
            }));
            true
        });
        !project.bullets.is_empty()
    });

    serde_json::json!({"accepted": accepted, "rejected": rejected})
}

#[cfg(test)]
mod tests {
    use super::validate_evidence;
    use crate::models::generation::*;

    #[test]
    fn rejects_foreign_evidence_ids() {
        let mut content = GeneratedResumeContent {
            summary: "Experienced Engineer".into(),
            skills: GeneratedSkills {
                languages: vec![],
                frameworks_and_tools: vec![],
                core_concepts: vec![],
            },
            experience: vec![],
            projects: vec![GeneratedProjectItem {
                project_id: 1,
                name: "x".into(),
                bullets: vec![GeneratedProjectBullet {
                    text: "claim".into(),
                    evidence_ids: vec![99],
                }],
            }],
            achievements: vec![],
        };
        let ranked = vec![RankedProjectInput {
            project_id: 1,
            name: "real".into(),
            summary: String::new(),
            technologies: vec![],
            evidence: vec![ProjectEvidenceItem {
                id: 2,
                claim: "fact".into(),
                source_file: "README.md".into(),
                line_start: Some(1),
                line_end: Some(1),
            }],
        }];
        let master_facts = MasterFacts::default();
        let report = validate_evidence(&mut content, &ranked, &master_facts, &[]);
        assert!(content.projects.is_empty());
        assert_eq!(report["rejected"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_metric_not_in_cited_evidence_claim() {
        let mut content = GeneratedResumeContent {
            summary: "Experienced Engineer".into(),
            skills: GeneratedSkills {
                languages: vec![],
                frameworks_and_tools: vec![],
                core_concepts: vec![],
            },
            experience: vec![],
            projects: vec![GeneratedProjectItem {
                project_id: 1,
                name: "ResilientQueue".into(),
                bullets: vec![GeneratedProjectBullet {
                    text: "Engineered queue engine that was 72% faster than baseline".into(),
                    evidence_ids: vec![10],
                }],
            }],
            achievements: vec![],
        };
        let ranked = vec![RankedProjectInput {
            project_id: 1,
            name: "ResilientQueue".into(),
            summary: "A queue".into(),
            technologies: vec![],
            evidence: vec![ProjectEvidenceItem {
                id: 10,
                claim: "Engineered an asynchronous embedded message queue engine utilizing WAL mode with crash-safe transaction logging.".into(),
                source_file: "README.md".into(),
                line_start: Some(1),
                line_end: Some(5),
            }],
        }];
        let master_facts = MasterFacts::default();
        let report = validate_evidence(&mut content, &ranked, &master_facts, &[]);

        assert!(content.projects.is_empty(), "Project bullet claiming 72% faster without 72 in claim must be rejected");
        let rejected = report["rejected"].as_array().expect("rejected list");
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0]["reason"].as_str().unwrap().contains("72%"));
    }

    #[test]
    fn experience_guard_rejects_unmatched_role_and_invented_metrics() {
        let master_facts = MasterFacts {
            candidate_name: "Test Candidate".into(),
            work_history: vec![WorkHistoryFact {
                title: "Senior Systems Engineer".into(),
                company: "CloudScale Systems".into(),
                date_range: "Jan 2023 -- Present".into(),
                location: Some("Remote".into()),
                highlights: vec![
                    "Architected high-throughput async processing engines handling over 50,000 requests per second.".into(),
                ],
            }],
            ..Default::default()
        };

        // Case 1: Invented metric 95% in experience bullet
        let mut content = GeneratedResumeContent {
            summary: "".into(),
            skills: GeneratedSkills { languages: vec![], frameworks_and_tools: vec![], core_concepts: vec![] },
            experience: vec![GeneratedExperienceItem {
                title: "Senior Systems Engineer".into(),
                company: "CloudScale Systems".into(),
                date_range: "Jan 2023 - Present".into(),
                location: Some("Remote".into()),
                bullets: vec![
                    "Improved latency by 95% across all clusters".into(),
                ],
            }],
            projects: vec![],
            achievements: vec![],
        };
        let report = validate_evidence(&mut content, &[], &master_facts, &[]);
        assert!(content.experience.is_empty());
        assert_eq!(report["rejected"].as_array().unwrap().len(), 1);
        assert!(report["rejected"][0]["reason"].as_str().unwrap().contains("95%"));

        // Case 2: Empty master experience -> renders empty
        let empty_master = MasterFacts::default();
        let mut content2 = GeneratedResumeContent {
            summary: "".into(),
            skills: GeneratedSkills { languages: vec![], frameworks_and_tools: vec![], core_concepts: vec![] },
            experience: vec![GeneratedExperienceItem {
                title: "Engineer".into(),
                company: "Startup".into(),
                date_range: "2020-2022".into(),
                location: None,
                bullets: vec!["Built features".into()],
            }],
            projects: vec![],
            achievements: vec![],
        };
        let report2 = validate_evidence(&mut content2, &[], &empty_master, &[]);
        assert!(content2.experience.is_empty());
        assert_eq!(report2["rejected"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn skills_guard_removes_unauthorized_skills_and_records_in_rejected() {
        let master_facts = MasterFacts {
            candidate_name: "Test Candidate".into(),
            known_skills: vec!["Rust".into(), "Python".into(), "Tokio".into()],
            ..Default::default()
        };
        let ranked = vec![RankedProjectInput {
            project_id: 1,
            name: "Queue".into(),
            summary: "Queue".into(),
            technologies: vec!["SQLite".into()],
            evidence: vec![],
        }];

        // Model returns Java and Kubernetes which appear in neither master nor projects
        let mut content = GeneratedResumeContent {
            summary: "".into(),
            skills: GeneratedSkills {
                languages: vec!["Rust".into(), "Java".into()],
                frameworks_and_tools: vec!["Tokio".into(), "Kubernetes".into()],
                core_concepts: vec!["Concurrency".into()],
            },
            experience: vec![],
            projects: vec![],
            achievements: vec![],
        };

        let report = validate_evidence(&mut content, &ranked, &master_facts, &[]);

        // Assert Java and Kubernetes were removed
        assert_eq!(content.skills.languages, vec!["Rust"]);
        assert_eq!(content.skills.frameworks_and_tools, vec!["Tokio"]);
        assert!(!content.skills.languages.contains(&"Java".to_string()));
        assert!(!content.skills.frameworks_and_tools.contains(&"Kubernetes".to_string()));

        // Assert rejection logged to evidence.json with section "skills"
        let rejected = report["rejected"].as_array().expect("rejected array");
        let java_rejection = rejected.iter().find(|r| r["item"] == "Java" && r["section"] == "skills");
        assert!(java_rejection.is_some(), "Java must be logged to rejected under section 'skills'");
        let k8s_rejection = rejected.iter().find(|r| r["item"] == "Kubernetes" && r["section"] == "skills");
        assert!(k8s_rejection.is_some(), "Kubernetes must be logged to rejected under section 'skills'");
    }

    #[test]
    fn tech_vocab_guard_rejects_unauthorized_technology_bullet() {
        let master_facts = MasterFacts {
            candidate_name: "Test Candidate".into(),
            known_skills: vec!["Rust".into(), "Tokio".into()],
            work_history: vec![WorkHistoryFact {
                title: "Senior Systems Engineer".into(),
                company: "CloudScale Systems".into(),
                date_range: "Jan 2023 -- Present".into(),
                location: Some("Remote".into()),
                highlights: vec![
                    "Architected high-throughput async processing engines handling 50,000 requests per second.".into(),
                ],
            }],
            ..Default::default()
        };

        // Technology vocabulary includes "Kafka" (e.g. from other projects in the DB)
        let extra_vocab = vec!["Kafka".into()];

        let mut content = GeneratedResumeContent {
            summary: "".into(),
            skills: GeneratedSkills { languages: vec![], frameworks_and_tools: vec![], core_concepts: vec![] },
            experience: vec![GeneratedExperienceItem {
                title: "Senior Systems Engineer".into(),
                company: "CloudScale Systems".into(),
                date_range: "Jan 2023 - Present".into(),
                location: Some("Remote".into()),
                bullets: vec![
                    "led a team of engineers using Kafka".into(),
                ],
            }],
            projects: vec![],
            achievements: vec![],
        };

        let report = validate_evidence(&mut content, &[], &master_facts, &extra_vocab);

        // Bullet must be rejected because Kafka is not in master entry highlights or master skills
        assert!(content.experience.is_empty(), "Experience entry with rejected Kafka bullet must be dropped");

        let rejected = report["rejected"].as_array().expect("rejected array");
        let kafka_rejection = rejected.iter().find(|r| {
            r["section"] == "experience" && r["term"] == "Kafka"
        });
        assert!(kafka_rejection.is_some(), "Kafka bullet must be rejected and logged to rejected[]");
        assert!(kafka_rejection.unwrap()["reason"].as_str().unwrap().contains("Kafka"));
    }

    #[test]
    fn summary_guard_rejects_unverified_metrics_and_absent_employers() {
        let master_facts = MasterFacts {
            candidate_name: "Test Candidate".into(),
            current_summary: Some("Experienced Systems Engineer with backend background.".into()),
            known_skills: vec!["Rust".into()],
            work_history: vec![WorkHistoryFact {
                title: "Senior Systems Engineer".into(),
                company: "CloudScale Systems".into(),
                date_range: "Jan 2023 -- Present".into(),
                location: Some("Remote".into()),
                highlights: vec!["Built engines".into()],
            }],
            ..Default::default()
        };

        // Case 1: Unverified number 10 in summary
        let mut content1 = GeneratedResumeContent {
            summary: "Systems engineer with 10 years experience.".into(),
            skills: GeneratedSkills { languages: vec![], frameworks_and_tools: vec![], core_concepts: vec![] },
            experience: vec![],
            projects: vec![],
            achievements: vec![],
        };
        let report1 = validate_evidence(&mut content1, &[], &master_facts, &[]);
        assert_eq!(content1.summary, "Experienced Systems Engineer with backend background.", "Summary must revert to master summary");
        let rej1 = report1["rejected"].as_array().unwrap();
        assert!(rej1.iter().any(|r| r["section"] == "summary" && r["item"] == "10"));

        // Case 2: Named employer absent from master
        let mut content2 = GeneratedResumeContent {
            summary: "Systems engineer at Google leading distributed teams.".into(),
            skills: GeneratedSkills { languages: vec![], frameworks_and_tools: vec![], core_concepts: vec![] },
            experience: vec![],
            projects: vec![],
            achievements: vec![],
        };
        let report2 = validate_evidence(&mut content2, &[], &master_facts, &[]);
        assert_eq!(content2.summary, "Experienced Systems Engineer with backend background.", "Summary must revert to master summary");
        let rej2 = report2["rejected"].as_array().unwrap();
        assert!(rej2.iter().any(|r| r["section"] == "summary" && r["item"] == "Google"));
    }
}

