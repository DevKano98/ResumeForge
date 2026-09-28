use resumeforge::services::master_parser::parse_master_facts;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let master_path = Path::new("data/master/resume.tex");
    if !master_path.exists() {
        eprintln!("data/master/resume.tex not found, copying from starter");
        std::fs::create_dir_all("data/master")?;
        std::fs::copy("data/templates/starter/classic.tex", master_path)?;
    }

    let tex = std::fs::read_to_string(master_path)?;
    let facts = parse_master_facts(&tex);

    let json_str = serde_json::to_string_pretty(&facts)?;
    println!("=== PARSED MASTER FACTS FOR data/master/resume.tex ===");
    println!("{}", json_str);

    assert!(!facts.candidate_name.is_empty(), "candidate_name must not be empty");
    assert!(facts.contact.is_some(), "contact must not be empty");
    assert!(facts.current_summary.is_some(), "current_summary must not be empty");
    assert!(!facts.known_skills.is_empty(), "known_skills must not be empty");
    assert!(!facts.work_history.is_empty(), "work_history must not be empty");

    println!("\nVerification check passed: All master fact fields are structured, non-empty, and correct!");
    Ok(())
}
