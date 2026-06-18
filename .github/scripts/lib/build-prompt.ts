import type { CIContext } from "./fetch-logs.ts";

export function buildPrompt({ failureLogs, branch }: CIContext): string {
  return `
You are analyzing a Rust CI failure in the meta-secret-core repository.
The tests ran on branch \`${branch}\`.

## Failing test output

\`\`\`
${failureLogs}
\`\`\`

## Your task

1. Identify the root cause of every failing test shown above.
2. Fix only the source code (under \`meta-secret/\`). Do not modify test files
   unless the test itself is clearly wrong.
3. Make the minimal change that makes the tests pass.
4. After applying the fix, your changes will automatically be submitted as a
   pull request. Write a clear, concise PR title and description that explains
   what you changed and why.

Rules:
- Do not reformat unrelated code.
- Do not bump dependency versions unless the error clearly requires it.
- Do not log secrets, key material, or raw shares.
`.trim();
}
