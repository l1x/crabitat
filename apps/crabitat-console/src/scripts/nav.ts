const sectionTitles: Record<string, string> = {
  issues: 'Issues',
  missions: 'Missions',
  agents: 'Agents',
  messages: 'Messages',
};

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
    if (titleEl) {
      titleEl.textContent = sectionTitles[section] ?? section;
    }
  });
});
