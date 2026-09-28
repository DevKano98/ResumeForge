const healthList = document.getElementById('health-list');
const masterContent = document.getElementById('master-content');
const masterMessage = document.getElementById('master-message');
let selectedStarter = false;

async function refreshHealth() {
  try {
    const status = await api.get('/api/system/status');
    healthList.replaceChildren();
    const entries = [
      ['SQLite', status.sqlite.connected && status.sqlite.foreign_keys_enabled, status.sqlite.details],
      ['GitHub CLI', status.gh.authenticated, status.gh.details],
      ['Antigravity', status.agy.outcome === 'success', status.agy.details],
      ['Tectonic', status.tectonic.installed && status.tectonic.bundle_cached, status.tectonic.details],
      ['Gitleaks', status.gitleaks ? status.gitleaks.installed : false, status.gitleaks ? status.gitleaks.details : 'Not checked'],
    ];
    for (const [name, ready, detail] of entries) {
      const row = document.createElement('div'); row.className = 'status-row';
      const label = document.createElement('strong'); label.textContent = `${ready ? '●' : '○'} ${name}`;
      const description = document.createElement('span'); description.textContent = detail;
      row.append(label, description); healthList.append(row);
    }
  } catch (error) { healthList.textContent = error.message; }
}

async function loadMasterFacts() {
  const panel = document.getElementById('master-facts-panel');
  const container = document.getElementById('master-facts-content');
  if (!panel || !container) return;
  try {
    const facts = await api.get('/api/template/facts');
    if (!facts || (!facts.candidate_name && (!facts.known_skills || facts.known_skills.length === 0) && (!facts.work_history || facts.work_history.length === 0))) {
      panel.style.display = 'none';
      return;
    }
    panel.style.display = 'block';
    container.replaceChildren();

    const addField = (label, value) => {
      if (!value) return;
      const row = document.createElement('div'); row.className = 'status-row';
      const lbl = document.createElement('strong'); lbl.textContent = label;
      const val = document.createElement('span'); val.textContent = value;
      row.append(lbl, val); container.append(row);
    };

    addField('Candidate Name:', facts.candidate_name);
    if (facts.contact) addField('Contact:', facts.contact);
    if (facts.current_summary) addField('Summary:', facts.current_summary);
    if (facts.known_skills && facts.known_skills.length > 0) {
      addField('Known Skills:', facts.known_skills.join(', '));
    }
    if (facts.work_history && facts.work_history.length > 0) {
      const expDiv = document.createElement('div');
      expDiv.style.marginTop = '0.5rem';
      expDiv.innerHTML = `<strong>Work History (${facts.work_history.length}):</strong>`;
      for (const item of facts.work_history) {
        const p = document.createElement('p');
        p.style.margin = '0.25rem 0 0.25rem 1rem';
        p.style.fontSize = '0.85rem';
        p.textContent = `• ${item.title} at ${item.company} (${item.date_range}): ${item.highlights.length} bullet(s)`;
        expDiv.appendChild(p);
      }
      container.appendChild(expDiv);
    }
    if (facts.education && facts.education.length > 0) {
      const eduDiv = document.createElement('div');
      eduDiv.style.marginTop = '0.5rem';
      eduDiv.innerHTML = `<strong>Education (${facts.education.length}):</strong>`;
      for (const item of facts.education) {
        const p = document.createElement('p');
        p.style.margin = '0.25rem 0 0.25rem 1rem';
        p.style.fontSize = '0.85rem';
        p.textContent = `• ${item.degree} — ${item.institution}`;
        eduDiv.appendChild(p);
      }
      container.appendChild(eduDiv);
    }
  } catch (err) {
    console.warn('Could not load master facts:', err);
  }
}

async function loadTemplates() {
  try {
    const [master, starters] = await Promise.all([api.get('/api/template'), api.get('/api/templates/starters')]);
    if (master.content) masterContent.value = master.content;
    const select = document.getElementById('starter-select');
    for (const starter of starters) {
      const option = document.createElement('option'); option.value = starter.id; option.textContent = starter.name;
      select.append(option);
    }
    select.addEventListener('change', () => {
      const starter = starters.find(item => item.id === select.value);
      if (starter) { masterContent.value = starter.content; selectedStarter = true; }
    });
  } catch (error) { masterMessage.textContent = error.message; }
}

document.getElementById('save-master').addEventListener('click', async () => {
  const button = document.getElementById('save-master');
  button.disabled = true; masterMessage.textContent = 'Validating LaTeX and saving…';
  try {
    const result = await api.post('/api/template', {content: masterContent.value, is_starter_template: selectedStarter});
    masterMessage.textContent = `Saved master version ${result.id}.`;
    selectedStarter = false;
    await loadMasterFacts();
  } catch (error) { masterMessage.textContent = error.message; }
  finally { button.disabled = false; }
});
document.getElementById('probe-button').addEventListener('click', async () => {
  const message = document.getElementById('probe-message'); message.textContent = 'Running headless probe…';
  try { const result = await api.get('/api/system/antigravity-probe'); message.textContent = result.details; await refreshHealth(); }
  catch (error) { message.textContent = error.message; }
});
refreshHealth(); loadTemplates(); loadMasterFacts();

let adaptedPreview = null;
document.getElementById('adapt-preview').addEventListener('click', async () => {
  const message = document.getElementById('adapt-message');
  const button = document.getElementById('adapt-preview');
  button.disabled = true; message.textContent = 'Adapting and compiling preview…';
  document.getElementById('adapt-save').disabled = true;
  try {
    adaptedPreview = await api.post('/api/template/adapt', {content: document.getElementById('paste-content').value});
    document.getElementById('adapt-diff').textContent = adaptedPreview.diff;
    masterContent.value = adaptedPreview.adapted_content;
    document.getElementById('adapt-save').disabled = false;
    message.textContent = 'Review the adapted LaTeX in the master editor above, then save it.';
  } catch (error) { adaptedPreview = null; message.textContent = error.message; }
  finally { button.disabled = false; }
});
document.getElementById('adapt-save').addEventListener('click', async () => {
  if (!adaptedPreview) return;
  const message = document.getElementById('adapt-message');
  if (masterContent.value !== adaptedPreview.adapted_content) {
    message.textContent = 'The preview changed. Run adaptation again before saving.'; return;
  }
  try {
    const saved = await api.post('/api/template', {content: adaptedPreview.adapted_content, adapted_from_paste: true});
    message.textContent = `Saved adapted master version ${saved.id}.`;
    document.getElementById('adapt-save').disabled = true;
    await loadMasterFacts();
  } catch (error) { message.textContent = error.message; }
});
