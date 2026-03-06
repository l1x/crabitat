import type { APIRoute } from "astro";
import { createWorkflow } from "../../../lib/api-client";

export const POST: APIRoute = async ({ request, redirect }) => {
  const form = await request.formData();
  const repo_id = form.get("repo_id") as string;
  const name = form.get("name") as string;
  const description = (form.get("description") as string) || "";

  // Collect steps from numbered form fields
  const steps: { name: string; prompt_template: string }[] = [];
  let i = 0;
  while (form.has(`step_name_${i}`)) {
    steps.push({
      name: form.get(`step_name_${i}`) as string,
      prompt_template: form.get(`step_prompt_${i}`) as string,
    });
    i++;
  }

  try {
    await createWorkflow(repo_id, { name, description, steps });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return redirect("/workflows?error=" + encodeURIComponent(msg));
  }

  return redirect("/workflows");
};
