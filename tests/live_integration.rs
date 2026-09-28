use resumeforge::services::pty_runner;

#[tokio::test]
#[ignore = "requires live Antigravity CLI installed and signed in"]
async fn test_live_antigravity_probe() -> anyhow::Result<()> {
    let outcome = pty_runner::run_agy_prompt("reply with the single word OK", 30).await?;
    assert_eq!(outcome.status, pty_runner::AgyOutcome::Success);
    assert_eq!(outcome.response.trim(), "OK");
    Ok(())
}
