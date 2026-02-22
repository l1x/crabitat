import type { APIRoute } from 'astro';

const CONTROL_PLANE_URL = import.meta.env.CONTROL_PLANE_URL || 'http://127.0.0.1:8800';

export const POST: APIRoute = async ({ request }) => {
  const body = await request.json();

  const res = await fetch(`${CONTROL_PLANE_URL}/v1/missions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  const data = await res.json();
  return new Response(JSON.stringify(data), {
    status: res.status,
    headers: { 'Content-Type': 'application/json' },
  });
};
