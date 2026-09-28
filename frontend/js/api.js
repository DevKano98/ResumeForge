// frontend/js/api.js — Clean REST client for ResumeForge
const api = {
  async get(url) {
    const res = await fetch(url);
    if (!res.ok) {
      let errText = await res.text();
      try {
        const json = JSON.parse(errText);
        if (json.error) errText = json.error;
      } catch (_) {}
      throw new Error(`GET ${url} failed (${res.status}): ${errText}`);
    }
    return res.json();
  },

  async post(url, data) {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    if (!res.ok) {
      let errText = await res.text();
      try {
        const json = JSON.parse(errText);
        if (json.error) errText = json.error;
      } catch (_) {}
      throw new Error(`POST ${url} failed (${res.status}): ${errText}`);
    }
    return res.json();
  },

  async delete(url) {
    const res = await fetch(url, { method: 'DELETE' });
    if (!res.ok) {
      let errText = await res.text();
      try {
        const json = JSON.parse(errText);
        if (json.error) errText = json.error;
      } catch (_) {}
      throw new Error(`DELETE ${url} failed (${res.status}): ${errText}`);
    }
    return res.json();
  },

  getWsUrl(path) {
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${proto}//${window.location.host}${path}`;
  },

  // High-level API convenience helpers
  getSystemStatus: () => api.get('/api/system/status'),
  getGithubStatus: () => api.get('/api/github/status'),
  syncGithub: (repos = []) => api.post('/api/github/sync', { repos }),
  getProjects: () => api.get('/api/projects'),
  getTemplate: () => api.get('/api/template'),
  getFacts: () => api.get('/api/template/facts'),
  getStarters: () => api.get('/api/templates/starters'),
  saveTemplate: (content, is_starter_template = false, adapted_from_paste = false) =>
    api.post('/api/template', { content, is_starter_template, adapted_from_paste }),
  listResumes: () => api.get('/api/resumes'),
  createResume: (data) => api.post('/api/resumes', data),
  getResume: (id) => api.get(`/api/resumes/${id}`),
  getResumeEvents: (id) => api.get(`/api/resumes/${id}/events`),
  getResumeEvidence: (id) => api.get(`/api/resumes/${id}/evidence`),
  deleteResume: (id) => api.delete(`/api/resumes/${id}`),
  regenerateResume: (id) => api.post(`/api/resumes/${id}/regenerate`, {})
};
