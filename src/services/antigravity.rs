use crate::models::generation::{ContentGenerationInput, GeneratedResumeContent};
use crate::services::pty_runner::{run_agy_prompt_with_events, AgyOutcome, AgyRunResult};
use thiserror::Error;

pub const DEFAULT_CONTENT_GEN_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_REPAIR_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_PROBE_TIMEOUT_SECS: u64 = 30;

#[derive(Error, Debug)]
pub enum ContentGenError {
    #[error("AI attempted unauthorized tool '{0}', denied by permission system")]
    AiToolDenied(String),

    #[error("AI returned an empty response")]
    AiEmptyResponse,

    #[error("AI generation timed out after {0}s")]
    AiTimeout(u64),

    #[error("AI process hung past deadline")]
    AiHung,

    #[error("AI output was invalid JSON or failed schema validation: {0}")]
    AiInvalidJson(String),

    #[error("AI execution error: {0}")]
    AiError(String),
}

/// Builds the ContentGenerationAgent prompt matching Section 5 step 3 specifications.
pub fn build_generation_prompt(input: &ContentGenerationInput) -> String {
    let mut p = String::new();

    p.push_str(
        "You are an expert Resume Tailoring Agent (ContentGenerationAgent) in ResumeForge.\n",
    );
    p.push_str("Your task is to tailor a software engineer's resume content to maximally match the target job description while strictly adhering to factual evidence.\n\n");

    p.push_str("==============================\n");
    p.push_str("CRITICAL EXECUTION CONSTRAINTS:\n");
    p.push_str("1. NO TOOL USE: You are running in a strictly non-interactive, headless environment with NO tool access. DO NOT attempt to run any commands or invoke any tools. Any tool call will immediately trigger an automated denial error.\n");
    p.push_str("2. NO FABRICATION: Do not invent facts, unverified metrics, or technologies not present in the master facts or project evidence.\n");
    p.push_str("3. EVIDENCE GROUNDING: In the projects section, EVERY bullet point MUST cite one or more evidence IDs from the provided project evidence list.\n");
    p.push_str("4. JSON FORMAT ONLY: Output ONLY a single valid JSON object adhering to the schema below. No Markdown backticks (no ```json code fences), no introductory text, no conversational remarks.\n");
    p.push_str("5. CATEGORIZED SKILLS: The skills field MUST be an object with exactly three array fields: 'languages', 'frameworks_and_tools', and 'core_concepts'. DO NOT return skills as a flat array of strings.\n");
    p.push_str("==============================\n\n");

    // Candidate Master Facts
    p.push_str("## CANDIDATE MASTER FACTS\n");
    p.push_str(&format!("Candidate Name: {}\n", input.candidate_name));
    if let Some(contact) = &input.master_facts.contact {
        p.push_str(&format!("Contact: {}\n", contact));
    }
    if let Some(summary) = &input.master_facts.current_summary {
        p.push_str(&format!(
            "Summary: {}\n",
            summary
        ));
    }
    if !input.master_facts.known_skills.is_empty() {
        p.push_str(&format!(
            "Known Skills: {}\n",
            input.master_facts.known_skills.join(", ")
        ));
    }
    if !input.master_facts.education.is_empty() {
        p.push_str("Education:\n");
        for edu in &input.master_facts.education {
            let dates = edu.date_range.as_deref().unwrap_or("");
            p.push_str(&format!("- {} in {} ({})\n", edu.degree, edu.institution, dates));
        }
    }
    if !input.master_facts.work_history.is_empty() {
        p.push_str("Work History (user-asserted facts only):\n");
        for exp in &input.master_facts.work_history {
            p.push_str(&format!(
                "- {} at {} ({}):\n",
                exp.title,
                exp.company,
                exp.date_range,
            ));
            for h in &exp.highlights {
                p.push_str(&format!("  * {}\n", h));
            }
        }
    }
    p.push('\n');

    // Target Job Description Analysis
    p.push_str("## TARGET JOB DESCRIPTION ANALYSIS\n");
    if let Some(company) = &input.jd_analysis.target_company {
        p.push_str(&format!("Target Company: {}\n", company));
    }
    p.push_str(&format!("Target Role: {}\n", input.jd_analysis.target_role));
    p.push_str(&format!(
        "Core Required Skills: {}\n",
        input.jd_analysis.core_skills.join(", ")
    ));
    p.push_str(&format!(
        "Secondary Skills: {}\n",
        input.jd_analysis.secondary_skills.join(", ")
    ));
    if !input.jd_analysis.key_requirements.is_empty() {
        p.push_str("Key Requirements:\n");
        for req in &input.jd_analysis.key_requirements {
            p.push_str(&format!("- {}\n", req));
        }
    }
    p.push('\n');

    // Ranked Projects & Evidence
    p.push_str("## RANKED PROJECTS & FACTUAL EVIDENCE\n");
    for proj in &input.ranked_projects {
        p.push_str(&format!(
            "### Project [ID: {}]: {}\n",
            proj.project_id, proj.name
        ));
        p.push_str(&format!("Summary: {}\n", proj.summary));
        p.push_str(&format!("Technologies: {}\n", proj.technologies.join(", ")));
        p.push_str("Available Evidence Items:\n");
        for ev in &proj.evidence {
            p.push_str(&format!(
                "  - [Evidence ID: {}] {}: {}\n",
                ev.id, ev.source_file, ev.claim
            ));
        }
        p.push('\n');
    }

    if let Some(instructions) = &input.extra_instructions {
        p.push_str(&format!(
            "## USER CUSTOM INSTRUCTIONS\n{}\n\n",
            instructions
        ));
    }

    // Required Output JSON Schema
    p.push_str("## REQUIRED JSON OUTPUT SCHEMA (STRICT)\n");
    p.push_str(
        r#"{
  "summary": "Compelling 2-3 sentence professional summary tailored to the target role.",
  "skills": {
    "languages": ["Language1", "Language2"],
    "frameworks_and_tools": ["Framework1", "Tool1"],
    "core_concepts": ["Concept1", "Concept2"]
  },
  "experience": [
    {
      "title": "Job Title",
      "company": "Company Name",
      "date_range": "Jan 2022 -- Present",
      "location": "Remote",
      "bullets": [
        "Strong action-verb bullet demonstrating high-impact achievement.",
        "Second bullet highlighting technical depth and measurable results."
      ]
    }
  ],
  "projects": [
    {
      "project_id": 1,
      "name": "Project Name",
      "bullets": [
        {
          "text": "Engineered high-performance module achieving specific results.",
          "evidence_ids": [101, 102]
        }
      ]
    }
  ],
  "achievements": [
    "Optional notable award, open-source milestone, or publication"
  ]
}"#,
    );
    p.push_str("\n\nReturn ONLY the JSON object now. Note: skills must be a structured object with languages, frameworks_and_tools, core_concepts arrays.");

    p
}

/// Semantic schema validator to enforce hard structural constraints per Section 5 step 3
pub fn validate_generated_content(parsed: &GeneratedResumeContent) -> Result<(), anyhow::Error> {
    if parsed.summary.trim().is_empty() {
        anyhow::bail!("Generated summary cannot be empty");
    }
    if parsed.skills.languages.is_empty() {
        anyhow::bail!("skills.languages must contain at least one language; skills must be an object with {{ languages, frameworks_and_tools, core_concepts }}");
    }
    if parsed.skills.frameworks_and_tools.is_empty() {
        anyhow::bail!("skills.frameworks_and_tools must contain at least one framework/tool");
    }
    if parsed.skills.core_concepts.is_empty() {
        anyhow::bail!("skills.core_concepts must contain at least one core concept");
    }
    if parsed.projects.is_empty() {
        anyhow::bail!("Generated projects list cannot be empty");
    }
    for proj in &parsed.projects {
        if proj.bullets.is_empty() {
            anyhow::bail!(
                "Project '{}' (id: {}) must have at least one bullet",
                proj.name,
                proj.project_id
            );
        }
        for (i, bullet) in proj.bullets.iter().enumerate() {
            if bullet.text.trim().is_empty() {
                anyhow::bail!(
                    "Bullet #{} in project '{}' has empty text",
                    i + 1,
                    proj.name
                );
            }
            if bullet.evidence_ids.is_empty() {
                anyhow::bail!(
                    "Bullet #{} in project '{}' must cite at least one evidence_id",
                    i + 1,
                    proj.name
                );
            }
        }
    }
    Ok(())
}

/// Strips markdown fences or outer conversational text, parses into GeneratedResumeContent,
/// and performs semantic schema validation.
pub fn parse_generation_response(raw_text: &str) -> Result<GeneratedResumeContent, anyhow::Error> {
    let cleaned = extract_clean_json(raw_text);
    let parsed: GeneratedResumeContent = serde_json::from_str(cleaned).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse JSON into expected schema: {}. Raw: {}",
            e,
            cleaned
        )
    })?;

    validate_generated_content(&parsed)?;

    Ok(parsed)
}

/// Helper to strip code fences and extract valid JSON substring.
pub fn extract_clean_json(raw: &str) -> &str {
    let trimmed = raw.trim();

    if let Some(stripped) = trimmed.strip_prefix("```json") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim();
        }
    } else if let Some(stripped) = trimmed.strip_prefix("```") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim();
        }
    }

    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end >= start {
                return &trimmed[start..=end];
            }
        }
    }

    trimmed
}

/// Orchestrates calling Antigravity CLI, mapping failure classifications, and parsing output.
/// If JSON is invalid or fails schema validation, performs one repair re-prompt per Section 6 point 4.
pub async fn run_content_generation(
    input: &ContentGenerationInput,
    timeout_secs: u64,
) -> Result<(GeneratedResumeContent, AgyRunResult), ContentGenError> {
    run_content_generation_streamed(input, timeout_secs, None).await
}

pub fn get_content_gen_timeout_secs(timeout_secs: u64) -> u64 {
    if let Ok(val) = std::env::var("RESUMEFORGE_CONTENT_GEN_TIMEOUT_SECS") {
        val.parse::<u64>().unwrap_or(DEFAULT_CONTENT_GEN_TIMEOUT_SECS)
    } else if timeout_secs == 0 {
        DEFAULT_CONTENT_GEN_TIMEOUT_SECS
    } else {
        timeout_secs
    }
}

pub async fn run_content_generation_streamed(
    input: &ContentGenerationInput,
    timeout_secs: u64,
    events: Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>,
) -> Result<(GeneratedResumeContent, AgyRunResult), ContentGenError> {
    let actual_timeout = get_content_gen_timeout_secs(timeout_secs);

    let prompt = build_generation_prompt(input);

    let run_res = run_agy_prompt_with_events(&prompt, actual_timeout, events.clone())
        .await
        .map_err(|e| ContentGenError::AiError(e.to_string()))?;

    // Check failure classifications from Section 6
    match &run_res.status {
        AgyOutcome::AiToolDenied => {
            let actions = run_res.denied_actions.join(", ");
            return Err(ContentGenError::AiToolDenied(actions));
        }
        AgyOutcome::AiEmptyResponse => {
            return Err(ContentGenError::AiEmptyResponse);
        }
        AgyOutcome::AiTimeout => {
            return Err(ContentGenError::AiTimeout(actual_timeout));
        }
        AgyOutcome::AiHung => {
            return Err(ContentGenError::AiHung);
        }
        AgyOutcome::AiError => {
            let detail = run_res
                .error_detail
                .clone()
                .unwrap_or_else(|| "Subprocess error".to_string());
            return Err(ContentGenError::AiError(detail));
        }
        _ => {}
    }

    // Try parsing JSON and validating schema
    match parse_generation_response(&run_res.response) {
        Ok(parsed) => Ok((parsed, run_res)),
        Err(first_err) => {
            // Section 6 point 4: Repair re-prompt once on invalid JSON / schema mismatch
            let repair_prompt = format!(
                "Original task and evidence (keep all factual constraints):\n{}\n\nYour previous response did not match the required schema: {}\n\n\
                Return ONLY valid JSON matching the exact required schema (skills MUST be an object with languages, frameworks_and_tools, core_concepts arrays):\n\
                {{\n\
                  \"summary\": \"...\",\n\
                  \"skills\": {{\n\
                    \"languages\": [\"...\"],\n\
                    \"frameworks_and_tools\": [\"...\"],\n\
                    \"core_concepts\": [\"...\"]\n\
                  }},\n\
                  \"experience\": [ ... ],\n\
                  \"projects\": [ ... ],\n\
                  \"achievements\": [ ... ]\n\
                }}\n\
                No markdown code fences, no introductory or conversational remarks. Output raw JSON only.\n\n\
                Previous output was:\n{}",
                prompt,
                first_err,
                run_res.response
            );

            let repair_res =
                run_agy_prompt_with_events(&repair_prompt, DEFAULT_REPAIR_TIMEOUT_SECS, events)
                    .await
                    .map_err(|e| ContentGenError::AiError(e.to_string()))?;

            match repair_res.status {
                AgyOutcome::AiToolDenied => {
                    return Err(ContentGenError::AiToolDenied(
                        repair_res.denied_actions.join(", "),
                    ))
                }
                AgyOutcome::AiTimeout => {
                    return Err(ContentGenError::AiTimeout(DEFAULT_REPAIR_TIMEOUT_SECS))
                }
                AgyOutcome::AiHung => return Err(ContentGenError::AiHung),
                AgyOutcome::AiEmptyResponse => return Err(ContentGenError::AiEmptyResponse),
                AgyOutcome::AiError => {
                    return Err(ContentGenError::AiError(
                        repair_res
                            .error_detail
                            .unwrap_or_else(|| "Repair call failed".into()),
                    ))
                }
                _ => {}
            }

            match parse_generation_response(&repair_res.response) {
                Ok(repaired) => Ok((repaired, repair_res)),
                Err(second_err) => Err(ContentGenError::AiInvalidJson(format!(
                    "Initial parse failed: {}; Repair attempt failed: {}",
                    first_err, second_err
                ))),
            }
        }
    }
}
