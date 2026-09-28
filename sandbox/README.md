# Windows Sandbox Testing for ResumeForge

Windows Sandbox provides a disposable, isolated Hyper-V environment to test ResumeForge on a pristine Windows 10/11 system with zero developer caches.

## Requirements
- Windows 10/11 Pro, Enterprise, or Education (Build 18305 or later)
- Virtualization enabled in BIOS/UEFI
- Windows Sandbox feature enabled:
  ```powershell
  Enable-WindowsOptionalFeature -FeatureName "Containers-DisposableClientVM" -All -Online
  ```

## Running in Windows Sandbox
1. Create the `sandbox\logs` directory on the host:
   ```cmd
   mkdir sandbox\logs
   ```
2. Double-click `sandbox\resumeforge.wsb` (or run `start sandbox\resumeforge.wsb`).
3. The sandbox automatically:
   - Mounts this repository as read-only at `C:\resumeforge-src`.
   - Mounts `sandbox\logs` as writable at `C:\resumeforge-logs`.
   - Copies the clean source tree into `C:\resumeforge` inside the VM.
   - Executes `start.cmd -Yes -NoBrowser` with fresh tools and no cached state.
   - Writes the full execution log to `sandbox\logs\`.

## Fallback: Fresh Local Windows User Account (Windows Home)
If Windows Sandbox is unavailable (e.g. on Windows Home editions):
1. Create a fresh local user account with standard privileges:
   ```cmd
   net user ResumeForgeTest Passw0rd123! /add
   ```
2. Sign in as `ResumeForgeTest`. This account starts with:
   - Empty `LOCALAPPDATA` (no Tectonic LaTeX cache)
   - Empty `USERPROFILE` (no cached `gh` credentials, no `agy` token)
   - Default clean system `PATH`
3. Clone or copy the ResumeForge repo to `C:\Users\ResumeForgeTest\resumeforge`.
4. Run `start.cmd` to verify end-to-end first-run setup.
5. Once verified, delete the test user from an elevated administrator terminal:
   ```cmd
   net user ResumeForgeTest /delete
   ```
