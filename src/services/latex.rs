use crate::models::generation::{
    GeneratedExperienceItem, GeneratedProjectItem, GeneratedResumeContent, GeneratedSkills,
};
use crate::services::command_runner::run_command;
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CompileResult {
    pub success: bool,
    pub pdf_bytes: Option<Vec<u8>>,
    pub pdf_path: Option<PathBuf>,
    pub error_message: Option<String>,
    pub line_number: Option<usize>,
}

/// Compiles a LaTeX string to PDF in a unique isolated temporary directory (`temp_base_dir/compile_<uuid>`).
/// The temporary directory is always cleaned up after compilation regardless of success or failure.
/// If `target_pdf_dest` is specified and compilation succeeds, the PDF is written to that destination.
pub async fn compile_latex_content(
    content: &str,
    temp_base_dir: &Path,
    target_pdf_dest: Option<&Path>,
) -> Result<CompileResult, anyhow::Error> {
    tokio::fs::create_dir_all(temp_base_dir).await?;
    let run_id = uuid::Uuid::new_v4();
    let run_dir = temp_base_dir.join(format!("compile_{}", run_id));
    tokio::fs::create_dir_all(&run_dir).await?;

    let tex_path = run_dir.join("input.tex");
    tokio::fs::write(&tex_path, content).await?;

    let compile_res = compile_latex_file(&tex_path, &run_dir).await;

    let final_res = match compile_res {
        Ok(res) if res.success => {
            let pdf_file = run_dir.join("input.pdf");
            let mut final_path = None;
            let mut bytes = None;

            if pdf_file.exists() {
                if let Ok(b) = tokio::fs::read(&pdf_file).await {
                    if let Some(dest) = target_pdf_dest {
                        if let Some(parent) = dest.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        let _ = tokio::fs::write(dest, &b).await;
                        final_path = Some(dest.to_path_buf());
                    }
                    bytes = Some(b);
                }
            }

            Ok(CompileResult {
                success: true,
                pdf_bytes: bytes,
                pdf_path: final_path,
                error_message: None,
                line_number: None,
            })
        }
        Ok(res) => Ok(res),
        Err(e) => Err(e),
    };

    // Always delete the unique temporary directory regardless of success or failure
    let _ = tokio::fs::remove_dir_all(&run_dir).await;

    final_res
}

/// Compiles an existing .tex file using tectonic.
pub async fn compile_latex_file(
    tex_path: &Path,
    out_dir: &Path,
) -> Result<CompileResult, anyhow::Error> {
    tokio::fs::create_dir_all(out_dir).await?;
    let out_dir_str = out_dir.to_string_lossy().to_string();
    let tex_path_str = tex_path.to_string_lossy().to_string();

    let output = run_command("tectonic", &[&tex_path_str, "--outdir", &out_dir_str]).await?;

    if output.status.success() {
        let pdf_filename = tex_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| format!("{}.pdf", s))
            .unwrap_or_else(|| "output.pdf".to_string());
        let pdf_path = out_dir.join(pdf_filename);
        let pdf_bytes = if pdf_path.exists() {
            tokio::fs::read(&pdf_path).await.ok()
        } else {
            None
        };
        Ok(CompileResult {
            success: true,
            pdf_bytes,
            pdf_path: if pdf_path.exists() {
                Some(pdf_path)
            } else {
                None
            },
            error_message: None,
            line_number: None,
        })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let combined = format!("{}\n{}", stdout, stderr);

        let (clean_error, line_num) = extract_latex_error(&combined);

        Ok(CompileResult {
            success: false,
            pdf_bytes: None,
            pdf_path: None,
            error_message: Some(clean_error),
            line_number: line_num,
        })
    }
}

/// Parses error lines and line numbers from tectonic / XeTeX compiler logs.
pub fn extract_latex_error(log: &str) -> (String, Option<usize>) {
    // 1. Check for standard format: error: filename.tex:LINE: MESSAGE
    let re_file_line = Regex::new(r"error:\s+[^:\r\n]+:(\d+):\s+(.+)").ok();
    if let Some(re) = &re_file_line {
        for cap in re.captures_iter(log) {
            let line_num: Option<usize> = cap.get(1).and_then(|m| m.as_str().parse().ok());
            let msg = cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            if !msg.is_empty() {
                return (msg, line_num);
            }
        }
    }

    // 2. Check for LaTeX ! Error and l.LINE
    let re_tex_error = Regex::new(r"!\s+(.+)").ok();
    let re_line = Regex::new(r"l\.(\d+)\s+").ok();

    let line_num = re_line.and_then(|re| {
        re.captures(log)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())
    });

    if let Some(re) = &re_tex_error {
        if let Some(cap) = re.captures(log) {
            if let Some(msg) = cap.get(1) {
                let trimmed = msg.as_str().trim().to_string();
                return (trimmed, line_num);
            }
        }
    }

    // 3. Fallback: find any line containing "error:" or return general failure message
    for line in log.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("error:") && !trimmed.contains("something bad happened") {
            return (trimmed.to_string(), line_num);
        }
    }

    (
        "LaTeX compilation halted with unrecoverable error".to_string(),
        line_num,
    )
}

/// Unconditionally escapes all 10 LaTeX special characters in raw text.
/// Every string reaching this function is plain text from GeneratedResumeContent (AI JSON).
/// Special characters handled: \ & % $ # _ { } ~ ^
pub fn escape_latex_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            _ => out.push(c),
        }
    }
    out
}

/// Robust balanced-brace replacer for LaTeX macro invocations.
/// Scans after `\begin{document}` (or from start if document environment not found)
/// to ensure preamble `\newcommand{\MacroName}[1]{#1}` definitions are NEVER modified.
/// Correctly counts nested braces `{ ... }` within the macro's body and replaces the entire
/// `\MacroName{ ... }` with `\MacroName{\n<new_arg_content>\n}`.
pub fn replace_macro_argument(
    source: &str,
    macro_name: &str,
    new_arg_content: &str,
) -> Result<String, anyhow::Error> {
    let search_offset = if let Some(pos) = source.find("\\begin{document}") {
        pos + "\\begin{document}".len()
    } else {
        0
    };

    let body = &source[search_offset..];
    let mut current_offset = 0;
    let mut found_range: Option<(usize, usize)> = None;

    while let Some(rel_idx) = body[current_offset..].find(macro_name) {
        let abs_idx = search_offset + current_offset + rel_idx;
        let after_macro_idx = abs_idx + macro_name.len();

        // Check token boundary: ensure macro_name is a distinct token
        let next_char = source[after_macro_idx..].chars().next();
        let is_distinct = match next_char {
            Some(c) => !c.is_alphanumeric() && c != '@' && c != '_',
            None => true,
        };

        if !is_distinct {
            current_offset += rel_idx + macro_name.len();
            continue;
        }

        // Check that it's not part of \newcommand or \renewcommand
        let prefix = &source[..abs_idx];
        let trimmed_prefix = prefix.trim_end();
        if trimmed_prefix.ends_with("\\newcommand")
            || trimmed_prefix.ends_with("\\renewcommand")
            || trimmed_prefix.ends_with("\\def")
        {
            current_offset += rel_idx + macro_name.len();
            continue;
        }

        // Find the opening brace '{'
        let mut char_indices = source[after_macro_idx..].char_indices();
        let mut open_brace_rel = None;
        while let Some((i, ch)) = char_indices.next() {
            if ch.is_whitespace() {
                continue;
            } else if ch == '{' {
                open_brace_rel = Some(i);
                break;
            } else {
                break;
            }
        }

        let open_brace_idx = match open_brace_rel {
            Some(i) => after_macro_idx + i,
            None => {
                current_offset += rel_idx + macro_name.len();
                continue;
            }
        };

        // Scan for matching closing brace '}' with depth tracking
        let mut depth = 1;
        let mut chars = source[open_brace_idx + 1..].char_indices().peekable();
        let mut close_brace_idx = None;

        while let Some((i, ch)) = chars.next() {
            if ch == '\\' {
                if let Some(&(_, next_ch)) = chars.peek() {
                    if next_ch == '{' || next_ch == '}' || next_ch == '\\' {
                        chars.next();
                    }
                }
            } else if ch == '%' {
                // LaTeX comment until newline
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            } else if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    close_brace_idx = Some(open_brace_idx + 1 + i);
                    break;
                }
            }
        }

        if let Some(close_idx) = close_brace_idx {
            found_range = Some((abs_idx, close_idx));
            break;
        } else {
            anyhow::bail!("Unmatched opening brace for macro '{}'", macro_name);
        }
    }

    if let Some((macro_start, macro_end)) = found_range {
        let mut result = String::with_capacity(source.len() + new_arg_content.len());
        result.push_str(&source[..macro_start]);
        result.push_str(macro_name);
        result.push_str("{\n");
        result.push_str(new_arg_content.trim());
        result.push_str("\n}");
        result.push_str(&source[macro_end + 1..]);
        Ok(result)
    } else {
        anyhow::bail!(
            "Placeholder macro '{}' not found in LaTeX template body",
            macro_name
        );
    }
}

/// Renders the categorized skills block into an itemize list for `\ResumeSkills`.
pub fn render_skills_block(skills: &GeneratedSkills) -> String {
    let mut out = String::new();
    out.push_str("\\begin{itemize}[leftmargin=0.15in, label={}]\n");

    if !skills.languages.is_empty() {
        let escaped: Vec<String> = skills
            .languages
            .iter()
            .map(|s| escape_latex_text(s))
            .collect();
        out.push_str(&format!(
            "    \\item \\textbf{{Languages:}} {}\n",
            escaped.join(", ")
        ));
    }
    if !skills.frameworks_and_tools.is_empty() {
        let escaped: Vec<String> = skills
            .frameworks_and_tools
            .iter()
            .map(|s| escape_latex_text(s))
            .collect();
        out.push_str(&format!(
            "    \\item \\textbf{{Frameworks \\& Tools:}} {}\n",
            escaped.join(", ")
        ));
    }
    if !skills.core_concepts.is_empty() {
        let escaped: Vec<String> = skills
            .core_concepts
            .iter()
            .map(|s| escape_latex_text(s))
            .collect();
        out.push_str(&format!(
            "    \\item \\textbf{{Core Concepts:}} {}\n",
            escaped.join(", ")
        ));
    }

    out.push_str("\\end{itemize}");
    out
}

/// Renders experience entries and bullets for `\ResumeExperience`.
pub fn render_experience_block(experience: &[GeneratedExperienceItem]) -> String {
    let mut out = String::new();
    for (idx, item) in experience.iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\\vspace{4pt}\n");
        }
        let title = escape_latex_text(&item.title);
        let dates = escape_latex_text(&item.date_range);
        let company = escape_latex_text(&item.company);
        out.push_str(&format!(
            "\\textbf{{{}}} \\hfill \\textbf{{{}}} \\\\\n",
            title, dates
        ));

        if let Some(loc) = &item.location {
            if !loc.trim().is_empty() {
                let location = escape_latex_text(loc);
                out.push_str(&format!(
                    "\\textit{{{}}} \\hfill \\textit{{{}}}\n",
                    company, location
                ));
            } else {
                out.push_str(&format!("\\textit{{{}}}\n", company));
            }
        } else {
            out.push_str(&format!("\\textit{{{}}}\n", company));
        }

        if !item.bullets.is_empty() {
            out.push_str("\\begin{itemize}[leftmargin=0.15in]\n");
            for bullet in &item.bullets {
                let escaped_bullet = escape_latex_text(bullet);
                out.push_str(&format!("    \\item {}\n", escaped_bullet));
            }
            out.push_str("\\end{itemize}\n");
        }
    }
    out
}

/// Renders project entries and bullets for `\ResumeProjects`.
pub fn render_projects_block(projects: &[GeneratedProjectItem]) -> String {
    let mut out = String::new();
    for (idx, item) in projects.iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\\vspace{4pt}\n");
        }
        let name = escape_latex_text(&item.name);
        out.push_str(&format!("\\textbf{{{}}}\n", name));

        if !item.bullets.is_empty() {
            out.push_str("\\begin{itemize}[leftmargin=0.15in]\n");
            for bullet in &item.bullets {
                let escaped_bullet = escape_latex_text(&bullet.text);
                out.push_str(&format!("    \\item {}\n", escaped_bullet));
            }
            out.push_str("\\end{itemize}\n");
        }
    }
    out
}

/// Applies the Stage B `\tighten` macro by injecting it immediately after `\begin{document}`.
pub fn apply_tighten_macro(latex: &str) -> Result<String, anyhow::Error> {
    if !latex.contains("\\tighten") {
        anyhow::bail!("Template does not define \\tighten macro");
    }

    if let Some(doc_start) = latex.find("\\begin{document}") {
        let insert_pos = doc_start + "\\begin{document}".len();
        let body = &latex[insert_pos..];
        if body.trim_start().starts_with("\\tighten") {
            return Ok(latex.to_string());
        }
        let mut out = String::with_capacity(latex.len() + 16);
        out.push_str(&latex[..insert_pos]);
        out.push_str("\n\\tighten");
        if !latex[insert_pos..].starts_with('\n') && !latex[insert_pos..].starts_with("\r\n") {
            out.push('\n');
        }
        out.push_str(&latex[insert_pos..]);
        Ok(out)
    } else {
        anyhow::bail!("LaTeX document is missing \\begin{{document}}");
    }
}

/// Injects generated resume content deterministically into the master template's placeholder macros.
/// Strictly respects styling: NEVER modifies layout, geometries, or fonts outside of the macros.
pub fn render_latex_template(
    template: &str,
    content: &GeneratedResumeContent,
    compact_mode: bool,
) -> Result<String, anyhow::Error> {
    let required_macros = [
        "\\ResumeSummary",
        "\\ResumeSkills",
        "\\ResumeExperience",
        "\\ResumeProjects",
    ];

    for m in &required_macros {
        if !template.contains(m) {
            anyhow::bail!("Template is missing required placeholder macro: {}", m);
        }
    }

    let summary_escaped = escape_latex_text(content.summary.trim());
    let skills_rendered = render_skills_block(&content.skills);
    let experience_rendered = render_experience_block(&content.experience);
    let projects_rendered = render_projects_block(&content.projects);

    let mut rendered = template.to_string();
    rendered = replace_macro_argument(&rendered, "\\ResumeSummary", &summary_escaped)?;
    rendered = replace_macro_argument(&rendered, "\\ResumeSkills", &skills_rendered)?;
    rendered = replace_macro_argument(&rendered, "\\ResumeExperience", &experience_rendered)?;
    rendered = replace_macro_argument(&rendered, "\\ResumeProjects", &projects_rendered)?;

    if !content.achievements.is_empty() && rendered.contains("\\ResumeAchievements") {
        let mut ach_out = String::new();
        ach_out.push_str("\\begin{itemize}[leftmargin=0.15in]\n");
        for ach in &content.achievements {
            let escaped = escape_latex_text(ach);
            ach_out.push_str(&format!("    \\item {}\n", escaped));
        }
        ach_out.push_str("\\end{itemize}");
        rendered = replace_macro_argument(&rendered, "\\ResumeAchievements", &ach_out)?;
    }

    if compact_mode {
        rendered = apply_tighten_macro(&rendered)?;
    }

    Ok(rendered)
}

/// Convenience wrapper for rendering with compact_mode = false.
pub fn render_latex(
    template: &str,
    content: &GeneratedResumeContent,
) -> Result<String, anyhow::Error> {
    render_latex_template(template, content, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_latex_text() {
        assert_eq!(escape_latex_text("Rust & C++"), "Rust \\& C++");
        assert_eq!(
            escape_latex_text("Reduced memory by 35%"),
            "Reduced memory by 35\\%"
        );
        assert_eq!(escape_latex_text("$100 cost"), "\\$100 cost");
        assert_eq!(escape_latex_text("C# language"), "C\\# language");
        assert_eq!(escape_latex_text("user_variable"), "user\\_variable");
        assert_eq!(escape_latex_text("{token}"), "\\{token\\}");
        assert_eq!(escape_latex_text("~tilde"), "\\textasciitilde{}tilde");
        assert_eq!(escape_latex_text("^hat"), "\\textasciicircum{}hat");
        assert_eq!(
            escape_latex_text(r"\input{secret.tex}"),
            "\\textbackslash{}input\\{secret.tex\\}"
        );
    }

    #[test]
    fn test_replace_macro_argument_nested_braces() {
        let template = r#"
\documentclass{article}
\newcommand{\ResumeSkills}[1]{#1}
\begin{document}
\ResumeSkills{
\begin{itemize}[label={}]
    \item \textbf{Old:} text
\end{itemize}
}
\end{document}"#;

        let new_content = r#"\begin{itemize}[label={}]
    \item \textbf{Languages:} Rust, Go
\end{itemize}"#;

        let result = replace_macro_argument(template, "\\ResumeSkills", new_content).unwrap();
        assert!(result.contains("\\newcommand{\\ResumeSkills}[1]{#1}"));
        assert!(result.contains("\\textbf{Languages:} Rust, Go"));
        assert!(!result.contains("\\textbf{Old:} text"));
    }

    #[test]
    fn test_apply_tighten_macro() {
        let doc = "\\documentclass{article}\n\\newcommand{\\tighten}{}\n\\begin{document}\nHello\n\\end{document}";
        let tightened = apply_tighten_macro(doc).unwrap();
        assert!(tightened.contains("\\begin{document}\n\\tighten\nHello"));

        // Idempotent: should not insert duplicate \tighten
        let tightened_again = apply_tighten_macro(&tightened).unwrap();
        assert_eq!(tightened, tightened_again);
    }

    fn ensure_test_toolchain_path() {
        if let Ok(user) = std::env::var("USERPROFILE") {
            let p = format!(
                "{}\\AppData\\Local\\agy\\bin;{}\\w64devkit\\bin;{}",
                user,
                user,
                std::env::var("PATH").unwrap_or_default()
            );
            std::env::set_var("PATH", p);
        }
    }

    fn make_test_resume_with_bullet(bullet: &str) -> GeneratedResumeContent {
        GeneratedResumeContent {
            summary: "Systems engineer specializing in high-throughput infrastructure.".to_string(),
            skills: GeneratedSkills {
                languages: vec!["Rust".to_string(), "C++".to_string()],
                frameworks_and_tools: vec!["Tokio".to_string(), "SQLite".to_string()],
                core_concepts: vec!["Distributed Systems".to_string()],
            },
            experience: vec![GeneratedExperienceItem {
                title: "Senior Software Engineer".to_string(),
                company: "CloudScale Systems".to_string(),
                date_range: "2022 -- Present".to_string(),
                location: Some("Remote".to_string()),
                bullets: vec![
                    bullet.to_string(),
                    "Architected low-latency distributed pipeline with sub-millisecond p99 latency.".to_string(),
                ],
            }],
            projects: vec![GeneratedProjectItem {
                project_id: 1,
                name: "ResilientQueue".to_string(),
                bullets: vec![crate::models::generation::GeneratedProjectBullet {
                    text: "Engineered crash-safe WAL transaction commit loop.".to_string(),
                    evidence_ids: vec![101],
                }],
            }],
            achievements: vec![],
        }
    }

    #[tokio::test]
    async fn test_adversarial_literal_braces_pipeline() {
        ensure_test_toolchain_path();
        let bullet = "Used a {key: value} config format for cluster topology definition";
        let content = make_test_resume_with_bullet(bullet);
        let template_src = std::fs::read_to_string("data/templates/starter/classic.tex").unwrap();

        let rendered = render_latex_template(&template_src, &content, false).unwrap();
        assert!(rendered.contains(r"\{key: value\}"));

        let temp_dir = std::path::PathBuf::from("data/temp/test_adv_braces");
        let pdf_dest = temp_dir.join("test_braces.pdf");
        let compile_res = compile_latex_content(&rendered, &temp_dir, Some(&pdf_dest))
            .await
            .unwrap();
        assert!(
            compile_res.success,
            "Compilation failed: {:?}",
            compile_res.error_message
        );

        let doc = lopdf::Document::load(&pdf_dest).unwrap();
        assert_eq!(doc.get_pages().len(), 1, "Must compile to exactly 1 page");
        let text = doc.extract_text(&[1]).unwrap();
        assert!(
            text.contains("{key: value}"),
            "PDF text must visually contain '{{key: value}}'"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_adversarial_input_include_pipeline() {
        ensure_test_toolchain_path();
        let bullet = "Neutralized command injections like \\input{secret.tex} and \\include{admin.tex} in pipeline";
        let content = make_test_resume_with_bullet(bullet);
        let template_src = std::fs::read_to_string("data/templates/starter/classic.tex").unwrap();

        let rendered = render_latex_template(&template_src, &content, false).unwrap();
        assert!(rendered.contains(r"\textbackslash{}input\{secret.tex\}"));
        assert!(rendered.contains(r"\textbackslash{}include\{admin.tex\}"));

        let temp_dir = std::path::PathBuf::from("data/temp/test_adv_input");
        let pdf_dest = temp_dir.join("test_input.pdf");
        let compile_res = compile_latex_content(&rendered, &temp_dir, Some(&pdf_dest))
            .await
            .unwrap();
        assert!(
            compile_res.success,
            "Compilation failed: {:?}",
            compile_res.error_message
        );

        let doc = lopdf::Document::load(&pdf_dest).unwrap();
        assert_eq!(doc.get_pages().len(), 1, "Must compile to exactly 1 page");
        let text = doc.extract_text(&[1]).unwrap();
        assert!(
            text.contains("input{secret.tex}"),
            "PDF text must visually contain 'input{{secret.tex}}'"
        );
        assert!(
            text.contains("include{admin.tex}"),
            "PDF text must visually contain 'include{{admin.tex}}'"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_adversarial_back_to_back_special_chars_pipeline() {
        ensure_test_toolchain_path();
        let bullet = "50% faster & 2x throughput (saved $10k)";
        let content = make_test_resume_with_bullet(bullet);
        let template_src = std::fs::read_to_string("data/templates/starter/classic.tex").unwrap();

        let rendered = render_latex_template(&template_src, &content, false).unwrap();
        assert!(rendered.contains(r"50\% faster \& 2x throughput (saved \$10k)"));

        let temp_dir = std::path::PathBuf::from("data/temp/test_adv_special");
        let pdf_dest = temp_dir.join("test_special.pdf");
        let compile_res = compile_latex_content(&rendered, &temp_dir, Some(&pdf_dest))
            .await
            .unwrap();
        assert!(
            compile_res.success,
            "Compilation failed: {:?}",
            compile_res.error_message
        );

        let doc = lopdf::Document::load(&pdf_dest).unwrap();
        assert_eq!(doc.get_pages().len(), 1, "Must compile to exactly 1 page");
        let text = doc.extract_text(&[1]).unwrap();
        assert!(
            text.contains("50% faster & 2x throughput (saved $10k)"),
            "PDF text must visually contain '50% faster & 2x throughput (saved $10k)'"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
