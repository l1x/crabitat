import type { APIRoute } from 'astro';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

interface GhRepoEntry {
  nameWithOwner: string;
  description: string;
  primaryLanguage: { name: string } | null;
  defaultBranchRef: { name: string } | null;
}

export const GET: APIRoute = async () => {
  try {
    const { stdout } = await execFileAsync('gh', [
      'repo',
      'list',
      '--json',
      'nameWithOwner,description,primaryLanguage,defaultBranchRef',
      '--limit',
      '100',
    ]);

    const raw: GhRepoEntry[] = JSON.parse(stdout);

    const repos = raw.map((r) => ({
      full_name: r.nameWithOwner,
      description: r.description || '',
      language: r.primaryLanguage?.name || '',
      default_branch: r.defaultBranchRef?.name || 'main',
    }));

    return new Response(JSON.stringify(repos), {
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (e: unknown) {
    const message = e instanceof Error ? e.message : String(e);
    return new Response(JSON.stringify({ error: `gh CLI failed: ${message}` }), {
      status: 502,
      headers: { 'Content-Type': 'application/json' },
    });
  }
};
