// frontend/js/wizard.js — First-run wizard & setup checklist controller
// Displays a full-screen, plain-language checklist until all 4 requirements are green.

class WizardController {
  constructor() {
    this.overlay = null;
    this.state = {
      agyReady: false,
      ghReady: false,
      latexReady: false,
      resumeReady: false,
      factsConfirmed: false,
      facts: null,
      ghUser: null,
      starters: []
    };
  }

  init() {
    this.overlay = document.getElementById('wizard-overlay');
    this.factsConfirmed = localStorage.getItem('rf_facts_confirmed') === 'true';
    this.bindEvents();
  }

  bindEvents() {
    document.getElementById('btn-open-wizard')?.addEventListener('click', () => {
      this.openWizard(true);
    });

    document.getElementById('btn-close-wizard')?.addEventListener('click', () => {
      this.closeWizard();
    });
  }

  async checkReadiness() {
    try {
      const [sysStatus, ghStatus, templateRes, starters] = await Promise.all([
        api.getSystemStatus().catch(() => null),
        api.getGithubStatus().catch(() => null),
        api.getTemplate().catch(() => null),
        api.getStarters().catch(() => [])
      ]);

      this.state.starters = starters || [];

      // 1. Antigravity check
      this.state.agyReady = !!(sysStatus?.agy?.installed && sysStatus?.agy?.outcome === 'success');

      // 2. GitHub check
      this.state.ghReady = !!(ghStatus?.authenticated);
      this.state.ghUser = ghStatus?.username || sysStatus?.gh?.username || null;

      // 3. LaTeX check
      this.state.latexReady = !!(sysStatus?.tectonic?.installed && sysStatus?.tectonic?.bundle_cached);

      // 4. Resume check
      this.state.resumeReady = !!(templateRes?.has_master);

      if (this.state.resumeReady) {
        try {
          this.state.facts = await api.getFacts();
        } catch (_) {
          this.state.facts = null;
        }
      }

      const allReady = this.state.agyReady &&
                       this.state.ghReady &&
                       this.state.latexReady &&
                       this.state.resumeReady &&
                       this.factsConfirmed;

      return {
        ready: allReady,
        missing: this.getMissingItems()
      };
    } catch (e) {
      console.warn("Readiness check failed:", e);
      return { ready: false, missing: ["System status check"] };
    }
  }

  getMissingItems() {
    const missing = [];
    if (!this.state.agyReady) missing.push("Antigravity signed in");
    if (!this.state.ghReady) missing.push("GitHub connected");
    if (!this.state.latexReady) missing.push("LaTeX engine ready");
    if (!this.state.resumeReady) missing.push("Master resume uploaded");
    else if (!this.factsConfirmed) missing.push("Confirm your extracted facts");
    return missing;
  }

  async openWizard(force = false) {
    await this.checkReadiness();
    this.renderWizardUI();
    this.overlay.classList.add('visible');
    this.overlay.setAttribute('aria-hidden', 'false');
  }

  closeWizard() {
    this.overlay.classList.remove('visible');
    this.overlay.setAttribute('aria-hidden', 'true');
    if (window.appController) {
      window.appController.refreshReadiness();
    }
  }

  renderWizardUI() {
    const missing = this.getMissingItems();
    const canComplete = missing.length === 0;

    let factsHtml = '';
    if (this.state.resumeReady && this.state.facts) {
      const f = this.state.facts;
      factsHtml = `
        <div class="wizard-facts-card">
          <div class="facts-header">
            <strong>Extracted Facts from Your Resume</strong>
            <span class="facts-status">${this.factsConfirmed ? '✓ Confirmed' : 'Please confirm'}</span>
          </div>
          <div class="facts-grid">
            <div class="fact-field">
              <span class="fact-label">Candidate Name</span>
              <span class="fact-value">${f.candidate_name || 'Not detected'}</span>
            </div>
            ${f.contact ? `
              <div class="fact-field">
                <span class="fact-label">Contact</span>
                <span class="fact-value">${f.contact}</span>
              </div>
            ` : ''}
            ${f.known_skills?.length ? `
              <div class="fact-field full-width">
                <span class="fact-label">Verified Skills (${f.known_skills.length})</span>
                <div class="skill-pills">${f.known_skills.map(s => `<span class="skill-pill">${s}</span>`).join('')}</div>
              </div>
            ` : ''}
            ${f.work_history?.length ? `
              <div class="fact-field full-width">
                <span class="fact-label">Career History (${f.work_history.length} roles)</span>
                <ul class="fact-list">
                  ${f.work_history.map(w => `<li><strong>${w.title}</strong> at ${w.company} <span class="text-muted">(${w.date_range})</span> — ${w.highlights.length} bullet points</li>`).join('')}
                </ul>
              </div>
            ` : ''}
            ${f.education?.length ? `
              <div class="fact-field full-width">
                <span class="fact-label">Education</span>
                <ul class="fact-list">
                  ${f.education.map(e => `<li><strong>${e.degree}</strong>, ${e.institution}</li>`).join('')}
                </ul>
              </div>
            ` : ''}
          </div>
          ${!this.factsConfirmed ? `
            <button id="btn-wizard-confirm-facts" class="btn btn-primary btn-sm mt-3">Confirm facts</button>
          ` : `
            <p class="facts-confirmed-note text-muted">Facts confirmed. You can update your master resume anytime in Settings.</p>
          `}
        </div>
      `;
    }

    this.overlay.innerHTML = `
      <div class="wizard-modal">
        <div class="wizard-header">
          <div>
            <span class="wizard-eyebrow">FIRST-TIME SETUP</span>
            <h2 class="wizard-title">Get started with ResumeForge</h2>
          </div>
          <button id="btn-close-wizard-top" class="drawer-close" aria-label="Close setup checklist">×</button>
        </div>
        <p class="wizard-subtitle">Complete these four quick checks to ensure your local engine is ready to tailor verified, single-page resumes.</p>

        <div class="wizard-checklist">
          <!-- Row 1: Antigravity -->
          <div class="wizard-row ${this.state.agyReady ? 'ready' : 'pending'}">
            <div class="row-icon">${this.state.agyReady ? '✓' : '1'}</div>
            <div class="row-content">
              <strong class="row-title">Antigravity signed in</strong>
              <p class="row-desc">${this.state.agyReady ? 'Antigravity CLI is installed and signed in to your Google account.' : 'Antigravity powers the headless resume generation turns. Sign in with your Google account.'}</p>
              ${!this.state.agyReady ? `
                <div class="row-action">
                  <div class="terminal-command">irm https://antigravity.google/cli/install.ps1 | iex</div>
                  <button class="btn btn-secondary btn-sm" onclick="window.wizardController.recheck()">Recheck</button>
                </div>
              ` : ''}
            </div>
            <div class="row-status">
              <span class="status-badge ${this.state.agyReady ? 'status-success' : 'status-neutral'}">${this.state.agyReady ? 'Active' : 'Action needed'}</span>
            </div>
          </div>

          <!-- Row 2: GitHub -->
          <div class="wizard-row ${this.state.ghReady ? 'ready' : 'pending'}">
            <div class="row-icon">${this.state.ghReady ? '✓' : '2'}</div>
            <div class="row-content">
              <strong class="row-title">GitHub connected</strong>
              <p class="row-desc">${this.state.ghReady ? `Authenticated as @${this.state.ghUser || 'user'}. Projects can be indexed for evidence.` : 'Connect your GitHub account using GitHub CLI to index your repositories for evidence.'}</p>
              ${!this.state.ghReady ? `
                <div class="row-action">
                  <div class="terminal-command">gh auth login --web</div>
                  <button class="btn btn-secondary btn-sm" onclick="window.wizardController.recheck()">Recheck</button>
                </div>
              ` : ''}
            </div>
            <div class="row-status">
              <span class="status-badge ${this.state.ghReady ? 'status-success' : 'status-neutral'}">${this.state.ghReady ? 'Connected' : 'Action needed'}</span>
            </div>
          </div>

          <!-- Row 3: LaTeX Engine -->
          <div class="wizard-row ${this.state.latexReady ? 'ready' : 'pending'}">
            <div class="row-icon">${this.state.latexReady ? '✓' : '3'}</div>
            <div class="row-content">
              <strong class="row-title">LaTeX ready</strong>
              <p class="row-desc">${this.state.latexReady ? 'Tectonic typesetting engine and packages are cached locally for offline single-page rendering.' : 'Tectonic compiles your resume to PDF. Packages download once automatically.'}</p>
              ${!this.state.latexReady ? `
                <div class="row-action">
                  <button class="btn btn-secondary btn-sm" onclick="window.wizardController.recheck()">Recheck engine</button>
                </div>
              ` : ''}
            </div>
            <div class="row-status">
              <span class="status-badge ${this.state.latexReady ? 'status-success' : 'status-neutral'}">${this.state.latexReady ? 'Ready' : 'Pending'}</span>
            </div>
          </div>

          <!-- Row 4: Master Resume -->
          <div class="wizard-row ${this.state.resumeReady && this.factsConfirmed ? 'ready' : 'pending'}">
            <div class="row-icon">${this.state.resumeReady && this.factsConfirmed ? '✓' : '4'}</div>
            <div class="row-content">
              <strong class="row-title">Your resume uploaded</strong>
              <p class="row-desc">${this.state.resumeReady ? (this.factsConfirmed ? 'Master resume uploaded and candidate facts confirmed.' : 'Master resume loaded. Please review and confirm your extracted facts below.') : 'Select a clean starter template or upload your existing LaTeX (.tex) resume.'}</p>
              ${!this.state.resumeReady ? `
                <div class="starter-buttons">
                  <button class="btn btn-secondary btn-sm" onclick="window.wizardController.useStarter('classic')">Use Classic Template</button>
                  <button class="btn btn-secondary btn-sm" onclick="window.wizardController.useStarter('modern')">Use Modern Template</button>
                  <label class="btn btn-secondary btn-sm file-label">
                    Upload .tex
                    <input type="file" accept=".tex" style="display:none" onchange="window.wizardController.handleFileUpload(event)">
                  </label>
                </div>
              ` : factsHtml}
            </div>
            <div class="row-status">
              <span class="status-badge ${this.state.resumeReady && this.factsConfirmed ? 'status-success' : 'status-neutral'}">${this.state.resumeReady && this.factsConfirmed ? 'Confirmed' : 'Action needed'}</span>
            </div>
          </div>
        </div>

        <div class="wizard-footer">
          <div class="wizard-footer-status">
            ${canComplete ? '<span class="status-pill green">All checks passed</span>' : `<span class="status-pill amber">${missing.length} action${missing.length > 1 ? 's' : ''} remaining</span>`}
          </div>
          <button id="btn-finish-wizard" class="btn btn-primary" ${!canComplete ? 'disabled' : ''}>Start creating resumes</button>
        </div>
      </div>
    `;

    document.getElementById('btn-close-wizard-top')?.addEventListener('click', () => this.closeWizard());
    document.getElementById('btn-finish-wizard')?.addEventListener('click', () => this.closeWizard());

    document.getElementById('btn-wizard-confirm-facts')?.addEventListener('click', () => {
      this.factsConfirmed = true;
      localStorage.setItem('rf_facts_confirmed', 'true');
      this.renderWizardUI();
    });
  }

  async recheck() {
    await this.checkReadiness();
    this.renderWizardUI();
  }

  async useStarter(starterId) {
    try {
      const starters = this.state.starters.length ? this.state.starters : await api.getStarters();
      const starter = starters.find(s => s.id === starterId) || starters[0];
      if (starter) {
        await api.saveTemplate(starter.content, true, false);
        this.factsConfirmed = false;
        await this.checkReadiness();
        this.renderWizardUI();
      }
    } catch (e) {
      alert(`Could not apply starter template: ${e.message}`);
    }
  }

  async handleFileUpload(event) {
    const file = event.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      await api.saveTemplate(text, false, false);
      this.factsConfirmed = false;
      await this.checkReadiness();
      this.renderWizardUI();
    } catch (e) {
      alert(`Could not read or save template: ${e.message}`);
    }
  }
}

window.wizardController = new WizardController();
