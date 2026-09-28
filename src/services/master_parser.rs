use crate::models::generation::{EducationFact, MasterFacts, WorkHistoryFact};
use regex::Regex;

/// Deterministically parses structured candidate facts from an active master LaTeX resume.
pub fn parse_master_facts(tex: &str) -> MasterFacts {
    let candidate_name = parse_candidate_name(tex);
    let contact = parse_contact(tex);
    let current_summary = parse_summary(tex);
    let known_skills = parse_skills(tex);
    let education = parse_education(tex);
    let work_history = parse_work_history(tex);

    MasterFacts {
        candidate_name,
        contact,
        current_summary,
        known_skills,
        education,
        work_history,
    }
}

/// Helper to extract content of a balanced macro like \ResumeSummary{...} or \ResumeExperience{...}
pub fn extract_balanced_macro_argument(tex: &str, macro_name: &str) -> Option<String> {
    let doc_start = tex.find("\\begin{document}").unwrap_or(0);
    let body = &tex[doc_start..];

    let mut current_offset = 0;
    while let Some(rel_idx) = body[current_offset..].find(macro_name) {
        let abs_idx = current_offset + rel_idx;
        let after_name = abs_idx + macro_name.len();

        let prefix = &body[..abs_idx].trim_end();
        if prefix.ends_with("\\newcommand")
            || prefix.ends_with("\\renewcommand")
            || prefix.ends_with("\\def")
        {
            current_offset = after_name;
            continue;
        }

        // Check token boundary
        if let Some(next_char) = body[after_name..].chars().next() {
            if next_char.is_alphanumeric() || next_char == '@' || next_char == '_' {
                current_offset = after_name;
                continue;
            }
        }

        // Find opening brace '{'
        let mut chars = body[after_name..].char_indices();
        let mut open_idx = None;
        for (i, ch) in chars.by_ref() {
            if ch.is_whitespace() {
                continue;
            } else if ch == '{' {
                open_idx = Some(after_name + i);
                break;
            } else {
                break;
            }
        }

        let Some(open_pos) = open_idx else {
            current_offset = after_name;
            continue;
        };

        // Scan for matching closing brace '}' with depth tracking
        let mut depth = 1;
        let mut close_pos = None;
        let mut iter = body[open_pos + 1..].char_indices().peekable();
        while let Some((i, ch)) = iter.next() {
            if ch == '\\' {
                if let Some(&(_, next_ch)) = iter.peek() {
                    if next_ch == '{' || next_ch == '}' || next_ch == '\\' {
                        iter.next();
                    }
                }
            } else if ch == '%' {
                for (_, c) in iter.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            } else if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    close_pos = Some(open_pos + 1 + i);
                    break;
                }
            }
        }

        {
            let close_idx = close_pos?;
            return Some(body[open_pos + 1..close_idx].to_string());
        }
    }

    None
}

/// Strips common LaTeX formatting commands, comments, and escapes into clean plain text.
pub fn clean_latex_markup(raw: &str) -> String {
    // Protect literal escaped percent signs '\%' before stripping comments
    let protected = raw.replace("\\%", "\u{E000}");

    let comment_re = Regex::new(r"(?m)%.*$").unwrap();
    let without_comments = comment_re.replace_all(&protected, "");

    let restored = without_comments.replace("\u{E000}", "%");

    let href_re = Regex::new(r"\\href\{[^}]*\}\{([^}]+)\}").unwrap();
    let text = href_re.replace_all(&restored, "$1");

    let text = text
        .replace("\\$", "$")
        .replace("\\&", "&")
        .replace("\\#", "#")
        .replace("\\_", "_")
        .replace("\\~", "~")
        .replace("\\^", "^")
        .replace("\\hfill", " ")
        .replace("\\\\", "\n")
        .replace("\\newline", "\n");

    let cmd_arg_re = Regex::new(r"\\[a-zA-Z]+\{([^}]*)\}").unwrap();
    let mut cleaned = text;
    for _ in 0..3 {
        cleaned = cmd_arg_re.replace_all(&cleaned, "$1").into_owned();
    }

    let bare_cmd_re = Regex::new(r"\\[a-zA-Z]+").unwrap();
    let cleaned = bare_cmd_re.replace_all(&cleaned, " ");

    let cleaned = cleaned.replace(['{', '}'], "");

    let space_re = Regex::new(r"[ \t]+").unwrap();
    let lines: Vec<String> = cleaned
        .lines()
        .map(|l| space_re.replace_all(l.trim(), " ").into_owned())
        .filter(|l| !l.is_empty())
        .collect();

    lines.join("\n")
}

fn parse_candidate_name(tex: &str) -> String {
    let re_huge_bf = Regex::new(r"\\Huge\s+(?:\\bfseries\s+)?(?:\\color\{[^}]+\}\s+)?\\textbf\{([^}]+)\}").unwrap();
    if let Some(c) = re_huge_bf.captures(tex) {
        if let Some(m) = c.get(1) {
            return m.as_str().trim().to_string();
        }
    }

    let re_huge = Regex::new(r"\\Huge\s+(?:\\bfseries\s+)?(?:\\color\{[^}]+\}\s+)?([^\s\\}]+(?:\s+[^\s\\}]+)*)").unwrap();
    if let Some(c) = re_huge.captures(tex) {
        if let Some(m) = c.get(1) {
            let name = clean_latex_markup(m.as_str());
            if !name.is_empty() {
                return name;
            }
        }
    }

    "Candidate".to_string()
}

fn parse_contact(tex: &str) -> Option<String> {
    let doc_start = tex.find("\\begin{document}").unwrap_or(0);
    let first_sec = tex[doc_start..].find("\\section").map(|p| doc_start + p).unwrap_or(tex.len());
    let header_block = &tex[doc_start..first_sec];

    let re_env = Regex::new(r"\\begin\{(?:center|flushleft)\}([\s\S]*?)\\end\{(?:center|flushleft)\}").unwrap();
    let text_to_scan = if let Some(cap) = re_env.captures(header_block) {
        cap.get(1).map(|m| m.as_str()).unwrap_or(header_block)
    } else {
        header_block
    };

    let href_re = Regex::new(r"\\href\{[^}]*\}\{([^}]+)\}").unwrap();
    let simplified = href_re.replace_all(text_to_scan, "$1");

    let clean = simplified
        .replace("\\$", "$")
        .replace("$|$", "|")
        .replace("$ | $", "|")
        .replace("\\|", "|")
        .replace("\\\\", "\n");

    let cmd_re = Regex::new(r"\\[a-zA-Z]+(?:\[[^\]]*\])?(?:\{[^}]*\})?").unwrap();
    let text = cmd_re.replace_all(&clean, " ");

    let name = parse_candidate_name(tex);
    let mut parts = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == name {
            continue;
        }
        for chunk in trimmed.split('|') {
            let clean_chunk = chunk.replace(['{', '}'], "");
            let item = clean_chunk.trim().trim_matches('|').trim();
            if !item.is_empty() && item != name && !item.starts_with('\\') {
                parts.push(item.to_string());
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn parse_summary(tex: &str) -> Option<String> {
    if let Some(arg) = extract_balanced_macro_argument(tex, "\\ResumeSummary") {
        let cleaned = clean_latex_markup(&arg);
        let summary = cleaned.lines().collect::<Vec<_>>().join(" ");
        if !summary.trim().is_empty() {
            return Some(summary.trim().to_string());
        }
    }

    if let Some(sec_idx) = tex.find("\\section{Summary}")
        .or_else(|| tex.find("\\section{Professional Summary}"))
    {
        let after_sec = &tex[sec_idx..];
        let end_idx = after_sec[8..].find("\\section").map(|p| 8 + p).unwrap_or(after_sec.len());
        let sec_text = &after_sec[..end_idx];
        let cleaned = clean_latex_markup(sec_text);
        let lines: Vec<&str> = cleaned.lines().filter(|l| !l.is_empty() && !l.contains("Summary")).collect();
        let summary = lines.join(" ");
        if !summary.trim().is_empty() {
            return Some(summary.trim().to_string());
        }
    }

    None
}

fn parse_skills(tex: &str) -> Vec<String> {
    let block = extract_balanced_macro_argument(tex, "\\ResumeSkills");
    let content = if let Some(b) = block {
        b
    } else if let Some(sec_idx) = tex.find("\\section{Technical Skills}")
        .or_else(|| tex.find("\\section{Skills}"))
    {
        let after_sec = &tex[sec_idx..];
        let end_idx = after_sec[8..].find("\\section").map(|p| 8 + p).unwrap_or(after_sec.len());
        after_sec[..end_idx].to_string()
    } else {
        return Vec::new();
    };

    let mut skills = Vec::new();
    let re_item = Regex::new(r"\\item\s+(?:\\textbf\{[^}:]+:\}\s*)?([^\n\r]+)").unwrap();
    for cap in re_item.captures_iter(&content) {
        if let Some(m) = cap.get(1) {
            let cleaned = clean_latex_markup(m.as_str());
            for skill in cleaned.split(',') {
                let trimmed = skill.trim();
                if !trimmed.is_empty() && !skills.iter().any(|s: &String| s.eq_ignore_ascii_case(trimmed)) {
                    skills.push(trimmed.to_string());
                }
            }
        }
    }

    skills
}

fn parse_work_history(tex: &str) -> Vec<WorkHistoryFact> {
    let block = extract_balanced_macro_argument(tex, "\\ResumeExperience");
    let content = if let Some(b) = block {
        b
    } else if let Some(sec_idx) = tex.find("\\section{Experience}")
        .or_else(|| tex.find("\\section{Work Experience}"))
    {
        let after_sec = &tex[sec_idx..];
        let end_idx = after_sec[8..].find("\\section").map(|p| 8 + p).unwrap_or(after_sec.len());
        after_sec[..end_idx].to_string()
    } else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    let mut search_idx = 0;
    let mut prev_end = 0;

    let re_bold = Regex::new(r"\\textbf\{([^}]+)\}").unwrap();
    let re_italic = Regex::new(r"\\textit\{([^}]+)\}").unwrap();
    let re_item = Regex::new(r"\\item\s+([^\n\r]+(?:\n[^\n\r\\]+)*)").unwrap();

    while let Some(item_start_rel) = content[search_idx..].find("\\begin{itemize}") {
        let item_start = search_idx + item_start_rel;
        let item_end_rel = content[item_start..].find("\\end{itemize}");
        let item_end = match item_end_rel {
            Some(end_rel) => item_start + end_rel + "\\end{itemize}".len(),
            None => content.len(),
        };

        let header_slice = &content[prev_end..item_start];
        let body_slice = &content[item_start..item_end];

        let bold_matches: Vec<String> = re_bold.captures_iter(header_slice)
            .filter_map(|c| c.get(1).map(|m| clean_latex_markup(m.as_str())))
            .filter(|s| !s.is_empty())
            .collect();

        let italic_matches: Vec<String> = re_italic.captures_iter(header_slice)
            .filter_map(|c| c.get(1).map(|m| clean_latex_markup(m.as_str())))
            .filter(|s| !s.is_empty())
            .collect();

        let (title, date_range) = match bold_matches.len() {
            0 => ("Software Engineer".to_string(), "".to_string()),
            1 => (bold_matches[0].clone(), "".to_string()),
            _ => (bold_matches[0].clone(), bold_matches[1].clone()),
        };

        let (company, location) = match italic_matches.len() {
            0 => ("".to_string(), None),
            1 => (italic_matches[0].clone(), None),
            _ => (italic_matches[0].clone(), Some(italic_matches[1].clone())),
        };

        let mut highlights = Vec::new();
        for item_cap in re_item.captures_iter(body_slice) {
            if let Some(item_match) = item_cap.get(1) {
                let cleaned_bullet = clean_latex_markup(item_match.as_str())
                    .lines()
                    .collect::<Vec<_>>()
                    .join(" ");
                let trimmed = cleaned_bullet.trim();
                if !trimmed.is_empty() {
                    highlights.push(trimmed.to_string());
                }
            }
        }

        if !company.is_empty() || !title.is_empty() {
            entries.push(WorkHistoryFact {
                title,
                company,
                date_range,
                location,
                highlights,
            });
        }

        prev_end = item_end;
        search_idx = item_end;
        if item_end >= content.len() {
            break;
        }
    }

    entries
}

fn parse_education(tex: &str) -> Vec<EducationFact> {
    let mut entries = Vec::new();
    if let Some(sec_idx) = tex.find("\\section{Education}") {
        let after_sec = &tex[sec_idx..];
        let end_idx = after_sec[8..].find("\\section").map(|p| 8 + p).unwrap_or(after_sec.len());
        let sec_text = &after_sec[..end_idx];

        let mut search_idx = 0;
        let mut prev_end = 0;

        let re_bold = Regex::new(r"\\textbf\{([^}]+)\}").unwrap();
        let re_italic = Regex::new(r"\\textit\{([^}]+)\}").unwrap();
        let re_item = Regex::new(r"\\item\s+([^\n\r]+(?:\n[^\n\r\\]+)*)").unwrap();

        while let Some(item_start_rel) = sec_text[search_idx..].find("\\begin{itemize}") {
            let item_start = search_idx + item_start_rel;
            let item_end_rel = sec_text[item_start..].find("\\end{itemize}");
            let item_end = match item_end_rel {
                Some(end_rel) => item_start + end_rel + "\\end{itemize}".len(),
                None => sec_text.len(),
            };

            let header_slice = &sec_text[prev_end..item_start];
            let body_slice = &sec_text[item_start..item_end];

            let bold_matches: Vec<String> = re_bold.captures_iter(header_slice)
                .filter_map(|c| c.get(1).map(|m| clean_latex_markup(m.as_str())))
                .filter(|s| !s.is_empty())
                .collect();

            let italic_matches: Vec<String> = re_italic.captures_iter(header_slice)
                .filter_map(|c| c.get(1).map(|m| clean_latex_markup(m.as_str())))
                .filter(|s| !s.is_empty())
                .collect();

            let (inst, date_range) = match bold_matches.len() {
                0 => ("".to_string(), None),
                1 => (bold_matches[0].clone(), None),
                _ => (bold_matches[0].clone(), Some(bold_matches[1].clone())),
            };

            let (degree, location) = match italic_matches.len() {
                0 => ("".to_string(), None),
                1 => (italic_matches[0].clone(), None),
                _ => (italic_matches[0].clone(), Some(italic_matches[1].clone())),
            };

            let mut highlights = Vec::new();
            for item_cap in re_item.captures_iter(body_slice) {
                if let Some(item_match) = item_cap.get(1) {
                    let cleaned = clean_latex_markup(item_match.as_str())
                        .lines()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !cleaned.trim().is_empty() {
                        highlights.push(cleaned.trim().to_string());
                    }
                }
            }

            if !inst.is_empty() || !degree.is_empty() {
                entries.push(EducationFact {
                    institution: inst,
                    degree,
                    date_range,
                    location,
                    highlights,
                });
            }

            prev_end = item_end;
            search_idx = item_end;
            if item_end >= sec_text.len() {
                break;
            }
        }

        if entries.is_empty() {
            let bold_matches: Vec<String> = re_bold.captures_iter(sec_text)
                .filter_map(|c| c.get(1).map(|m| clean_latex_markup(m.as_str())))
                .filter(|s| !s.is_empty())
                .collect();
            let italic_matches: Vec<String> = re_italic.captures_iter(sec_text)
                .filter_map(|c| c.get(1).map(|m| clean_latex_markup(m.as_str())))
                .filter(|s| !s.is_empty())
                .collect();

            if !bold_matches.is_empty() || !italic_matches.is_empty() {
                let inst = bold_matches.first().cloned().unwrap_or_default();
                let date_range = bold_matches.get(1).cloned();
                let degree = italic_matches.first().cloned().unwrap_or_default();
                let location = italic_matches.get(1).cloned();
                if !inst.is_empty() || !degree.is_empty() {
                    entries.push(EducationFact {
                        institution: inst,
                        degree,
                        date_range,
                        location,
                        highlights: vec![],
                    });
                }
            }
        }
    }
    entries
}

/// Checks if master resume has sections whose contents failed to be parsed into structured facts.
pub fn check_master_parsing_warnings(tex: &str, facts: &MasterFacts) -> Vec<String> {
    let mut warnings = Vec::new();

    let has_exp = tex.contains("\\section{Experience}")
        || tex.contains("\\section{Work Experience}")
        || (tex.contains("\\ResumeExperience") && !tex.contains("\\newcommand{\\ResumeExperience}"));

    if has_exp && facts.work_history.is_empty() {
        warnings.push("Master resume has an Experience section, but master_parser extracted 0 work history entries. Check template formatting.".to_string());
    }

    let has_edu = tex.contains("\\section{Education}")
        || (tex.contains("\\ResumeEducation") && !tex.contains("\\newcommand{\\ResumeEducation}"));

    if has_edu && facts.education.is_empty() {
        warnings.push("Master resume has an Education section, but master_parser extracted 0 education entries. Check template formatting.".to_string());
    }

    warnings
}

/// Normalizes dates by replacing double dashes and extra whitespace.
pub fn normalize_dates(s: &str) -> String {
    s.replace("--", "-")
        .replace(['–', '—'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Canonicalizes skill names, mapping common aliases to standard form.
pub fn canonicalize_skill(skill: &str) -> String {
    let lower = skill.trim().to_lowercase();
    match lower.as_str() {
        "golang" => "go".to_string(),
        "js" | "javascript" => "javascript".to_string(),
        "ts" | "typescript" => "typescript".to_string(),
        "py" | "python" => "python".to_string(),
        "postgres" | "postgresql" => "postgresql".to_string(),
        "k8s" | "kubernetes" => "kubernetes".to_string(),
        "cpp" | "c++" => "c++".to_string(),
        "c#" | "csharp" => "c#".to_string(),
        "react" | "reactjs" | "react.js" => "react".to_string(),
        "vue" | "vuejs" | "vue.js" => "vue".to_string(),
        "node" | "nodejs" | "node.js" => "node.js".to_string(),
        _ => lower,
    }
}

/// Checks if a word appears as a distinct token in the target text.
pub fn contains_word_token(text: &str, word: &str) -> bool {
    let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
    if let Ok(re) = Regex::new(&pattern) {
        re.is_match(text)
    } else {
        text.to_lowercase().contains(&word.to_lowercase())
    }
}

/// Normalizes text for comparison.
pub fn normalize_str(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Extracts numeric metrics and percentages from a bullet string.
/// Matches:
/// - Percentages: e.g. "35%", "72%"
/// - Formatted numbers: e.g. "50,000", "1,000,000"
/// - Standalone numbers: e.g. "50000", "99", "72"
pub fn extract_metrics(text: &str) -> Vec<String> {
    let re = Regex::new(r"\b\d+(?:,\d{3})*(?:\.\d+)?%?").unwrap();
    let mut metrics = Vec::new();
    for m in re.find_iter(text) {
        let val = m.as_str().trim();
        if !val.is_empty() && !metrics.contains(&val.to_string()) {
            metrics.push(val.to_string());
        }
    }
    metrics
}

/// Returns true if a candidate metric appears in the source text.
pub fn metric_appears_in_source(metric: &str, source_text: &str) -> bool {
    if source_text.contains(metric) {
        return true;
    }

    // Check with commas removed, e.g. 50,000 vs 50000
    let clean_metric = metric.replace(',', "");
    let clean_source = source_text.replace(',', "");
    if clean_source.contains(&clean_metric) {
        return true;
    }

    // Check percentage equivalent (e.g., "72%" vs "72 percent")
    if let Some(num) = metric.strip_suffix('%') {
        let percent_str = format!("{} percent", num.trim());
        if source_text.to_lowercase().contains(&percent_str) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_classic_template_master_facts() {
        let classic_tex = include_str!("../../data/templates/starter/classic.tex");
        let facts = parse_master_facts(classic_tex);

        assert_eq!(facts.candidate_name, "Candidate Name");
        assert!(facts.contact.is_some());
        let contact = facts.contact.unwrap();
        assert!(contact.contains("candidate@example.com"));
        assert!(contact.contains("555"));

        assert!(facts.current_summary.is_some());
        let summary = facts.current_summary.unwrap();
        assert!(summary.contains("Experienced Systems Engineer"));
        assert!(!summary.contains("\\ResumeSummary"));

        assert!(!facts.known_skills.is_empty());
        assert!(facts.known_skills.contains(&"Rust".to_string()));
        assert!(facts.known_skills.contains(&"Tokio".to_string()));

        assert_eq!(facts.work_history.len(), 1);
        let work = &facts.work_history[0];
        assert_eq!(work.title, "Senior Systems Engineer");
        assert_eq!(work.company, "CloudScale Systems");
        assert_eq!(work.date_range, "Jan 2023 -- Present");
        assert_eq!(work.location.as_deref(), Some("Remote"));
        assert_eq!(work.highlights.len(), 2);
        assert!(work.highlights[0].contains("50,000"));
        assert!(work.highlights[1].contains("35%"));
    }

    #[test]
    fn parses_modern_template_master_facts() {
        let modern_tex = include_str!("../../data/templates/starter/modern.tex");
        let facts = parse_master_facts(modern_tex);

        assert_eq!(facts.candidate_name, "Candidate Name");
        assert!(facts.contact.is_some());
        assert!(facts.current_summary.is_some());
        assert_eq!(facts.work_history.len(), 1);
        assert_eq!(facts.work_history[0].company, "CloudScale Systems");
    }

    #[test]
    fn extracts_and_verifies_metrics() {
        let bullet = "Achieved 72% faster throughput and handled 50,000 requests.";
        let metrics = extract_metrics(bullet);
        assert!(metrics.contains(&"72%".to_string()));
        assert!(metrics.contains(&"50,000".to_string()));

        let claim_valid = "Engineered async engine handling 50,000 requests with 72% faster responses.";
        assert!(metric_appears_in_source("72%", claim_valid));
        assert!(metric_appears_in_source("50,000", claim_valid));

        let claim_invalid = "Engineered async engine with WAL mode.";
        assert!(!metric_appears_in_source("72%", claim_invalid));
        assert!(!metric_appears_in_source("50,000", claim_invalid));
    }
}
