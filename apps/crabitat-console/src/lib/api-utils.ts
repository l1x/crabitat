export function json(data: unknown, init: ResponseInit = {}): Response {
  const headers = new Headers(init.headers);
  if (!headers.has('content-type')) {
    headers.set('content-type', 'application/json; charset=utf-8');
  }

  return new Response(JSON.stringify(data, null, 2), {
    ...init,
    headers,
  });
}

export async function readJson<T>(request: Request): Promise<T> {
  try {
    return (await request.json()) as T;
  } catch {
    throw new Error('Invalid JSON body');
  }
}

export function badRequest(message: string): Response {
  return json(
    {
      ok: false,
      error: message,
    },
    { status: 400 },
  );
}

export function methodNotAllowed(allowed: string[]): Response {
  return json(
    {
      ok: false,
      error: `Method not allowed. Allowed: ${allowed.join(', ')}`,
    },
    {
      status: 405,
      headers: {
        allow: allowed.join(', '),
      },
    },
  );
}
