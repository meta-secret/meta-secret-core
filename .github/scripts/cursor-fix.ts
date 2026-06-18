/**
 * Cursor auto-fix entrypoint.
 *
 * Required env vars (injected by the GitHub Actions workflow):
 *   CURSOR_API_KEY    - Cursor API key (team service-account or personal)
 *   HEAD_BRANCH       - Branch where the tests failed
 *   GITHUB_REPOSITORY - owner/repo  (set automatically by GitHub Actions)
 *
 * The workflow writes raw `gh run view --log-failed` output to
 * /tmp/failure_logs.txt before running this script.
 */

import { fetchCIContext } from "./lib/fetch-logs.ts";
import { buildPrompt } from "./lib/build-prompt.ts";
import { runFixAgent } from "./lib/run-agent.ts";

const apiKey = process.env.CURSOR_API_KEY;
if (!apiKey) {
  console.error("CURSOR_API_KEY is not set");
  process.exit(1);
}

const context = fetchCIContext();
const prompt = buildPrompt(context);

await runFixAgent({ apiKey, prompt, context });
