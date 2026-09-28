use crate::models::generation::GeneratedResumeContent;
use crate::services::event_bus::{self, EventBus};
use crate::services::latex::{compile_latex_content, render_latex_template};
use crate::services::pdf;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DroppedBulletInfo {
    pub section: String,
    pub item_name: String,
    pub bullet_text: String,
    pub relevance_score: usize,
    pub attempt: usize,
}

#[derive(Clone)]
pub struct RenderLoopContext<'a> {
    pub pool: &'a SqlitePool,
    pub bus: &'a EventBus,
    pub resume_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalResumeStatus {
    Ready,
    ReadySparse,
    PageLimitError,
    LatexError,
}

#[derive(Debug, Clone)]
pub enum RenderLoopOutcome {
    Success {
        attempts: usize,
        dropped_bullets: Vec<DroppedBulletInfo>,
        final_content: GeneratedResumeContent,
        final_latex: String,
        pdf_path: Option<PathBuf>,
        page_count: usize,
        content_density_pct: f32,
        is_sparse: bool,
        status: FinalResumeStatus,
        compact_mode_applied: bool,
    },
    PageLimitError {
        attempts: usize,
        dropped_bullets: Vec<DroppedBulletInfo>,
        final_content: GeneratedResumeContent,
        last_failing_latex: String,
        last_failing_pdf: Option<PathBuf>,
        page_count: usize,
        compact_mode_applied: bool,
    },
    CompileError {
        attempt: usize,
        error_message: String,
        line_number: Option<usize>,
    },
}

/// Computes the keyword relevance score of a bullet against target JD skills/keywords.
pub fn score_bullet_relevance(bullet_text: &str, keywords: &[String]) -> usize {
    let lower_text = bullet_text.to_lowercase();
    let mut score = 0;

    for kw in keywords {
        let trimmed = kw.trim().to_lowercase();
        if trimmed.is_empty() {
            continue;
        }
        if lower_text.contains(&trimmed) {
            score += 1;
        }
    }
    score
}

/// Identifies and drops the weakest-relevance bullet from the generated resume content.
/// Constraints & Tie-Breaking Rules:
/// 1. Singleton Protection: Never drops a bullet if it is the only remaining bullet for that item (avoids empty sections).
/// 2. Lowest Relevance First: Bullets with the lowest keyword relevance score are dropped first.
/// 3. Experience vs. Project Tie-Breaker: When relevance scores tie or near-tie between an
///    evidence-grounded project bullet and a self-asserted experience bullet, experience bullets
///    are dropped FIRST because project bullets carry repo-verified factual evidence backing.
/// 4. Intra-Section Tie-Breaker: Drops from entries with higher remaining bullet count first, then later entries.
pub fn drop_weakest_bullet(
    content: &mut GeneratedResumeContent,
    jd_keywords: &[String],
    attempt: usize,
) -> Option<DroppedBulletInfo> {
    // 1. Find best eligible candidate in experience
    let mut best_exp: Option<(usize, usize, usize, usize)> = None; // (exp_idx, bullet_idx, score, total_bullets)
    for (exp_idx, exp) in content.experience.iter().enumerate() {
        let total = exp.bullets.len();
        if total <= 1 {
            continue;
        }
        for (bullet_idx, bullet) in exp.bullets.iter().enumerate() {
            let score = score_bullet_relevance(bullet, jd_keywords);
            match &best_exp {
                None => best_exp = Some((exp_idx, bullet_idx, score, total)),
                Some((best_exp_idx, _, best_score, best_total)) => {
                    if score < *best_score
                        || (score == *best_score && total > *best_total)
                        || (score == *best_score
                            && total == *best_total
                            && exp_idx >= *best_exp_idx)
                    {
                        best_exp = Some((exp_idx, bullet_idx, score, total));
                    }
                }
            }
        }
    }

    // 2. Find best eligible candidate in projects
    let mut best_proj: Option<(usize, usize, usize, usize)> = None; // (proj_idx, bullet_idx, score, total_bullets)
    for (proj_idx, proj) in content.projects.iter().enumerate() {
        let total = proj.bullets.len();
        if total <= 1 {
            continue;
        }
        for (bullet_idx, bullet) in proj.bullets.iter().enumerate() {
            let score = score_bullet_relevance(&bullet.text, jd_keywords);
            match &best_proj {
                None => best_proj = Some((proj_idx, bullet_idx, score, total)),
                Some((best_proj_idx, _, best_score, best_total)) => {
                    if score < *best_score
                        || (score == *best_score && total > *best_total)
                        || (score == *best_score
                            && total == *best_total
                            && proj_idx >= *best_proj_idx)
                    {
                        best_proj = Some((proj_idx, bullet_idx, score, total));
                    }
                }
            }
        }
    }

    // 3. Compare experience candidate vs project candidate:
    // TIE-BREAK RULE: Experience bullets are dropped before project bullets on equal or near-equal scores
    // because project bullets carry repo-verified factual evidence backing.
    let choose_exp = match (best_exp, best_proj) {
        (Some((_, _, exp_score, _)), Some((_, _, proj_score, _))) => exp_score <= proj_score,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => return None,
    };

    if choose_exp {
        let (exp_idx, bullet_idx, score, _) = best_exp.unwrap();
        let item_name = content.experience[exp_idx].company.clone();
        let bullet_text = content.experience[exp_idx].bullets.remove(bullet_idx);
        Some(DroppedBulletInfo {
            section: "experience".to_string(),
            item_name,
            bullet_text,
            relevance_score: score,
            attempt,
        })
    } else {
        let (proj_idx, bullet_idx, score, _) = best_proj.unwrap();
        let item_name = content.projects[proj_idx].name.clone();
        let bullet = content.projects[proj_idx].bullets.remove(bullet_idx);
        Some(DroppedBulletInfo {
            section: "projects".to_string(),
            item_name,
            bullet_text: bullet.text,
            relevance_score: score,
            attempt,
        })
    }
}

/// Full RenderCompileLoopAgent orchestration (up to 4 iterations).
/// Attempts 1..=3 (Stage A): content-shortening loop dropping weakest-relevance bullets.
/// Attempt 4 (Stage B): \tighten macro compaction if template defines it.
/// Also calculates content density percentage and assigns status:
/// - 1 page, density >= 60% -> FinalResumeStatus::Ready
/// - 1 page, density < 60% -> FinalResumeStatus::ReadySparse
/// If still >1 page after attempt 4 -> RenderLoopOutcome::PageLimitError
pub async fn run_render_compile_loop(
    template: &str,
    initial_content: &GeneratedResumeContent,
    jd_keywords: &[String],
    temp_base_dir: &Path,
    target_pdf_dest: Option<&Path>,
    ctx: Option<&RenderLoopContext<'_>>,
) -> Result<RenderLoopOutcome, anyhow::Error> {
    let mut current_content = initial_content.clone();
    let mut dropped_bullets = Vec::new();
    let max_shortening_attempts = 3;
    let template_has_tighten = template.contains("\\tighten");

    // Stage A: Attempts 1..=3
    for attempt in 1..=max_shortening_attempts {
        if let Some(c) = ctx {
            sqlx::query("UPDATE resumes SET status = 'rendering_latex' WHERE id = ?")
                .bind(c.resume_id)
                .execute(c.pool)
                .await?;
        }
        if let Some(c) = ctx {
            let _ = event_bus::emit_event(
                c.pool,
                c.bus,
                c.resume_id,
                "render_compile",
                "command_output",
                serde_json::json!({
                    "chunk": format!("Attempt {}/4: rendering LaTeX...\n", attempt),
                    "attempt": attempt,
                    "stage": "rendering",
                }),
            )
            .await;
        }

        let rendered_latex = render_latex_template(template, &current_content, false)?;

        let temp_run_dir = temp_base_dir.join(format!("attempt_{}", attempt));
        tokio::fs::create_dir_all(&temp_run_dir).await?;
        let attempt_pdf = temp_run_dir.join("resume.pdf");

        if let Some(c) = ctx {
            sqlx::query("UPDATE resumes SET status = 'compiling_pdf' WHERE id = ?")
                .bind(c.resume_id)
                .execute(c.pool)
                .await?;
        }
        let compile_res =
            compile_latex_content(&rendered_latex, &temp_run_dir, Some(&attempt_pdf)).await?;

        if !compile_res.success {
            let error_msg = compile_res
                .error_message
                .unwrap_or_else(|| "LaTeX compilation failed".to_string());
            if let Some(c) = ctx {
                let _ = event_bus::emit_event(
                    c.pool,
                    c.bus,
                    c.resume_id,
                    "render_compile",
                    "node_error",
                    serde_json::json!({
                        "error": "latex_error",
                        "attempt": attempt,
                        "detail": error_msg,
                        "line_number": compile_res.line_number,
                    }),
                )
                .await;
            }
            let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;
            return Ok(RenderLoopOutcome::CompileError {
                attempt,
                error_message: error_msg,
                line_number: compile_res.line_number,
            });
        }

        if let Some(c) = ctx {
            sqlx::query("UPDATE resumes SET status = 'checking_pages' WHERE id = ?")
                .bind(c.resume_id)
                .execute(c.pool)
                .await?;
        }
        let page_count = pdf::get_page_count(&attempt_pdf)?;

        if page_count == 1 {
            let density = pdf::estimate_content_density(&attempt_pdf)?;
            let is_sparse = density < 60.0;
            let status = if is_sparse {
                FinalResumeStatus::ReadySparse
            } else {
                FinalResumeStatus::Ready
            };

            if let Some(dest) = target_pdf_dest {
                if let Some(parent) = dest.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let _ = tokio::fs::copy(&attempt_pdf, dest).await;
            }

            if let Some(c) = ctx {
                let status_str = if is_sparse { "ready_sparse" } else { "ready" };
                let _ = event_bus::emit_event(
                    c.pool,
                    c.bus,
                    c.resume_id,
                    "render_compile",
                    "command_output",
                    serde_json::json!({
                        "chunk": format!(
                            "Attempt {}: 1 page (density {:.1}%), page limit satisfied (status: {})\n",
                            attempt, density, status_str
                        ),
                        "attempt": attempt,
                        "page_count": 1,
                        "density": density,
                        "is_sparse": is_sparse,
                    }),
                )
                .await;

                let _ = event_bus::emit_event(
                    c.pool,
                    c.bus,
                    c.resume_id,
                    "render_compile",
                    "node_done",
                    serde_json::json!({
                        "status": status_str,
                        "attempts_used": attempt,
                        "page_count": 1,
                        "density": density,
                        "is_sparse": is_sparse,
                        "compact_mode_applied": false,
                        "dropped_bullets_count": dropped_bullets.len(),
                    }),
                )
                .await;
            }

            let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;

            return Ok(RenderLoopOutcome::Success {
                attempts: attempt,
                dropped_bullets,
                final_content: current_content,
                final_latex: rendered_latex,
                pdf_path: target_pdf_dest.map(|p| p.to_path_buf()),
                page_count: 1,
                content_density_pct: density,
                is_sparse,
                status,
                compact_mode_applied: false,
            });
        }

        // >1 page: emit over-limit status
        if let Some(c) = ctx {
            let _ = event_bus::emit_event(
                c.pool,
                c.bus,
                c.resume_id,
                "render_compile",
                "command_output",
                serde_json::json!({
                    "chunk": format!("Attempt {}: {} pages, over 1-page limit\n", attempt, page_count),
                    "attempt": attempt,
                    "page_count": page_count,
                }),
            )
            .await;
        }

        if attempt < max_shortening_attempts {
            if let Some(dropped) = drop_weakest_bullet(&mut current_content, jd_keywords, attempt) {
                if let Some(c) = ctx {
                    let _ = event_bus::emit_event(
                        c.pool,
                        c.bus,
                        c.resume_id,
                        "render_compile",
                        "artifact_produced",
                        serde_json::json!({
                            "kind": "dropped_bullet",
                            "attempt": attempt,
                            "section": dropped.section,
                            "item": dropped.item_name,
                            "bullet": dropped.bullet_text,
                            "relevance_score": dropped.relevance_score,
                            "reason": format!(
                                "Dropped weakest-relevance {} bullet (score {}) to fit 1-page limit",
                                dropped.section, dropped.relevance_score
                            ),
                        }),
                    )
                    .await;
                }
                dropped_bullets.push(dropped);
            } else {
                // Cannot drop any more bullets (all items are singletons) -> break early to Stage B
                let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;
                break;
            }
        }

        let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;
    }

    // Stage B: Attempt 4 with \tighten macro
    if template_has_tighten {
        if let Some(c) = ctx {
            sqlx::query("UPDATE resumes SET status = 'rendering_latex' WHERE id = ?")
                .bind(c.resume_id)
                .execute(c.pool)
                .await?;
        }
        if let Some(c) = ctx {
            let _ = event_bus::emit_event(
                c.pool,
                c.bus,
                c.resume_id,
                "render_compile",
                "command_output",
                serde_json::json!({
                    "chunk": "Attempt 4/4: applying Stage B \\tighten macro compaction...\n",
                    "attempt": 4,
                    "stage": "tightening",
                    "compact_mode": true,
                }),
            )
            .await;
        }

        let rendered_latex = render_latex_template(template, &current_content, true)?;

        let temp_run_dir = temp_base_dir.join("attempt_4_tighten");
        tokio::fs::create_dir_all(&temp_run_dir).await?;
        let attempt_pdf = temp_run_dir.join("resume.pdf");

        if let Some(c) = ctx {
            sqlx::query("UPDATE resumes SET status = 'compiling_pdf' WHERE id = ?")
                .bind(c.resume_id)
                .execute(c.pool)
                .await?;
        }
        let compile_res =
            compile_latex_content(&rendered_latex, &temp_run_dir, Some(&attempt_pdf)).await?;

        if !compile_res.success {
            let error_msg = compile_res
                .error_message
                .unwrap_or_else(|| "LaTeX compilation failed under \\tighten".to_string());
            if let Some(c) = ctx {
                let _ = event_bus::emit_event(
                    c.pool,
                    c.bus,
                    c.resume_id,
                    "render_compile",
                    "node_error",
                    serde_json::json!({
                        "error": "latex_error",
                        "attempt": 4,
                        "detail": error_msg,
                        "line_number": compile_res.line_number,
                    }),
                )
                .await;
            }
            let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;
            return Ok(RenderLoopOutcome::CompileError {
                attempt: 4,
                error_message: error_msg,
                line_number: compile_res.line_number,
            });
        }

        if let Some(c) = ctx {
            sqlx::query("UPDATE resumes SET status = 'checking_pages' WHERE id = ?")
                .bind(c.resume_id)
                .execute(c.pool)
                .await?;
        }
        let page_count = pdf::get_page_count(&attempt_pdf)?;

        if page_count == 1 {
            let density = pdf::estimate_content_density(&attempt_pdf)?;
            let is_sparse = density < 60.0;
            let status = if is_sparse {
                FinalResumeStatus::ReadySparse
            } else {
                FinalResumeStatus::Ready
            };

            if let Some(dest) = target_pdf_dest {
                if let Some(parent) = dest.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let _ = tokio::fs::copy(&attempt_pdf, dest).await;
            }

            if let Some(c) = ctx {
                let status_str = if is_sparse { "ready_sparse" } else { "ready" };
                let _ = event_bus::emit_event(
                    c.pool,
                    c.bus,
                    c.resume_id,
                    "render_compile",
                    "command_output",
                    serde_json::json!({
                        "chunk": format!(
                            "Attempt 4: 1 page (density {:.1}%), \\tighten compaction successful (status: {})\n",
                            density, status_str
                        ),
                        "attempt": 4,
                        "page_count": 1,
                        "density": density,
                        "is_sparse": is_sparse,
                        "compact_mode": true,
                    }),
                )
                .await;

                let _ = event_bus::emit_event(
                    c.pool,
                    c.bus,
                    c.resume_id,
                    "render_compile",
                    "node_done",
                    serde_json::json!({
                        "status": status_str,
                        "attempts_used": 4,
                        "page_count": 1,
                        "density": density,
                        "is_sparse": is_sparse,
                        "compact_mode_applied": true,
                        "dropped_bullets_count": dropped_bullets.len(),
                    }),
                )
                .await;
            }

            let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;

            return Ok(RenderLoopOutcome::Success {
                attempts: 4,
                dropped_bullets,
                final_content: current_content,
                final_latex: rendered_latex,
                pdf_path: target_pdf_dest.map(|p| p.to_path_buf()),
                page_count: 1,
                content_density_pct: density,
                is_sparse,
                status,
                compact_mode_applied: true,
            });
        }

        // >1 page even after \tighten
        if let Some(c) = ctx {
            let _ = event_bus::emit_event(
                c.pool,
                c.bus,
                c.resume_id,
                "render_compile",
                "node_error",
                serde_json::json!({
                    "error": "page_limit_error",
                    "attempts_used": 4,
                    "page_count": page_count,
                    "compact_mode_applied": true,
                    "detail": format!(
                        "Page limit exceeded: document is {} pages after 3 shortening attempts and \\tighten compaction",
                        page_count
                    ),
                }),
            )
            .await;
        }

        let _ = tokio::fs::remove_dir_all(&temp_run_dir).await;

        Ok(RenderLoopOutcome::PageLimitError {
            attempts: 4,
            dropped_bullets,
            final_content: current_content,
            last_failing_latex: rendered_latex,
            last_failing_pdf: Some(attempt_pdf),
            page_count,
            compact_mode_applied: true,
        })
    } else {
        // No \tighten available in template -> terminal PageLimitError after 3 attempts
        if let Some(c) = ctx {
            let _ = event_bus::emit_event(
                c.pool,
                c.bus,
                c.resume_id,
                "render_compile",
                "node_error",
                serde_json::json!({
                    "error": "page_limit_error",
                    "attempts_used": 3,
                    "compact_mode_applied": false,
                    "detail": "Document exceeds 1 page after 3 shortening attempts and template does not define \\tighten",
                }),
            )
            .await;
        }

        let rendered_latex = render_latex_template(template, &current_content, false)?;
        Ok(RenderLoopOutcome::PageLimitError {
            attempts: 3,
            dropped_bullets,
            final_content: current_content,
            last_failing_latex: rendered_latex,
            last_failing_pdf: None,
            page_count: 2,
            compact_mode_applied: false,
        })
    }
}

/// Convenience alias maintaining compatibility with Step 13 callers.
pub async fn run_stage_a_render_loop(
    template: &str,
    initial_content: &GeneratedResumeContent,
    jd_keywords: &[String],
    temp_base_dir: &Path,
    target_pdf_dest: Option<&Path>,
    ctx: Option<&RenderLoopContext<'_>>,
) -> Result<RenderLoopOutcome, anyhow::Error> {
    run_render_compile_loop(
        template,
        initial_content,
        jd_keywords,
        temp_base_dir,
        target_pdf_dest,
        ctx,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::generation::{
        GeneratedExperienceItem, GeneratedProjectBullet, GeneratedProjectItem, GeneratedSkills,
    };

    #[test]
    fn test_score_bullet_relevance() {
        let kws = vec!["Rust".to_string(), "Tokio".to_string(), "WAL".to_string()];
        let bullet1 = "Engineered crash-safe WAL transaction commit loop in async Rust.";
        let bullet2 = "Organized team standups and managed JIRA ticket sprint boards.";

        assert_eq!(score_bullet_relevance(bullet1, &kws), 2); // WAL, Rust
        assert_eq!(score_bullet_relevance(bullet2, &kws), 0);
    }

    #[test]
    fn test_drop_weakest_bullet_preserves_singletons() {
        let kws = vec!["Rust".to_string(), "Tokio".to_string()];
        let mut content = GeneratedResumeContent {
            summary: "Summary".to_string(),
            skills: GeneratedSkills {
                languages: vec!["Rust".to_string()],
                frameworks_and_tools: vec!["Tokio".to_string()],
                core_concepts: vec!["Concurrency".to_string()],
            },
            experience: vec![GeneratedExperienceItem {
                title: "Engineer".to_string(),
                company: "Company".to_string(),
                date_range: "2023".to_string(),
                location: None,
                bullets: vec![
                    "High relevance bullet with Rust and Tokio".to_string(),
                    "Low relevance bullet with generic management".to_string(),
                ],
            }],
            projects: vec![GeneratedProjectItem {
                project_id: 1,
                name: "Proj1".to_string(),
                bullets: vec![GeneratedProjectBullet {
                    text: "Only bullet in project".to_string(),
                    evidence_ids: vec![1],
                }],
            }],
            achievements: vec![],
        };

        let dropped = drop_weakest_bullet(&mut content, &kws, 1).unwrap();
        assert_eq!(dropped.section, "experience");
        assert_eq!(dropped.relevance_score, 0);
        assert_eq!(content.experience[0].bullets.len(), 1);
        assert_eq!(content.projects[0].bullets.len(), 1);

        let dropped_again = drop_weakest_bullet(&mut content, &kws, 2);
        assert!(dropped_again.is_none());
        assert_eq!(content.experience[0].bullets.len(), 1);
        assert_eq!(content.projects[0].bullets.len(), 1);
    }

    #[test]
    fn test_drop_weakest_bullet_prefers_experience_over_project_on_tie() {
        let kws = vec!["Rust".to_string()];
        let mut content = GeneratedResumeContent {
            summary: "Summary".to_string(),
            skills: GeneratedSkills {
                languages: vec!["Rust".to_string()],
                frameworks_and_tools: vec![],
                core_concepts: vec![],
            },
            experience: vec![GeneratedExperienceItem {
                title: "Engineer".to_string(),
                company: "TechCorp".to_string(),
                date_range: "2023".to_string(),
                location: None,
                bullets: vec![
                    "Experience bullet A (unrelated task)".to_string(), // score 0
                    "Experience bullet B (unrelated task)".to_string(), // score 0
                ],
            }],
            projects: vec![GeneratedProjectItem {
                project_id: 101,
                name: "ProjectAlpha".to_string(),
                bullets: vec![
                    GeneratedProjectBullet {
                        text: "Project bullet A (unrelated feature)".to_string(), // score 0
                        evidence_ids: vec![1],
                    },
                    GeneratedProjectBullet {
                        text: "Project bullet B (unrelated feature)".to_string(), // score 0
                        evidence_ids: vec![2],
                    },
                ],
            }],
            achievements: vec![],
        };

        // When both experience and project have eligible score 0 bullets, experience MUST be dropped first!
        let dropped = drop_weakest_bullet(&mut content, &kws, 1).unwrap();
        assert_eq!(
            dropped.section, "experience",
            "Tie-break rule requires experience bullet to be dropped before project bullet"
        );
        assert_eq!(dropped.relevance_score, 0);
    }
}
