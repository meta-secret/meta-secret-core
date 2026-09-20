export type FailureClassification =
  | "cancelled_superseded"
  | "infra_transient"
  | "source_failure"
  | "external_deploy"
  | "security_sensitive"
  | "permission_policy"
  | "unknown";

export type MonitorAction = "ignore" | "rerun" | "cursor_fix" | "notify";

export interface JobInput {
  id: number;
  name: string;
  conclusion: string | null;
  status: string;
  log: string;
}

export interface ClassifiedJob extends JobInput {
  classification: FailureClassification;
  signatures: string[];
  redactions: number;
}

export interface PullRequestContext {
  number: number;
  headSha: string;
  headBranch: string;
  headRepository: string;
  title: string;
  body: string;
}

const SECURITY: Array<[RegExp, string]> = [
  [/master[ _-]?key/i, "master-key-output"],
  [/key[ _-]?share/i, "key-share-output"],
  [/private[ _-]?key/i, "private-key-output"],
  [/secret material/i, "secret-material-output"],
  [/crypto|cryptograph|signature|signed action|shamir|sss/i, "crypto-path"],
  [/unsafe\s+(?:code|block)/i, "unsafe-code"],
  [/sops_age_key|sops age key/i, "sops-key-output"],
];
const POLICY: Array<[RegExp, string]> = [
  [/resource not accessible|insufficient permission|permission denied/i, "permission-denied"],
  [/unauthorized|forbidden|\bHTTP 403\b/i, "authorization-failure"],
  [/missing (?:the )?(?:required )?(?:token|secret)/i, "missing-credential"],
];
const INFRA: Array<[RegExp, string]> = [
  [/runner.*(?:unavailable|failure|lost)|hosted runner/i, "runner-failure"],
  [/rate limit|too many requests|\bHTTP 429\b/i, "rate-limit"],
  [/\bHTTP 5\d\d\b|internal server error|service unavailable/i, "service-5xx"],
  [/connection reset|connection refused|network is unreachable|temporary failure/i, "network-failure"],
  [/timed out|timeout|deadline exceeded/i, "timeout"],
  [/docker.*(?:pull|push)|ghcr\.io.*(?:error|failed)/i, "container-registry"],
  [/no space left on device/i, "runner-storage"],
];
const SOURCE: Array<[RegExp, string]> = [
  [/process completed with exit code 101/i, "rust-process-failure"],
  [/cargo (?:build|test|clippy)|rustc.*error/i, "rust-toolchain-failure"],
  [/test(?:s)? failed|assertion.*failed|panicked at/i, "test-failure"],
  [/eslint.*error|typescript.*error|lint.*failed/i, "lint-failure"],
];
const EXTERNAL: Array<[RegExp, string]> = [[/cloudflare|wrangler|pages deploy|preview deploy/i, "external-deploy"]];

/** Redact credentials/key material before persistence or external prompts. */
export function redactLog(input: string): { text: string; count: number } {
  let count = 0;
  const withoutKeys = input.replace(/-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----/gi, () => {
    count += 1;
    return "<redacted-private-key>";
  });
  const text = withoutKeys.split("\n").map((line) => {
    let result = line;
    if (/(?:github_token|gh_token|cursor_api_key|cloudflare_api_token|sops_age_key|master[ _-]?key|key[ _-]?share|private[ _-]?key|secret material)/i.test(line)) {
      const replaced = line.replace(/([=:]\s*).+$/, "$1<redacted>");
      if (replaced !== line) {
        result = replaced;
        count += 1;
      }
    }
    return result.replace(/\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b/g, () => {
      count += 1;
      return "<redacted-token>";
    });
  }).join("\n");
  return { text, count };
}

function matches(log: string, patterns: Array<[RegExp, string]>): string[] {
  return patterns.filter(([pattern]) => pattern.test(log)).map(([, name]) => name);
}

export function classifyJob(job: JobInput): ClassifiedJob {
  const redacted = redactLog(job.log);
  if (job.conclusion === "cancelled" || job.conclusion === "skipped") return { ...job, classification: "cancelled_superseded", signatures: ["job-cancelled-or-skipped"], log: redacted.text, redactions: redacted.count };
  if (job.conclusion !== "failure") return { ...job, classification: "unknown", signatures: [], log: redacted.text, redactions: redacted.count };
  const security = matches(redacted.text, SECURITY);
  if (security.length) return { ...job, classification: "security_sensitive", signatures: security, log: redacted.text, redactions: redacted.count };
  const policy = matches(redacted.text, POLICY);
  if (policy.length) return { ...job, classification: "permission_policy", signatures: policy, log: redacted.text, redactions: redacted.count };
  const external = matches(redacted.text, EXTERNAL);
  if (external.length || /deploy|preview|pages/i.test(job.name)) return { ...job, classification: "external_deploy", signatures: external.length ? external : ["deployment-job-failure"], log: redacted.text, redactions: redacted.count };
  const infra = matches(redacted.text, INFRA);
  if (infra.length) return { ...job, classification: "infra_transient", signatures: infra, log: redacted.text, redactions: redacted.count };
  const source = matches(redacted.text, SOURCE);
  if (source.length || /^(test|build|lint)/i.test(job.name)) return { ...job, classification: "source_failure", signatures: source.length ? source : ["source-job-failure"], log: redacted.text, redactions: redacted.count };
  return { ...job, classification: "unknown", signatures: ["unclassified-failure"], log: redacted.text, redactions: redacted.count };
}

const PRIORITY: FailureClassification[] = ["security_sensitive", "permission_policy", "unknown", "source_failure", "infra_transient", "external_deploy", "cancelled_superseded"];

export function aggregateClassification(jobs: ClassifiedJob[], conclusion: string): FailureClassification {
  if (conclusion === "cancelled" || jobs.length === 0) return "cancelled_superseded";
  return PRIORITY.find((item) => jobs.some((job) => job.classification === item)) ?? "unknown";
}

export function decideAction({ jobs, conclusion, runAttempt, pullRequest, repository }: { jobs: ClassifiedJob[]; conclusion: string; runAttempt: number; pullRequest?: PullRequestContext; repository: string }): { classification: FailureClassification; action: MonitorAction; reason: string; retryEligible: boolean; autoFixEligible: boolean } {
  const classification = aggregateClassification(jobs, conclusion);
  const classes = new Set(jobs.filter((job) => job.conclusion === "failure").map((job) => job.classification));
  const trusted = pullRequest !== undefined && pullRequest.headRepository === repository;
  const marked = Boolean(pullRequest?.body.includes("CI Monitor: auto-fix") || pullRequest?.title.includes("[ci-auto-fix]"));
  if (classification === "cancelled_superseded") return { classification, action: "ignore", reason: "The run was cancelled or superseded by a newer commit.", retryEligible: false, autoFixEligible: false };
  if (classification === "security_sensitive" || classification === "permission_policy") return { classification, action: "notify", reason: "Sensitive or permission-related output requires maintainer review.", retryEligible: false, autoFixEligible: false };
  if (classes.size > 1 || classification === "unknown") return { classification, action: "notify", reason: classes.size > 1 ? "Mixed failure classes must be isolated before automated remediation." : "The failure did not match a safe automated classification.", retryEligible: false, autoFixEligible: false };
  if (classification === "infra_transient") {
    const retryEligible = runAttempt < 3;
    return { classification, action: retryEligible ? "rerun" : "notify", reason: retryEligible ? `Transient infrastructure failure; rerun failed jobs (attempt ${runAttempt}/3).` : "Transient failure reached the two-retry budget.", retryEligible, autoFixEligible: false };
  }
  if (classification === "source_failure") {
    const autoFixEligible = trusted && !marked;
    return { classification, action: autoFixEligible ? "cursor_fix" : "notify", reason: autoFixEligible ? "Deterministic source failure on a trusted PR; Cursor remediation is allowed once." : marked ? "This PR already contains a CI Monitor auto-fix marker." : "Source remediation is disabled for fork or non-PR runs.", retryEligible: false, autoFixEligible };
  }
  return { classification, action: "notify", reason: "Deployment failures are external and require maintainer review.", retryEligible: false, autoFixEligible: false };
}

export function renderSummary({ runId, runAttempt, sha, branch, repository, conclusion, pullRequest, jobs, decision, totalRedactions }: { runId: number; runAttempt: number; sha: string; branch: string; repository: string; conclusion: string; pullRequest?: PullRequestContext; jobs: ClassifiedJob[]; decision: ReturnType<typeof decideAction>; totalRedactions: number }): string {
  const rows = jobs.length ? jobs.map((job) => `| ${job.name} | ${job.conclusion ?? job.status} | ${job.classification} | ${job.signatures.join(", ")} |`).join("\n") : "| (none) | — | cancelled_superseded | — |";
  return ["# CI Monitor Summary", "", `- Run: ${runId} (attempt ${runAttempt})`, `- Repository: ${repository}`, `- Branch: ${branch}`, `- SHA: ${sha}`, `- Pull request: ${pullRequest ? `#${pullRequest.number}` : "none"}`, `- Conclusion: ${conclusion}`, `- Classification: **${decision.classification}**`, `- Action: **${decision.action}**`, `- Reason: ${decision.reason}`, `- Redacted values: ${totalRedactions}`, "", "| Job | Conclusion | Classification | Signatures |", "| --- | --- | --- | --- |", rows, "", "Logs in this artifact are bounded and sanitized. Treat them as untrusted diagnostic data."].join("\n");
}
