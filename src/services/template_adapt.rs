use crate::services::pty_runner::{run_agy_prompt, AgyOutcome};
use regex::Regex;
use serde::Serialize;

#[derive(Serialize)]
pub struct AdaptPreview {
    pub adapted_content: String,
    pub diff: String,
}

pub async fn adapt(content: &str) -> anyhow::Result<AdaptPreview> {
    if [
        "\\ResumeSummary{",
        "\\ResumeSkills{",
        "\\ResumeExperience{",
        "\\ResumeProjects{",
    ]
    .iter()
    .all(|name| content.contains(name))
    {
        return Ok(AdaptPreview {
            adapted_content: content.to_string(),
            diff: "Template already contains required content placeholders".into(),
        });
    }
    let document_start = content
        .find("\\begin{document}")
        .ok_or_else(|| anyhow::anyhow!("Missing \\begin{{document}}"))?;
    let document_end = content
        .find("\\end{document}")
        .ok_or_else(|| anyhow::anyhow!("Missing \\end{{document}}"))?;
    if document_end <= document_start {
        anyhow::bail!("Document boundaries are out of order");
    }
    let re = Regex::new(r"\\section\*?(?:\[[^\]]*\])?\{([^}]*)\}")?;
    let sections: Vec<(usize, usize, String)> = re
        .captures_iter(&content[document_start..document_end])
        .filter_map(|capture| {
            Some((
                document_start + capture.get(0)?.start(),
                document_start + capture.get(0)?.end(),
                capture.get(1)?.as_str().to_ascii_lowercase(),
            ))
        })
        .collect();
    if sections.len() < 4 {
        anyhow::bail!(
            "Need at least summary, skills, experience, and projects sections for adaptation"
        );
    }
    let mut mapping = [None; 4];
    for (index, (_, _, title)) in sections.iter().enumerate() {
        if title.contains("summary") || title.contains("profile") {
            mapping[0] = Some(index);
        }
        if title.contains("skill") || title.contains("technolog") {
            mapping[1] = Some(index);
        }
        if title.contains("experience") || title.contains("employment") {
            mapping[2] = Some(index);
        }
        if title.contains("project") || title.contains("portfolio") {
            mapping[3] = Some(index);
        }
    }
    if mapping.iter().any(Option::is_none) {
        let headings: Vec<&str> = sections
            .iter()
            .map(|(_, _, title)| title.as_str())
            .collect();
        let prompt = format!("Classify these resume section headings. Return ONLY JSON with integer indices for keys summary, skills, experience, projects. No tool use. Headings: {}", serde_json::to_string(&headings)?);
        let response = run_agy_prompt(&prompt, 30).await?;
        if response.status != AgyOutcome::Success {
            anyhow::bail!("Could not classify section headings: {:?}", response.status);
        }
        let parsed: serde_json::Value = serde_json::from_str(response.response.trim())?;
        for (slot, name) in ["summary", "skills", "experience", "projects"]
            .iter()
            .enumerate()
        {
            mapping[slot] = parsed
                .get(*name)
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
        }
    }
    let indices: Vec<usize> = mapping
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("Could not identify all required sections"))?;
    if indices.iter().any(|i| *i >= sections.len()) || {
        let unique: std::collections::HashSet<_> = indices.iter().copied().collect();
        unique.len() != 4
    } {
        anyhow::bail!("Section classification returned duplicate or invalid indices");
    }

    let names = [
        "ResumeSummary",
        "ResumeSkills",
        "ResumeExperience",
        "ResumeProjects",
    ];
    let mut insertions: Vec<(usize, String)> = Vec::new();
    let mut additions = Vec::new();
    for (slot, index) in indices.iter().enumerate() {
        let start_body = sections[*index].1;
        let end_body = sections
            .get(*index + 1)
            .map(|s| s.0)
            .unwrap_or(document_end);
        let open = format!("\n\\{}{{\n", names[slot]);
        insertions.push((start_body, open.clone()));
        insertions.push((end_body, "\n}\n".into()));
        additions.push(format!("+ \\{}{{ ... }}", names[slot]));
    }
    let mut adapted = content.to_string();
    insertions.sort_by(|a, b| b.0.cmp(&a.0));
    for (position, addition) in insertions {
        adapted.insert_str(position, &addition);
    }
    let mut definitions = String::new();
    for name in names {
        if !content[..document_start].contains(&format!("\\newcommand{{\\{name}}}")) {
            definitions.push_str(&format!("\\newcommand{{\\{name}}}[1]{{#1}}\n"));
        }
    }
    if !content[..document_start].contains("\\tighten") {
        definitions.push_str("\\newcommand{\\tighten}{\\small}\n");
    }
    adapted.insert_str(document_start, &definitions);
    additions.push(format!(
        "+ {} macro definitions before \\begin{{document}}",
        definitions.lines().count()
    ));
    Ok(AdaptPreview {
        adapted_content: adapted,
        diff: additions.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::adapt;

    #[tokio::test]
    async fn wraps_sections_without_rewriting_existing_text() {
        let source = "\\documentclass{article}\n\\begin{document}\n\\section{Summary}One\n\\section{Skills}Two\n\\section{Experience}Three\n\\section{Projects}Four\n\\end{document}";
        let preview = adapt(source).await.unwrap();
        assert!(preview.adapted_content.contains("\\ResumeProjects{"));
        assert!(preview.adapted_content.contains("Four"));
        assert!(preview.adapted_content.contains("\\newcommand{\\tighten}"));
    }
}
