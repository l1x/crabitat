import type { APIRoute } from 'astro';

const CONTROL_PLANE_URL = import.meta.env.CONTROL_PLANE_URL || 'http://127.0.0.1:8800';

export const GET: APIRoute = async ({ params, url }) => {
  const { repo_id } = params;
  const refresh = url.searchParams.get('refresh');
  const qs = refresh === 'true' ? '?refresh=true' : '';

  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repo_id}/issues${qs}`);
  const data = await res.json();

  return new Response(JSON.stringify(data), {
    status: res.status,
    headers: { 'Content-Type': 'application/json' },
  });
};
