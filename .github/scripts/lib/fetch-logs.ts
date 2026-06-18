/**
 * Reads failure logs and context from environment variables.
 * The workflow writes the raw `gh run view --log-failed` output to
 * /tmp/failure_logs.txt and exports HEAD_BRANCH before running this script.
 */

import { readFileSync } from "fs";

const LOG_FILE = "/tmp/failure_logs.txt";
const MAX_LOG_CHARS = 8_000;

export interface CIContext {
  failureLogs: string;
  branch: string;
  repo: string;
}

export function fetchCIContext(): CIContext {
  let failureLogs = "";
  try {
    failureLogs = readFileSync(LOG_FILE, "utf8").slice(0, MAX_LOG_CHARS);
  } catch {
    // Fallback to env var if the file doesn't exist (e.g. local testing)
    failureLogs = (process.env.FAILURE_LOGS ?? "").slice(0, MAX_LOG_CHARS);
  }

  const branch = process.env.HEAD_BRANCH ?? "main";
  const repo = process.env.GITHUB_REPOSITORY ?? "";

  return { failureLogs, branch, repo };
}
