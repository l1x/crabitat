const sectionCounts: Record<string, string> = {};

document.querySelectorAll<HTMLElement>('.nav-item[data-section]').forEach((item) => {
  const badge = item.querySelector('.count-badge');
  if (badge) {
    sectionCounts[item.dataset.section!] = badge.textContent ?? '';
  }
});

document.querySelectorAll<HTMLElement>('.nav-item[data-section]').forEach((navItem) => {
  navItem.addEventListener('click', () => {
    const section = navItem.dataset.section;
    if (!section) return;

    document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
    navItem.classList.add('active');

    document.querySelectorAll<HTMLElement>('.section[data-section]').forEach((s) => {
      s.hidden = s.dataset.section !== section;
    });

    const titleEl = document.getElementById('page-title');
    const countEl = document.getElementById('page-count');
    if (titleEl) {
      titleEl.textContent = navItem.textContent?.trim().replace(/\d+$/, '').trim() ?? '';
    }
    if (countEl) {
      const count = sectionCounts[section];
      countEl.textContent = count ?? '';
      countEl.style.display = count ? '' : 'none';
    }
  });
});
