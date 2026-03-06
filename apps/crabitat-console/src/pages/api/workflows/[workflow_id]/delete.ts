import type { APIRoute } from "astro";
import { deleteWorkflow } from "../../../../lib/api-client";

export const POST: APIRoute = async ({ params, redirect }) => {
  const { workflow_id } = params;

  try {
    await deleteWorkflow(workflow_id!);
  } catch {
    // Silently redirect back on error
  }

  return redirect("/workflows");
};
