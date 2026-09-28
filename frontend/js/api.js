// api.js — ResumeForge API client utilities
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

  getQueryParam(param) {
    const params = new URLSearchParams(window.location.search);
    return params.get(param);
  }
};
