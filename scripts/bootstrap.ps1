# scripts/bootstrap.ps1 - Single-command zero-config launcher for ResumeForge
[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$Reset,
    [switch]$Update,
    [switch]$FromSource,
    [switch]$InstallAgy,
    [switch]$WipeData,
    [switch]$NoBrowser
)

$ErrorActionPreference = 'Stop'

# ==============================================================================
# 1. Centralized Release Configuration
# ==============================================================================
$RELEASE_CONFIG = @{
    Repo            = "resumeforge/resumeforge"
    Tag             = "v0.1.0"
    AssetZipName    = "resumeforge-v0.1.0-windows-x86_64.zip"
    AssetSha256Name = "resumeforge-v0.1.0-windows-x86_64.zip.sha256"
    BaseUrl         = if ($env:RESUMEFORGE_RELEASE_BASE_URL) {
                          $env:RESUMEFORGE_RELEASE_BASE_URL.TrimEnd('/')
                      } else {
                          "https://github.com/resumeforge/resumeforge/releases/download/v0.1.0"
                      }
}

# ==============================================================================
# 2. Project Directory Resolution & Process PATH
# ==============================================================================
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = (Resolve-Path (Join-Path $scriptDir '..')).Path
Set-Location -LiteralPath $projectRoot

$stateFile = Join-Path $projectRoot '.resumeforge-setup.json'
$toolsDir = Join-Path $projectRoot 'tools'
$toolsBin = Join-Path $toolsDir 'bin'
$toolsTemp = Join-Path $toolsDir 'temp'
$dataDir = Join-Path $projectRoot 'data'

# Ensure portable tools/bin exists and is PREPENDED to the current process PATH
if (-not (Test-Path -LiteralPath $toolsBin)) {
    New-Item -ItemType Directory -Path $toolsBin -Force | Out-Null
}
$env:PATH = "$toolsBin;$env:PATH"

# ==============================================================================
# 3. State Management Helpers
# ==============================================================================
function Get-SetupState {
    if (Test-Path -LiteralPath $stateFile) {
        try {
            return Get-Content -LiteralPath $stateFile -Raw -Encoding UTF8 | ConvertFrom-Json
        } catch {
            return $null
        }
    }
    return $null
}

function Save-SetupState($stateObj) {
    $stateObj.last_updated = (Get-Date).ToString("o")
    $json = $stateObj | ConvertTo-Json -Depth 5
    Set-Content -LiteralPath $stateFile -Value $json -Encoding UTF8
}

# ==============================================================================
# 4. Command Flag Handlers: -Reset, -WipeData, -Update
# ==============================================================================
if ($Reset) {
    Write-Host "[Reset] Clearing setup state and local tools..." -ForegroundColor Yellow
    if (Test-Path -LiteralPath $stateFile) { Remove-Item -LiteralPath $stateFile -Force }
    if (Test-Path -LiteralPath $toolsDir) { Remove-Item -LiteralPath $toolsDir -Recurse -Force }
    New-Item -ItemType Directory -Path $toolsBin -Force | Out-Null
    Write-Host "[Reset] Setup state and tools cleared. (data\ was preserved)" -ForegroundColor Green
    return
}

if ($WipeData) {
    Write-Host "`n[!] WARNING: -WipeData will delete all database records, custom templates, and generated resumes in data\" -ForegroundColor Red
    $confirm = Read-Host "Type 'DELETE' to confirm wiping data"
    if ($confirm -eq 'DELETE') {
        if (Test-Path -LiteralPath $dataDir) {
            Remove-Item -LiteralPath $dataDir -Recurse -Force
        }
        Write-Host "[WipeData] Data directory deleted." -ForegroundColor Green
    } else {
        Write-Host "[WipeData] Cancelled. Confirmation input did not match 'DELETE'." -ForegroundColor Yellow
    }
    return
}

if ($Update) {
    Write-Host "[Update] Checking for git updates and re-fetching binary..." -ForegroundColor Cyan
    if (Test-Path -LiteralPath (Join-Path $projectRoot '.git')) {
        $remotes = (& git remote 2>$null)
        if ($remotes) {
            $env:GIT_TERMINAL_PROMPT = "0"
            try {
                & git -c core.askPass= pull --ff-only 2>&1 | Out-Null
            } catch {
                Write-Warning "git pull --ff-only encountered an issue: $_"
            }
        } else {
            Write-Host "No git remotes configured; skipping git pull." -ForegroundColor Gray
        }
    }
    $downloadedBin = Join-Path $toolsBin 'resumeforge.exe'
    if (Test-Path -LiteralPath $downloadedBin) {
        Remove-Item -LiteralPath $downloadedBin -Force
    }
    $st = Get-SetupState
    if ($st) {
        $st.binary_ready = $false
        Save-SetupState $st
    }
    $localReleaseBin = Join-Path $projectRoot 'target\release\resumeforge.exe'
    if (Test-Path -LiteralPath $localReleaseBin) {
        Copy-Item -LiteralPath $localReleaseBin -Destination $downloadedBin -Force
        Write-Host "[Update] Binary refreshed to version $(& $downloadedBin --version)." -ForegroundColor Green
    }
    Write-Host "[Update] Update completed successfully." -ForegroundColor Green
    return
}

$state = Get-SetupState
if (-not $state) {
    $state = [PSCustomObject]@{
        version = 1
        tools = [PSCustomObject]@{}
        binary_ready = $false
        binary_source = "none"
        binary_version = $null
        tectonic_warmed = $false
        probe_result = $null
        probe_time = $null
        last_updated = $null
    }
}

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "           ResumeForge Windows Bootstrapper             " -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

# ==============================================================================
# 5. Portable Tools Installation (gh, gitleaks, tectonic)
# ==============================================================================
$toolDefs = @(
    @{
        Name = "gh"
        Version = "2.101.0"
        Exe = "gh.exe"
        ZipName = "gh_2.101.0_windows_amd64.zip"
        Url = "https://github.com/cli/cli/releases/download/v2.101.0/gh_2.101.0_windows_amd64.zip"
        Sha256 = "bc6c814367b193cd8e713611d61e36013c0ef843b8f516458fe3eda039192794"
        SubPath = "gh_2.101.0_windows_amd64\bin\gh.exe"
    },
    @{
        Name = "gitleaks"
        Version = "8.30.1"
        Exe = "gitleaks.exe"
        ZipName = "gitleaks_8.30.1_windows_x64.zip"
        Url = "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_windows_x64.zip"
        Sha256 = "d29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e"
        SubPath = "gitleaks.exe"
    },
    @{
        Name = "tectonic"
        Version = "0.17.0"
        Exe = "tectonic.exe"
        ZipName = "tectonic-0.17.0-x86_64-pc-windows-msvc.zip"
        Url = "https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%400.17.0/tectonic-0.17.0-x86_64-pc-windows-msvc.zip"
        Sha256 = "f61ce51f0b0ade1015b7de7ef368541c5424e9756ecbd0d7af97d6d48030845f"
        SubPath = "tectonic.exe"
    }
)

$missingTools = @()
foreach ($tool in $toolDefs) {
    $exePath = Join-Path $toolsBin $tool.Exe
    $isInstalled = $false

    if ((Test-Path -LiteralPath $exePath) -and $state.tools -and ($state.tools.PSObject.Properties[$tool.Name]) -and ($state.tools.$($tool.Name) -eq $tool.Version)) {
        $isInstalled = $true
    } elseif (Test-Path -LiteralPath $exePath) {
        $isInstalled = $true
    }

    if (-not $isInstalled) {
        $missingTools += $tool
    } else {
        Write-Host " [+] $($tool.Name) ($($tool.Version)) is already installed." -ForegroundColor Green
    }
}

if ($missingTools.Count -gt 0) {
    Write-Host "`nRequired portable command-line tools need to be installed:" -ForegroundColor Yellow
    Write-Host "Target directory: $toolsBin (local to repository, child process PATH only)`n"
    foreach ($m in $missingTools) {
        Write-Host "  - $($m.Name) ($($m.Version))"
        Write-Host "    Download: $($m.Url)"
        Write-Host "    SHA256:   $($m.Sha256)"
    }
    Write-Host ""

    if (-not $Yes) {
        $confirm = Read-Host "Proceed with downloading and installing these portable tools? [Y/n]"
        if ($confirm -and $confirm.Trim().ToLower() -eq 'n') {
            Write-Error "Setup cancelled by user."
            exit 1
        }
    }

    if (-not (Test-Path -LiteralPath $toolsTemp)) {
        New-Item -ItemType Directory -Path $toolsTemp -Force | Out-Null
    }

    foreach ($tool in $missingTools) {
        Write-Host "Downloading $($tool.Name) $($tool.Version)..." -ForegroundColor Cyan
        $zipDest = Join-Path $toolsTemp $tool.ZipName

        (New-Object System.Net.WebClient).DownloadFile($tool.Url, $zipDest)

        Write-Host "Verifying SHA256 checksum for $($tool.ZipName)..." -ForegroundColor Cyan
        $actualHash = (Get-FileHash -Path $zipDest -Algorithm SHA256).Hash.ToLower()
        if ($actualHash -ne $tool.Sha256.ToLower()) {
            Remove-Item -LiteralPath $zipDest -Force
            throw "Security error: Checksum mismatch for $($tool.ZipName)! Expected $($tool.Sha256) but got $actualHash."
        }
        Write-Host "Checksum verified OK." -ForegroundColor Green

        $extractDir = Join-Path $toolsTemp ("ext_" + $tool.Name)
        if (Test-Path -LiteralPath $extractDir) { Remove-Item -LiteralPath $extractDir -Recurse -Force }
        Expand-Archive -LiteralPath $zipDest -DestinationPath $extractDir -Force

        $extractedExe = Join-Path $extractDir $tool.SubPath
        if (-not (Test-Path -LiteralPath $extractedExe)) {
            $found = Get-ChildItem -Path $extractDir -Filter $tool.Exe -Recurse | Select-Object -First 1
            if ($found) { $extractedExe = $found.FullName }
        }

        if (-not (Test-Path -LiteralPath $extractedExe)) {
            throw "Could not locate $($tool.Exe) inside extracted archive $($tool.ZipName)."
        }

        Copy-Item -LiteralPath $extractedExe -Destination (Join-Path $toolsBin $tool.Exe) -Force
        Remove-Item -LiteralPath $zipDest -Force
        Remove-Item -LiteralPath $extractDir -Recurse -Force

        if (-not $state.tools) {
            $state.tools = [PSCustomObject]@{}
        }
        $state.tools | Add-Member -MemberType NoteProperty -Name $tool.Name -Value $tool.Version -Force
        Save-SetupState $state

        Write-Host " [+] $($tool.Name) installed to tools\bin\$($tool.Exe)" -ForegroundColor Green
    }

    if (Test-Path -LiteralPath $toolsTemp) {
        Remove-Item -LiteralPath $toolsTemp -Recurse -Force
    }
}

# ==============================================================================
# 6. Antigravity CLI ('agy') Detection & Verification
# ==============================================================================
Write-Host "`nChecking Antigravity CLI ('agy')..." -ForegroundColor Cyan
$agyCmd = Get-Command agy -ErrorAction SilentlyContinue

if (-not $agyCmd) {
    Write-Host "`n[!] Google Antigravity CLI ('agy') is not detected on your system." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "OFFICIAL INSTALLATION INSTRUCTIONS (Google Antigravity Docs: https://antigravity.google/docs/cli):" -ForegroundColor White
    Write-Host "  Command to run in PowerShell:" -ForegroundColor White
    Write-Host "     irm https://antigravity.google/cli/install.ps1 | iex" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Each user signs in with their own Google account via browser:" -ForegroundColor White
    Write-Host "     agy" -ForegroundColor Green
    Write-Host "  ResumeForge runs locally and never stores or manages your Google credentials." -ForegroundColor Gray
    Write-Host ""

    if ($InstallAgy) {
        Write-Host "Running official Antigravity installer..." -ForegroundColor Cyan
        try {
            Invoke-Expression (Invoke-RestMethod "https://antigravity.google/cli/install.ps1")
        } catch {
            Write-Warning "Automated installer encountered an error: $_"
        }
    } else {
        if (-not $Yes) {
            Read-Host "Press Enter once you have installed and signed in to agy..."
        }
    }

    $agyCmd = Get-Command agy -ErrorAction SilentlyContinue
    if (-not $agyCmd) {
        throw "Antigravity CLI ('agy') not found in PATH. Please run 'irm https://antigravity.google/cli/install.ps1 | iex' and authenticate."
    }
}

Write-Host " [+] agy found at $($agyCmd.Source)" -ForegroundColor Green

# Initial probe during first setup only (cache result in state)
if (-not $state.probe_result) {
    Write-Host "Probing Antigravity model response (first setup only)..." -ForegroundColor Cyan
    try {
        $probeRes = & agy -p "reply with the single word OK" 2>&1 | Out-String
        if ($LASTEXITCODE -eq 0 -and ($probeRes -match "OK|ok")) {
            $state.probe_result = "OK"
            $state.probe_time = (Get-Date).ToString("o")
            Save-SetupState $state
            Write-Host " [+] Antigravity probe passed cleanly and cached in setup state." -ForegroundColor Green
        } else {
            Write-Warning "Antigravity probe warning (exit code $LASTEXITCODE): $probeRes"
        }
    } catch {
        Write-Warning "Antigravity probe returned an error: $_. Please verify 'agy' authentication."
    }
} else {
    Write-Host " [+] Antigravity probe previously verified ($($state.probe_result) at $($state.probe_time))." -ForegroundColor Green
}

# ==============================================================================
# 7. GitHub CLI Authentication
# ==============================================================================
Write-Host "`nChecking GitHub CLI authentication..." -ForegroundColor Cyan
try {
    $ghStatus = & gh auth status 2>&1 | Out-String
    $ghAuthenticated = ($LASTEXITCODE -eq 0)
} catch {
    $ghAuthenticated = $false
}

if (-not $ghAuthenticated) {
    if (-not $Yes) {
        Write-Host "[!] GitHub CLI is not authenticated." -ForegroundColor Yellow
        Write-Host "Starting interactive web login: gh auth login --web..." -ForegroundColor White
        Write-Host "Each user signs in with their own GitHub account. ResumeForge never stores your token." -ForegroundColor Gray
        & gh auth login --web -h github.com -p https -s repo,read:user
        & gh auth setup-git
        $ghStatus = & gh auth status 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "GitHub authentication incomplete. Private repositories will not be available until authenticated."
        } else {
            Write-Host " [+] GitHub CLI is authenticated." -ForegroundColor Green
        }
    } else {
        Write-Warning "GitHub CLI is not authenticated. Interactive login was skipped because -Yes was specified."
        Write-Host "You can authenticate later with 'gh auth login --web' to index private repositories." -ForegroundColor Gray
    }
} else {
    Write-Host " [+] GitHub CLI is authenticated." -ForegroundColor Green
}

# ==============================================================================
# 8. Tectonic LaTeX Warm-up
# ==============================================================================
$tectonicCacheDir = $null
if ($env:LOCALAPPDATA) {
    $tectonicCacheDir = Join-Path $env:LOCALAPPDATA 'TectonicProject\Tectonic\cache'
}
$isWarmed = ($state.tectonic_warmed -eq $true) -or ($tectonicCacheDir -and (Test-Path -LiteralPath $tectonicCacheDir))

if (-not $isWarmed) {
    Write-Host "`n[Tectonic LaTeX Engine Warm-up]" -ForegroundColor Cyan
    Write-Host "Downloading LaTeX packages (one-time setup, may take 1-2 minutes)..." -ForegroundColor Yellow

    $warmupDir = Join-Path ([System.IO.Path]::GetTempPath()) "tectonic-warmup-$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $warmupDir -Force | Out-Null
    $warmupTex = Join-Path $warmupDir "warmup.tex"
    Set-Content -LiteralPath $warmupTex -Value "\documentclass{article}\begin{document}ResumeForge Ready\end{document}" -Encoding UTF8

    try {
        & tectonic $warmupTex --outdir $warmupDir | Out-Null
        $warmupPdf = Join-Path $warmupDir "warmup.pdf"
        if (Test-Path -LiteralPath $warmupPdf) {
            $state.tectonic_warmed = $true
            Save-SetupState $state
            Write-Host " [+] Tectonic LaTeX bundle downloaded and cached successfully." -ForegroundColor Green
        } else {
            Write-Warning "Tectonic warmup completed without creating PDF."
        }
    } catch {
        Write-Warning "Tectonic warmup failed: $_"
    } finally {
        if (Test-Path -LiteralPath $warmupDir) { Remove-Item -LiteralPath $warmupDir -Recurse -Force }
    }
} else {
    Write-Host " [+] Tectonic LaTeX bundle is cached." -ForegroundColor Green
}

# ==============================================================================
# 9. ResumeForge Binary Resolution
# ==============================================================================
Write-Host "`nChecking ResumeForge binary..." -ForegroundColor Cyan
$serverExe = $null
$binarySource = "none"

# Resolution order:
# 1. tools\bin\resumeforge.exe (downloaded release binary)
# 2. -FromSource flag -> build with cargo
# 3. Download release zip and verify SHA256
# 4. Fallback to target\release\resumeforge.exe if present
$downloadedBin = Join-Path $toolsBin 'resumeforge.exe'
$targetReleaseBin = Join-Path $projectRoot 'target\release\resumeforge.exe'

if (Test-Path -LiteralPath $downloadedBin) {
    $serverExe = $downloadedBin
    $binarySource = "downloaded"
} elseif ($FromSource) {
    # Build from source explicitly requested
    Write-Host "Building ResumeForge from source with Cargo..." -ForegroundColor Cyan
    $cargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargoCmd) {
        Write-Host "[!] Rust toolchain not detected. Installing rustup..." -ForegroundColor Yellow
        $rustupInit = Join-Path $toolsBin "rustup-init.exe"
        (New-Object System.Net.WebClient).DownloadFile("https://win.rustup.rs/x86_64", $rustupInit)
        Start-Process -FilePath $rustupInit -ArgumentList "-y" -Wait
        Remove-Item -LiteralPath $rustupInit -Force
        if ($env:USERPROFILE) {
            $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
            $env:PATH = "$cargoBin;$env:PATH"
        }
        $cargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
        if (-not $cargoCmd) {
            throw "Rust installation completed, but cargo was not found in PATH."
        }
    }

    # Check for MSVC build tools
    $clCmd = Get-Command cl.exe -ErrorAction SilentlyContinue
    if (-not $clCmd) {
        Write-Warning "Visual Studio C++ Build Tools (MSVC) cl.exe not detected in PATH."
        Write-Host "If build fails, install Microsoft C++ Build Tools from:" -ForegroundColor Yellow
        Write-Host "https://visualstudio.microsoft.com/visual-cpp-build-tools/" -ForegroundColor White
    }

    Write-Host "Running cargo build --release --bin resumeforge..." -ForegroundColor Cyan
    & cargo build --release --bin resumeforge
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo build failed with exit code $LASTEXITCODE."
    }
    $serverExe = $targetReleaseBin
    $binarySource = "from_source"
} else {
    # Try downloading release zip
    Write-Host "Checking for release binary from $($RELEASE_CONFIG.BaseUrl)..." -ForegroundColor Cyan
    $zipUrl = "$($RELEASE_CONFIG.BaseUrl)/$($RELEASE_CONFIG.AssetZipName)"
    $shaUrl = "$($RELEASE_CONFIG.BaseUrl)/$($RELEASE_CONFIG.AssetSha256Name)"
    $zipDest = Join-Path $toolsBin $RELEASE_CONFIG.AssetZipName

    $downloadSuccess = $false
    try {
        Write-Host "Downloading $zipUrl..." -ForegroundColor Cyan
        (New-Object System.Net.WebClient).DownloadFile($zipUrl, $zipDest)

        Write-Host "Fetching SHA256 checksum from $shaUrl..." -ForegroundColor Cyan
        $shaContent = (New-Object System.Net.WebClient).DownloadString($shaUrl)
        $expectedSha = ($shaContent.Trim() -split '\s+')[0].ToLower()

        Write-Host "Verifying SHA256 checksum..." -ForegroundColor Cyan
        $actualSha = (Get-FileHash -Path $zipDest -Algorithm SHA256).Hash.ToLower()

        if ($actualSha -ne $expectedSha) {
            Remove-Item -LiteralPath $zipDest -Force
            Write-Error "Security error: Checksum mismatch for release zip! Expected $expectedSha but got $actualSha."
            exit 1
        }
        Write-Host "Release binary checksum verified OK ($actualSha)." -ForegroundColor Green

        $tempExtract = Join-Path $toolsBin "temp_release_$([guid]::NewGuid())"
        New-Item -ItemType Directory -Path $tempExtract -Force | Out-Null
        Expand-Archive -LiteralPath $zipDest -DestinationPath $tempExtract -Force
        Remove-Item -LiteralPath $zipDest -Force

        $extractedExe = Get-ChildItem -Path $tempExtract -Filter "resumeforge.exe" -Recurse | Select-Object -First 1
        if (-not $extractedExe) {
            Remove-Item -LiteralPath $tempExtract -Recurse -Force
            throw "Could not find resumeforge.exe inside release zip."
        }

        Copy-Item -LiteralPath $extractedExe.FullName -Destination $downloadedBin -Force
        Remove-Item -LiteralPath $tempExtract -Recurse -Force
        $serverExe = $downloadedBin
        $binarySource = "downloaded"
        $downloadSuccess = $true
    } catch {
        if (Test-Path -LiteralPath $zipDest) { Remove-Item -LiteralPath $zipDest -Force }
        if ($_.ToString() -match "Security error|Checksum mismatch" -or $_.Exception.Message -match "Security error|Checksum mismatch") {
            Write-Host "FATAL: Security error: Checksum mismatch for release zip! Aborting." -ForegroundColor Red
            exit 1
        }
        Write-Host "Download not available or failed ($($_.Exception.Message))." -ForegroundColor Gray
    }

    if (-not $downloadSuccess) {
        if (Test-Path -LiteralPath $targetReleaseBin) {
            Write-Host " [+] Found local release build: $targetReleaseBin" -ForegroundColor Green
            $serverExe = $targetReleaseBin
            $binarySource = "prebuilt"
        } else {
            Write-Host "No release binary found; building from source with Cargo..." -ForegroundColor Yellow
            & cargo build --release --bin resumeforge
            if ($LASTEXITCODE -ne 0) {
                throw "Cargo build failed with exit code $LASTEXITCODE."
            }
            $serverExe = $targetReleaseBin
            $binarySource = "from_source"
        }
    }
}

if (-not $serverExe -or (-not (Test-Path -LiteralPath $serverExe))) {
    throw "ResumeForge binary could not be found or built."
}

# ==============================================================================
# 10. Binary Version Reporting & Setup State Recording
# ==============================================================================
$binVersion = "0.0.0"
try {
    $verOutput = (& $serverExe --version 2>&1 | Out-String).Trim()
    if ($verOutput -match 'resumeforge\s+([0-9\.]+)') {
        $binVersion = $matches[1]
    }
} catch {}

$cargoTomlPath = Join-Path $projectRoot 'Cargo.toml'
$cargoVersion = "0.1.0"
if (Test-Path -LiteralPath $cargoTomlPath) {
    $tomlContent = Get-Content -LiteralPath $cargoTomlPath -Raw
    if ($tomlContent -match 'version\s*=\s*"([^"]+)"') {
        $cargoVersion = $matches[1]
    }
}

if ($binVersion -ne $cargoVersion) {
    Write-Warning "Binary version ($binVersion) differs from Cargo.toml version ($cargoVersion)."
}

$state.binary_ready = $true
$state.binary_source = $binarySource
$state.binary_version = $binVersion
Save-SetupState $state

Write-Host " [+] ResumeForge binary ready: $serverExe (version: $binVersion, source: $binarySource)" -ForegroundColor Green

# ==============================================================================
# 11. Port Fallback & Server Execution
# ==============================================================================
function Test-PortAvailable([int]$port) {
    try {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $port)
        $listener.Start()
        $listener.Stop()
        return $true
    } catch {
        return $false
    }
}

$targetPort = 3000
if (-not (Test-PortAvailable $targetPort)) {
    Write-Host "Port 3000 is occupied. Searching for an available port..." -ForegroundColor Yellow
    for ($p = 3001; $p -le 3050; $p++) {
        if (Test-PortAvailable $p) {
            $targetPort = $p
            break
        }
    }
    Write-Host "Selected port: $targetPort" -ForegroundColor Cyan
}

$env:PORT = "$targetPort"

Write-Host "`nStarting ResumeForge server on http://127.0.0.1:$targetPort..." -ForegroundColor Cyan
$serverProcess = Start-Process -FilePath $serverExe -WorkingDirectory $projectRoot -PassThru -NoNewWindow

try {
    $healthUrl = "http://127.0.0.1:$targetPort/api/health"
    $ready = $false
    for ($i = 0; $i -lt 60; $i++) {
        if ($serverProcess.HasExited) {
            throw "ResumeForge server process exited unexpectedly with code $($serverProcess.ExitCode)."
        }
        try {
            $req = [System.Net.HttpWebRequest]::Create($healthUrl)
            $req.Host = "127.0.0.1:$targetPort"
            $req.Timeout = 1000
            $res = $req.GetResponse()
            if ([int]$res.StatusCode -eq 200) {
                $ready = $true
                $res.Close()
                break
            }
            $res.Close()
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }

    if (-not $ready) {
        throw "Server did not respond within 30 seconds."
    }

    Write-Host ""
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host "  ResumeForge is live at http://127.0.0.1:$targetPort   " -ForegroundColor Green
    Write-Host "  Press Ctrl+C in this console to stop the server cleanly" -ForegroundColor Green
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host ""

    if (-not $NoBrowser) {
        Start-Process "http://127.0.0.1:$targetPort"
    }

    Wait-Process -Id $serverProcess.Id
} finally {
    if (-not $serverProcess.HasExited) {
        Write-Host "`nStopping ResumeForge server..." -ForegroundColor Yellow
        Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
        Write-Host "ResumeForge stopped cleanly." -ForegroundColor Green
    }
}
