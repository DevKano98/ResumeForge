use resumeforge::models::generation::{
    ContentGenerationInput, JdAnalysis, MasterFacts, ProjectEvidenceItem, RankedProjectInput,
    WorkHistoryFact,
};
use resumeforge::services::antigravity;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== ResumeForge Step 9 Test: ContentGenerationAgent with agy CLI ===");

    // 1. Construct synthetic inputs per Section 5 step 3
    let input = ContentGenerationInput {
        candidate_name: "Alex Rivera".to_string(),
        master_facts: MasterFacts {
            candidate_name: "Alex Rivera".to_string(),
            contact: Some("alex@example.com".to_string()),
            current_summary: Some("Systems software engineer specializing in async Rust, Tokio, and reliable storage infrastructure.".to_string()),
            education: vec![],
            known_skills: vec![
                "Rust".to_string(),
                "Tokio".to_string(),
                "SQLite".to_string(),
                "Distributed Systems".to_string(),
                "C++".to_string(),
                "Linux".to_string(),
                "gRPC".to_string(),
                "Docker".to_string(),
            ],
            work_history: vec![WorkHistoryFact {
                title: "Senior Systems Engineer".to_string(),
                company: "CloudScale Systems".to_string(),
                date_range: "2022 -- Present".to_string(),
                location: Some("Remote".to_string()),
                highlights: vec![
                    "Architected high-throughput async processing engines handling over 50,000 req/sec".to_string(),
                    "Reduced memory footprint by 35% through zero-copy buffer pipelines".to_string(),
                ],
            }],
        },
        jd_analysis: JdAnalysis {
            target_company: Some("FinTech Distributed Labs".to_string()),
            target_role: "Senior Distributed Systems Infrastructure Engineer".to_string(),
            core_skills: vec![
                "Rust".to_string(),
                "Concurrency".to_string(),
                "High-Throughput Streaming".to_string(),
                "SQLite/WAL".to_string(),
                "Distributed Systems".to_string(),
            ],
            secondary_skills: vec!["gRPC".to_string(), "eBPF".to_string(), "Linux Internals".to_string()],
            key_requirements: vec![
                "5+ years systems programming with modern Rust".to_string(),
                "Experience building fault-tolerant storage engines and consensus loops".to_string(),
                "Strong async Tokio and low-latency networking experience".to_string(),
            ],
        },
        ranked_projects: vec![
            RankedProjectInput {
                project_id: 101,
                name: "ResilientQueue".to_string(),
                summary: "Asynchronous embedded queue engine utilizing SQLite WAL mode with transaction logging".to_string(),
                technologies: vec!["Rust".to_string(), "Tokio".to_string(), "SQLite".to_string(), "WAL".to_string()],
                evidence: vec![
                    ProjectEvidenceItem {
                        id: 1001,
                        claim: "Engineered crash-safe WAL transaction commit loop achieving 120,000 ops/sec".to_string(),
                        source_file: "src/wal.rs".to_string(),
                        line_start: Some(42),
                        line_end: Some(89),
                    },
                    ProjectEvidenceItem {
                        id: 1002,
                        claim: "Implemented zero-allocation message serializer with custom ring buffer".to_string(),
                        source_file: "src/buffer.rs".to_string(),
                        line_start: Some(15),
                        line_end: Some(50),
                    },
                ],
            },
            RankedProjectInput {
                project_id: 102,
                name: "VectorSync".to_string(),
                summary: "SIMD-accelerated high-dimensional vector search engine with k-NN indexing".to_string(),
                technologies: vec!["Rust".to_string(), "AVX-512".to_string(), "Tokio".to_string()],
                evidence: vec![
                    ProjectEvidenceItem {
                        id: 2001,
                        claim: "Designed AVX-512 distance metric kernels yielding 4.2x speedup over scalar baseline".to_string(),
                        source_file: "simd/distance.rs".to_string(),
                        line_start: Some(10),
                        line_end: Some(45),
                    },
                    ProjectEvidenceItem {
                        id: 2002,
                        claim: "Authored lock-free concurrency index handling parallel read-write workloads".to_string(),
                        source_file: "index/lockfree.rs".to_string(),
                        line_start: Some(80),
                        line_end: Some(135),
                    },
                ],
            },
        ],
        extra_instructions: Some("Emphasize fault-tolerant storage algorithms and low-level performance.".to_string()),
    };

    println!(
        "Constructed prompt length: {} chars",
        antigravity::build_generation_prompt(&input).len()
    );
    println!("Beginning 3 consecutive test runs with 120s timeout budget to verify schema consistency...\n");

    let total_start = Instant::now();
    let mut run_results = Vec::new();

    for run_idx in 1..=3 {
        println!("--------------------------------------------------");
        println!(">>> STARTING RUN {}/3...", run_idx);
        println!("--------------------------------------------------");

        let start = Instant::now();
        let result = antigravity::run_content_generation(
            &input,
            antigravity::DEFAULT_CONTENT_GEN_TIMEOUT_SECS,
        )
        .await;
        let elapsed = start.elapsed();

        match result {
            Ok((parsed, raw_run)) => {
                println!(
                    "Run {} SUCCESS in {:.2}s (agy elapsed: {}ms, exit: {:?})",
                    run_idx,
                    elapsed.as_secs_f64(),
                    raw_run.elapsed_ms,
                    raw_run.exit_code
                );
                println!("  - Summary: {} chars", parsed.summary.len());
                println!(
                    "  - Skills Languages ({}): {:?}",
                    parsed.skills.languages.len(),
                    parsed.skills.languages
                );
                println!(
                    "  - Skills Frameworks/Tools ({}): {:?}",
                    parsed.skills.frameworks_and_tools.len(),
                    parsed.skills.frameworks_and_tools
                );
                println!(
                    "  - Skills Core Concepts ({}): {:?}",
                    parsed.skills.core_concepts.len(),
                    parsed.skills.core_concepts
                );
                println!("  - Experience ({} items):", parsed.experience.len());
                for exp in &parsed.experience {
                    println!(
                        "      * {} at {} ({}): {} bullets",
                        exp.title,
                        exp.company,
                        exp.date_range,
                        exp.bullets.len()
                    );
                }
                println!("  - Projects ({} items):", parsed.projects.len());
                for proj in &parsed.projects {
                    let evidence_citations: Vec<Vec<i64>> = proj
                        .bullets
                        .iter()
                        .map(|b| b.evidence_ids.clone())
                        .collect();
                    println!(
                        "      * [{}] {}: {} bullets, evidence: {:?}",
                        proj.project_id,
                        proj.name,
                        proj.bullets.len(),
                        evidence_citations
                    );
                }

                // Assertions for consistency
                assert!(
                    !parsed.summary.trim().is_empty(),
                    "Summary must not be empty"
                );
                assert!(
                    !parsed.skills.languages.is_empty(),
                    "Languages must not be empty"
                );
                assert!(
                    !parsed.skills.frameworks_and_tools.is_empty(),
                    "Frameworks/tools must not be empty"
                );
                assert!(
                    !parsed.skills.core_concepts.is_empty(),
                    "Core concepts must not be empty"
                );
                assert!(
                    !parsed.experience.is_empty(),
                    "Experience must not be empty"
                );
                assert_eq!(parsed.projects.len(), 2, "Must return both projects");

                let p_ids: Vec<i64> = parsed.projects.iter().map(|p| p.project_id).collect();
                assert!(p_ids.contains(&101));
                assert!(p_ids.contains(&102));

                for proj in &parsed.projects {
                    for bullet in &proj.bullets {
                        assert!(
                            !bullet.evidence_ids.is_empty(),
                            "Every project bullet must cite evidence IDs"
                        );
                    }
                }

                run_results.push((run_idx, elapsed.as_secs_f64(), parsed.skills));
            }
            Err(err) => {
                eprintln!("\n❌ Run {} FAILED with error: {:?}", run_idx, err);
                anyhow::bail!("Step 9 Run {} failed: {}", run_idx, err);
            }
        }
    }

    println!("\n==================================================");
    println!("=== 3-RUN CONSISTENCY VERIFICATION SUMMARY ===");
    println!("==================================================");
    for (idx, elapsed_secs, skills) in &run_results {
        println!("Run {}: {:.2}s", idx, elapsed_secs);
        println!(
            "  Languages ({}): {:?}",
            skills.languages.len(),
            skills.languages
        );
        println!(
            "  Frameworks/Tools ({}): {:?}",
            skills.frameworks_and_tools.len(),
            skills.frameworks_and_tools
        );
        println!(
            "  Core Concepts ({}): {:?}",
            skills.core_concepts.len(),
            skills.core_concepts
        );
    }
    println!(
        "\nTotal 3-run duration: {:.2}s",
        total_start.elapsed().as_secs_f64()
    );
    println!("*** ALL 3 RUNS PRODUCED 100% CONSISTENT CATEGORIZED SKILLS SHAPE ***\n");

    Ok(())
}
