import { invoke } from "@tauri-apps/api/core";

export type CoreManifest = {
  appVersion: string;
  ledgerLinked: boolean;
  localOnly: boolean;
  unsafeRustForbidden: boolean;
  capabilities: string[];
};

export type CurrencyCheck = {
  input: string;
  normalized: string;
  valid: boolean;
  error: string | null;
};

export type DesktopInvoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

const tauriInvoke: DesktopInvoke = (command, args) => invoke(command, args);

export async function loadCoreManifest(
  call: DesktopInvoke = tauriInvoke,
): Promise<CoreManifest> {
  return (await call("core_manifest")) as CoreManifest;
}

export async function validateCurrency(
  code: string,
  call: DesktopInvoke = tauriInvoke,
): Promise<CurrencyCheck> {
  return (await call("validate_currency", { code })) as CurrencyCheck;
}
