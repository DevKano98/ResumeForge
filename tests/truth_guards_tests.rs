use resumeforge::models::generation::{
    GeneratedProjectBullet, GeneratedProjectItem, GeneratedResumeContent,
    GeneratedSkills, MasterFacts, ProjectEvidenceItem, RankedProjectInput, WorkHistoryFact,
};
use resumeforge::services::resume_generator::validate_evidence;

fn sample_master_facts() -> MasterFacts {
    MasterFacts {
        candidate_name: "Jane Developer".to_string(),
        contact: Some("jane@example.com | 555-0199".to_string()),
        current_summary: Some("Systems engineer with 10 years experience in distributed storage.".to_string()),
        education: vec![],
        known_skills: vec!["Rust".to_string(), "Go".to_string(), "PostgreSQL".to_string(), "Linux".to_string()],
        work_history: vec![WorkHistoryFact {
            title: "Senior Systems Engineer".to_string(),
            company: "ScaleCorp".to_string(),
            date_range: "2020 -- 2024".to_string(),
            location: Some("San Francisco".to_string()),
            highlights: vec![
                "Architected async engine handling 50,000 req/sec with 35% memory savings.".to_string(),
            ],
        }],
    }
}

fn sample_ranked_projects() -> Vec<RankedProjectInput> {
    vec![RankedProjectInput {
        project_id: 101,
        name: "StorageEngine".to_string(),
        summary: "Embedded storage engine".to_string(),
        technologies: vec!["Rust".to_string(), "RocksDB".to_string()],
        evidence: vec![ProjectEvidenceItem {
            id: 501,
            claim: "Engineered LSM-tree write path achieving 100,000 IOPS on NVMe".to_string(),
            source_file: "src/lsm.rs".to_string(),
            line_start: Some(10),
            line_end: Some(50),
        }],
    }]
}

#[test]
fn test_project_guard_rejects_ungrounded_metric() {
    let master = sample_master_facts();
    let ranked = sample_ranked_projects();

    let mut content = GeneratedResumeContent {
        summary: "Systems engineer with 10 years experience in distributed storage.".to_string(),
        skills: GeneratedSkills {
            languages: vec!["Rust".to_string()],
            frameworks_and_tools: vec![],
            core_concepts: vec![],
        },
        experience: vec![],
        projects: vec![GeneratedProjectItem {
            project_id: 101,
            name: "StorageEngine".to_string(),
            bullets: vec![
                // 100,000 is present in evidence 501 -> should be accepted
                GeneratedProjectBullet {
                    text: "Engineered LSM-tree write path achieving 100,000 IOPS on NVMe".to_string(),
                    evidence_ids: vec![501],
                },
                // 99.99% is NOT present in evidence -> should be rejected!
                GeneratedProjectBullet {
                    text: "Maintained 99.99% uptime across production clusters".to_string(),
                    evidence_ids: vec![501],
                },
            ],
        }],
        achievements: vec![],
    };

    let result = validate_evidence(&mut content, &ranked, &master, &[]);
    let rejected = result["rejected"].as_array().expect("rejected list present");

    assert!(
        rejected.iter().any(|r| {
            r.get("bullet").and_then(|b| b.as_str()).is_some_and(|b| b.contains("99.99%"))
                || r.get("reason").and_then(|b| b.as_str()).is_some_and(|b| b.contains("99.99%"))
                || r.get("metric").and_then(|b| b.as_str()).is_some_and(|b| b.contains("99.99%"))
        }),
        "Ungrounded metric 99.99% must be recorded in rejected list: {:?}",
        rejected
    );

    // Ensure rejected bullet was removed from content
    assert_eq!(content.projects[0].bullets.len(), 1);
    assert!(content.projects[0].bullets[0].text.contains("100,000 IOPS"));
}

#[test]
fn test_skills_guard_removes_unauthorized_skills() {
    let master = sample_master_facts();
    let ranked = sample_ranked_projects();

    let mut content = GeneratedResumeContent {
        summary: "Systems engineer with 10 years experience in distributed storage.".to_string(),
        skills: GeneratedSkills {
            // "Rust" and "Go" are in master; "Kubernetes" and "Haskell" are in neither
            languages: vec!["Rust".to_string(), "Go".to_string(), "Haskell".to_string()],
            frameworks_and_tools: vec!["Kubernetes".to_string(), "RocksDB".to_string()],
            core_concepts: vec!["Linux".to_string()],
        },
        experience: vec![],
        projects: vec![],
        achievements: vec![],
    };

    let result = validate_evidence(&mut content, &ranked, &master, &[]);
    let rejected = result["rejected"].as_array().expect("rejected list present");

    assert!(
        rejected.iter().any(|r| r["item"].as_str().unwrap().contains("Haskell")),
        "Haskell must be rejected: {:?}",
        rejected
    );
    assert!(
        rejected.iter().any(|r| r["item"].as_str().unwrap().contains("Kubernetes")),
        "Kubernetes must be rejected: {:?}",
        rejected
    );

    // Confirmed removed from content
    assert!(!content.skills.languages.contains(&"Haskell".to_string()));
    assert!(!content.skills.frameworks_and_tools.contains(&"Kubernetes".to_string()));
    assert!(content.skills.languages.contains(&"Rust".to_string()));
    assert!(content.skills.frameworks_and_tools.contains(&"RocksDB".to_string()));
}
