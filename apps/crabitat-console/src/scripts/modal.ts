const dialog = document.getElementById('new-mission-modal') as HTMLDialogElement | null;
const ctaBtn = document.getElementById('cta-btn');
const closeBtn = document.getElementById('modal-close-btn');
const cancelBtn = document.getElementById('modal-cancel-btn');
const form = document.getElementById('new-mission-form') as HTMLFormElement | null;

if (dialog && ctaBtn) {
  ctaBtn.addEventListener('click', () => dialog.showModal());
}

if (dialog && closeBtn) {
  closeBtn.addEventListener('click', () => dialog.close());
}

if (dialog && cancelBtn) {
  cancelBtn.addEventListener('click', () => dialog.close());
}

// Close on backdrop click
if (dialog) {
  dialog.addEventListener('click', (e) => {
    if (e.target === dialog) dialog.close();
  });
}

if (form && dialog) {
  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const formData = new FormData(form);
    const colony_id = formData.get('colony_id') as string;
    const prompt = formData.get('prompt') as string;

    if (!colony_id || !prompt) return;

    const submitBtn = form.querySelector<HTMLButtonElement>('[type="submit"]');
    if (submitBtn) {
      submitBtn.disabled = true;
      submitBtn.textContent = 'Creating...';
    }

    try {
      const res = await fetch('/api/missions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ colony_id, prompt }),
      });

      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: 'Unknown error' }));
        alert(`Failed to create mission: ${err.error ?? res.statusText}`);
        return;
      }

      form.reset();
      dialog.close();
    } finally {
      if (submitBtn) {
        submitBtn.disabled = false;
        submitBtn.textContent = 'Create Mission';
      }
    }
  });
}
