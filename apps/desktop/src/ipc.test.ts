import assert from "node:assert/strict";
import test from "node:test";

import {
  loadCoreManifest,
  validateCurrency,
  type DesktopInvoke,
} from "./ipc.ts";

test("loads the native core manifest through the canonical command", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const call: DesktopInvoke = async (command, args) => {
    calls.push({ command, args });
    return {
      appVersion: "1.2.0",
      ledgerLinked: true,
      localOnly: true,
      unsafeRustForbidden: true,
      capabilities: ["Double-entry ledger"],
    };
  };

  const manifest = await loadCoreManifest(call);

  assert.equal(manifest.ledgerLinked, true);
  assert.deepEqual(calls, [{ command: "core_manifest", args: undefined }]);
});

test("passes currency validation to the native ledger command", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const call: DesktopInvoke = async (command, args) => {
    calls.push({ command, args });
    return {
      input: "sar",
      normalized: "SAR",
      valid: true,
      error: null,
    };
  };

  const result = await validateCurrency("sar", call);

  assert.equal(result.normalized, "SAR");
  assert.equal(result.valid, true);
  assert.deepEqual(calls, [
    { command: "validate_currency", args: { code: "sar" } },
  ]);
});
