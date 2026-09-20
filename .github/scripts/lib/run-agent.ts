import { Agent, CursorAgentError } from "@cursor/sdk";
import type { CIContext } from "./fetch-logs.ts";

export interface AgentOptions {
  apiKey: string;
  prompt: string;
  context: CIContext;
}

async function verifyBranchAtSha(repo: string, branch: string, sha: string): Promise<void> {
  const token = process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;
  if (!token) throw new Error("Cannot verify Cursor branch without GITHUB_TOKEN");
  if (!/^[A-Za-z0-9._/-]+$/.test(branch)) throw new Error("Refusing an unsafe branch name");
  const response = await fetch(`https://api.github.com/repos/${repo}/git/ref/heads/${branch}`, {
    headers: { Accept: "application/vnd.github+json", Authorization: `Bearer ${token}`, "X-GitHub-Api-Version": "2022-11-28" },
  });
  if (!response.ok) throw new Error(`Cannot verify branch ${branch} (${response.status})`);
  const payload = (await response.json()) as { object?: { sha?: string } };
  if (payload.object?.sha !== sha) throw new Error(`Refusing remediation: branch ${branch} moved from ${sha} to ${payload.object?.sha ?? "unknown"}`);
}

export async function runFixAgent({
  apiKey,
  prompt,
  context,
}: AgentOptions): Promise<void> {
  const { branch, repo, sha } = context;
  if (!repo || !branch || !sha) throw new Error("Cursor remediation requires repository, branch, and SHA context");
  // The SDK accepts a branch rather than a commit pin. Refuse a moved branch.
  await verifyBranchAtSha(repo, branch, sha);

  let result;
  try {
    result = await Agent.prompt(prompt, {
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
  } catch (err) {
    if (err instanceof CursorAgentError) {
      console.error(
        `Agent failed to start: ${err.message} (retryable=${err.isRetryable})`
      );
      process.exit(1);
    }
    throw err;
  }

  console.log("Agent status:", result.status);
  if (result.result) console.log(result.result);

  if (result.status === "error") {
    console.error("Agent run completed with error status.");
    process.exit(2);
  }
}
