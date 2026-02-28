import type { APIRoute } from 'astro';

const CONTROL_PLANE_URL = import.meta.env.CONTROL_PLANE_URL || 'http://127.0.0.1:8800';

export const DELETE: APIRoute = async ({ params }) => {
  const { repo_id } = params;

  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repo_id}`, {
    method: 'DELETE',
  });

  const data = await res.json();
  return new Response(JSON.stringify(data), {
    status: res.status,
    headers: { 'Content-Type': 'application/json' },
  });
};

export const POST: APIRoute = async ({ params, request }) => {
  const { repo_id } = params;
  const body = await request.json();

  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repo_id}/update`, {
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
