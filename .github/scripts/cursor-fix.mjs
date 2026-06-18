/**
 * Runs a Cursor cloud agent to analyze CI test failures and open a fix PR.
 *
 * Required env vars (all injected by the workflow):
 *   CURSOR_API_KEY   - Cursor API key (team service-account or personal)
 *   FAILURE_LOGS     - Raw text of the failed test output
 *   HEAD_BRANCH      - Branch where the tests failed
 *   GITHUB_REPOSITORY - owner/repo (automatically set by GitHub Actions)
 */

import { Agent, CursorAgentError } from "@cursor/sdk";

const apiKey = process.env.CURSOR_API_KEY;
const failureLogs = process.env.FAILURE_LOGS ?? "";
const branch = process.env.HEAD_BRANCH ?? "main";
const repo = process.env.GITHUB_REPOSITORY ?? "";

if (!apiKey) {
  console.error("CURSOR_API_KEY is not set");
  process.exit(1);
}

const prompt = `
You are analyzing a Rust CI failure in the meta-secret-core repository.
The tests ran on branch \`${branch}\`.

## Failing test output

\`\`\`
${failureLogs.slice(0, 8000)}
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
`;

try {
  const result = await Agent.prompt(prompt, {
    apiKey,
    model: { id: "composer-2.5" },
    cloud: {
      repos: [
        {
          remote: `https://github.com/${repo}`,
          branch,
        },
      ],
      autoCreatePR: true,
      skipReviewerRequest: true,
    },
  });

  console.log("Agent status:", result.status);
  if (result.result) console.log(result.result);

  if (result.status === "error") {
    console.error("Agent run completed with error status.");
    process.exit(2);
  }
} catch (err) {
  if (err instanceof CursorAgentError) {
    console.error(
      `Agent failed to start: ${err.message} (retryable=${err.isRetryable})`
    );
    process.exit(1);
  }
  throw err;
}
