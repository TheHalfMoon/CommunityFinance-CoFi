import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflow = readFileSync(
  new URL("../workflows/jev-review.yml", import.meta.url),
  "utf8",
);

test("unrelated issue comments cannot enter Jev cancellation concurrency", () => {
  assert.doesNotMatch(workflow, /^concurrency:\s*$/m);
  const afterJob = workflow.split("jobs:\n  jev-review:\n")[1];
  assert.ok(afterJob, "exact expected Jev job exists");
  const jobHeader = afterJob.split("\n    steps:")[0];
  assert.match(
    jobHeader,
    /    concurrency:\n      group: cofi-jev-\$\{\{ github\.event\.issue\.number \}\}\n      cancel-in-progress: true/,
  );
});

test("only explicit PR Jev requests run the credentialed reviewer", () => {
  assert.match(
    workflow,
    /    if: >-\n      github\.event\.issue\.pull_request &&\n      github\.event\.comment\.body == '\/jev-review'/,
  );
  assert.match(workflow, /  contents: read/);
  assert.match(workflow, /  pull-requests: read/);
});

test("authenticated and exact-head review safeguards remain active", () => {
  for (const required of [
    "TYPESAFE_API_KEY",
    "git merge-base",
    "git diff --check",
    "JEV_EXACT_RANGE=",
    "JEV_STATUS=PASSED",
    "JEV_BLOCKING_FINDINGS=0",
    "if report.get(\"blocking_findings\")",
    "git show \"$BASE_SHA:.github/scripts/jev-exact-diff.mjs\"",
  ]) {
    assert.ok(workflow.includes(required), `missing safety invariant: ${required}`);
  }
});
