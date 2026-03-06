import type { APIRoute } from "astro";
import { createRepo } from "../../../lib/api-client";

export const POST: APIRoute = async ({ request, redirect }) => {
  const form = await request.formData();
  const owner = form.get("owner") as string;
  const name = form.get("name") as string;
  const local_path = form.get("local_path") as string;

  try {
    await createRepo({ owner, name, local_path });
  } catch (e: any) {
    // For now, just redirect back — error handling can be improved later
    return redirect("/repos?error=" + encodeURIComponent(e.message));
  }

  return redirect("/repos");
};
