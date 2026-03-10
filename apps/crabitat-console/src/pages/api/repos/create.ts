import type { APIRoute } from "astro";
import { createRepo } from "../../../lib/api-client";

export const POST: APIRoute = async ({ request, redirect }) => {
  const form = await request.formData();
  const owner = form.get("owner") as string;
  const name = form.get("name") as string;
  const local_path = form.get("local_path") as string;

  try {
    await createRepo({ owner, name, local_path, repo_url: null });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return redirect("/repos?error=" + encodeURIComponent(msg));
  }

  return redirect("/repos");
};
