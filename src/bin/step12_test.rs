use resumeforge::models::generation::{
    GeneratedExperienceItem, GeneratedProjectBullet, GeneratedProjectItem, GeneratedResumeContent,
    GeneratedSkills,
};
use resumeforge::services::latex::{compile_latex_content, render_latex_template};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== ResumeForge Step 12 Test: Deterministic LaTeX Renderer ===");

    // Ensure PATH has tectonic and toolchain
    if let Ok(user) = std::env::var("USERPROFILE") {
        let p = format!(
            "{}\\AppData\\Local\\agy\\bin;{}\\w64devkit\\bin;{}",
            user,
            user,
            std::env::var("PATH").unwrap_or_default()
        );
        std::env::set_var("PATH", p);
    }

    // 1. Construct realistic generated content with tricky special characters
    let content = GeneratedResumeContent {
        summary: "Lead Distributed Systems Architect with 8+ years designing fault-tolerant storage engines, high-throughput streaming (50k+ req/sec), and zero-copy Rust microservices. Reduced p99 latency by 35% across distributed clusters.".to_string(),
        skills: GeneratedSkills {
            languages: vec![
                "Rust".to_string(),
                "C++".to_string(),
                "Go".to_string(),
                "Python".to_string(),
                "SQL".to_string(),
            ],
            frameworks_and_tools: vec![
                "Tokio".to_string(),
                "Axum".to_string(),
                "SQLite & WAL".to_string(),
                "gRPC".to_string(),
                "Docker".to_string(),
            ],
            core_concepts: vec![
                "Distributed Systems".to_string(),
                "Concurrency & Lock-Free Structures".to_string(),
                "Zero-Copy Deserialization".to_string(),
            ],
        },
        experience: vec![
            GeneratedExperienceItem {
                title: "Senior Systems Infrastructure Engineer".to_string(),
                company: "CloudScale Systems & Technologies".to_string(),
                date_range: "Jan 2022 -- Present".to_string(),
                location: Some("Remote, USA".to_string()),
                bullets: vec![
                    "Architected high-throughput async processing engines handling over 50,000 requests per second with sub-millisecond p99 latency.".to_string(),
                    "Reduced memory footprint by 35% by redesigning buffer allocation and zero-copy deserialization pipelines.".to_string(),
                    "Engineered crash-safe transaction commit loop with strict ACID guarantees, yielding a $120k/yr cloud infrastructure cost reduction.".to_string(),
                ],
            },
        ],
        projects: vec![
            GeneratedProjectItem {
                project_id: 101,
                name: "ResilientQueue".to_string(),
                bullets: vec![
                    GeneratedProjectBullet {
                        text: "Engineered an asynchronous embedded message queue engine utilizing WAL mode with crash-safe transaction logging.".to_string(),
                        evidence_ids: vec![1001, 1002],
                    },
                    GeneratedProjectBullet {
                        text: "Implemented real-time telemetry streaming over WebSockets with custom Axum middleware.".to_string(),
                        evidence_ids: vec![1003],
                    },
                ],
            },
            GeneratedProjectItem {
                project_id: 102,
                name: "VectorSync".to_string(),
                bullets: vec![
                    GeneratedProjectBullet {
                        text: "Designed SIMD AVX-512 distance metric kernels yielding 4.2x speedup over scalar baseline.".to_string(),
                        evidence_ids: vec![2001],
                    },
                    GeneratedProjectBullet {
                        text: "Authored lock-free concurrency index handling parallel read-write workloads with zero data races.".to_string(),
                        evidence_ids: vec![2002],
                    },
                ],
            },
        ],
        achievements: vec![],
    };

    let temp_dir = PathBuf::from("data/temp/step12_test");
    tokio::fs::create_dir_all(&temp_dir).await?;

    for template_name in ["classic", "modern"] {
        println!("\n--- Testing template: {} ---", template_name);
        let template_path = format!("data/templates/starter/{}.tex", template_name);
        let template_src = tokio::fs::read_to_string(&template_path).await?;

        // A. Render standard template
        let rendered_tex = render_latex_template(&template_src, &content, false)?;
        println!("  ✓ Successfully rendered {} template", template_name);

        // Verification checks on rendered LaTeX:
        // 1. Must contain candidate's injected summary
        assert!(rendered_tex.contains("50k+ req/sec"));
        // 2. Special characters must be properly escaped
        assert!(rendered_tex.contains("35\\%"));
        assert!(rendered_tex.contains("\\&"));
        assert!(rendered_tex.contains("\\$120k/yr"));
        // 3. Preamble macro definitions must remain untouched
        assert!(rendered_tex.contains("\\newcommand{\\ResumeSummary}[1]{#1}"));
        assert!(rendered_tex.contains("\\newcommand{\\ResumeSkills}[1]{#1}"));
        assert!(rendered_tex.contains("\\newcommand{\\ResumeExperience}[1]{#1}"));
        assert!(rendered_tex.contains("\\newcommand{\\ResumeProjects}[1]{#1}"));
        // 4. Old placeholder content must NOT be present
        assert!(!rendered_tex.contains("Experienced Systems Engineer with extensive background"));
        println!("  ✓ Placeholders replaced cleanly without touching preamble definitions");

        // B. Compile to PDF with Tectonic
        let pdf_dest = temp_dir.join(format!("{}_rendered.pdf", template_name));
        let compile_res = compile_latex_content(&rendered_tex, &temp_dir, Some(&pdf_dest)).await?;
        assert!(
            compile_res.success,
            "Compilation failed: {:?}",
            compile_res.error_message
        );
        println!(
            "  ✓ Successfully compiled rendered LaTeX to PDF: {:?}",
            pdf_dest
        );

        // C. Check page count
        let doc = lopdf::Document::load(&pdf_dest)?;
        let page_count = doc.get_pages().len();
        println!("  ✓ Rendered PDF page count: {}", page_count);
        assert_eq!(page_count, 1, "Rendered resume must fit on exactly 1 page!");

        // D. Test compact_mode = true (\tighten injection)
        let tightened_tex = render_latex_template(&template_src, &content, true)?;
        assert!(tightened_tex.contains("\\tighten"));
        let tightened_pdf = temp_dir.join(format!("{}_tightened.pdf", template_name));
        let tight_compile =
            compile_latex_content(&tightened_tex, &temp_dir, Some(&tightened_pdf)).await?;
        assert!(
            tight_compile.success,
            "Tightened compilation failed: {:?}",
            tight_compile.error_message
        );
        let tight_doc = lopdf::Document::load(&tightened_pdf)?;
        assert_eq!(
            tight_doc.get_pages().len(),
            1,
            "Tightened resume must also be 1 page!"
        );
        println!("  ✓ Compact mode (\\tighten) rendered and compiled cleanly");
    }

    // 5. Test error handling on missing required macro
    let bad_template =
        "\\documentclass{article}\n\\begin{document}\n\\ResumeSummary{Test}\n\\end{document}";
    let err_res = render_latex_template(bad_template, &content, false);
    assert!(err_res.is_err());
    let err_msg = err_res.unwrap_err().to_string();
    assert!(err_msg.contains("missing required placeholder macro"));
    println!(
        "\n  ✓ Error correctly triggered for template missing required macros: {}",
        err_msg
    );

    // Clean up temp test directory
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    println!("\n=== STEP 12 VERIFICATION PASSED: Deterministic LaTeX Renderer is rock-solid! ===");
    Ok(())
}
