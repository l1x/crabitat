# Create PRD

## Objective

Generate a PRD in Markdown format from a user prompt. The PRD must be specific enough for task decomposition into tickets.

## Process

1. **Analyze Input** - Read the user feature request and provided context.
2. **Identify Gaps** - Determine if critical information is missing (data requirements, integration points, specific user flows).
3. **Clarify (If Needed)** - If gaps exist, ask 3-5 clarifying questions.
   - Format: Numbered list. Open-ended questions preferred over multiple-choice to avoid missing context.
   - Example: "1. Which authentication provider should be used for this login flow?"
   - **STOP. Wait for user response before proceeding.**
4. **Generate PRD** - If no critical gaps exist, generate the document using the structure below.
5. **Output** - Provide the output as a Markdown code block with the suggested filename.

## PRD Structure

### Required Sections

1. **Overview** - Feature description, problem statement, goal
2. **Goals** - Measurable objectives (3-5 max)
3. **User Stories** - Format: "As [role], I want [action], so that [benefit]"
4. **Functional Requirements** - Numbered list with acceptance criteria per item
5. **Non-functional Requirements** - Numbered list with acceptance criteria per item

```
   FR-1: [Requirement]
   - Acceptance: [Verifiable condition]

   NFR-1: [Requirement]
   - Acceptance: [Verifiable condition]
```

6. **Non-Goals** - Explicit exclusions
7. **Success Metrics** - Key metrics to track post-launch success (e.g., adoption rate, performance improvement, error reduction). Include both product and business metrics where possible.

### Optional Sections

1. **Design Considerations** - UI/UX constraints, mockups
2. **Technical Constraints** - Dependencies, environment, execution context
3. **Open Questions** - Unresolved items
4. **Diagram** - Mermaid diagram illustrating the process if applicable

## Output

- **Format:** Markdown
- **File Naming Convention:** `prds/prd-[kebab-case-feature-name]-[YYYY-MM-DD].md`

## Constraints

- Respect project's existing tech stack
- Do NOT implement, planning only
- Target reader: junior developer
