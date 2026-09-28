// canvas.js — Live Orchestration Canvas Client for ResumeForge
document.addEventListener('DOMContentLoaded', () => {
  const resumeId = api.getQueryParam('id') || '1';

  // DOM Elements
  const displayId = document.getElementById('display-id');
  const resumeTitle = document.getElementById('resume-title');
  const resumeStatusBadge = document.getElementById('resume-status-badge');
  const wsStatusBadge = document.getElementById('ws-status-badge');
  const btnViewPdf = document.getElementById('btn-view-pdf');
  const btnViewTex = document.getElementById('btn-view-tex');
  const btnReconnect = document.getElementById('btn-reconnect');
  const btnClearConsole = document.getElementById('btn-clear-console');
  const chkAutoscroll = document.getElementById('chk-autoscroll');
  const consoleBody = document.getElementById('console-body');
  const errorDetail = document.getElementById('resume-error');
  const eventCountSpan = document.getElementById('event-count');

  const TERMINAL_STATUSES = new Set([
    'ready',
    'ready_sparse',
    'github_error',
    'ai_empty_response',
    'ai_tool_denied',
    'ai_timeout',
    'ai_hung',
    'ai_invalid_json',
    'ai_error',
    'latex_error',
    'page_limit_error',
    'validation_error',
    'cancelled',
  ]);

  let ws = null;
  let eventCount = 0;
  let lastSeenSeq = 0;
  let reconnectTimeout = null;
  let isTerminal = false;
  let currentBackoffMs = 1000;
  const MAX_BACKOFF_MS = 15000;

  displayId.textContent = resumeId;

  // 1. Load initial resume metadata
  async function loadResumeMetadata() {
    try {
      const resume = await api.get(`/api/resumes/${resumeId}`);
      if (resume) {
        let title = `Resume #${resume.id}`;
        if (resume.company || resume.role) {
          title += ` — ${resume.role || ''} ${resume.company ? '@ ' + resume.company : ''}`;
        }
        resumeTitle.textContent = title;
        updateResumeStatusBadge(resume.status);
        errorDetail.textContent = resume.error_detail || '';

        if (TERMINAL_STATUSES.has(resume.status)) {
          isTerminal = true;
          if (reconnectTimeout) {
            clearTimeout(reconnectTimeout);
            reconnectTimeout = null;
          }
          if (ws && ws.readyState === WebSocket.OPEN) {
            wsStatusBadge.textContent = 'WS: Complete';
            wsStatusBadge.className = 'status-badge ready';
          }
        }

        if (resume.status === 'ready' || resume.status === 'ready_sparse') {
          btnViewPdf.href = `/api/resumes/${resume.id}/pdf`;
          btnViewPdf.style.display = 'inline-flex';
          btnViewTex.href = `/api/resumes/${resume.id}/tex`;
          btnViewTex.style.display = 'inline-flex';
        }
        try {
          const evidence = await api.get(`/api/resumes/${resumeId}/evidence`);
          const panel = document.getElementById('evidence-panel');
          panel.hidden = false;
          const acceptedCount = evidence.accepted ? evidence.accepted.length : 0;
          const rejectedCount = evidence.rejected ? evidence.rejected.length : 0;
          document.getElementById('evidence-summary').textContent = `${acceptedCount} accepted, ${rejectedCount} rejected items`;
          const rejected = document.getElementById('evidence-rejected'); rejected.replaceChildren();

          if (evidence.warnings && evidence.warnings.length > 0) {
            for (const warn of evidence.warnings) {
              const row = document.createElement('p');
              row.style.color = '#eab308';
              row.style.fontWeight = 'bold';
              row.textContent = `⚠️ Warning: ${warn}`;
              rejected.append(row);
            }
          }

          if (evidence.rejected) {
            for (const item of evidence.rejected) {
              const row = document.createElement('p');
              const sec = item.section ? `[${item.section}] ` : '';
              const target = item.item || item.bullet || item.skill || item.term || (item.bullet ? item.bullet.text : '') || item.project_id || '';
              row.textContent = `${sec}${item.reason}: ${typeof target === 'object' ? JSON.stringify(target) : target}`;
              rejected.append(row);
            }
          }
        } catch (_) {}
      }
    } catch (err) {
      console.warn('Could not load resume metadata:', err.message);
    }
  }

  function updateResumeStatusBadge(status) {
    resumeStatusBadge.textContent = status || 'unknown';
    resumeStatusBadge.className = `status-badge ${status || 'queued'}`;
  }

  // 2. Node state pills management
  function updateNodeState(node, state) {
    const pill = document.getElementById(`pill-${node}`);
    if (pill) {
      pill.textContent = state;
      pill.className = `node-pill ${state}`;
    }
    const graphNode = document.querySelector(`.graph-node[data-node="${node}"]`);
    if (graphNode) graphNode.setAttribute('class', `graph-node ${state}`);
    const edge = document.querySelector(`.graph-edge[data-after="${node}"]`);
    if (edge && state === 'done') edge.setAttribute('class', 'graph-edge active');
  }

  // 3. Append event to live scrolling console
  function appendEventToConsole(event) {
    eventCount++;
    eventCountSpan.textContent = eventCount;

    const entry = document.createElement('div');
    entry.className = `log-entry ${event.type}`;

    const timeStr = event.ts ? new Date(event.ts).toLocaleTimeString() : new Date().toLocaleTimeString();

    // Meta line
    const meta = document.createElement('div');
    meta.className = 'log-meta';
    meta.innerHTML = `
      <span class="meta-seq">#${event.seq}</span>
      <span class="meta-time">${timeStr}</span>
      <span class="meta-node">${escapeHtml(event.node)}</span>
      <span class="meta-type ${event.type}">${escapeHtml(event.type)}</span>
    `;
    entry.appendChild(meta);

    // Payload content line
    const content = document.createElement('div');
    content.className = 'log-content';

    if (event.payload) {
      if (typeof event.payload === 'string') {
        content.textContent = event.payload;
      } else if (event.payload.chunk) {
        content.textContent = event.payload.chunk;
      } else if (event.payload.stage && event.payload.detail) {
        content.innerHTML = `<strong style="color:var(--accent-red);">${escapeHtml(event.payload.stage)}:</strong> ${escapeHtml(event.payload.detail)}`;
      } else if (event.payload.kind === 'json' && event.payload.content) {
        const pre = document.createElement('pre');
        pre.className = 'log-payload-box';
        try {
          const parsed = JSON.parse(event.payload.content);
          pre.textContent = JSON.stringify(parsed, null, 2);
        } catch (_) {
          pre.textContent = event.payload.content;
        }
        content.appendChild(pre);
      } else if (event.payload.kind === 'latex_diff' && event.payload.content) {
        const pre = document.createElement('pre'); pre.className = 'log-payload-box';
        for (const line of event.payload.content.split('\n')) {
          if (!line) continue;
          const span = document.createElement('span');
          span.className = line.startsWith('+') ? 'diff-added' : line.startsWith('-') ? 'diff-removed' : '';
          span.textContent = `${line}\n`; pre.append(span);
        }
        content.append(pre);
      } else {
        const pre = document.createElement('pre');
        pre.className = 'log-payload-box';
        pre.textContent = JSON.stringify(event.payload, null, 2);
        content.appendChild(pre);
      }
    }

    entry.appendChild(content);
    consoleBody.appendChild(entry);

    if (chkAutoscroll.checked) {
      consoleBody.scrollTop = consoleBody.scrollHeight;
    }

    // Update node status based on event type
    if (event.type === 'command_started') {
      updateNodeState(event.node, 'running');
    } else if (event.type === 'node_done') {
      updateNodeState(event.node, 'done');
      if (event.node === 'render_compile') {
        isTerminal = true;
        loadResumeMetadata();
      }
    } else if (event.type === 'node_error') {
      updateNodeState(event.node, 'error');
      updateResumeStatusBadge('error');
      isTerminal = true;
      loadResumeMetadata();
    } else if (event.type === 'artifact_produced') {
      updateNodeState(event.node, 'done');
      if (event.node === 'render_compile') {
        loadResumeMetadata();
      }
    }
  }

  function escapeHtml(str) {
    if (!str) return '';
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }

  // 4. Connect WebSocket with capped backoff and terminal shutoff
  function connectWebSocket() {
    if (ws) {
      try { ws.close(); } catch (_) {}
    }

    const wsUrl = api.getWsUrl(`/ws/resumes/${resumeId}/live`);
    wsStatusBadge.textContent = 'WS: Connecting...';
    wsStatusBadge.className = 'status-badge queued';

    ws = new WebSocket(wsUrl);

    ws.onopen = () => {
      wsStatusBadge.textContent = isTerminal ? 'WS: Complete' : 'WS: Connected';
      wsStatusBadge.className = isTerminal ? 'status-badge ready' : 'status-badge connected';
      currentBackoffMs = 1000;
      if (reconnectTimeout) {
        clearTimeout(reconnectTimeout);
        reconnectTimeout = null;
      }
    };

    ws.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data);
        if (event && typeof event.seq === 'number') {
          if (event.seq > lastSeenSeq) {
            lastSeenSeq = event.seq;
            appendEventToConsole(event);
          }
        }
      } catch (err) {
        console.error('Error parsing canvas event:', err, e.data);
      }
    };

    ws.onclose = () => {
      if (isTerminal) {
        wsStatusBadge.textContent = 'WS: Complete';
        wsStatusBadge.className = 'status-badge ready';
        return;
      }

      wsStatusBadge.textContent = `WS: Disconnected (retry in ${Math.round(currentBackoffMs / 1000)}s)...`;
      wsStatusBadge.className = 'status-badge disconnected';

      if (!reconnectTimeout) {
        reconnectTimeout = setTimeout(() => {
          reconnectTimeout = null;
          currentBackoffMs = Math.min(currentBackoffMs * 2, MAX_BACKOFF_MS);
          connectWebSocket();
        }, currentBackoffMs);
      }
    };

    ws.onerror = (err) => {
      console.warn('WebSocket error:', err);
    };
  }

  // Button actions
  btnReconnect.addEventListener('click', () => {
    isTerminal = false;
    currentBackoffMs = 1000;
    connectWebSocket();
  });

  btnClearConsole.addEventListener('click', () => {
    consoleBody.innerHTML = '';
    eventCount = 0;
    eventCountSpan.textContent = '0';
  });

  document.getElementById('btn-regenerate').addEventListener('click', async () => {
    try { const next = await api.post(`/api/resumes/${resumeId}/regenerate`, {}); window.location.href = `resume.html?id=${next.id}`; }
    catch (error) { errorDetail.textContent = error.message; }
  });

  // Initialize
  loadResumeMetadata();
  connectWebSocket();
  setInterval(() => { if (!isTerminal) loadResumeMetadata(); }, 2000);
});
