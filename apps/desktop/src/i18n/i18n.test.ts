import assert from "node:assert/strict";
import test from "node:test";

import {
  createTranslator,
  getDictionariesForTest,
  localeMeta,
  normalizeLocale,
  resolveInitialLocale,
  supportedLocales,
  translateCapability,
} from "./index.ts";

test("ships complete non-empty dictionaries for every supported locale", () => {
  const dictionaries = getDictionariesForTest();
  const englishKeys = Object.keys(dictionaries.en).sort();

  assert.deepEqual(Object.keys(dictionaries).sort(), [...supportedLocales].sort());

  for (const locale of supportedLocales) {
    const keys = Object.keys(dictionaries[locale]).sort();
    assert.deepEqual(keys, englishKeys, `${locale} translation keys must match English`);

    for (const [key, value] of Object.entries(dictionaries[locale])) {
      assert.ok(value.trim().length > 0, `${locale}:${key} must not be empty`);
    }
  }
});

test("normalizes requested locales and defaults unsupported languages to English", () => {
  assert.equal(normalizeLocale("ar-SA"), "ar");
  assert.equal(normalizeLocale("fr-FR"), "fr");
  assert.equal(normalizeLocale("de-DE"), "de");
  assert.equal(normalizeLocale("es-MX"), "es");
  assert.equal(normalizeLocale("it-IT"), "it");
  assert.equal(normalizeLocale("zh-Hans-CN"), "zh-CN");
  assert.equal(normalizeLocale("ja-JP"), "en");
});

test("prefers a persisted locale over navigator languages", () => {
  assert.equal(resolveInitialLocale("de", ["ar-SA", "en-US"]), "de");
  assert.equal(resolveInitialLocale(null, ["fr-FR", "en-US"]), "fr");
  assert.equal(resolveInitialLocale(null, ["ja-JP", "en-US"]), "en");
});

test("marks Arabic as RTL and every other supported locale as LTR", () => {
  assert.equal(localeMeta.ar.direction, "rtl");

  for (const locale of supportedLocales) {
    if (locale !== "ar") assert.equal(localeMeta[locale].direction, "ltr");
  }
});

test("interpolates translated values and translates canonical capabilities", () => {
  const ar = createTranslator("ar");
  assert.equal(ar("validation.accepted", { code: "SAR" }), "تم قبول SAR");
  assert.equal(translateCapability("Double-entry ledger", ar), "دفتر أستاذ بالقيد المزدوج");
  assert.equal(translateCapability("Future capability", ar), "Future capability");
});
