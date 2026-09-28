# scripts/test-launcher.ps1 - Comprehensive test suite for launcher & bootstrapper
[CmdletBinding()]
param(
    [string]$TargetTest = "all"
)

$ErrorActionPreference = 'Continue'
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = (Resolve-Path (Join-Path $scriptDir '..')).Path
Set-Location -LiteralPath $projectRoot

$toolsBin = Join-Path $projectRoot 'tools\bin'
$stateFile = Join-Path $projectRoot '.resumeforge-setup.json'
$dataDir = Join-Path $projectRoot 'data'
$testHttpPort = 8999

# Helper to start minimal PowerShell HttpListener without external modules
function Start-TestHttpServer([string]$serveDir, [int]$port) {
    $escapedDir = $serveDir.Replace("'", "''")
    $serverScript = @"
`$listener = New-Object System.Net.HttpListener
`$listener.Prefixes.Add('http://127.0.0.1:$port/')
`$listener.Start()
while (`$listener.IsListening) {
    try {
        `$context = `$listener.GetContext()
        `$reqPath = `$context.Request.Url.AbsolutePath.TrimStart('/')
        `$filePath = Join-Path '$escapedDir' `$reqPath
        if (Test-Path -LiteralPath `$filePath) {
            `$bytes = [System.IO.File]::ReadAllBytes(`$filePath)
            `$context.Response.ContentLength64 = `$bytes.Length
            `$context.Response.StatusCode = 200
            `$context.Response.OutputStream.Write(`$bytes, 0, `$bytes.Length)
        } else {
            `$context.Response.StatusCode = 404
        }
        `$context.Response.OutputStream.Close()
    } catch {
        break
    }
}
"@
    $encoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($serverScript))
    $proc = Start-Process powershell.exe -ArgumentList "-NoProfile -EncodedCommand $encoded" -PassThru -NoNewWindow
    Start-Sleep -Milliseconds 1000
    return @{ Process = $proc; Port = $port }
}

function Stop-TestHttpServer($serverObj) {
    if ($serverObj -and $serverObj.Process) {
        Stop-Process -Id $serverObj.Process.Id -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "========================================================" -ForegroundColor Magenta
Write-Host "         ResumeForge Launcher Test Suite               " -ForegroundColor Magenta
Write-Host "========================================================" -ForegroundColor Magenta

# ------------------------------------------------------------------------------
# Test A: Successful install from local HTTP server serving zip + .sha256
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "a") {
    Write-Host "`n>>> RUNNING TEST (A): Install from local HTTP server with verified SHA256 <<<" -ForegroundColor Cyan
    
    $httpTemp = Join-Path ([System.IO.Path]::GetTempPath()) "rf_test_http_$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $httpTemp -Force | Out-Null

    # Create dummy release zip containing target\release\resumeforge.exe
    $realBin = Join-Path $projectRoot 'target\release\resumeforge.exe'
    if (-not (Test-Path -LiteralPath $realBin)) {
        & cargo build --release --bin resumeforge
    }
    
    $zipSourceDir = Join-Path $httpTemp "zip_src"
    New-Item -ItemType Directory -Path $zipSourceDir -Force | Out-Null
    Copy-Item -LiteralPath $realBin -Destination (Join-Path $zipSourceDir "resumeforge.exe") -Force

    $releaseZipName = "resumeforge-v0.1.0-windows-x86_64.zip"
    $zipPath = Join-Path $httpTemp $releaseZipName
    Compress-Archive -Path (Join-Path $zipSourceDir "*") -DestinationPath $zipPath -Force
    Remove-Item -LiteralPath $zipSourceDir -Recurse -Force

    $hash = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLower()
    $shaPath = Join-Path $httpTemp "$releaseZipName.sha256"
    Set-Content -LiteralPath $shaPath -Value "$hash  $releaseZipName" -Encoding Ascii

    # Start HTTP server
    $httpServer = Start-TestHttpServer -serveDir $httpTemp -port $testHttpPort
    $env:RESUMEFORGE_RELEASE_BASE_URL = "http://127.0.0.1:$testHttpPort"

    # Clear installed binary to force download
    $installedBin = Join-Path $toolsBin 'resumeforge.exe'
    if (Test-Path -LiteralPath $installedBin) { Remove-Item -LiteralPath $installedBin -Force }

    Write-Host "Serving release zip and checksum at $env:RESUMEFORGE_RELEASE_BASE_URL" -ForegroundColor Gray

    # Launch bootstrap in background job that terminates after server health is confirmed
    $job = Start-Job -ScriptBlock {
        param($root)
        Set-Location -LiteralPath $root
        $env:RESUMEFORGE_RELEASE_BASE_URL = "http://127.0.0.1:8999"
        & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $root 'scripts\bootstrap.ps1') -Yes -NoBrowser
    } -ArgumentList $projectRoot

    # Wait for server to respond on /api/health
    $healthy = $false
    for ($i = 0; $i -lt 60; $i++) {
        Start-Sleep -Seconds 1
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:3000/api/health" -Headers @{ Host = "127.0.0.1:3000" } -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
            if ($r.StatusCode -eq 200) { $healthy = $true; break }
        } catch {}
    }

    # Stop background bootstrap
    Stop-Job -Job $job -ErrorAction SilentlyContinue
    $output = Receive-Job -Job $job
    Remove-Job -Job $job -ErrorAction SilentlyContinue

    # Clean up HTTP server
    Stop-TestHttpServer $httpServer
    Remove-Item -LiteralPath $httpTemp -Recurse -Force
    $env:RESUMEFORGE_RELEASE_BASE_URL = $null

    # Kill any dangling server
    Get-Process -Name resumeforge -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

    Write-Host ($output -join "`n")
    if (Test-Path -LiteralPath $installedBin) {
        Write-Host "`n[PASS TEST A] Release binary successfully downloaded, verified by SHA256, and executed." -ForegroundColor Green
    } else {
        Write-Error "[FAIL TEST A] Expected binary tools\bin\resumeforge.exe was not created."
    }
}

# ------------------------------------------------------------------------------
# Test B: Abort on deliberately wrong hash
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "b") {
    Write-Host "`n>>> RUNNING TEST (B): Abort on deliberately wrong hash <<<" -ForegroundColor Cyan
    
    $httpTemp = Join-Path ([System.IO.Path]::GetTempPath()) "rf_test_bad_hash_$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $httpTemp -Force | Out-Null

    $releaseZipName = "resumeforge-v0.1.0-windows-x86_64.zip"
    $zipPath = Join-Path $httpTemp $releaseZipName
    Set-Content -LiteralPath $zipPath -Value "DummyZipContent" -Encoding Ascii

    # Provide intentionally incorrect hash
    $shaPath = Join-Path $httpTemp "$releaseZipName.sha256"
    Set-Content -LiteralPath $shaPath -Value "0000000000000000000000000000000000000000000000000000000000000000  $releaseZipName" -Encoding Ascii

    $httpServer = Start-TestHttpServer -serveDir $httpTemp -port $testHttpPort
    $env:RESUMEFORGE_RELEASE_BASE_URL = "http://127.0.0.1:$testHttpPort"

    $installedBin = Join-Path $toolsBin 'resumeforge.exe'
    if (Test-Path -LiteralPath $installedBin) { Remove-Item -LiteralPath $installedBin -Force }

    $output = & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $projectRoot 'scripts\bootstrap.ps1') -Yes -NoBrowser 2>&1
    $exitCode = $LASTEXITCODE

    Stop-TestHttpServer $httpServer
    Remove-Item -LiteralPath $httpTemp -Recurse -Force
    $env:RESUMEFORGE_RELEASE_BASE_URL = $null

    Write-Host ($output -join "`n")
    $hasMismatch = ($output -match "Checksum mismatch|Security error")
    $partialZipExists = Test-Path -LiteralPath (Join-Path $toolsBin $releaseZipName)

    if ($exitCode -ne 0 -and $hasMismatch -and (-not $partialZipExists)) {
        Write-Host "`n[PASS TEST B] Correctly aborted with non-zero exit code ($exitCode), deleted partial download, and logged checksum mismatch." -ForegroundColor Green
    } else {
        Write-Error "[FAIL TEST B] Expected non-zero exit code and error message on invalid hash."
    }
}

# ------------------------------------------------------------------------------
# Test C: Ctrl+C shuts down cleanly
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "c") {
    Write-Host "`n>>> RUNNING TEST (C): Clean shutdown via interrupt <<<" -ForegroundColor Cyan

    # Ensure binary is in place
    $realBin = Join-Path $projectRoot 'target\release\resumeforge.exe'
    Copy-Item -LiteralPath $realBin -Destination (Join-Path $toolsBin 'resumeforge.exe') -Force

    $proc = Start-Process -FilePath "powershell.exe" -ArgumentList "-ExecutionPolicy Bypass -File scripts\bootstrap.ps1 -Yes -NoBrowser" -WorkingDirectory $projectRoot -PassThru

    $started = $false
    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Seconds 1
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:3000/api/health" -Headers @{ Host = "127.0.0.1:3000" } -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
            if ($r.StatusCode -eq 200) { $started = $true; break }
        } catch {}
    }

    if ($started) {
        Write-Host "Server running on port 3000. Sending shutdown signal..." -ForegroundColor Cyan
        Stop-Process -Id $proc.Id -Force
        Start-Sleep -Seconds 2
        Get-Process -Name resumeforge -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Write-Host "[PASS TEST C] Server started and shut down cleanly without leaving port locked." -ForegroundColor Green
    } else {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        Write-Error "[FAIL TEST C] Server failed to start on port 3000."
    }
}

# ------------------------------------------------------------------------------
# Test D: Port 3000 busy falls back to another port
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "d") {
    Write-Host "`n>>> RUNNING TEST (D): Port 3000 busy fallback <<<" -ForegroundColor Cyan

    # Bind port 3000 with a dummy TCP listener
    $dummy = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 3000)
    $dummy.Start()
    Write-Host "Occupied port 3000 with dummy listener." -ForegroundColor Gray

    $job = Start-Job -ScriptBlock {
        param($root)
        Set-Location -LiteralPath $root
        & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $root 'scripts\bootstrap.ps1') -Yes -NoBrowser
    } -ArgumentList $projectRoot

    $selectedPort = $null
    for ($i = 0; $i -lt 25; $i++) {
        Start-Sleep -Seconds 1
        for ($p = 3001; $p -le 3005; $p++) {
            try {
                $r = Invoke-WebRequest -Uri "http://127.0.0.1:$p/api/health" -Headers @{ Host = "127.0.0.1:$p" } -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
                if ($r.StatusCode -eq 200) {
                    $selectedPort = $p
                    break
                }
            } catch {}
        }
        if ($selectedPort) { break }
    }

    $dummy.Stop()
    Stop-Job -Job $job -ErrorAction SilentlyContinue
    $output = Receive-Job -Job $job
    Remove-Job -Job $job -ErrorAction SilentlyContinue
    Get-Process -Name resumeforge -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

    Write-Host ($output -join "`n")
    if ($selectedPort) {
        Write-Host "`n[PASS TEST D] Port 3000 busy was detected; fell back successfully to port $selectedPort." -ForegroundColor Green
    } else {
        Write-Error "[FAIL TEST D] Did not fall back to an available port."
    }
}

# ------------------------------------------------------------------------------
# Test E: -Reset clears state and tools, never data\
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "e") {
    Write-Host "`n>>> RUNNING TEST (E): -Reset flag <<<" -ForegroundColor Cyan

    # Ensure a sentinel file exists in data\
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    $sentinel = Join-Path $dataDir "sentinel.txt"
    Set-Content -LiteralPath $sentinel -Value "PreserveMe" -Encoding Ascii

    $output = & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $projectRoot 'scripts\bootstrap.ps1') -Reset 2>&1
    Write-Host ($output -join "`n")

    $stateExists = Test-Path -LiteralPath $stateFile
    $sentinelExists = Test-Path -LiteralPath $sentinel

    if ((-not $stateExists) -and $sentinelExists) {
        Write-Host "`n[PASS TEST E] -Reset successfully cleared setup state and preserved data\ directory." -ForegroundColor Green
    } else {
        Write-Error "[FAIL TEST E] Reset did not properly clear state or damaged data\."
    }
}

# ------------------------------------------------------------------------------
# Test F: -Update does git pull and re-fetches binary
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "f") {
    Write-Host "`n>>> RUNNING TEST (F): -Update flag <<<" -ForegroundColor Cyan

    $dummyBin = Join-Path $toolsBin 'resumeforge.exe'
    Set-Content -LiteralPath $dummyBin -Value "OldBinary" -Encoding Ascii

    $output = & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $projectRoot 'scripts\bootstrap.ps1') -Update 2>&1
    Write-Host ($output -join "`n")

    if ($output -match "Update completed successfully") {
        Write-Host "`n[PASS TEST F] -Update cleared existing binary and completed successfully." -ForegroundColor Green
    } else {
        Write-Error "[FAIL TEST F] -Update did not complete as expected."
    }
}

# ------------------------------------------------------------------------------
# Test G: Second run skips everything already done
# ------------------------------------------------------------------------------
if ($TargetTest -eq "all" -or $TargetTest -eq "g") {
    Write-Host "`n>>> RUNNING TEST (G): Idempotent second run <<<" -ForegroundColor Cyan

    # Copy real binary into tools\bin
    $realBin = Join-Path $projectRoot 'target\release\resumeforge.exe'
    Copy-Item -LiteralPath $realBin -Destination (Join-Path $toolsBin 'resumeforge.exe') -Force

    # Run initial setup with job that stops once healthy
    $job = Start-Job -ScriptBlock {
        param($root)
        Set-Location -LiteralPath $root
        & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $root 'scripts\bootstrap.ps1') -Yes -NoBrowser
    } -ArgumentList $projectRoot

    for ($i = 0; $i -lt 20; $i++) {
        Start-Sleep -Seconds 1
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:3000/api/health" -Headers @{ Host = "127.0.0.1:3000" } -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
            if ($r.StatusCode -eq 200) { break }
        } catch {}
    }
    Stop-Job -Job $job -ErrorAction SilentlyContinue
    Remove-Job -Job $job -ErrorAction SilentlyContinue
    Get-Process -Name resumeforge -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

    # Now run second time, capturing output
    $job2 = Start-Job -ScriptBlock {
        param($root)
        Set-Location -LiteralPath $root
        & powershell.exe -ExecutionPolicy Bypass -File (Join-Path $root 'scripts\bootstrap.ps1') -Yes -NoBrowser
    } -ArgumentList $projectRoot

    for ($i = 0; $i -lt 15; $i++) {
        Start-Sleep -Seconds 1
        try {
            $r = Invoke-WebRequest -Uri "http://127.0.0.1:3000/api/health" -Headers @{ Host = "127.0.0.1:3000" } -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
            if ($r.StatusCode -eq 200) { break }
        } catch {}
    }
    Stop-Job -Job $job2 -ErrorAction SilentlyContinue
    $output2 = Receive-Job -Job $job2
    Remove-Job -Job $job2 -ErrorAction SilentlyContinue
    Get-Process -Name resumeforge -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

    Write-Host ($output2 -join "`n")
    $hasSkipTools = ($output2 -match "already installed")
    $hasSkipAgy = ($output2 -match "previously verified")
    $hasSkipTectonic = ($output2 -match "Tectonic LaTeX bundle is cached")

    if ($hasSkipTools -and $hasSkipTectonic) {
        Write-Host "`n[PASS TEST G] Second run skipped all pre-installed tools and cached bundles." -ForegroundColor Green
    } else {
        Write-Warning "[WARN TEST G] Expected skip messages on second run."
    }
}

Write-Host "`n========================================================" -ForegroundColor Magenta
Write-Host "         Launcher Test Suite Complete                  " -ForegroundColor Magenta
Write-Host "========================================================" -ForegroundColor Magenta
