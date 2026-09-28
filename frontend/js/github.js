const githubStatus = document.getElementById('github-status');
const syncMessage = document.getElementById('sync-message');
const projectsList = document.getElementById('projects-list');

async function loadStatus() {
  try {
    const status = await api.get('/api/github/status');
    githubStatus.textContent = status.authenticated
      ? `Connected as ${status.username || 'GitHub user'}. Scopes: ${status.scopes.join(', ') || 'not reported'}.`
      : status.details;
    document.getElementById('sync-button').disabled = !status.authenticated;
  } catch (error) { githubStatus.textContent = error.message; }
}
async function loadProjects() {
  try {
    const projects = await api.get('/api/projects');
    projectsList.replaceChildren();
    if (!projects.length) { projectsList.textContent = 'No indexed projects yet.'; return; }
    for (const project of projects) {
      const card = document.createElement('article'); card.className = 'item-card';
      const title = document.createElement('h3'); title.textContent = project.name;
      const summary = document.createElement('p'); summary.textContent = project.summary || 'No repository description.';
      const tech = document.createElement('small');
      try { tech.textContent = JSON.parse(project.technologies_json).join(' · '); } catch (_) {}
      card.append(title, summary, tech); projectsList.append(card);
    }
  } catch (error) { projectsList.textContent = error.message; }
}
document.getElementById('sync-button').addEventListener('click', async (event) => {
  const button = event.currentTarget; button.disabled = true; syncMessage.textContent = 'Cloning and scanning repositories…';
  try {
    const report = await api.post('/api/github/sync', {});
    syncMessage.textContent = `${report.indexed} indexed, ${report.unchanged} unchanged, ${report.errors.length} errors.` +
      (report.errors.length ? ` ${report.errors.join(' | ')}` : '');
    await loadProjects();
  } catch (error) { syncMessage.textContent = error.message; }
  finally { button.disabled = false; }
});
loadStatus(); loadProjects();
