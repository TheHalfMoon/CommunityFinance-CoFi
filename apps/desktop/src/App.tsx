import { FormEvent, useEffect, useMemo, useState } from "react";

import {
  loadCoreManifest,
  validateCurrency,
  type CoreManifest,
  type CurrencyCheck,
} from "./ipc";

const navigation = [
  "Home",
  "Community",
  "Payments",
  "Billing",
  "Projects",
  "Analytics",
  "Settings",
] as const;

type NavigationItem = (typeof navigation)[number];

function App() {
  const [active, setActive] = useState<NavigationItem>("Home");
  const [manifest, setManifest] = useState<CoreManifest | null>(null);
  const [coreError, setCoreError] = useState<string | null>(null);
  const [currency, setCurrency] = useState("USD");
  const [currencyCheck, setCurrencyCheck] = useState<CurrencyCheck | null>(null);
  const [checkingCurrency, setCheckingCurrency] = useState(false);

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
    if (coreError) return "Core unavailable";
    if (!manifest) return "Connecting";
    return manifest.ledgerLinked ? "Core connected" : "Core degraded";
  }, [coreError, manifest]);

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

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <img src="/cofi-mark.svg" alt="" className="brand-mark" />
          <div className="brand-copy">
            <strong>Community</strong>
            <span>Finance</span>
          </div>
        </div>

        <nav className="navigation" aria-label="Application">
          {navigation.map((item) => (
            <button
              type="button"
              key={item}
              className={active === item ? "nav-item active" : "nav-item"}
              onClick={() => setActive(item)}
            >
              <span className="nav-dot" aria-hidden="true" />
              <span>{item}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <div className="privacy-pill">
            <span className="status-dot" />
            Local-only runtime
          </div>
          <p>Your financial data stays on this device.</p>
        </div>
      </aside>

      <main className="main-panel">
        <header className="topbar">
          <div>
            <p className="eyebrow">COFI DESKTOP</p>
            <h1>{active}</h1>
          </div>

          <div className="topbar-status">
            <span className={coreError ? "signal error" : "signal"} />
            <span>{connectionLabel}</span>
            {manifest ? <small>v{manifest.appVersion}</small> : null}
          </div>
        </header>

        {active === "Home" ? (
          <div className="dashboard">
            <section className="hero-card">
              <div className="hero-card-copy">
                <span className="section-kicker">LOCAL FINANCIAL INFRASTRUCTURE</span>
                <h2>Auditable finance, running on your desktop.</h2>
                <p>
                  CoFi keeps accounting, authorization, governance, and reconciliation
                  behind a deterministic Rust core. This shell talks to that core through
                  typed native IPC.
                </p>
              </div>

              <div className="hero-state" aria-label="Core state">
                <div className="core-orbit">
                  <img src="/cofi-mark.svg" alt="" />
                </div>
                <div>
                  <strong>{manifest?.ledgerLinked ? "Ledger linked" : "Checking core"}</strong>
                  <span>No cloud runtime required</span>
                </div>
              </div>
            </section>

            <section className="status-grid" aria-label="Runtime status">
              <article className="status-card">
                <span className="status-label">Runtime</span>
                <strong>{manifest?.localOnly ? "Local only" : "Checking"}</strong>
                <p>No remote service is required for this shell.</p>
              </article>

              <article className="status-card">
                <span className="status-label">Ledger</span>
                <strong>{manifest?.ledgerLinked ? "Connected" : "Checking"}</strong>
                <p>The desktop host links directly to the canonical CoFi ledger crate.</p>
              </article>

              <article className="status-card">
                <span className="status-label">Rust safety</span>
                <strong>{manifest?.unsafeRustForbidden ? "Unsafe forbidden" : "Checking"}</strong>
                <p>The host preserves the repository safety boundary.</p>
              </article>

              <article className="status-card">
                <span className="status-label">Workspace</span>
                <strong>Not created yet</strong>
                <p>No financial records are fabricated in the empty state.</p>
              </article>
            </section>

            <section className="content-grid">
              <article className="panel capabilities-panel">
                <div className="panel-heading">
                  <div>
                    <span className="section-kicker">CORE</span>
                    <h3>Available capabilities</h3>
                  </div>
                  <span className="panel-badge">Rust</span>
                </div>

                <div className="capability-list">
                  {(manifest?.capabilities ?? [
                    "Loading canonical capabilities…",
                  ]).map((capability) => (
                    <div className="capability-row" key={capability}>
                      <span className="check-mark">✓</span>
                      <span>{capability}</span>
                    </div>
                  ))}
                </div>
              </article>

              <article className="panel validation-panel">
                <div className="panel-heading">
                  <div>
                    <span className="section-kicker">LIVE IPC</span>
                    <h3>Validate a currency code</h3>
                  </div>
                  <span className="panel-badge">cofi-ledger</span>
                </div>

                <p className="panel-copy">
                  This test crosses the native Tauri boundary and asks the real ledger
                  crate to validate an ISO-style currency code.
                </p>

                <form className="currency-form" onSubmit={checkCurrency}>
                  <label htmlFor="currency-code">Currency code</label>
                  <div className="field-row">
                    <input
                      id="currency-code"
                      value={currency}
                      onChange={(event) => setCurrency(event.target.value)}
                      maxLength={8}
                      autoComplete="off"
                      spellCheck={false}
                    />
                    <button type="submit" disabled={checkingCurrency}>
                      {checkingCurrency ? "Checking…" : "Validate"}
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
                        ? currencyCheck.normalized + " accepted"
                        : currencyCheck.normalized + " rejected"}
                    </strong>
                    <span>
                      {currencyCheck.valid
                        ? "Validated by cofi-ledger."
                        : currencyCheck.error ?? "Invalid currency code."}
                    </span>
                  </div>
                ) : (
                  <div className="validation-result neutral">
                    <strong>Ready</strong>
                    <span>Try USD, SAR, EUR, or an invalid value.</span>
                  </div>
                )}
              </article>
            </section>

            <section className="panel empty-activity">
              <div className="panel-heading">
                <div>
                  <span className="section-kicker">ACTIVITY</span>
                  <h3>Nothing recorded yet</h3>
                </div>
                <span className="panel-badge">Local workspace</span>
              </div>
              <p>
                Financial activity will appear only after a real local workspace and
                persistence layer are connected. The desktop shell does not seed fake
                transactions.
              </p>
            </section>
          </div>
        ) : (
          <section className="placeholder-view">
            <span className="section-kicker">DESKTOP FOUNDATION</span>
            <h2>{active} is core-ready.</h2>
            <p>
              The native shell is in place. This surface will be enabled only when its
              canonical CoFi engine and local persistence boundary are connected.
            </p>
            <button type="button" onClick={() => setActive("Home")}>
              Return to Home
            </button>
          </section>
        )}
      </main>
    </div>
  );
}

export default App;
