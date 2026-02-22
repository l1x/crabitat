function setupSearch(inputId: string, containerId: string) {
  const input = document.getElementById(inputId) as HTMLInputElement | null;
  const container = document.getElementById(containerId);
  if (!input || !container) return;

  input.addEventListener('input', () => {
    const query = input.value.toLowerCase().trim();
    container.querySelectorAll<HTMLElement>('[data-searchable]').forEach((el) => {
      const text = el.dataset.searchable?.toLowerCase() ?? '';
      el.style.display = !query || text.includes(query) ? '' : 'none';
    });
  });
}

function setupFilterPills(filterGroup: string, containerId: string) {
  const pillContainer = document.querySelector<HTMLElement>(`[data-filter-group="${filterGroup}"]`);
  const container = document.getElementById(containerId);
  if (!pillContainer || !container) return;

  pillContainer.querySelectorAll<HTMLElement>('.filter-pill').forEach((pill) => {
    pill.addEventListener('click', () => {
      pillContainer.querySelectorAll('.filter-pill').forEach((p) => p.classList.remove('active'));
      pill.classList.add('active');

      const filter = pill.dataset.filter ?? 'all';
      const attr = container.closest('[data-section="crabs"]') ? 'data-state' : 'data-status';

      container.querySelectorAll<HTMLElement>(`[${attr}]`).forEach((el) => {
        if (filter === 'all') {
          el.style.display = '';
        } else {
          el.style.display = el.getAttribute(attr) === filter ? '' : 'none';
        }
      });
    });
  });
}

function setupSort(selectId: string, containerId: string) {
  const select = document.getElementById(selectId) as HTMLSelectElement | null;
  const container = document.getElementById(containerId);
  if (!select || !container) return;

  select.addEventListener('change', () => {
    const ascending = select.value === 'oldest';
    const items = Array.from(container.children) as HTMLElement[];

    items.sort((a, b) => {
      const aMs = Number(a.dataset.createdAtMs ?? a.dataset.updatedAtMs ?? 0);
      const bMs = Number(b.dataset.createdAtMs ?? b.dataset.updatedAtMs ?? 0);
      return ascending ? aMs - bMs : bMs - aMs;
    });

    items.forEach((item) => container.appendChild(item));
  });
}

// Initialize all search/filter/sort on load
setupSearch('search-colonies', 'colony-container');
setupSearch('search-crabs', 'crab-container');
setupSearch('search-missions', 'mission-container');
setupSearch('search-tasks', 'task-container');
setupSearch('search-runs', 'run-container');

setupFilterPills('crabs', 'crab-container');
setupFilterPills('tasks', 'task-container');
setupFilterPills('runs', 'run-container');

setupSort('sort-missions', 'mission-container');
setupSort('sort-tasks', 'task-container');
setupSort('sort-runs', 'run-container');
