document.getElementById('create-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const button = form.querySelector('button[type="submit"]');
  const message = document.getElementById('create-message');
  const data = Object.fromEntries(new FormData(form).entries());
  if (!data.job_description.trim()) { message.textContent = 'Paste a job description first.'; return; }
  button.disabled = true;
  message.textContent = 'Adding generation to the queue…';
  try {
    const resume = await api.post('/api/resumes', data);
    window.location.href = `resume.html?id=${encodeURIComponent(resume.id)}`;
  } catch (error) {
    message.textContent = error.message;
    button.disabled = false;
  }
});
