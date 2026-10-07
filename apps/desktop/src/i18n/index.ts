import { ar } from "./locales/ar";
import { de } from "./locales/de";
import { en, type TranslationKey, type TranslationMessages } from "./locales/en";
import { es } from "./locales/es";
import { fr } from "./locales/fr";
import { it } from "./locales/it";
import { zhCN } from "./locales/zh-CN";

export const supportedLocales = ["en", "ar", "fr", "de", "es", "it", "zh-CN"] as const;

export type SupportedLocale = (typeof supportedLocales)[number];

export const localeMeta: Record<
  SupportedLocale,
  { nativeName: string; direction: "ltr" | "rtl" }
> = {
  en: { nativeName: "English", direction: "ltr" },
  ar: { nativeName: "العربية", direction: "rtl" },
  fr: { nativeName: "Français", direction: "ltr" },
  de: { nativeName: "Deutsch", direction: "ltr" },
  es: { nativeName: "Español", direction: "ltr" },
  it: { nativeName: "Italiano", direction: "ltr" },
  "zh-CN": { nativeName: "简体中文", direction: "ltr" },
};

const dictionaries: Record<SupportedLocale, TranslationMessages> = {
  en,
  ar,
  fr,
  de,
  es,
  it,
  "zh-CN": zhCN,
};

const storageKey = "cofi.locale";

export function normalizeLocale(input: string | null | undefined): SupportedLocale {
  if (!input) return "en";

  const normalized = input.trim().toLowerCase();
  if (normalized === "zh-cn" || normalized.startsWith("zh")) return "zh-CN";
  if (normalized.startsWith("ar")) return "ar";
  if (normalized.startsWith("fr")) return "fr";
  if (normalized.startsWith("de")) return "de";
  if (normalized.startsWith("es")) return "es";
  if (normalized.startsWith("it")) return "it";
  return "en";
}

export function resolveInitialLocale(
  persisted: string | null,
  navigatorLanguages: readonly string[],
): SupportedLocale {
  if (persisted) {
    const normalized = normalizeLocale(persisted);
    if (supportedLocales.includes(normalized)) return normalized;
  }

  for (const candidate of navigatorLanguages) {
    const normalized = normalizeLocale(candidate);
    if (normalized !== "en" || candidate.toLowerCase().startsWith("en")) {
      return normalized;
    }
  }

  return "en";
}

export function getInitialLocale(): SupportedLocale {
  if (typeof window === "undefined") return "en";

  let persisted: string | null = null;
  try {
    persisted = window.localStorage.getItem(storageKey);
  } catch {
    persisted = null;
  }
  const languages =
    typeof navigator !== "undefined"
      ? navigator.languages.length > 0
        ? navigator.languages
        : [navigator.language]
      : [];

  return resolveInitialLocale(persisted, languages);
}

export function persistLocale(locale: SupportedLocale): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(storageKey, locale);
  } catch {
    // Locale persistence is optional; the active session still remains localized.
  }
}

export function applyDocumentLocale(locale: SupportedLocale): void {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale;
  document.documentElement.dir = localeMeta[locale].direction;
}

export function createTranslator(locale: SupportedLocale) {
  const dictionary = dictionaries[locale] ?? en;

  return (key: TranslationKey, values: Record<string, string> = {}): string => {
    let value = dictionary[key] ?? en[key];

    for (const [token, replacement] of Object.entries(values)) {
      value = value.replaceAll(`{${token}}`, replacement);
    }

    return value;
  };
}

const capabilityKeys: Record<string, TranslationKey> = {
  "Double-entry ledger": "capability.doubleEntryLedger",
  "Billing and invoicing": "capability.billingInvoicing",
  "Payments and payout reconciliation": "capability.paymentsPayoutReconciliation",
  "Shared community funds": "capability.sharedCommunityFunds",
  "Governance and authorized spending": "capability.governanceAuthorizedSpending",
  "Tamper-evident audit chains": "capability.tamperEvidentAuditChains",
};

export function translateCapability(
  capability: string,
  translate: ReturnType<typeof createTranslator>,
): string {
  const key = capabilityKeys[capability];
  return key ? translate(key) : capability;
}

export function getDictionariesForTest(): Readonly<
  Record<SupportedLocale, TranslationMessages>
> {
  return dictionaries;
}
