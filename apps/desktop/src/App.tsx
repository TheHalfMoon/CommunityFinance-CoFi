import { FormEvent, useEffect, useMemo, useState } from "react";

import {
  applyDocumentLocale,
  createTranslator,
  getInitialLocale,
  localeMeta,
  persistLocale,
  supportedLocales,
  translateCapability,
  type SupportedLocale,
} from "./i18n";
import {
  loadCoreManifest,
  validateCurrency,
  type CoreManifest,
  type CurrencyCheck,
} from "./ipc";

const navigation = [
  { id: "home", label: "nav.home" },
  { id: "community", label: "nav.community" },
  { id: "payments", label: "nav.payments" },
  { id: "billing", label: "nav.billing" },
  { id: "projects", label: "nav.projects" },
  { id: "analytics", label: "nav.analytics" },
  { id: "settings", label: "nav.settings" },
] as const;

type NavigationItem = (typeof navigation)[number]["id"];

function App() {
  const [active, setActive] = useState<NavigationItem>("home");
  const [locale, setLocale] = useState<SupportedLocale>(getInitialLocale);
  const [manifest, setManifest] = useState<CoreManifest | null>(null);
  const [coreError, setCoreError] = useState<string | null>(null);
  const [currency, setCurrency] = useState("USD");
  const [currencyCheck, setCurrencyCheck] = useState<CurrencyCheck | null>(null);
  const [checkingCurrency, setCheckingCurrency] = useState(false);

  const t = useMemo(() => createTranslator(locale), [locale]);
  const activeNavigation = navigation.find((item) => item.id === active) ?? navigation[0];

  useEffect(() => {
    applyDocumentLocale(locale);
    persistLocale(locale);
  }, [locale]);

  useEffect(() => {
    let mounted = true;

    loadCoreManifest()
      .then((value) => {
        if (mounted) setManifest(value);
      })
      .catch((error: unknown) => {
        if (mounted) {
          setCoreError(error instanceof Error ? error.message : String(error));
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  const connectionLabel = useMemo(() => {
    if (coreError) return t("core.unavailable");
    if (!manifest) return t("core.connecting");
    return manifest.ledgerLinked ? t("core.connected") : t("core.degraded");
  }, [coreError, manifest, t]);

  async function checkCurrency(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setCheckingCurrency(true);

    try {
      const result = await validateCurrency(currency);
      setCurrencyCheck(result);
    } catch (error: unknown) {
      setCurrencyCheck({
        input: currency,
        normalized: currency.trim().toUpperCase(),
        valid: false,
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setCheckingCurrency(false);
    }
  }

  function changeLocale(value: string) {
    if (supportedLocales.includes(value as SupportedLocale)) {
      setLocale(value as SupportedLocale);
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <img src="/cofi-mark.svg" alt="" className="brand-mark" />
          <div className="brand-copy">
            <strong>{t("brand.community")}</strong>
            <span>{t("brand.finance")}</span>
          </div>
        </div>

        <nav className="navigation" aria-label={t("nav.application")}>
          {navigation.map((item) => (
            <button
              type="button"
              key={item.id}
              className={active === item.id ? "nav-item active" : "nav-item"}
              onClick={() => setActive(item.id)}
            >
              <span className="nav-dot" aria-hidden="true" />
              <span>{t(item.label)}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <label className="language-control">
            <span>{t("language.label")}</span>
            <select
              aria-label={t("language.aria")}
              value={locale}
              onChange={(event) => changeLocale(event.target.value)}
            >
              {supportedLocales.map((item) => (
                <option key={item} value={item}>
                  {localeMeta[item].nativeName}
                </option>
              ))}
            </select>
          </label>

          <div className="privacy-pill">
            <span className="status-dot" />
            {t("privacy.localOnlyRuntime")}
          </div>
          <p>{t("privacy.dataStays")}</p>
        </div>
      </aside>

      <main className="main-panel">
        <header className="topbar">
          <div>
            <p className="eyebrow">{t("app.eyebrow")}</p>
            <h1>{t(activeNavigation.label)}</h1>
          </div>

          <div className="topbar-status">
            <span className={coreError ? "signal error" : "signal"} />
            <span>{connectionLabel}</span>
            {manifest ? <small>v{manifest.appVersion}</small> : null}
          </div>
        </header>

        {active === "home" ? (
          <div className="dashboard">
            <section className="hero-card">
              <div className="hero-card-copy">
                <span className="section-kicker">{t("hero.kicker")}</span>
                <h2>{t("hero.title")}</h2>
                <p>{t("hero.body")}</p>
              </div>

              <div className="hero-state" aria-label={t("hero.coreState")}>
                <div className="core-orbit">
                  <img src="/cofi-mark.svg" alt="" />
                </div>
                <div>
                  <strong>
                    {manifest?.ledgerLinked ? t("hero.ledgerLinked") : t("hero.checkingCore")}
                  </strong>
                  <span>{t("hero.noCloud")}</span>
                </div>
              </div>
            </section>

            <section className="status-grid" aria-label={t("runtime.status")}>
              <article className="status-card">
                <span className="status-label">{t("status.runtime")}</span>
                <strong>{manifest?.localOnly ? t("status.localOnly") : t("status.checking")}</strong>
                <p>{t("status.runtimeBody")}</p>
              </article>

              <article className="status-card">
                <span className="status-label">{t("status.ledger")}</span>
                <strong>{manifest?.ledgerLinked ? t("status.connected") : t("status.checking")}</strong>
                <p>{t("status.ledgerBody")}</p>
              </article>

              <article className="status-card">
                <span className="status-label">{t("status.rustSafety")}</span>
                <strong>
                  {manifest?.unsafeRustForbidden ? t("status.unsafeForbidden") : t("status.checking")}
                </strong>
                <p>{t("status.rustSafetyBody")}</p>
              </article>

              <article className="status-card">
                <span className="status-label">{t("status.workspace")}</span>
                <strong>{t("status.notCreated")}</strong>
                <p>{t("status.workspaceBody")}</p>
              </article>
            </section>

            <section className="content-grid">
              <article className="panel capabilities-panel">
                <div className="panel-heading">
                  <div>
                    <span className="section-kicker">{t("core.kicker")}</span>
                    <h3>{t("core.availableCapabilities")}</h3>
                  </div>
                  <span className="panel-badge">Rust</span>
                </div>

                <div className="capability-list">
                  {(manifest?.capabilities ?? [t("core.loadingCapabilities")]).map((capability) => (
                    <div className="capability-row" key={capability}>
                      <span className="check-mark">✓</span>
                      <span>{translateCapability(capability, t)}</span>
                    </div>
                  ))}
                </div>
              </article>

              <article className="panel validation-panel">
                <div className="panel-heading">
                  <div>
                    <span className="section-kicker">{t("validation.kicker")}</span>
                    <h3>{t("validation.title")}</h3>
                  </div>
                  <span className="panel-badge">cofi-ledger</span>
                </div>

                <p className="panel-copy">{t("validation.body")}</p>

                <form className="currency-form" onSubmit={checkCurrency}>
                  <label htmlFor="currency-code">{t("validation.currencyCode")}</label>
                  <div className="field-row">
                    <input
                      id="currency-code"
                      value={currency}
                      onChange={(event) => setCurrency(event.target.value)}
                      maxLength={8}
                      autoComplete="off"
                      spellCheck={false}
                      dir="ltr"
                    />
                    <button type="submit" disabled={checkingCurrency}>
                      {checkingCurrency ? t("validation.checking") : t("validation.validate")}
                    </button>
                  </div>
                </form>

                {currencyCheck ? (
                  <div
                    className={
                      currencyCheck.valid
                        ? "validation-result valid"
                        : "validation-result invalid"
                    }
                  >
                    <strong>
                      {currencyCheck.valid
                        ? t("validation.accepted", { code: currencyCheck.normalized })
                        : t("validation.rejected", { code: currencyCheck.normalized })}
                    </strong>
                    <span>
                      {currencyCheck.valid
                        ? t("validation.validatedByLedger")
                        : t("validation.invalidCurrency")}
                    </span>
                  </div>
                ) : (
                  <div className="validation-result neutral">
                    <strong>{t("validation.ready")}</strong>
                    <span>{t("validation.hint")}</span>
                  </div>
                )}
              </article>
            </section>

            <section className="panel empty-activity">
              <div className="panel-heading">
                <div>
                  <span className="section-kicker">{t("activity.kicker")}</span>
                  <h3>{t("activity.emptyTitle")}</h3>
                </div>
                <span className="panel-badge">{t("activity.localWorkspace")}</span>
              </div>
              <p>{t("activity.emptyBody")}</p>
            </section>
          </div>
        ) : (
          <section className="placeholder-view">
            <span className="section-kicker">{t("placeholder.kicker")}</span>
            <h2>{t("placeholder.title", { section: t(activeNavigation.label) })}</h2>
            <p>{t("placeholder.body")}</p>
            <button type="button" onClick={() => setActive("home")}>
              {t("placeholder.returnHome")}
            </button>
          </section>
        )}
      </main>
    </div>
  );
}

export default App;
