use resumeforge::{
    config::Config,
    db,
    models::github::GitHubRepository,
    services::github_indexer,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== GITLEAKS FAIL-CLOSED DRILL ===");

    // 1. Remove gitleaks from PATH
    let original_path = std::env::var("PATH").unwrap_or_default();
    let filtered_paths: Vec<_> = std::env::split_paths(&original_path)
        .filter(|p| {
            let s = p.to_string_lossy().to_lowercase();
            !s.contains("gitleaks")
        })
        .collect();
    let new_path = std::env::join_paths(filtered_paths)?;
    std::env::set_var("PATH", new_path);

    let test_dir = PathBuf::from(format!("data/test_drill_gitleaks_{}", uuid::Uuid::new_v4()));
    let repo_dir = test_dir.join("local_fixture_repo");
    std::fs::create_dir_all(&repo_dir)?;

    // Initialize fixture git repo
    let run_git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&repo_dir)
            .output()
            .expect("git command failed")
    };
    run_git(&["init"]);
    run_git(&["config", "user.name", "Test"]);
    run_git(&["config", "user.email", "test@example.com"]);
    std::fs::write(repo_dir.join("README.md"), "# Test Project\nFeatures async logging.")?;
    run_git(&["add", "README.md"]);
    run_git(&["commit", "-m", "initial commit"]);

    let db_path = test_dir.join("test.db");
    let pool = db::init_db(&db_path).await?;
    let config = Config {
        host: "127.0.0.1".into(),
        port: 3000,
        data_dir: test_dir.clone(),
        frontend_dir: PathBuf::from("frontend"),
    };

    let repo = GitHubRepository {
        id: "fixture_leak".into(),
        name: "test-leak-repo".into(),
        name_with_owner: "test/test-leak-repo".into(),
        description: Some("repo to test missing gitleaks".into()),
        is_private: false,
        url: repo_dir.to_string_lossy().to_string(),
        homepage_url: None,
        pushed_at: None,
    };

    // Attempt sync without gitleaks
    println!("Invoking sync_repository with gitleaks removed from PATH...");
    let sync_result = github_indexer::sync_repository(&pool, &config, &repo).await;

    println!("\n=== INDEXER RETURNED ERROR ===");
    match sync_result {
        Ok(_) => panic!("Expected sync to fail when gitleaks is missing!"),
        Err(err) => {
            println!("Error message:\n{}", err);
            assert!(
                err.to_string().contains("gitleaks is required before indexing repository content"),
                "Error did not cite gitleaks requirement: {}",
                err
            );
        }
    }

    println!("\n=== DATABASE PROOF: NO ROWS WRITTEN ===");
    let project_count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
        .fetch_one(&pool)
        .await?;
    let evidence_count: i64 = sqlx::query_scalar("SELECT count(*) FROM project_evidence")
        .fetch_one(&pool)
        .await?;

    println!("projects count:         {}", project_count);
    println!("project_evidence count: {}", evidence_count);

    assert_eq!(project_count, 0, "No projects should be written on scan failure");
    assert_eq!(evidence_count, 0, "No evidence should be written on scan failure");

    pool.close().await;
    let _ = std::fs::remove_dir_all(&test_dir);
    println!("\nDrill 4(b) passed: gitleaks requirement verified and failed closed with 0 rows written!");
    Ok(())
}
