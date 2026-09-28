$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$bundle = Join-Path $projectRoot 'dist\ResumeForge'
if (Test-Path -LiteralPath $bundle) { throw "Package already exists at $bundle. Move or rename it before packaging again." }

Push-Location -LiteralPath $projectRoot
try {
    for ($buildAttempt = 1; $buildAttempt -le 4; $buildAttempt++) {
        cargo build --release --bin resumeforge --jobs 1
        if ($LASTEXITCODE -eq 0) { break }
        if ($buildAttempt -eq 4) { throw 'Release build failed after four attempts.' }
        Write-Warning "Release build attempt $buildAttempt failed; retrying after a transient Windows file lock."
        Start-Sleep -Seconds 3
    }
    New-Item -ItemType Directory -Path (Join-Path $bundle 'data\templates\starter') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\resumeforge.exe') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $projectRoot 'frontend') -Destination $bundle -Recurse
    Copy-Item -LiteralPath (Join-Path $projectRoot 'data\templates\starter\classic.tex') -Destination (Join-Path $bundle 'data\templates\starter')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'data\templates\starter\modern.tex') -Destination (Join-Path $bundle 'data\templates\starter')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $projectRoot 'PROJECT_STATUS.md') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $projectRoot 'MASTER_BUILD_PROMPT.md') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $projectRoot 'HANDOVER_CODEX.md') -Destination $bundle
    @'
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
Write-Host 'ResumeForge is starting at http://127.0.0.1:3000'
$server = Start-Process -FilePath (Join-Path $PSScriptRoot 'resumeforge.exe') -WorkingDirectory $PSScriptRoot -NoNewWindow -PassThru
try {
    for ($attempt = 0; $attempt -lt 90; $attempt++) {
        if ($server.HasExited) { throw "ResumeForge exited with code $($server.ExitCode) before becoming ready." }
        try {
            $response = Invoke-WebRequest -Uri 'http://127.0.0.1:3000/api/health' -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200) { break }
        } catch { Start-Sleep -Seconds 1 }
    }
    if ($attempt -ge 90) { throw 'ResumeForge did not become ready within 90 seconds.' }
    Start-Process 'http://127.0.0.1:3000'
    Wait-Process -Id $server.Id
} finally {
    if (-not $server.HasExited) { Stop-Process -Id $server.Id }
}
'@ | Set-Content -LiteralPath (Join-Path $bundle 'run.ps1') -Encoding UTF8
    Write-Host "Package created at $bundle"
} finally {
    Pop-Location
}
