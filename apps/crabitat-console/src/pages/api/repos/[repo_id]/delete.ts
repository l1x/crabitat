import type { APIRoute } from "astro";
import { deleteRepo } from "../../../../lib/api-client";

export const POST: APIRoute = async ({ params, redirect }) => {
  const { repo_id } = params;

  try {
    await deleteRepo(repo_id!);
  } catch {
    // Silently redirect back on error
  }

  return redirect("/repos");
};
