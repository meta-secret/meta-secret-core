import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { aggregateClassification, classifyJob, decideAction, renderSummary, type ClassifiedJob, type PullRequestContext } from "./lib/ci-monitor.ts";

const MAX_LOG_CHARS = 24_000;
const apiVersion = "2022-11-28";
interface EventPayload { workflow_run?: { id: number; run_attempt?: number; head_branch?: string; head_sha?: string; conclusion?: string | null; event?: string } }
interface ApiJob { id: number; name: string; status: string; conclusion: string | null }
interface ApiPullRequest { number: number; title: string; body: string | null; head: { sha: string; ref: string; repo: { full_name: string } | null } }

function required(name: string): string { const value = process.env[name]; if (!value) throw new Error(`Missing required environment variable: ${name}`); return value; }
async function setOutput(name: string, value: string): Promise<void> {
  const file = process.env.GITHUB_OUTPUT;
  if (!file) return;
  const escaped = value.replace(/%/g, "%25").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
  await appendFile(file, `${name}=${escaped}\n`, "utf8");
}
async function github<T>(repo: string, token: string, path: string): Promise<T> {
  const response = await fetch(`https://api.github.com/repos/${repo}${path}`, { headers: { Accept: "application/vnd.github+json", Authorization: `Bearer ${token}`, "X-GitHub-Api-Version": apiVersion } });
  if (!response.ok) throw new Error(`GitHub API ${response.status} for ${path}`);
  return (await response.json()) as T;
}
async function fetchLog(repo: string, token: string, jobId: number): Promise<string> {
  const response = await fetch(`https://api.github.com/repos/${repo}/actions/jobs/${jobId}/logs`, { headers: { Accept: "application/vnd.github+json", Authorization: `Bearer ${token}`, "X-GitHub-Api-Version": apiVersion } });
  if (!response.ok) return `<log unavailable: GitHub API ${response.status}>`;
  const text = await response.text();
  return text.length <= MAX_LOG_CHARS ? text : `${text.slice(0, MAX_LOG_CHARS / 2)}\n<log truncated>\n${text.slice(-MAX_LOG_CHARS / 2)}`;
}
async function findPullRequest(repo: string, token: string, sha: string): Promise<PullRequestContext | undefined> {
  const pulls = await github<ApiPullRequest[]>(repo, token, `/commits/${sha}/pulls`);
  const match = pulls.find((pull) => pull.head.sha === sha && pull.head.repo?.full_name === repo);
  return match ? { number: match.number, headSha: match.head.sha, headBranch: match.head.ref, headRepository: match.head.repo?.full_name ?? "", title: match.title, body: match.body ?? "" } : undefined;
}

async function main(): Promise<void> {
  const token = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  if (!token) throw new Error("GH_TOKEN or GITHUB_TOKEN is required");
  const repository = required("GITHUB_REPOSITORY");
  const event = JSON.parse(await readFile(required("GITHUB_EVENT_PATH"), "utf8")) as EventPayload;
  const run = event.workflow_run;
  if (!run?.id || !run.head_sha) throw new Error("workflow_run payload is incomplete");
  const runAttempt = run.run_attempt ?? 1;
  const branch = run.head_branch ?? "";
  const sha = run.head_sha;
  const conclusion = run.conclusion ?? "unknown";
  const pullRequest = await findPullRequest(repository, token, sha);
  const { jobs: apiJobs } = await github<{ jobs: ApiJob[] }>(repository, token, `/actions/runs/${run.id}/jobs?per_page=100`);
  const jobs: ClassifiedJob[] = [];
  for (const job of apiJobs) {
    if (job.conclusion === "success" || job.conclusion === "neutral") continue;
    jobs.push(classifyJob({ id: job.id, name: job.name, status: job.status, conclusion: job.conclusion, log: await fetchLog(repository, token, job.id) }));
  }
  const decision = decideAction({ jobs, conclusion, runAttempt, pullRequest, repository });
  const redactions = jobs.reduce((total, job) => total + job.redactions, 0);
  const outputDirectory = join(process.env.RUNNER_TEMP ?? "/tmp", "ci-monitor");
  await mkdir(outputDirectory, { recursive: true });
  await writeFile(join(outputDirectory, "ci-monitor.json"), `${JSON.stringify({ schemaVersion: 1, workflow: "CI", runId: run.id, runAttempt, event: run.event ?? "unknown", headSha: sha, headBranch: branch, repository, pullRequest: pullRequest ?? null, conclusion, classification: aggregateClassification(jobs, conclusion), action: decision.action, retry: { attempt: runAttempt, maxAttempts: 3, eligible: decision.retryEligible }, redactionCount: redactions, jobs: jobs.map(({ log: _log, ...job }) => job) }, null, 2)}\n`, "utf8");
  const summary = renderSummary({ runId: run.id, runAttempt, sha, branch, repository, conclusion, pullRequest, jobs, decision, totalRedactions: redactions });
  await writeFile(join(outputDirectory, "ci-monitor-summary.md"), summary, "utf8");
  await writeFile(join(outputDirectory, "cursor-prompt.log"), jobs.filter((job) => job.conclusion === "failure").map((job) => `## ${job.name} (${job.classification})\n${job.log}`).join("\n\n"), "utf8");
  if (process.env.GITHUB_STEP_SUMMARY) await writeFile(process.env.GITHUB_STEP_SUMMARY, summary, "utf8");
  await setOutput("action", decision.action);
  await setOutput("classification", decision.classification);
  await setOutput("retry_eligible", String(decision.retryEligible));
  await setOutput("auto_fix_eligible", String(decision.autoFixEligible));
  await setOutput("run_id", String(run.id));
  await setOutput("run_attempt", String(runAttempt));
  await setOutput("head_sha", sha);
  await setOutput("head_branch", branch);
  await setOutput("pr_number", pullRequest ? String(pullRequest.number) : "");
  await setOutput("artifact_name", `ci-monitor-${run.id}-${runAttempt}`);
}

await main();
