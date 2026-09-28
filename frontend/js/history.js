const historyList = document.getElementById('history-list');
const historyMessage = document.getElementById('history-message');
async function loadHistory() {
  try {
    const resumes = await api.get('/api/resumes');
    historyList.replaceChildren();
    if (!resumes.length) { historyList.textContent = 'No resumes yet.'; return; }
    for (const resume of resumes) {
      const card = document.createElement('article'); card.className = 'item-card';
      const title = document.createElement('a'); title.href = `resume.html?id=${resume.id}`;
      title.textContent = `#${resume.id} ${resume.role || 'Resume'} ${resume.company ? `at ${resume.company}` : ''}`;
      const detail = document.createElement('p'); detail.textContent = `${resume.status} · ${resume.created_at}${resume.parent_resume_id ? ` · from #${resume.parent_resume_id}` : ''}`;
      const actions = document.createElement('div'); actions.className = 'btn-group';
      const regenerate = document.createElement('button'); regenerate.className = 'btn btn-secondary'; regenerate.textContent = 'Regenerate';
      regenerate.addEventListener('click', async () => {
        regenerate.disabled = true;
        try { const next = await api.post(`/api/resumes/${resume.id}/regenerate`, {}); window.location.href = `resume.html?id=${next.id}`; }
        catch (error) { historyMessage.textContent = error.message; regenerate.disabled = false; }
      });
      const remove = document.createElement('button'); remove.className = 'btn btn-secondary'; remove.textContent = 'Delete';
      remove.addEventListener('click', async () => {
        if (!window.confirm(`Permanently delete resume #${resume.id} and its files?`)) return;
        try { await api.delete(`/api/resumes/${resume.id}`); await loadHistory(); }
        catch (error) { historyMessage.textContent = error.message; }
      });
      actions.append(regenerate, remove); card.append(title, detail, actions); historyList.append(card);
    }
  } catch (error) { historyMessage.textContent = error.message; }
}
loadHistory();
