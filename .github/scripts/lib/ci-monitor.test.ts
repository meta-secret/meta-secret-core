import { describe, expect, test } from "bun:test";
import { classifyJob, decideAction, redactLog } from "./ci-monitor.ts";

describe("CI monitor", () => {
  test("redacts token and multiline private key", () => {
    const result = redactLog("GITHUB_TOKEN=ghp_123456789012345678901234\n-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----");
    expect(result.text).not.toContain("ghp_123");
    expect(result.text).not.toContain("secret");
    expect(result.count).toBeGreaterThan(0);
  });
  test("classifies Rust test failures as source", () => {
    const job = classifyJob({ id: 1, name: "test", conclusion: "failure", status: "completed", log: "cargo test ... FAILED\nprocess completed with exit code 101" });
    expect(job.classification).toBe("source_failure");
  });
  test("classifies registry failure as transient", () => {
    const job = classifyJob({ id: 2, name: "test", conclusion: "failure", status: "completed", log: "ghcr.io: HTTP 503 service unavailable" });
    expect(job.classification).toBe("infra_transient");
  });
  test("never sends sensitive failures to Cursor", () => {
    const job = classifyJob({ id: 3, name: "test", conclusion: "failure", status: "completed", log: "assertion failed while handling Master Key" });
    const result = decideAction({ jobs: [job], conclusion: "failure", runAttempt: 1, repository: "meta-secret/meta-secret-core" });
    expect(result.action).toBe("notify");
  });
  test("stops retry after two retries", () => {
    const job = classifyJob({ id: 4, name: "test", conclusion: "failure", status: "completed", log: "runner lost connection; timeout" });
    expect(decideAction({ jobs: [job], conclusion: "failure", runAttempt: 1, repository: "meta-secret/meta-secret-core" }).action).toBe("rerun");
    expect(decideAction({ jobs: [job], conclusion: "failure", runAttempt: 3, repository: "meta-secret/meta-secret-core" }).action).toBe("notify");
  });
  test("does not fix fork or already marked PR", () => {
    const job = classifyJob({ id: 5, name: "test", conclusion: "failure", status: "completed", log: "cargo test failed" });
    const fork = decideAction({ jobs: [job], conclusion: "failure", runAttempt: 1, repository: "meta-secret/meta-secret-core", pullRequest: { number: 1, headSha: "sha", headBranch: "feature", headRepository: "attacker/repo", title: "Fix", body: "" } });
    const marked = decideAction({ jobs: [job], conclusion: "failure", runAttempt: 1, repository: "meta-secret/meta-secret-core", pullRequest: { number: 2, headSha: "sha", headBranch: "cursor/fix", headRepository: "meta-secret/meta-secret-core", title: "[ci-auto-fix] Fix", body: "CI Monitor: auto-fix" } });
    expect(fork.action).toBe("notify");
    expect(marked.action).toBe("notify");
  });
});
