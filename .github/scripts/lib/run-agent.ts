import { Agent, CursorAgentError } from "@cursor/sdk";
import type { CIContext } from "./fetch-logs.ts";

export interface AgentOptions {
  apiKey: string;
  prompt: string;
  context: CIContext;
}

export async function runFixAgent({
  apiKey,
  prompt,
  context,
}: AgentOptions): Promise<void> {
  const { branch, repo } = context;

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
