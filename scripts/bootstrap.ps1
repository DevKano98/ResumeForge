# scripts/bootstrap.ps1 - Single-command zero-config launcher for ResumeForge
[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$Reset,
    [switch]$Update,
    [switch]$FromSource
)

$ErrorActionPreference = 'Continue'

# 1. Project Directory Resolution
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = (Resolve-Path (Join-Path $scriptDir '..')).Path
Set-Location -LiteralPath $projectRoot

$stateFile = Join-Path $projectRoot '.resumeforge-setup.json'
$toolsDir = Join-Path $projectRoot 'tools'
$toolsBin = Join-Path $toolsDir 'bin'
$toolsTemp = Join-Path $toolsDir 'temp'

# Ensure portable tools/bin is in process PATH
if (-not (Test-Path -LiteralPath $toolsBin)) {
    New-Item -ItemType Directory -Path $toolsBin -Force | Out-Null
}
$env:PATH = "$toolsBin;$env:PATH"

# Also check for standard user agy install directory
if ($env:USERPROFILE) {
    $userAgyBin = Join-Path $env:USERPROFILE 'AppData\Local\agy\bin'
    if ((Test-Path -LiteralPath $userAgyBin) -and ($env:PATH -notlike "*$userAgyBin*")) {
        $env:PATH = "$userAgyBin;$env:PATH"
    }
}

# 2. State Management Helpers
function Get-SetupState {
    if (Test-Path -LiteralPath $stateFile) {
        try {
            return Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
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

if ($Reset) {
    Write-Host "[Reset] Clearing setup state and local tools..." -ForegroundColor Yellow
    if (Test-Path -LiteralPath $stateFile) { Remove-Item -LiteralPath $stateFile -Force }
    if (Test-Path -LiteralPath $toolsDir) { Remove-Item -LiteralPath $toolsDir -Recurse -Force }
    New-Item -ItemType Directory -Path $toolsBin -Force | Out-Null
    Write-Host "[Reset] State cleared." -ForegroundColor Green
}

if ($Update) {
    Write-Host "[Update] Checking for repository and binary updates..." -ForegroundColor Cyan
    if (Test-Path -LiteralPath (Join-Path $projectRoot '.git')) {
        try {
            & git pull
        } catch {
            Write-Warning "git pull failed: $_"
        }
    }
    $downloadedBin = Join-Path $toolsBin 'resumeforge.exe'
    if (Test-Path -LiteralPath $downloadedBin) {
        Remove-Item -LiteralPath $downloadedBin -Force
    }
    if ($state) {
        $state.binary_ready = $false
        Save-SetupState $state
    }
}

$state = Get-SetupState
if (-not $state) {
    $state = [PSCustomObject]@{
        version = 1
        tools = [PSCustomObject]@{}
        binary_ready = $false
        binary_source = "none"
        tectonic_warmed = $false
        last_updated = $null
    }
}

Write-Host "========================================================" -ForegroundColor Cyan
Write-Host "           ResumeForge Windows Bootstrapper             " -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Cyan

# 3. Portable Tools Installation (gh, gitleaks, tectonic)
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
    Write-Host "Target directory: $toolsBin (local to repository, no admin rights required)`n"
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
        
        # Download
        (New-Object System.Net.WebClient).DownloadFile($tool.Url, $zipDest)

        # Verify SHA256 checksum
        Write-Host "Verifying SHA256 checksum for $($tool.ZipName)..." -ForegroundColor Cyan
        $actualHash = (Get-FileHash -Path $zipDest -Algorithm SHA256).Hash.ToLower()
        if ($actualHash -ne $tool.Sha256.ToLower()) {
            Remove-Item -LiteralPath $zipDest -Force
            throw "Security error: Checksum mismatch for $($tool.ZipName)! Expected $($tool.Sha256) but got $actualHash."
        }
        Write-Host "Checksum verified OK." -ForegroundColor Green

        # Extract
        $extractDir = Join-Path $toolsTemp ("ext_" + $tool.Name)
        if (Test-Path -LiteralPath $extractDir) { Remove-Item -LiteralPath $extractDir -Recurse -Force }
        Expand-Archive -LiteralPath $zipDest -DestinationPath $extractDir -Force

        $extractedExe = Join-Path $extractDir $tool.SubPath
        if (-not (Test-Path -LiteralPath $extractedExe)) {
            # Search for exe in extract directory
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

# 4. Antigravity CLI ('agy') Detection & Verification
Write-Host "`nChecking Antigravity CLI ('agy')..." -ForegroundColor Cyan
$agyCmd = Get-Command agy -ErrorAction SilentlyContinue

if (-not $agyCmd) {
    Write-Host "`n[!] Google Antigravity CLI ('agy') is not detected on your system." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "OFFICIAL INSTALLATION (Google Antigravity):" -ForegroundColor White
    Write-Host "  1. Open a new PowerShell terminal and run:" -ForegroundColor White
    Write-Host "     irm https://antigravity.google/cli/install.ps1 | iex" -ForegroundColor Green
    Write-Host ""
    Write-Host "  2. Launch agy once to authenticate with your Google account:" -ForegroundColor White
    Write-Host "     agy" -ForegroundColor Green
    Write-Host ""
    Write-Host "     Note: Each user signs in with their own Google account via the browser." -ForegroundColor Gray
    Write-Host "     ResumeForge runs locally and never manages or stores your Google credentials." -ForegroundColor Gray
    Write-Host ""

    if (-not $Yes) {
        Read-Host "Press Enter once you have installed and signed in to agy..."
    }

    # Re-check PATH
    if ($env:USERPROFILE) {
        $userAgyBin = Join-Path $env:USERPROFILE 'AppData\Local\agy\bin'
        if ((Test-Path -LiteralPath $userAgyBin) -and ($env:PATH -notlike "*$userAgyBin*")) {
            $env:PATH = "$userAgyBin;$env:PATH"
        }
    }
    $agyCmd = Get-Command agy -ErrorAction SilentlyContinue
    if (-not $agyCmd) {
        throw "Antigravity CLI ('agy') is still not found in PATH. Please complete the installation and restart start.cmd."
    }
}

Write-Host " [+] agy found at $($agyCmd.Source)" -ForegroundColor Green

# Quick headless probe
Write-Host "Probing Antigravity model response..." -ForegroundColor Cyan
try {
    $probeRes = & agy -p "reply with the single word OK" 2>&1 | Out-String
    if ($LASTEXITCODE -eq 0 -and ($probeRes -match "OK|ok")) {
        Write-Host " [+] Antigravity probe passed cleanly." -ForegroundColor Green
    } else {
        Write-Warning "Antigravity probe warning (exit code $LASTEXITCODE): $probeRes"
    }
} catch {
    Write-Warning "Antigravity probe failed with error: $_. Please verify your login with 'agy'."
}

# 5. GitHub CLI Authentication
Write-Host "`nChecking GitHub CLI authentication..." -ForegroundColor Cyan
try {
    $ghStatus = & gh auth status 2>&1 | Out-String
} catch {
    $ghStatus = $null
}

if ($LASTEXITCODE -ne 0) {
    if (-not $Yes) {
        Write-Host "[!] GitHub CLI is not authenticated." -ForegroundColor Yellow
        Write-Host "Starting interactive web login (gh auth login --web)..." -ForegroundColor White
        Write-Host "Each user signs in with their own GitHub account. ResumeForge never stores your token." -ForegroundColor Gray
        & gh auth login --web -h github.com -p https -s repo,read:user
        & gh auth setup-git
        $ghStatus = & gh auth status 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) {
            throw "GitHub CLI authentication was not completed."
        }
        Write-Host " [+] GitHub CLI is authenticated." -ForegroundColor Green
    } else {
        Write-Warning "GitHub CLI is not authenticated. Interactive login was skipped because -Yes was specified."
        Write-Host "You can authenticate later with 'gh auth login --web' to sync private repositories." -ForegroundColor Gray
    }
} else {
    Write-Host " [+] GitHub CLI is authenticated." -ForegroundColor Green
}

# 6. Tectonic LaTeX Warm-up
$tectonicCacheDir = $null
if ($env:LOCALAPPDATA) {
    $tectonicCacheDir = Join-Path $env:LOCALAPPDATA 'TectonicProject\Tectonic\cache'
}
$isWarmed = ($state.tectonic_warmed -eq $true) -and ($tectonicCacheDir -and (Test-Path -LiteralPath $tectonicCacheDir))

if (-not $isWarmed) {
    Write-Host "`n[Tectonic LaTeX Engine Warm-up]" -ForegroundColor Cyan
    Write-Host "Downloading LaTeX packages (one-time setup, may take 1-2 minutes)..." -ForegroundColor Yellow

    $warmupDir = Join-Path ([System.IO.Path]::GetTempPath()) "tectonic-warmup"
    if (Test-Path -LiteralPath $warmupDir) { Remove-Item -LiteralPath $warmupDir -Recurse -Force }
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

# 7. ResumeForge Binary Resolution
Write-Host "`nChecking ResumeForge binary..." -ForegroundColor Cyan
$serverExe = $null

$candidates = @(
    (Join-Path $projectRoot 'target\release\resumeforge.exe'),
    (Join-Path $toolsBin 'resumeforge.exe'),
    (Join-Path $projectRoot 'target\debug\resumeforge.exe')
)

foreach ($c in $candidates) {
    if (Test-Path -LiteralPath $c) {
        $serverExe = $c
        break
    }
}

if ($FromSource -or (-not $serverExe)) {
    if (-not $FromSource -and (-not $serverExe)) {
        Write-Host "No local prebuilt binary found. Checking for GitHub release..." -ForegroundColor Cyan
        # Try to download latest release zip if available
        $releaseDownloaded = $false
        try {
            $repo = "BirendraArchana/resumeforge"
            try {
                $remote = & git remote get-url origin 2>$null
                if ($remote -match 'github\.com[:/]([^/]+/[^/\.]+?)(?:\.git)?$') {
                    $repo = $matches[1]
                }
            } catch {}
            $rel = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -ErrorAction Stop
            $zipAsset = $rel.assets | Where-Object { $_.name -like "*windows-x86_64.zip*" } | Select-Object -First 1
            $shaAsset = $rel.assets | Where-Object { $_.name -like "*windows-x86_64.zip.sha256*" } | Select-Object -First 1

            if ($zipAsset) {
                Write-Host "Downloading ResumeForge $($rel.tag_name) from GitHub ($repo)..." -ForegroundColor Cyan
                $zipFile = Join-Path $toolsBin "resumeforge-release.zip"
                (New-Object System.Net.WebClient).DownloadFile($zipAsset.browser_download_url, $zipFile)

                if ($shaAsset) {
                    Write-Host "Verifying SHA256 checksum for release zip..." -ForegroundColor Cyan
                    $expectedSha = (Invoke-RestMethod $shaAsset.browser_download_url) -split '\s+' | Select-Object -First 1
                    $actualSha = (Get-FileHash -Path $zipFile -Algorithm SHA256).Hash.ToLower()
                    if ($expectedSha -and ($actualSha -ne $expectedSha.ToLower())) {
                        Remove-Item -LiteralPath $zipFile -Force
                        throw "Checksum verification failed for ResumeForge release zip! Expected $expectedSha but got $actualSha"
                    }
                    Write-Host "Release checksum verified OK." -ForegroundColor Green
                }

                $tempExtract = Join-Path $toolsBin "temp_release"
                if (Test-Path -LiteralPath $tempExtract) { Remove-Item -LiteralPath $tempExtract -Recurse -Force }
                Expand-Archive -LiteralPath $zipFile -DestinationPath $tempExtract -Force
                Remove-Item -LiteralPath $zipFile -Force

                $downloadedExe = Join-Path $tempExtract "resumeforge.exe"
                if (-not (Test-Path -LiteralPath $downloadedExe)) {
                    $found = Get-ChildItem -Path $tempExtract -Filter "resumeforge.exe" -Recurse | Select-Object -First 1
                    if ($found) { $downloadedExe = $found.FullName }
                }

                if (Test-Path -LiteralPath $downloadedExe) {
                    $destExe = Join-Path $toolsBin "resumeforge.exe"
                    Copy-Item -LiteralPath $downloadedExe -Destination $destExe -Force
                    Remove-Item -LiteralPath $tempExtract -Recurse -Force
                    $serverExe = $destExe
                    $releaseDownloaded = $true
                    $state.binary_ready = $true
                    $state.binary_source = "downloaded"
                    Save-SetupState $state
                    Write-Host " [+] Downloaded and verified ResumeForge release binary." -ForegroundColor Green
                } else {
                    Remove-Item -LiteralPath $tempExtract -Recurse -Force
                    throw "Could not find resumeforge.exe in downloaded release zip."
                }
            }
        } catch {
            Write-Host "GitHub release binary not available ($($_.Exception.Message))." -ForegroundColor Gray
            Write-Host "Building from source..." -ForegroundColor Yellow
            $FromSource = $true
        }
    }

    if ($FromSource) {
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
                throw "Rust installation completed, but cargo was not found in PATH. Please restart start.cmd."
            }
        }

        # Check for Visual Studio C++ Build Tools (MSVC)
        $clCmd = Get-Command cl.exe -ErrorAction SilentlyContinue
        if (-not $clCmd) {
            $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
            $hasMsvc = $false
            if (Test-Path -LiteralPath $vswhere) {
                $vsInstall = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64
                if ($vsInstall) { $hasMsvc = $true }
            }
            if (-not $hasMsvc) {
                Write-Warning "Visual Studio C++ Build Tools (MSVC) were not detected."
                Write-Host "If the build fails with linker errors, install Microsoft C++ Build Tools from:" -ForegroundColor Yellow
                Write-Host "https://visualstudio.microsoft.com/visual-cpp-build-tools/" -ForegroundColor White
            }
        }

        Write-Host "Running cargo build --release --bin resumeforge..." -ForegroundColor Cyan
        & cargo build --release --bin resumeforge
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed with exit code $LASTEXITCODE."
        }
        $serverExe = Join-Path $projectRoot 'target\release\resumeforge.exe'
        $state.binary_ready = $true
        $state.binary_source = "from_source"
        Save-SetupState $state
        Write-Host " [+] Build completed successfully." -ForegroundColor Green
    }
}

Write-Host " [+] Using binary: $serverExe" -ForegroundColor Green

# 8. Start ResumeForge Server
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
    Write-Host "Port 3000 is currently occupied. Searching for an available port..." -ForegroundColor Yellow
    for ($p = 3001; $p -le 3050; $p++) {
        if (Test-PortAvailable $p) {
            $targetPort = $p
            break
        }
    }
    Write-Host "Selected available port: $targetPort" -ForegroundColor Cyan
}

$env:PORT = "$targetPort"

Write-Host "`nStarting ResumeForge server on port $targetPort..." -ForegroundColor Cyan
$serverProcess = Start-Process -FilePath $serverExe -WorkingDirectory $projectRoot -PassThru -NoNewWindow

try {
    # Poll /api/health until responsive
    $healthUrl = "http://127.0.0.1:$targetPort/api/health"
    $ready = $false
    for ($i = 0; $i -lt 60; $i++) {
        if ($serverProcess.HasExited) {
            throw "ResumeForge server process exited unexpectedly with code $($serverProcess.ExitCode)."
        }
        try {
            $res = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
            if ($res.StatusCode -eq 200) {
                $ready = $true
                break
            }
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }

    if (-not $ready) {
        throw "Server did not become healthy within 30 seconds."
    }

    Write-Host ""
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host "  ResumeForge is live at http://127.0.0.1:$targetPort   " -ForegroundColor Green
    Write-Host "  Press Ctrl+C in this console to stop the server cleanly" -ForegroundColor Green
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host ""

    Start-Process "http://127.0.0.1:$targetPort"
    Wait-Process -Id $serverProcess.Id
} finally {
    if (-not $serverProcess.HasExited) {
        Write-Host "`nStopping ResumeForge server..." -ForegroundColor Yellow
        Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
        Write-Host "ResumeForge stopped cleanly." -ForegroundColor Green
    }
}
