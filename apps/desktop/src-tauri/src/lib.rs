use cofi_ledger::Currency;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreManifest {
    app_version: &'static str,
    ledger_linked: bool,
    local_only: bool,
    unsafe_rust_forbidden: bool,
    capabilities: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrencyCheck {
    input: String,
    normalized: String,
    valid: bool,
    error: Option<String>,
}

#[tauri::command]
fn core_manifest() -> CoreManifest {
    CoreManifest {
        app_version: env!("CARGO_PKG_VERSION"),
        ledger_linked: Currency::new("USD").is_ok(),
        local_only: true,
        unsafe_rust_forbidden: true,
        capabilities: vec![
            "Double-entry ledger",
            "Billing and invoicing",
            "Payments and payout reconciliation",
            "Shared community funds",
            "Governance and authorized spending",
            "Tamper-evident audit chains",
        ],
    }
}

#[tauri::command]
fn validate_currency(code: String) -> CurrencyCheck {
    let normalized = code.trim().to_ascii_uppercase();

    match Currency::new(&normalized) {
        Ok(_) => CurrencyCheck {
            input: code,
            normalized,
            valid: true,
            error: None,
        },
        Err(error) => CurrencyCheck {
            input: code,
            normalized,
            valid: false,
            error: Some(error.to_string()),
        },
    }
}

pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![core_manifest, validate_currency])
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::{core_manifest, validate_currency};

    #[test]
    fn manifest_links_the_canonical_ledger() {
        let manifest = core_manifest();

        assert!(manifest.ledger_linked);
        assert!(manifest.local_only);
        assert!(manifest.unsafe_rust_forbidden);
        assert!(!manifest.capabilities.is_empty());
    }

    #[test]
    fn currency_validation_uses_the_ledger_contract() {
        let accepted = validate_currency(" sar ".to_owned());
        let rejected = validate_currency("SAR1".to_owned());

        assert!(accepted.valid);
        assert_eq!(accepted.normalized, "SAR");
        assert!(!rejected.valid);
        assert!(rejected.error.is_some());
    }
}
