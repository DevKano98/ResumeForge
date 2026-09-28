use crate::db::projects;
use crate::models::generation::{JdAnalysis, ProjectEvidenceItem, RankedProjectInput};
use crate::models::github::GitHubRepository;
use crate::services::secret_scanner::{contains_secret_pattern, is_ignored_path};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SOURCE_FILES: &[&str] = &[
    "README.md",
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "Dockerfile",
];
const TECHNOLOGIES: &[&str] = &[
    "Rust",
    "Tokio",
    "SQLite",
    "Python",
    "JavaScript",
    "TypeScript",
    "React",
    "Node.js",
    "Go",
    "Docker",
    "Kubernetes",
    "PostgreSQL",
    "AWS",
    "Azure",
    "Redis",
    "GraphQL",
    "REST",
    "Java",
    "C++",
    "FastAPI",
    "Axum",
];

pub fn analyse_job(job: &str, company: Option<&str>, role: Option<&str>) -> JdAnalysis {
    let lower = job.to_ascii_lowercase();
    let skills: Vec<String> = TECHNOLOGIES
        .iter()
        .filter(|tech| lower.contains(&tech.to_ascii_lowercase()))
        .map(|tech| (*tech).to_string())
        .collect();
    let first_line = job
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Software Engineer");
    let inferred_role = if first_line.len() <= 80 {
        first_line
    } else {
        "Software Engineer"
    };
    let requirements = job
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('-') || line.starts_with('•'))
        .take(12)
        .map(|line| line.trim_start_matches(['-', '•', ' ']).to_string())
        .collect();
    JdAnalysis {
        target_company: company.map(str::to_string),
        target_role: role
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(inferred_role)
            .to_string(),
        core_skills: skills.iter().take(8).cloned().collect(),
        secondary_skills: skills.iter().skip(8).cloned().collect(),
        key_requirements: requirements,
    }
}

pub async fn index_project(
    pool: &SqlitePool,
    repository_id: i64,
    repo: &GitHubRepository,
    clone_path: &Path,
    sha: &str,
    excluded: &HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let mut evidence: Vec<(String, String, i64)> = Vec::new();
    let mut technologies = Vec::<String>::new();
    let mut features = Vec::<String>::new();
    let clone_absolute = tokio::fs::canonicalize(clone_path).await?;
    for name in SOURCE_FILES {
        let relative = Path::new(name);
        if is_ignored_path(relative) || excluded.contains(relative) {
            continue;
        }
        let file = clone_path.join(relative);
        let absolute = match tokio::fs::canonicalize(&file).await {
            Ok(path) => path,
            Err(_) => continue,
        };
        if !absolute.starts_with(&clone_absolute) {
            continue;
        }
        let metadata = match tokio::fs::metadata(&file).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !metadata.is_file() || metadata.len() > 1_000_000 {
            continue;
        }
        let content = tokio::fs::read_to_string(&file).await?;
        if contains_secret_pattern(&content) {
            continue;
        }
        let lower = content.to_ascii_lowercase();
        for tech in TECHNOLOGIES {
            if lower.contains(&tech.to_ascii_lowercase()) && !technologies.iter().any(|v| v == tech)
            {
                technologies.push((*tech).to_string());
            }
        }
        for (line_number, line) in content.lines().enumerate() {
            let clean = line.trim().trim_start_matches(['#', '-', '*', ' ']).trim();
            if clean.len() < 12 || clean.len() > 240 || clean.starts_with('!') {
                continue;
            }
            let useful = if *name == "README.md" {
                line.trim_start().starts_with(['#', '-', '*'])
            } else {
                [
                    "dependencies",
                    "description",
                    "name",
                    "version",
                    "FROM",
                    "RUN",
                ]
                .iter()
                .any(|word| clean.starts_with(word))
            };
            if useful && evidence.len() < 120 {
                evidence.push((
                    clean.to_string(),
                    (*name).to_string(),
                    line_number as i64 + 1,
                ));
                if *name == "README.md" && features.len() < 24 {
                    features.push(clean.to_string());
                }
            }
        }
    }
    let confidence = if evidence.is_empty() { 0.0 } else { 0.7 };
    let mut tx = pool.begin().await?;
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM projects WHERE repository_id=? ORDER BY id LIMIT 1")
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await?;
    let project_id = if let Some(id) = existing {
        sqlx::query("UPDATE projects SET name=?, summary=?, technologies_json=?, features_json=?, confidence=?, updated_at=datetime('now') WHERE id=?")
            .bind(&repo.name).bind(&repo.description).bind(serde_json::to_string(&technologies)?)
            .bind(serde_json::to_string(&features)?).bind(confidence).bind(id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM project_evidence WHERE project_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        id
    } else {
        sqlx::query_scalar("INSERT INTO projects (repository_id, name, summary, technologies_json, features_json, confidence) VALUES (?, ?, ?, ?, ?, ?) RETURNING id")
            .bind(repository_id).bind(&repo.name).bind(&repo.description)
            .bind(serde_json::to_string(&technologies)?).bind(serde_json::to_string(&features)?)
            .bind(confidence).fetch_one(&mut *tx).await?
    };
    for (claim, file, line) in evidence {
        sqlx::query("INSERT INTO project_evidence (project_id, claim, source_file, line_start, line_end, commit_sha, confidence) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(project_id).bind(claim).bind(file).bind(line).bind(line).bind(sha).bind(confidence)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct RankedProject {
    pub score: f64,
    pub input: RankedProjectInput,
}

pub async fn rank_projects(
    pool: &SqlitePool,
    jd: &JdAnalysis,
) -> anyhow::Result<Vec<RankedProject>> {
    let mut ranked = Vec::new();
    let keywords: Vec<String> = jd
        .core_skills
        .iter()
        .chain(jd.secondary_skills.iter())
        .map(|s| s.to_ascii_lowercase())
        .collect();
    for project in projects::list(pool).await? {
        let evidence = projects::evidence_for_project(pool, project.id).await?;
        if evidence.is_empty() {
            continue;
        }
        let technologies: Vec<String> =
            serde_json::from_str(&project.technologies_json).unwrap_or_default();
        let haystack = format!(
            "{} {} {}",
            project.name,
            project.summary.as_deref().unwrap_or_default(),
            project.features_json
        )
        .to_ascii_lowercase();
        let tech_lower: Vec<String> = technologies
            .iter()
            .map(|v| v.to_ascii_lowercase())
            .collect();
        let score = keywords
            .iter()
            .map(|keyword| {
                if tech_lower.iter().any(|v| v == keyword) {
                    3.0
                } else if haystack.contains(keyword) {
                    1.0
                } else {
                    0.0
                }
            })
            .sum();
        ranked.push(RankedProject {
            score,
            input: RankedProjectInput {
                project_id: project.id,
                name: project.name,
                summary: project.summary.unwrap_or_default(),
                technologies,
                evidence: evidence
                    .into_iter()
                    .map(|item| ProjectEvidenceItem {
                        id: item.id,
                        claim: item.claim,
                        source_file: item.source_file,
                        line_start: item.line_start,
                        line_end: item.line_end,
                    })
                    .collect(),
            },
        });
    }
    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.input.project_id.cmp(&b.input.project_id))
    });
    ranked.truncate(5);
    Ok(ranked)
}

#[cfg(test)]
mod tests {
    use super::analyse_job;

    #[test]
    fn extracts_role_and_skill_overlap() {
        let jd = analyse_job(
            "Senior Rust Engineer\n- Build Tokio services using SQLite",
            None,
            None,
        );
        assert_eq!(jd.target_role, "Senior Rust Engineer");
        assert!(jd.core_skills.contains(&"Rust".to_string()));
        assert!(jd.core_skills.contains(&"Tokio".to_string()));
    }
}
