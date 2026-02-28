import type { APIRoute } from 'astro';
import { readdir, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { homedir } from 'node:os';

export const GET: APIRoute = async ({ url }) => {
  const raw = url.searchParams.get('path') || homedir();
  const dir = resolve(raw);

  try {
    const info = await stat(dir);
    if (!info.isDirectory()) {
      return new Response(JSON.stringify({ error: 'Not a directory' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    const entries = await readdir(dir, { withFileTypes: true });
    const dirs: string[] = [];
    let isGitRepo = false;

    for (const entry of entries) {
      if (entry.name === '.git') {
        isGitRepo = true;
      }
      if (entry.isDirectory() && !entry.name.startsWith('.')) {
        dirs.push(entry.name);
      }
    }

    dirs.sort((a, b) => a.localeCompare(b));

    return new Response(
      JSON.stringify({ path: dir, dirs, isGitRepo }),
      { status: 200, headers: { 'Content-Type': 'application/json' } },
    );
  } catch {
    return new Response(JSON.stringify({ error: `Cannot read: ${dir}` }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    });
  }
};
