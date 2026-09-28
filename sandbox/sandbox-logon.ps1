# sandbox-logon.ps1 - Windows Sandbox automated clean test script
$ErrorActionPreference = 'Continue'
$logDir = "C:\resumeforge-logs"
if (-not (Test-Path -LiteralPath $logDir)) {
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
}
$logFile = Join-Path $logDir "sandbox-run-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"

function LogMsg($msg) {
    $line = "[$(Get-Date -Format 'HH:mm:ss')] $msg"
    Write-Host $line
    Add-Content -LiteralPath $logFile -Value $line
}

LogMsg "=== Windows Sandbox Clean Run Test Started ==="
LogMsg "OS: $([System.Environment]::OSVersion.VersionString)"

# Copy repository from read-only host mount to local writable directory
$src = "C:\resumeforge-src"
$dst = "C:\resumeforge"

LogMsg "Copying repo from $src to $dst (excluding target/tools caches)..."
New-Item -ItemType Directory -Path $dst -Force | Out-Null

Get-ChildItem -Path $src -Exclude "target", "tools", ".tmp", ".git" | ForEach-Object {
    Copy-Item -Path $_.FullName -Destination $dst -Recurse -Force
}

Set-Location -LiteralPath $dst

LogMsg "Executing start.cmd -Yes -NoBrowser in clean sandbox..."
cmd.exe /c "start.cmd -Yes -NoBrowser" *>> $logFile

LogMsg "Sandbox execution finished with exit code $LASTEXITCODE."
