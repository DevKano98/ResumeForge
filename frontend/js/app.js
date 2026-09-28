// frontend/js/app.js — Core application controller & client router for ResumeForge
// Coordinates navigation between Create, History, Projects, Settings, and Canvas Run views.

class AppController {
  constructor() {
    this.currentView = 'create';
    this.readiness = { ready: false, missing: [] };
  }

  async init() {
    // 1. Initialize sub-controllers
    if (window.wizardController) {
      window.wizardController.init();
    }
    if (window.canvasController) {
      window.canvasController.init(document.getElementById('canvas-container'));
    }

    // 2. Setup navigation links
    document.querySelectorAll('.nav-link').forEach(link => {
      link.addEventListener('click', (e) => {
        const targetView = link.getAttribute('data-view');
        if (targetView) {
          e.preventDefault();
          window.location.hash = `#/${targetView}`;
        }
      });
    });

    // 3. Listen to hash changes
    window.addEventListener('hashchange', () => this.handleRoute());

    // 4. Bind Create Screen Form & Auto-fill
    this.bindCreateScreen();

    // 5. Check initial readiness
    await this.refreshReadiness();

    // 6. Route to initial view
    this.handleRoute();

    // 7. If setup incomplete and never confirmed, auto-prompt first-run wizard
    if (!this.readiness.ready && !localStorage.getItem('rf_wizard_dismissed') && !window.location.href.includes('nowizard')) {
      window.wizardController.openWizard();
    }

    // Prefill sample job description if requested for testing/demos
    if (window.location.href.includes('sample')) {
      const jdInput = document.getElementById('create-jd');
      const compInput = document.getElementById('create-company');
      const roleInput = document.getElementById('create-role');
      if (jdInput && compInput && roleInput) {
        jdInput.value = "We are seeking a Senior Systems Engineer at Cloudflare to design, implement, and maintain high-throughput distributed edge systems. You will work with Rust, Go, Tokio, zero-copy networking, and modern container orchestration.\n\nResponsibilities:\n- Design scalable distributed microservices handling 50k+ requests/sec\n- Optimize memory layout, zero-copy buffers, and asynchronous network primitives\n- Maintain 99.99% service availability across multi-region edge clusters\n\nRequirements:\n- 5+ years experience in systems engineering with Rust or Go\n- Strong background in distributed consensus, concurrency, and Linux internals\n- Track record of building reliable, memory-safe backend infrastructure.";
        compInput.value = "Cloudflare";
        roleInput.value = "Senior Systems Engineer";
      }
    }
  }

  async refreshReadiness() {
    if (window.wizardController) {
      this.readiness = await window.wizardController.checkReadiness();
    }

    // Update left nav system status indicator
    const sysDot = document.getElementById('nav-sys-dot');
    const sysLabel = document.getElementById('nav-sys-label');
    if (sysDot && sysLabel) {
      if (this.readiness.ready) {
        sysDot.className = 'status-dot-sm green';
        sysLabel.textContent = 'Engine Ready';
      } else {
        sysDot.className = 'status-dot-sm amber';
        sysLabel.textContent = 'Setup Needed';
      }
    }

    // Update Create view lock banner & button
    const lockBanner = document.getElementById('create-lock-banner');
    const btnGenerate = document.getElementById('btn-generate-resume');
    const hintEl = document.getElementById('create-button-help');
    if (lockBanner && btnGenerate) {
      if (!this.readiness.ready) {
        lockBanner.style.display = 'flex';
        document.getElementById('lock-missing-text').textContent =
          `Setup required: ${this.readiness.missing.join(', ')}. Complete these before generating.`;
        btnGenerate.disabled = true;
        btnGenerate.textContent = `Generate resume (Locked: ${this.readiness.missing[0] || 'Setup needed'})`;
        btnGenerate.title = `Missing: ${this.readiness.missing.join(', ')}`;
        if (hintEl) {
          hintEl.textContent = `Setup required before generating: ${this.readiness.missing.join(', ')}.`;
        }
      } else {
        lockBanner.style.display = 'none';
        btnGenerate.disabled = false;
        btnGenerate.textContent = "Generate resume";
        btnGenerate.title = "";
        if (hintEl) {
          hintEl.textContent = '';
        }
      }
    }
  }

  handleRoute() {
    const rawHash = window.location.hash || '#/create';
    const cleanHash = rawHash.split('?')[0];
    const parts = cleanHash.replace(/^#\/?/, '').split('/');
    const view = parts[0] || 'create';
    const param = parts[1];

    this.currentView = view;

    // Update active state in left nav
    document.querySelectorAll('.nav-link').forEach(link => {
      const v = link.getAttribute('data-view');
      if (v === view || (view === 'run' && v === 'create')) {
        link.classList.add('active');
      } else {
        link.classList.remove('active');
      }
    });

    // Switch visible main view container
    document.querySelectorAll('.view-section').forEach(sec => {
      sec.style.display = 'none';
    });

    switch (view) {
      case 'create':
        document.getElementById('view-create').style.display = 'block';
        this.refreshReadiness();
        break;

      case 'run':
        document.getElementById('view-run').style.display = 'block';
        this.loadRunView(param);
        break;

      case 'history':
        document.getElementById('view-history').style.display = 'block';
        this.loadHistoryView();
        break;

      case 'projects':
        document.getElementById('view-projects').style.display = 'block';
        this.loadProjectsView();
        break;

      case 'settings':
        document.getElementById('view-settings').style.display = 'block';
        this.loadSettingsView();
        break;

      default:
        document.getElementById('view-create').style.display = 'block';
        break;
    }
  }

  bindCreateScreen() {
    const jdInput = document.getElementById('create-jd');
    const compInput = document.getElementById('create-company');
    const roleInput = document.getElementById('create-role');
    const form = document.getElementById('form-create-resume');
    const btnReplayDemo = document.getElementById('btn-replay-demo');

    // Smart auto-fill for Company & Role on paste or typing
    if (jdInput) {
      jdInput.addEventListener('input', () => {
        const text = jdInput.value;
        if (!text) return;

        // Auto-extract Role if empty
        if (!roleInput.value.trim()) {
          const roleMatch = text.match(/(?:title|role|position|looking for a|seeking an?|hiring an?)\s*:?\s*([A-Z][A-Za-z0-9\s-]{3,40}(?:Engineer|Developer|Manager|Scientist|Architect|Specialist|Designer|Lead|Director))/i) ||
                            text.match(/^[#*\s]*([A-Z][A-Za-z0-9\s-]{3,40}(?:Engineer|Developer|Manager|Scientist|Architect|Specialist|Designer|Lead|Director))/m);
          if (roleMatch && roleMatch[1]) {
            roleInput.value = roleMatch[1].trim();
          }
        }

        // Auto-extract Company if empty
        if (!compInput.value.trim()) {
          const compMatch = text.match(/(?:at|for|company|about)\s+([A-Z][A-Za-z0-9&.]{2,30}(?:\s+(?:Inc|LLC|Technologies|Labs|Systems|Corporation|Corp))?)/) ||
                            text.match(/About\s+([A-Z][A-Za-z0-9&.]{2,30})/);
          if (compMatch && compMatch[1]) {
            const candidate = compMatch[1].trim();
            if (!candidate.toLowerCase().includes("role") && !candidate.toLowerCase().includes("the")) {
              compInput.value = candidate;
            }
          }
        }
      });
    }

    // Submit generation
    if (form) {
      form.addEventListener('submit', async (e) => {
        e.preventDefault();
        const jd = jdInput.value.trim();
        const company = compInput.value.trim() || undefined;
        const role = roleInput.value.trim() || undefined;
        const extra = document.getElementById('create-extra')?.value.trim() || undefined;

        if (!jd) {
          alert("Please paste a job description.");
          jdInput.focus();
          return;
        }

        const btn = document.getElementById('btn-generate-resume');
        btn.disabled = true;
        btn.textContent = "Launching generation...";

        try {
          const newResume = await api.createResume({
            job_description: jd,
            company,
            role,
            extra_instructions: extra
          });
          btn.disabled = false;
          btn.textContent = "Generate resume";
          window.location.hash = `#/run/${newResume.id}`;
        } catch (err) {
          btn.disabled = false;
          btn.textContent = "Generate resume";
          alert(`Generation request failed: ${err.message}`);
        }
      });
    }

    // Replay demo button
    if (btnReplayDemo) {
      btnReplayDemo.addEventListener('click', (e) => {
        e.preventDefault();
        window.location.hash = '#/run/demo';
      });
    }
  }

  loadRunView(id) {
    const titleEl = document.getElementById('run-title');
    const subtitleEl = document.getElementById('run-subtitle');
    const replayControls = document.getElementById('run-replay-controls');

    if (id === 'demo') {
      titleEl.textContent = "Senior Systems Engineer at Cloudflare";
      subtitleEl.textContent = "Demo Replay Mode (Simulation)";
      replayControls.style.display = 'flex';

      window.canvasController.resumeId = 999;
      window.canvasController.isReplay = true;
      window.canvasController.resetState();

      // Setup speed selector
      const speedSelect = document.getElementById('replay-speed-select');
      if (speedSelect) {
        speedSelect.onchange = (e) => {
          window.replayController.setSpeed(e.target.value);
        };
      }

      const pauseBtn = document.getElementById('btn-replay-pause');
      if (pauseBtn) {
        pauseBtn.textContent = "Pause";
        pauseBtn.onclick = () => {
          if (window.replayController.isPlaying) {
            window.replayController.pause();
            pauseBtn.textContent = "Resume";
          } else {
            window.replayController.resume();
            pauseBtn.textContent = "Pause";
          }
        };
      }

      const restartBtn = document.getElementById('btn-replay-restart');
      if (restartBtn) {
        restartBtn.onclick = () => {
          window.canvasController.resetState();
          window.replayController.start(
            (ev) => window.canvasController.handleEvent(ev),
            () => window.canvasController.onPipelineComplete(DEMO_RUN)
          );
        };
      }

      window.replayController.start(
        (ev) => window.canvasController.handleEvent(ev),
        () => window.canvasController.onPipelineComplete(DEMO_RUN)
      );
    } else {
      replayControls.style.display = 'none';
      titleEl.textContent = `Resume #${id}`;
      subtitleEl.textContent = "Live Generation Timeline";

      window.canvasController.loadResumeRun(parseInt(id, 10));
    }
  }

  async loadHistoryView() {
    const listContainer = document.getElementById('history-table-body');
    const emptyCard = document.getElementById('history-empty-card');
    listContainer.innerHTML = '<tr><td colspan="6" class="text-center py-4">Loading history...</td></tr>';

    try {
      const resumes = await api.listResumes();
      listContainer.innerHTML = '';

      if (!resumes || resumes.length === 0) {
        emptyCard.style.display = 'block';
        return;
      }
      emptyCard.style.display = 'none';

      resumes.forEach(r => {
        const tr = document.createElement('tr');
        const statusInfo = formatResumeStatus(r.status);
        const dateStr = r.created_at ? new Date(r.created_at).toLocaleDateString(undefined, {
          month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
        }) : 'Recent';

        tr.innerHTML = `
          <td><strong>${r.company || 'Direct Application'}</strong></td>
          <td>${r.role || 'Tailored Resume'}</td>
          <td><span class="status-badge ${statusInfo.badgeClass}">${statusInfo.label}</span></td>
          <td>${r.page_count ? `${r.page_count} page` : '—'}</td>
          <td class="text-muted">${dateStr}</td>
          <td class="action-cell">
            <a href="#/run/${r.id}" class="btn btn-secondary btn-sm">View</a>
            ${(r.status === 'completed' || r.status === 'ready' || r.status === 'ready_sparse') ? `<a href="/api/resumes/${r.id}/pdf" target="_blank" class="btn btn-secondary btn-sm">PDF</a>` : ''}
            <button class="btn btn-ghost btn-sm text-danger" onclick="window.appController.deleteResume(${r.id})">Delete</button>
          </td>
        `;
        listContainer.appendChild(tr);
      });
    } catch (e) {
      listContainer.innerHTML = `<tr><td colspan="6" class="text-danger">Failed to load history: ${e.message}</td></tr>`;
    }
  }

  async deleteResume(id) {
    if (!confirm(`Permanently delete resume #${id}?`)) return;
    try {
      await api.deleteResume(id);
      this.loadHistoryView();
    } catch (e) {
      alert(`Deletion failed: ${e.message}`);
    }
  }

  async loadProjectsView() {
    const grid = document.getElementById('projects-grid');
    const emptyCard = document.getElementById('projects-empty-card');
    const syncBtn = document.getElementById('btn-sync-projects');
    const syncStatus = document.getElementById('projects-sync-status');

    grid.innerHTML = '<p class="text-muted">Loading indexed projects...</p>';

    if (syncBtn) {
      syncBtn.onclick = async () => {
        syncBtn.disabled = true;
        syncStatus.textContent = "Scanning repositories with gitleaks and extracting evidence...";
        try {
          const res = await api.syncGithub();
          syncStatus.textContent = `Sync complete: ${res.indexed} indexed, ${res.unchanged} unchanged.`;
          this.loadProjectsView();
        } catch (e) {
          syncStatus.textContent = `Sync failed: ${e.message}`;
        } finally {
          syncBtn.disabled = false;
        }
      };
    }

    try {
      const projects = await api.getProjects();
      grid.innerHTML = '';

      if (!projects || projects.length === 0) {
        emptyCard.style.display = 'block';
        return;
      }
      emptyCard.style.display = 'none';

      projects.forEach(p => {
        let techList = [];
        try { techList = JSON.parse(p.technologies_json); } catch (_) {}

        const card = document.createElement('article');
        card.className = 'project-card';
        card.innerHTML = `
          <div class="project-card-header">
            <h3 class="project-name">${p.name}</h3>
            <span class="evidence-tag">Verified Evidence</span>
          </div>
          <p class="project-summary">${p.summary || 'Indexed repository facts and technical architecture.'}</p>
          <div class="project-tech-pills">
            ${techList.map(t => `<span class="tech-pill">${t}</span>`).join('')}
          </div>
        `;
        grid.appendChild(card);
      });
    } catch (e) {
      grid.innerHTML = `<p class="text-danger">Failed to load projects: ${e.message}</p>`;
    }
  }

  async loadSettingsView() {
    const sysList = document.getElementById('settings-system-list');
    const factsContainer = document.getElementById('settings-facts-container');
    const ghStatusContainer = document.getElementById('settings-gh-status');
    const latexTextarea = document.getElementById('settings-latex-source');
    const startersContainer = document.getElementById('settings-starters');

    sysList.innerHTML = '<p class="text-muted">Checking tool statuses...</p>';

    try {
      const [sysStatus, ghStatus, templateRes, facts, starters] = await Promise.all([
        api.getSystemStatus(),
        api.getGithubStatus(),
        api.getTemplate(),
        api.getFacts().catch(() => null),
        api.getStarters().catch(() => [])
      ]);

      // 1. Tool status list
      sysList.innerHTML = `
        <div class="settings-status-row">
          <div class="status-indicator">
            <span class="status-dot-sm ${sysStatus.agy.outcome === 'success' ? 'green' : 'red'}"></span>
            <strong>Antigravity AI</strong>
          </div>
          <span class="status-desc">${sysStatus.agy.details || 'Headless AI agent CLI'}</span>
          <span class="status-badge ${sysStatus.agy.outcome === 'success' ? 'status-success' : 'status-error'}">${sysStatus.agy.outcome === 'success' ? 'Ready' : 'Issue detected'}</span>
        </div>

        <div class="settings-status-row">
          <div class="status-indicator">
            <span class="status-dot-sm ${ghStatus.authenticated ? 'green' : 'amber'}"></span>
            <strong>GitHub Connection</strong>
          </div>
          <div class="status-desc">
            ${ghStatus.authenticated ? `Logged in as @${ghStatus.username || 'user'}` : `<span>Not connected. Run <code class="inline-code">gh auth login --web</code> in terminal</span>`}
          </div>
          <span class="status-badge ${ghStatus.authenticated ? 'status-success' : 'status-warning'}">${ghStatus.authenticated ? 'Connected' : 'Action needed'}</span>
        </div>

        <div class="settings-status-row">
          <div class="status-indicator">
            <span class="status-dot-sm ${sysStatus.tectonic.installed && sysStatus.tectonic.bundle_cached ? 'green' : 'red'}"></span>
            <strong>Tectonic LaTeX Engine</strong>
          </div>
          <span class="status-desc">${sysStatus.tectonic.details}</span>
          <span class="status-badge ${sysStatus.tectonic.installed && sysStatus.tectonic.bundle_cached ? 'status-success' : 'status-error'}">${sysStatus.tectonic.installed && sysStatus.tectonic.bundle_cached ? 'Ready' : 'Pending'}</span>
        </div>

        <div class="settings-status-row">
          <div class="status-indicator">
            <span class="status-dot-sm ${sysStatus.gitleaks?.installed ? 'green' : 'amber'}"></span>
            <strong>Gitleaks Secret Scanner</strong>
          </div>
          <span class="status-desc">${sysStatus.gitleaks?.details || 'Local secret detection'}</span>
          <span class="status-badge status-success">Active</span>
        </div>
      `;

      // 2. Extracted Master Facts
      if (facts && facts.candidate_name) {
        factsContainer.innerHTML = `
          <div class="facts-grid">
            <div class="fact-field">
              <span class="fact-label">Candidate Name</span>
              <span class="fact-value">${facts.candidate_name}</span>
            </div>
            ${facts.contact ? `
              <div class="fact-field">
                <span class="fact-label">Contact</span>
                <span class="fact-value">${facts.contact}</span>
              </div>
            ` : ''}
            ${facts.known_skills?.length ? `
              <div class="fact-field full-width">
                <span class="fact-label">Verified Known Skills (${facts.known_skills.length})</span>
                <div class="skill-pills">${facts.known_skills.map(s => `<span class="skill-pill">${s}</span>`).join('')}</div>
              </div>
            ` : ''}
            ${facts.work_history?.length ? `
              <div class="fact-field full-width">
                <span class="fact-label">Work History (${facts.work_history.length} entries)</span>
                <ul class="fact-list">
                  ${facts.work_history.map(w => `<li><strong>${w.title}</strong> at ${w.company} (${w.date_range}) — ${w.highlights.length} bullet points</li>`).join('')}
                </ul>
              </div>
            ` : ''}
          </div>
        `;
      } else {
        factsContainer.innerHTML = `<p class="text-muted">No master resume loaded. Select a starter template or upload your .tex file below.</p>`;
      }

      // 3. Raw LaTeX source editor
      if (latexTextarea && templateRes.content) {
        latexTextarea.value = templateRes.content;
      }

      // 4. Starters
      if (startersContainer) {
        startersContainer.innerHTML = '';
        starters.forEach(s => {
          const btn = document.createElement('button');
          btn.className = 'btn btn-secondary btn-sm mr-2 mb-2';
          btn.textContent = `Apply ${s.name}`;
          btn.onclick = async () => {
            if (confirm(`Apply starter template '${s.name}' as your active master resume?`)) {
              await api.saveTemplate(s.content, true, false);
              this.loadSettingsView();
              this.refreshReadiness();
            }
          };
          startersContainer.appendChild(btn);
        });
      }

      // Save raw LaTeX button
      const btnSaveLatex = document.getElementById('btn-save-master-latex');
      if (btnSaveLatex) {
        btnSaveLatex.onclick = async () => {
          btnSaveLatex.disabled = true;
          try {
            await api.saveTemplate(latexTextarea.value, false, false);
            alert("Master resume template saved successfully.");
            this.loadSettingsView();
            this.refreshReadiness();
          } catch (e) {
            alert(`Save failed: ${e.message}`);
          } finally {
            btnSaveLatex.disabled = false;
          }
        };
      }

    } catch (e) {
      sysList.innerHTML = `<p class="text-danger">Failed to load system settings: ${e.message}</p>`;
    }
  }
}

window.appController = new AppController();
document.addEventListener('DOMContentLoaded', () => {
  window.appController.init();
});
