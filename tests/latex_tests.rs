use resumeforge::services::latex::{
    compile_latex_content, escape_latex_text, render_latex_template,
};
use resumeforge::models::generation::{
    GeneratedExperienceItem, GeneratedProjectBullet, GeneratedProjectItem, GeneratedResumeContent,
    GeneratedSkills,
};
use std::path::{Path, PathBuf};

struct TestDir(PathBuf);
impl TestDir {
    fn new(prefix: &str) -> Self {
        let p = std::env::temp_dir().join(format!("rf_test_{}_{}", prefix, uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&p);
        Self(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_unconditional_latex_escaping() {
    // Basic escaping
    assert_eq!(escape_latex_text("C# & C++"), "C\\# \\& C++");
    assert_eq!(escape_latex_text("100% test"), "100\\% test");
    assert_eq!(escape_latex_text("$50M ARR"), "\\$50M ARR");
    assert_eq!(escape_latex_text("foo_bar"), "foo\\_bar");
    assert_eq!(escape_latex_text("{nested}"), "\\{nested\\}");
    assert_eq!(escape_latex_text("~tilde"), "\\textasciitilde{}tilde");
    assert_eq!(escape_latex_text("^hat"), "\\textasciicircum{}hat");

    // Unconditional escaping: already escaped strings should be escaped again
    // (the model produces plain text, never pre-escaped LaTeX)
    assert_eq!(escape_latex_text("100\\%"), "100\\textbackslash{}\\%");
}

#[tokio::test]
async fn test_adversarial_latex_compilation() -> anyhow::Result<()> {
    let tmp = TestDir::new("latex");
    let compile_dir = tmp.path().join("compiles");
    std::fs::create_dir_all(&compile_dir)?;

    let classic_template = include_str!("../data/templates/starter/classic.tex");

    // Content containing heavy adversarial special characters
    let content = GeneratedResumeContent {
        summary: "Lead Engineer handling 99.9% uptime, $10M+ cost savings & 100% compliance across C# / C++ infrastructure.".to_string(),
        skills: GeneratedSkills {
            languages: vec!["C++".to_string(), "C#".to_string(), "Rust_2021".to_string()],
            frameworks_and_tools: vec!["ASP.NET & Node.js".to_string(), "Docker & K8s".to_string()],
            core_concepts: vec!["{Zero-Copy}".to_string(), "I/O ~50ms".to_string()],
        },
        experience: vec![GeneratedExperienceItem {
            title: "Senior Engineer & Architect".to_string(),
            company: "Tech_Corp & Sons".to_string(),
            date_range: "2020 -- 2024".to_string(),
            location: Some("Remote ($US)".to_string()),
            bullets: vec![
                "Engineered async pipeline handling 50,000+ msgs/sec with <1% packet drop & 35% memory savings.".to_string(),
                "Architected distributed cache with ~10ms latency saving $500K/year across {multi-region} AWS clusters.".to_string(),
            ],
        }],
        projects: vec![GeneratedProjectItem {
            project_id: 101,
            name: "Proj_Alpha & Beta".to_string(),
            bullets: vec![
                GeneratedProjectBullet {
                    text: "Built fault-tolerant WAL engine with 100% crash-safety and zero data loss under stress.".to_string(),
                    evidence_ids: vec![1],
                },
            ],
        }],
        achievements: vec![],
    };

    let rendered_latex = render_latex_template(classic_template, &content, false)?;
    assert!(!rendered_latex.contains("Tech_Corp & Sons"), "Unescaped ampersand or underscore must not exist");
    assert!(rendered_latex.contains("Tech\\_Corp \\& Sons"), "Must contain escaped text");

    let pdf_out = compile_dir.join("adversarial_output.pdf");
    let result = compile_latex_content(&rendered_latex, &compile_dir, Some(&pdf_out)).await?;
    assert!(result.success, "Compilation must succeed with properly escaped LaTeX: {:?}", result.error_message);
    assert!(pdf_out.exists(), "Compiled PDF must exist");

    Ok(())
}
