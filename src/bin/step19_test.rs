use resumeforge::{config::Config, db, models::github::GitHubRepository, services::github_indexer};
use std::path::PathBuf;
use tokio::process::Command;

async fn git(args: &[&str]) -> anyhow::Result<()> {
    let result = Command::new("git").args(args).output().await?;
    if !result.status.success() {
        anyhow::bail!("git failed: {}", String::from_utf8_lossy(&result.stderr));
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(format!("data/test_step19_{}", uuid::Uuid::new_v4()));
    let source = root.join("source");
    std::fs::create_dir_all(&source)?;
    let source_str = source.to_string_lossy().to_string();
    git(&["init", "--initial-branch=main", &source_str]).await?;
    std::fs::write(
        source.join("README.md"),
        "# Sample Queue\n- Implements a Rust Tokio message queue with SQLite persistence.\n",
    )?;
    std::fs::write(
        source.join("Cargo.toml"),
        "[package]\nname = \"sample-queue\"\nversion = \"0.1.0\"\n[dependencies]\ntokio = \"1\"\n",
    )?;
    std::fs::write(source.join(".env"), "API_KEY=supersecretvalue\n")?;
    git(&["-C", &source_str, "add", "."]).await?;
    git(&[
        "-C",
        &source_str,
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "-m",
        "first",
    ])
    .await?;

    let config = Config {
        host: "127.0.0.1".into(),
        port: 3000,
        data_dir: root.clone(),
        frontend_dir: PathBuf::from("frontend"),
    };
    let pool = db::init_db(&root.join("test.db")).await?;
    let repo = GitHubRepository {
        id: "local-fixture".into(),
        name: "sample-queue".into(),
        name_with_owner: "test/sample-queue".into(),
        description: Some("Queue fixture".into()),
        is_private: true,
        url: source_str.clone(),
        homepage_url: None,
        pushed_at: None,
    };
    assert!(github_indexer::sync_repository(&pool, &config, &repo).await?);
    assert!(!github_indexer::sync_repository(&pool, &config, &repo).await?);
    let indexed = db::projects::list(&pool).await?;
    assert_eq!(indexed.len(), 1);
    let evidence = db::projects::evidence_for_project(&pool, indexed[0].id).await?;
    assert!(!evidence.is_empty());
    assert!(evidence
        .iter()
        .all(|item| !item.claim.contains("supersecretvalue")));
    std::fs::write(source.join("README.md"), "# Sample Queue\n- Implements a Rust Tokio message queue with SQLite persistence.\n- Adds a durable retry feature.\n")?;
    git(&["-C", &source_str, "add", "README.md"]).await?;
    git(&[
        "-C",
        &source_str,
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "commit",
        "-m",
        "second",
    ])
    .await?;
    assert!(github_indexer::sync_repository(&pool, &config, &repo).await?);
    let updated = db::projects::evidence_for_project(&pool, indexed[0].id).await?;
    assert!(updated
        .iter()
        .any(|item| item.claim.contains("durable retry")));
    pool.close().await;
    for attempt in 0..20 {
        match std::fs::remove_dir_all(&root) {
            Ok(()) => break,
            Err(_) if attempt < 19 => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await
            }
            Err(error) => return Err(error.into()),
        }
    }
    println!(
        "Step 19 passed: clone, gitleaks scan, knowledge evidence, SHA skip, incremental sync"
    );
    Ok(())
}
