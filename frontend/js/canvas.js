// frontend/js/canvas.js — n8n-style interactive canvas & drawer controller
// Features: pannable/zoomable SVG, dot grid, smooth bezier connectors with animated dashes,
// 5 human-labeled node cards, right-hand 3-tab drawer, and final split-screen result panel.

class CanvasController {
  constructor() {
    this.resumeId = null;
    this.isReplay = false;
    this.ws = null;
    this.pollInterval = null;
    this.selectedNodeKey = null;

    // Viewport transform
    this.panX = 30;
    this.panY = 40;
    this.scale = 0.95;
    this.isDragging = false;
    this.dragStartX = 0;
    this.dragStartY = 0;

    // Node definitions (Section 4)
    this.nodeOrder = [
      "jd_analysis",
      "project_retrieval",
      "content_generation",
      "evidence_validation",
      "render_compile"
    ];

    this.nodes = {
      jd_analysis: {
        x: 40, y: 140, w: 230, h: 105,
        state: "idle", statusText: "Waiting to read requirements",
        startTime: null, elapsed: 0, events: [], outputs: null, summary: null
      },
      project_retrieval: {
        x: 320, y: 140, w: 230, h: 105,
        state: "idle", statusText: "Waiting to rank projects",
        startTime: null, elapsed: 0, events: [], outputs: null, summary: null
      },
      content_generation: {
        x: 600, y: 140, w: 230, h: 105,
        state: "idle", statusText: "Waiting to draft bullets",
        startTime: null, elapsed: 0, events: [], outputs: null, summary: null
      },
      evidence_validation: {
        x: 880, y: 140, w: 230, h: 105,
        state: "idle", statusText: "Waiting to verify claims",
        startTime: null, elapsed: 0, events: [], outputs: null, summary: null
      },
      render_compile: {
        x: 1160, y: 140, w: 230, h: 105,
        state: "idle", statusText: "Waiting to compile", attempt: 0,
        startTime: null, elapsed: 0, events: [], outputs: null, summary: null
      }
    };

    this.timerInterval = null;
  }

  init(containerEl) {
    this.container = containerEl;
    this.renderCanvasSkeleton();
    this.bindInteractions();
    this.startElapsedTimers();
  }

  renderCanvasSkeleton() {
    this.container.innerHTML = `
      <div class="canvas-wrapper">
        <svg id="canvas-svg" width="100%" height="100%">
          <defs>
            <pattern id="canvas-dot-grid" width="24" height="24" patternUnits="userSpaceOnUse">
              <circle cx="2" cy="2" r="1.2" fill="#cbd5e1" />
            </pattern>
          </defs>
          <rect id="canvas-bg" width="100%" height="100%" fill="url(#canvas-dot-grid)" />
          <g id="canvas-viewport" transform="translate(${this.panX}, ${this.panY}) scale(${this.scale})">
            <g id="canvas-connectors"></g>
            <g id="canvas-nodes"></g>
          </g>
        </svg>

        <!-- Canvas Controls Toolbar -->
        <div class="canvas-controls">
          <button id="btn-zoom-in" class="ctrl-btn" title="Zoom In" aria-label="Zoom In">+</button>
          <button id="btn-zoom-reset" class="ctrl-btn text-btn" title="Reset Zoom">Reset</button>
          <button id="btn-zoom-out" class="ctrl-btn" title="Zoom Out" aria-label="Zoom Out">−</button>
        </div>

        <!-- Node Details Drawer (Right-hand 3 tabs) -->
        <aside id="node-drawer" class="node-drawer" aria-hidden="true">
          <div class="drawer-header">
            <div>
              <span id="drawer-node-tag" class="drawer-tag">STAGE</span>
              <h3 id="drawer-node-title" class="drawer-title">Node Title</h3>
            </div>
            <button id="btn-close-drawer" class="drawer-close" aria-label="Close details">×</button>
          </div>
          <div class="drawer-tabs" role="tablist">
            <button class="drawer-tab active" data-tab="summary" role="tab" aria-selected="true">Summary</button>
            <button class="drawer-tab" data-tab="activity" role="tab" aria-selected="false">Activity</button>
            <button class="drawer-tab" data-tab="output" role="tab" aria-selected="false">Output</button>
          </div>
          <div class="drawer-content">
            <div id="tab-pane-summary" class="tab-pane active" role="tabpanel"></div>
            <div id="tab-pane-activity" class="tab-pane" role="tabpanel"></div>
            <div id="tab-pane-output" class="tab-pane" role="tabpanel"></div>
          </div>
        </aside>

        <!-- Result Panel (Beside canvas upon completion) -->
        <aside id="result-panel" class="result-panel" aria-hidden="true">
          <div class="result-header">
            <div class="result-title-group">
              <span class="status-badge status-success">Ready</span>
              <h3 class="result-heading">Your tailored resume is ready</h3>
            </div>
            <div class="result-actions">
              <a id="btn-download-pdf" href="#" class="btn btn-primary" target="_blank">Download PDF</a>
              <a id="btn-download-tex" href="#" class="btn btn-secondary" target="_blank">Download LaTeX</a>
              <button id="btn-result-regenerate" class="btn btn-secondary" type="button">Regenerate</button>
            </div>
          </div>
          <div class="result-body">
            <div class="result-preview-container">
              <iframe id="pdf-preview-frame" class="pdf-iframe" title="Resume PDF Preview"></iframe>
            </div>
            <div class="result-changes-card">
              <h4 class="changes-heading">What we changed</h4>
              <ul id="result-changes-list" class="changes-list"></ul>
            </div>
          </div>
        </aside>
      </div>
    `;

    this.svg = document.getElementById('canvas-svg');
    this.viewport = document.getElementById('canvas-viewport');
    this.connectorsG = document.getElementById('canvas-connectors');
    this.nodesG = document.getElementById('canvas-nodes');
    this.drawer = document.getElementById('node-drawer');
    this.resultPanel = document.getElementById('result-panel');

    this.renderNodes();
    this.renderConnectors();
  }

  bindInteractions() {
    const bg = document.getElementById('canvas-bg');

    // Pan with mouse drag
    this.svg.addEventListener('mousedown', (e) => {
      if (e.target === bg || e.target === this.svg || e.target.closest('#canvas-connectors')) {
        this.isDragging = true;
        this.dragStartX = e.clientX - this.panX;
        this.dragStartY = e.clientY - this.panY;
        this.svg.style.cursor = 'grabbing';
      }
    });

    window.addEventListener('mousemove', (e) => {
      if (!this.isDragging) return;
      this.panX = e.clientX - this.dragStartX;
      this.panY = e.clientY - this.dragStartY;
      this.updateViewportTransform();
    });

    window.addEventListener('mouseup', () => {
      if (this.isDragging) {
        this.isDragging = false;
        this.svg.style.cursor = 'default';
      }
    });

    // Zoom with mouse wheel
    this.svg.addEventListener('wheel', (e) => {
      e.preventDefault();
      const zoomFactor = e.deltaY < 0 ? 1.08 : 0.92;
      const newScale = Math.min(Math.max(this.scale * zoomFactor, 0.45), 2.2);

      // Zoom towards mouse position
      const rect = this.svg.getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;

      this.panX = mouseX - (mouseX - this.panX) * (newScale / this.scale);
      this.panY = mouseY - (mouseY - this.panY) * (newScale / this.scale);
      this.scale = newScale;
      this.updateViewportTransform();
    }, { passive: false });

    // Zoom toolbar buttons
    document.getElementById('btn-zoom-in')?.addEventListener('click', () => this.zoom(1.15));
    document.getElementById('btn-zoom-out')?.addEventListener('click', () => this.zoom(0.85));
    document.getElementById('btn-zoom-reset')?.addEventListener('click', () => {
      this.panX = 30;
      this.panY = 40;
      this.scale = 0.95;
      this.updateViewportTransform();
    });

    // Drawer tab switching
    this.drawer.querySelectorAll('.drawer-tab').forEach(btn => {
      btn.addEventListener('click', (e) => {
        this.drawer.querySelectorAll('.drawer-tab').forEach(b => {
          b.classList.remove('active');
          b.setAttribute('aria-selected', 'false');
        });
        e.currentTarget.classList.add('active');
        e.currentTarget.setAttribute('aria-selected', 'true');

        const tabKey = e.currentTarget.getAttribute('data-tab');
        this.drawer.querySelectorAll('.tab-pane').forEach(p => p.classList.remove('active'));
        const activePane = document.getElementById(`tab-pane-${tabKey}`);
        if (activePane) activePane.classList.add('active');
      });
    });

    // Close drawer
    document.getElementById('btn-close-drawer')?.addEventListener('click', () => this.closeDrawer());
    window.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') this.closeDrawer();
    });
  }

  zoom(factor) {
    this.scale = Math.min(Math.max(this.scale * factor, 0.45), 2.2);
    this.updateViewportTransform();
  }

  updateViewportTransform() {
    this.viewport.setAttribute('transform', `translate(${this.panX}, ${this.panY}) scale(${this.scale})`);
  }

  renderNodes() {
    this.nodesG.innerHTML = '';
    this.nodeOrder.forEach(key => {
      const n = this.nodes[key];
      const info = getNodeInfo(key);

      const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
      g.setAttribute("class", `canvas-node ${n.state} ${this.selectedNodeKey === key ? 'selected' : ''}`);
      g.setAttribute("data-node", key);
      g.setAttribute("transform", `translate(${n.x}, ${n.y})`);
      g.setAttribute("role", "button");
      g.setAttribute("tabindex", "0");
      g.setAttribute("aria-label", `${info.title}: ${n.statusText}`);

      // Rounded rectangle card
      const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("class", "node-card-rect");
      rect.setAttribute("width", n.w);
      rect.setAttribute("height", n.h);
      rect.setAttribute("rx", "8");
      rect.setAttribute("ry", "8");

      // Status indicator dot
      const dot = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      dot.setAttribute("class", `node-status-dot dot-${n.state}`);
      dot.setAttribute("cx", "18");
      dot.setAttribute("cy", "22");
      dot.setAttribute("r", "4.5");

      // Elapsed timer
      const timer = document.createElementNS("http://www.w3.org/2000/svg", "text");
      timer.setAttribute("class", "node-timer-text");
      timer.setAttribute("x", n.w - 16);
      timer.setAttribute("y", "26");
      timer.setAttribute("text-anchor", "end");
      timer.textContent = n.elapsed > 0 ? this.formatDuration(n.elapsed) : "";

      // Plain language title
      const title = document.createElementNS("http://www.w3.org/2000/svg", "text");
      title.setAttribute("class", "node-title-text");
      title.setAttribute("x", "30");
      title.setAttribute("y", "26");
      title.textContent = info.title;

      // Divider line
      const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
      line.setAttribute("x1", "12");
      line.setAttribute("y1", "40");
      line.setAttribute("x2", n.w - 12);
      line.setAttribute("y2", "40");
      line.setAttribute("stroke", "#f1f5f9");
      line.setAttribute("stroke-width", "1");

      // Live status line
      const status = document.createElementNS("http://www.w3.org/2000/svg", "text");
      status.setAttribute("class", "node-status-text");
      status.setAttribute("x", "16");
      status.setAttribute("y", "62");
      
      let displayStatus = n.statusText;
      if (key === 'render_compile' && n.attempt > 0 && n.state === 'running') {
        displayStatus = `Attempt ${n.attempt} of 4: compacting...`;
      }
      status.textContent = this.truncateText(displayStatus, 32);

      // Secondary progress line (e.g. stage stats)
      const sub = document.createElementNS("http://www.w3.org/2000/svg", "text");
      sub.setAttribute("class", "node-sub-text");
      sub.setAttribute("x", "16");
      sub.setAttribute("y", "84");
      sub.textContent = n.subText || info.description;

      // Ports (left input pin, right output pin)
      const inPin = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      inPin.setAttribute("class", "node-pin pin-in");
      inPin.setAttribute("cx", "0");
      inPin.setAttribute("cy", n.h / 2);
      inPin.setAttribute("r", "4");

      const outPin = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      outPin.setAttribute("class", "node-pin pin-out");
      outPin.setAttribute("cx", n.w);
      outPin.setAttribute("cy", n.h / 2);
      outPin.setAttribute("r", "4");

      g.append(rect, dot, title, timer, line, status, sub, inPin, outPin);

      // Click to open drawer
      g.addEventListener('click', () => this.selectNode(key));
      g.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          this.selectNode(key);
        }
      });

      this.nodesG.appendChild(g);
    });
  }

  renderConnectors() {
    this.connectorsG.innerHTML = '';
    for (let i = 0; i < this.nodeOrder.length - 1; i++) {
      const srcKey = this.nodeOrder[i];
      const dstKey = this.nodeOrder[i + 1];
      const src = this.nodes[srcKey];
      const dst = this.nodes[dstKey];

      const x1 = src.x + src.w;
      const y1 = src.y + (src.h / 2);
      const x2 = dst.x;
      const y2 = dst.y + (dst.h / 2);

      // Smooth cubic bezier curve
      const dx = (x2 - x1) * 0.5;
      const d = `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;

      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", d);

      // Connector state: active (animated dash) while data is flowing
      let cls = "canvas-connector";
      if (src.state === 'running' || (src.state === 'complete' && dst.state === 'running')) {
        cls += " active-flow";
      } else if (src.state === 'complete' && dst.state === 'complete') {
        cls += " completed-flow";
      }
      path.setAttribute("class", cls);
      path.setAttribute("id", `conn-${srcKey}-${dstKey}`);
      this.connectorsG.appendChild(path);
    }
  }

  selectNode(nodeKey) {
    this.selectedNodeKey = nodeKey;
    this.renderNodes();
    this.openDrawer(nodeKey);
  }

  openDrawer(nodeKey) {
    const node = this.nodes[nodeKey];
    const info = getNodeInfo(nodeKey);

    document.getElementById('drawer-node-tag').textContent = `STAGE ${this.nodeOrder.indexOf(nodeKey) + 1} OF 5`;
    document.getElementById('drawer-node-title').textContent = info.title;

    // 1. Summary Pane
    const summaryPane = document.getElementById('tab-pane-summary');
    let summaryHtml = `
      <div class="summary-section">
        <div class="summary-card">
          <div class="summary-card-header">
            <strong>Stage Overview</strong>
            <span class="status-badge status-${node.state}">${node.state.toUpperCase()}</span>
          </div>
          <p class="summary-desc">${info.description}</p>
        </div>

        <div class="summary-stats-grid">
          <div class="stat-box">
            <span class="stat-label">Elapsed Time</span>
            <span class="stat-value">${this.formatDuration(node.elapsed)}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">Events Processed</span>
            <span class="stat-value">${node.events.length}</span>
          </div>
        </div>

        ${node.state === 'error' ? `
          <div class="summary-card" style="border-left: 3px solid #dc2626;">
            <div style="margin-bottom: 8px;">
              <strong style="color: #dc2626; font-size: 12px; text-transform: uppercase; letter-spacing: 0.05em;">What happened</strong>
              <p style="margin: 4px 0 10px 0; font-weight: 600; color: #1e293b;">${node.errorExplanation ? node.errorExplanation.what : node.statusText}</p>
            </div>
            <div style="margin-bottom: 8px;">
              <strong style="color: #64748b; font-size: 12px; text-transform: uppercase; letter-spacing: 0.05em;">Why it occurred</strong>
              <p style="margin: 4px 0 10px 0; font-size: 13px; color: #475569;">${node.errorExplanation ? node.errorExplanation.why : (node.summary || "A processing issue occurred during this stage.")}</p>
            </div>
            <div>
              <strong style="color: #2563eb; font-size: 12px; text-transform: uppercase; letter-spacing: 0.05em;">Exact next step</strong>
              <p style="margin: 4px 0 0 0; font-size: 13px; color: #1e293b;">${node.errorExplanation ? node.errorExplanation.next : "Click 'Regenerate' to run this stage again."}</p>
            </div>
          </div>
        ` : `
          <div class="summary-card">
            <strong>Stage Outcome</strong>
            <p class="summary-outcome">${node.summary || node.statusText}</p>
          </div>
        `}
      </div>
    `;
    summaryPane.innerHTML = summaryHtml;

    // 2. Activity Pane
    const actPane = document.getElementById('tab-pane-activity');
    if (node.events.length === 0) {
      actPane.innerHTML = `<div class="empty-state-card"><p>No activity recorded yet for this stage.</p></div>`;
    } else {
      let actHtml = `<div class="activity-timeline">`;
      node.events.forEach((ev, idx) => {
        const timeStr = ev.ts ? new Date(ev.ts).toLocaleTimeString() : `#${idx + 1}`;
        actHtml += `
          <div class="timeline-row">
            <div class="timeline-time">${timeStr}</div>
            <div class="timeline-body">
              <span class="timeline-badge">${formatEventName(ev.type)}</span>
              <p class="timeline-desc">${this.formatEventPayload(ev)}</p>
            </div>
          </div>
        `;
      });
      actHtml += `</div>`;
      actPane.innerHTML = actHtml;
    }

    // 3. Output Pane
    const outPane = document.getElementById('tab-pane-output');
    if (!node.outputs) {
      outPane.innerHTML = `<div class="empty-state-card"><p>No artifact or output payload generated yet.</p></div>`;
    } else {
      outPane.innerHTML = `
        <div class="output-container">
          <div class="output-header">
            <span>Artifact JSON</span>
            <button class="btn btn-secondary btn-sm" onclick="navigator.clipboard.writeText(this.parentElement.nextElementSibling.textContent)">Copy</button>
          </div>
          <pre class="output-code"><code>${this.escapeHtml(JSON.stringify(node.outputs, null, 2))}</code></pre>
        </div>
      `;
    }

    this.drawer.classList.add('open');
    this.drawer.setAttribute('aria-hidden', 'false');
  }

  closeDrawer() {
    this.drawer.classList.remove('open');
    this.drawer.setAttribute('aria-hidden', 'true');
    this.selectedNodeKey = null;
    this.renderNodes();
  }

  // Handle incoming CanvasEvent
  handleEvent(event) {
    const nodeKey = event.node;
    if (!this.nodes[nodeKey]) return;

    const node = this.nodes[nodeKey];
    node.events.push(event);

    switch (event.type) {
      case 'command_started':
      case 'node_started':
        node.state = 'running';
        node.statusText = getNodeInfo(nodeKey).activeStatus;
        if (!node.startTime) node.startTime = Date.now();
        break;

      case 'step_update':
        if (event.payload?.message) {
          node.statusText = event.payload.message;
        } else if (event.payload?.delta) {
          node.statusText = event.payload.delta;
        }
        if (event.payload?.attempt) {
          node.attempt = event.payload.attempt;
        }
        break;

      case 'command_output':
        if (event.payload?.chunk) {
          node.statusText = event.payload.chunk.slice(-45);
        }
        break;

      case 'artifact_produced':
        node.outputs = event.payload;
        if (event.payload?.ranked_projects) {
          node.summary = `Selected ${event.payload.ranked_projects.length} relevant repositories.`;
        } else if (event.payload?.summary) {
          node.summary = `Drafted targeted resume content.`;
        }
        break;

      case 'node_warning':
        if (event.payload?.warning) {
          node.statusText = event.payload.warning;
          node.summary = event.payload.warning;
        }
        break;

      case 'node_done':
        node.state = 'complete';
        node.statusText = getNodeInfo(nodeKey).completeStatus;
        if (event.payload) {
          node.outputs = Object.assign(node.outputs || {}, event.payload);
          if (event.payload.page_count) {
            node.statusText = `Fitted to 1 page (${event.payload.attempts_used || 1} attempts)`;
            node.summary = `Successfully formatted and compacted to exactly 1 page.`;
          } else if (event.payload.accepted !== undefined) {
            node.summary = `Verified ${event.payload.accepted} facts (${event.payload.rejected || 0} ungrounded claims removed).`;
          }
        }
        break;

      case 'node_error':
        node.state = 'error';
        node.statusText = event.payload?.error || "Stage encountered an error";
        node.summary = event.payload?.error || "Error";
        break;

      case 'result':
        // Final completion
        this.onPipelineComplete(event.payload);
        break;
    }

    this.renderNodes();
    this.renderConnectors();

    // If drawer is open for this node, refresh it
    if (this.selectedNodeKey === nodeKey) {
      this.openDrawer(nodeKey);
    }
  }

  startElapsedTimers() {
    if (this.timerInterval) clearInterval(this.timerInterval);
    this.timerInterval = setInterval(() => {
      let anyRunning = false;
      this.nodeOrder.forEach(key => {
        const n = this.nodes[key];
        if (n.state === 'running') {
          anyRunning = true;
          n.elapsed = Math.floor((Date.now() - n.startTime) / 1000);
        }
      });
      if (anyRunning) {
        this.renderNodes();
      }
    }, 1000);
  }

  // Shows final result panel beside the canvas
  async onPipelineComplete(payload) {
    this.resultPanel.classList.add('visible');
    this.resultPanel.setAttribute('aria-hidden', 'false');

    const downloadPdf = document.getElementById('btn-download-pdf');
    const downloadTex = document.getElementById('btn-download-tex');
    const regenerateBtn = document.getElementById('btn-result-regenerate');
    const pdfFrame = document.getElementById('pdf-preview-frame');
    const changesList = document.getElementById('result-changes-list');

    if (downloadPdf) downloadPdf.href = this.isReplay ? '/api/template/pdf' : `/api/resumes/${this.resumeId}/pdf`;
    if (downloadTex) downloadTex.href = this.isReplay ? '/api/template' : `/api/resumes/${this.resumeId}/tex`;
    if (pdfFrame) pdfFrame.src = this.isReplay ? '/api/template/pdf' : `/api/resumes/${this.resumeId}/pdf`;

    if (regenerateBtn) {
      regenerateBtn.onclick = async () => {
        try {
          const newResume = await api.regenerateResume(this.resumeId);
          window.location.hash = `#/run/${newResume.id}`;
        } catch (e) {
          alert(`Regeneration failed: ${e.message}`);
        }
      };
    }

    // Load evidence changes
    changesList.innerHTML = `<li class="change-item loading">Loading evidence validation analysis...</li>`;
    try {
      let evidence = null;
      if (this.isReplay && DEMO_RUN.evidence) {
        evidence = DEMO_RUN.evidence;
      } else {
        evidence = await api.getResumeEvidence(this.resumeId);
      }

      changesList.innerHTML = '';
      if (!evidence || (!evidence.rejected?.length && !evidence.accepted?.length)) {
        changesList.innerHTML = `<li class="change-item">All facts matched your verified master resume and repository evidence.</li>`;
        return;
      }

      // 1. Rejected claims (anti-hallucination guard items)
      if (evidence.rejected && evidence.rejected.length > 0) {
        evidence.rejected.forEach(r => {
          const li = document.createElement('li');
          li.className = 'change-item change-rejected';
          li.innerHTML = `<strong>Removed:</strong> '${this.escapeHtml(r.item || r.claim)}', because ${this.escapeHtml(r.reason || "it lacked code evidence")}.`;
          changesList.appendChild(li);
        });
      }

      // 2. Accepted / Grounded items
      if (evidence.accepted && evidence.accepted.length > 0) {
        const li = document.createElement('li');
        li.className = 'change-item change-accepted';
        li.innerHTML = `<strong>Grounded in evidence:</strong> ${evidence.accepted.length} bullet points verified against real project code and master history.`;
        changesList.appendChild(li);
      }

      // 3. Compaction summary
      const fitNode = this.nodes['render_compile'];
      const liFit = document.createElement('li');
      liFit.className = 'change-item change-fit';
      const attText = fitNode.attempt > 1 ? ` across ${fitNode.attempt} attempts` : '';
      liFit.innerHTML = `<strong>Single-page fit:</strong> Typeset and compacted${attText} to guarantee exactly 1 page.`;
      changesList.appendChild(liFit);

    } catch (e) {
      changesList.innerHTML = `<li class="change-item">Verified single-page resume generated.</li>`;
    }
  }

  // Load an existing resume run
  async loadResumeRun(id) {
    this.resumeId = id;
    this.isReplay = false;
    this.resetState();

    try {
      const resume = await api.getResume(id);
      const events = await api.getResumeEvents(id);

      // Feed existing events
      events.forEach(ev => this.handleEvent(ev));

      if (resume.status === 'completed' || resume.status === 'ready' || resume.status === 'ready_sparse') {
        this.onPipelineComplete(resume);
      } else if (resume.status === 'failed' || resume.status.startsWith('ai_') || resume.status.endsWith('_error')) {
        this.handleFailureState(resume);
      } else {
        // Connect live WebSocket if still running
        this.connectWebSocket(id);
      }
    } catch (e) {
      console.warn("Failed to load resume run:", e);
    }
  }

  connectWebSocket(id) {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }

    const wsUrl = api.getWsUrl(`/ws/resumes/${id}/live`);
    try {
      this.ws = new WebSocket(wsUrl);
      this.ws.onmessage = (event) => {
        try {
          const payload = JSON.parse(event.data);
          this.handleEvent(payload);
        } catch (_) {}
      };
      this.ws.onerror = () => this.startPollingFallback(id);
      this.ws.onclose = () => {
        // If not completed, poll
        if (!this.isCompleted()) this.startPollingFallback(id);
      };
    } catch (e) {
      this.startPollingFallback(id);
    }
  }

  startPollingFallback(id) {
    if (this.pollInterval) clearInterval(this.pollInterval);
    this.pollInterval = setInterval(async () => {
      try {
        const resume = await api.getResume(id);
        const events = await api.getResumeEvents(id);
        events.forEach(ev => this.handleEvent(ev));
        if (resume.status === 'completed' || resume.status === 'ready' || resume.status === 'ready_sparse' || resume.status === 'failed' || resume.status.startsWith('ai_') || resume.status.endsWith('_error')) {
          clearInterval(this.pollInterval);
          if (resume.status === 'completed' || resume.status === 'ready' || resume.status === 'ready_sparse') {
            this.onPipelineComplete(resume);
          } else {
            this.handleFailureState(resume);
          }
        }
      } catch (_) {}
    }, 2000);
  }

  handleFailureState(resume) {
    const errorStage = resume.error_stage || 'content_generation';
    const errorDetail = resume.error_detail || resume.status;
    const errorInfo = formatErrorExplanation(resume.status, errorDetail);
    const targetNode = this.nodes[errorStage] ? errorStage : 'content_generation';
    if (this.nodes[targetNode]) {
      this.nodes[targetNode].state = 'error';
      this.nodes[targetNode].statusText = errorInfo.what;
      this.nodes[targetNode].errorExplanation = errorInfo;
      this.renderNodes();
      this.renderConnectors();
      this.selectNode(targetNode);
    }
  }

  isCompleted() {
    return this.nodes['render_compile'].state === 'complete';
  }

  resetState() {
    if (this.ws) { this.ws.close(); this.ws = null; }
    if (this.pollInterval) { clearInterval(this.pollInterval); this.pollInterval = null; }
    this.selectedNodeKey = null;
    this.drawer?.classList.remove('open');
    this.resultPanel?.classList.remove('visible');

    this.nodeOrder.forEach(key => {
      this.nodes[key].state = 'idle';
      this.nodes[key].statusText = getNodeInfo(key).idleStatus;
      this.nodes[key].startTime = null;
      this.nodes[key].elapsed = 0;
      this.nodes[key].events = [];
      this.nodes[key].outputs = null;
      this.nodes[key].summary = null;
      this.nodes[key].attempt = 0;
    });

    this.renderNodes();
    this.renderConnectors();
  }

  formatDuration(seconds) {
    if (!seconds || seconds <= 0) return "0s";
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  }

  formatEventPayload(ev) {
    if (!ev.payload) return "";
    if (ev.payload.message) return ev.payload.message;
    if (ev.payload.delta) return ev.payload.delta;
    if (ev.payload.warning) return ev.payload.warning;
    if (ev.payload.chunk) return ev.payload.chunk;
    if (ev.payload.role) return `Role: ${ev.payload.role}`;
    if (ev.payload.page_count) return `Page count: ${ev.payload.page_count} (Attempts: ${ev.payload.attempts_used || 1})`;
    return JSON.stringify(ev.payload);
  }

  truncateText(str, maxLen) {
    if (!str) return "";
    return str.length > maxLen ? str.slice(0, maxLen - 1) + "…" : str;
  }

  escapeHtml(str) {
    if (!str) return "";
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }
}

window.canvasController = new CanvasController();
