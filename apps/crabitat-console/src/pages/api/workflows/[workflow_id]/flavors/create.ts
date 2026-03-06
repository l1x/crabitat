import type { APIRoute } from "astro";
import { createFlavor } from "../../../../../lib/api-client";

export const POST: APIRoute = async ({ params, request, redirect }) => {
  const { workflow_id } = params;
  const form = await request.formData();
  const name = form.get("name") as string;
  const context = (form.get("context") as string) || undefined;

  try {
    await createFlavor(workflow_id!, { name, context });
  } catch {
    // Silently redirect back on error
  }

  return redirect("/workflows");
};
