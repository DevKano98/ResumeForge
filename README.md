# ResumeForge

> Local-first, privacy-preserving resume tailoring engine that transforms your verified master LaTeX resume and GitHub engineering projects into target-grounded, single-page resumes.

---

## ⚡ Quickstart

```cmd
git clone https://github.com/resumeforge/resumeforge.git
cd resumeforge
start.cmd
```

`start.cmd` automatically bootstraps portable CLI tools (`gh`, `gitleaks`, `tectonic`), warms up the LaTeX engine, verifies your local Antigravity CLI, and launches the application at `http://127.0.0.1:3000`.

---

## 📋 Prerequisites

1. **Operating System**: Windows 10 or 11 (64-bit).
2. **Internet Connection**: For initial portable tool downloads, GitHub repository syncing, and AI turn generation.
3. **Google Account**: For Google Antigravity (`agy`), which powers the headless resume generation turns.
   - If `agy` is not already installed on your system, install it via PowerShell:
     ```powershell
     irm https://antigravity.google/cli/install.ps1 | iex
     ```
   - Run `agy` in your terminal once to sign in with your Google account. ResumeForge runs locally and never handles or stores your Google account credentials.
4. **GitHub Account**: Authenticated via GitHub CLI (`gh auth login`). ResumeForge uses GitHub credentials locally to clone your repositories; your tokens are never stored by ResumeForge.

---

## 🚀 Launcher Options

You can pass command-line flags to `start.cmd` or `scripts/bootstrap.ps1`:

| Flag | Description |
| :--- | :--- |
| `-Yes` | Non-interactive mode. Skips prompts and automatically proceeds with tool installations. |
| `-Reset` | Clears `.resumeforge-setup.json` and resets local portable tools. |
| `-Update` | Runs `git pull` to update the repository and checks for updated release binaries. |
| `-FromSource` | Forces building the binary locally with `cargo build --release` (requires Rust toolchain and MSVC build tools). |

Examples:
```cmd
start.cmd -Yes
start.cmd -Update
start.cmd -Reset
start.cmd -FromSource
```

---

## 🛠️ How It Works

1. **Setup & Master LaTeX**: In the **Setup** tab, configure your master resume LaTeX document or select a starter template (`Classic` or `Modern`). ResumeForge parses your candidate facts (skills, work history, highlights) deterministically.
2. **Repository Sync & Evidence**: In the **GitHub** tab, sync your public or private repositories. ResumeForge indexes your projects, parses architecture facts and code evidence, and scans for secrets with `gitleaks`.
3. **Tailored Generation**: On the **Create** tab, paste a job description. The Antigravity AI agent proposes tailored bullets and summary points.
4. **Deterministic Anti-Hallucination Guards**:
   - **Skills Guard**: Eliminates any skill absent from the master resume or project evidence.
   - **Text & Metric Guard**: Verifies every numerical metric against master history and cited project evidence claims.
   - **Technology Vocabulary Guard**: Prohibits ungrounded technology claims in work history or project bullets.
5. **Stage B Compaction & Single-Page PDF**: Tectonic XeTeX renders the LaTeX snapshot. If content spills over one page, an automated shortening and `\tighten` loop optimizes typography to produce an exact single-page PDF.

---

## 🔍 Troubleshooting

### `agy` command not found
- Ensure you installed the Antigravity CLI:
  ```powershell
  irm https://antigravity.google/cli/install.ps1 | iex
  ```
- If you just installed `agy`, open a new terminal window or run `start.cmd` again.
- Verify `agy` works by running:
  ```powershell
  agy -p "reply with the single word OK"
  ```

### Port 3000 is occupied
- ResumeForge automatically detects busy ports and selects the next available port (e.g., `3001`, `3002`). The console will display the exact URL where the server is running.

### GitHub CLI authentication failed
- Run `gh auth login --web` in your terminal to authenticate with GitHub, followed by `gh auth setup-git` to configure Git credential helpers.

### Tectonic compilation errors
- ResumeForge caches the LaTeX bundle on first run. If compilation fails due to an interrupted download, run `start.cmd -Reset` to refresh the local tools and warm-up cache.

### Building from source fails
- If running with `-FromSource`, ensure the Microsoft Visual Studio C++ Build Tools (MSVC) are installed from [visualstudio.microsoft.com/visual-cpp-build-tools/](https://visualstudio.microsoft.com/visual-cpp-build-tools/).

---

## 🔒 Privacy & Local-First Guarantee

- **Zero Cloud Storage**: All candidate data, SQLite databases (`data/resumeforge.db`), master snapshots, and compiled PDFs remain strictly on your local disk.
- **Fail-Closed Secret Scanning**: Repository syncing requires a local `gitleaks` scan before indexing. Files containing detected secrets are excluded from evidence indexing.
- **No Token Storage**: Git and Google authentications are delegated entirely to the official native CLIs (`gh` and `agy`).

---

## 📜 License

Distributed under the [MIT License](LICENSE).
