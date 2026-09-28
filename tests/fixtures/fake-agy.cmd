@echo off
setlocal enabledelayedexpansion

REM tests/fixtures/fake-agy.cmd - Fake Antigravity CLI fixture for testing
REM Checks command-line arguments to determine whether to replay probe, generation, or error

set "ARGS=%*"

REM 1. Probe check ("reply with the single word OK")
echo !ARGS! | findstr /i "reply with the single word OK" >nul
if !ERRORLEVEL! equ 0 (
    echo {"event":"result","result":{"response":"OK","denied_actions":[]}}
    exit /b 0
)

REM 2. Forced timeout mode
if defined FAKE_AGY_TIMEOUT (
    timeout /t 10 /nobreak >nul
    exit /b 0
)

REM 3. Forced tool denied mode
if defined FAKE_AGY_TOOL_DENIED (
    echo {"event":"result","result":{"response":"","denied_actions":[{"action":"run_command","display_name":"Run Bash Command"}]}}
    exit /b 0
)

REM 4. Forced invalid JSON mode
if defined FAKE_AGY_INVALID_JSON (
    echo {"event":"result","result":{"response":"This is plain text not valid JSON","denied_actions":[]}}
    exit /b 0
)

REM 5. Standard Content Generation Replay
REM Output realistic stream-json event line
echo {"event":"step_update","step_update":{"text_delta":"Analyzing job description...\n"}}
echo {"event":"step_update","step_update":{"text_delta":"Formulating grounded experience and project bullets...\n"}}

REM Read response payload from fixture file if present, else output default valid resume JSON
if exist "%~dp0response.json" (
    powershell.exe -NoProfile -Command "$c = Get-Content '%~dp0response.json' -Raw; $jsonObj = @{ event = 'result'; result = @{ response = $c; denied_actions = @() } } | ConvertTo-Json -Compress; Write-Output $jsonObj"
) else (
    echo {"event":"result","result":{"response":"{\"summary\":\"Software engineer specializing in Rust, TypeScript, and distributed systems.\",\"skills\":{\"languages\":[\"Rust\",\"TypeScript\",\"Python\"],\"frameworks_and_tools\":[\"React\",\"Node.js\",\"Docker\",\"Git\"],\"core_concepts\":[\"API Design\",\"Distributed Systems\",\"Testing\"]},\"experience\":[{\"company\":\"Acme Corp\",\"title\":\"Senior Software Engineer\",\"date_range\":\"2021 - Present\",\"location\":\"Remote\",\"bullets\":[\"Architected scalable microservices supporting 50000 daily active users with 99.9% uptime.\",\"Refactored legacy backend reducing API latency by 40%.\"]}],\"projects\":[{\"name\":\"ResumeForge\",\"technologies\":[\"Rust\",\"SQLite\",\"LaTeX\"],\"bullets\":[\"Built local-first resume compiler with deterministic truth verification (evidence #ev-001).\",\"Engineered real-time rendering loop keeping PDF layout strictly to 1 page (evidence #ev-002).\"]}]}","denied_actions":[]}}
)

exit /b 0
