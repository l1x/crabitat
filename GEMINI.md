# Crabitat Agentic Tool Mapping

To ensure consistency across the control plane and worker environments, the following naming conventions are used for agentic tools:

| Tool | Internal Name / CLI |
| :--- | :--- |
| Gemini | `gemini` |
| Claude | `claude` |
| ChatGPT | `codex` |

These mappings should be used when configuring `environment_paths` or passing the `--agent` flag to `crabitat-crab`.
