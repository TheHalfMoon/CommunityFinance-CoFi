# CoFi Desktop

CoFi Desktop is the native desktop shell for Community Finance.

## Current boundary

The application is intentionally local-first. The Tauri host links directly to the canonical `cofi-ledger` Rust crate and exposes typed IPC commands to the React interface.

The current desktop foundation provides:

- a native Tauri 2 host;
- React and TypeScript UI;
- direct linkage to `cofi-ledger`;
- live ledger-backed currency validation over IPC;
- explicit empty states instead of fabricated financial records;
- Windows MSI and NSIS packaging;
- a high-contrast purple application icon for taskbar visibility.

Surfaces that are not yet connected to their canonical engine remain visibly unavailable instead of simulating data.

## Requirements

- Node.js 24
- Rust 1.85 or newer
- Windows WebView2 runtime
- Windows build tools when producing native installers

## Development

```powershell
cd apps/desktop
npm ci
npm run desktop:dev
```

## Qualification

```powershell
npm run build
npm run test:ipc
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run desktop:build
```

Installers are written beneath `src-tauri/target/release/bundle/`.

## Windows signing

The current zero-cost Windows pipeline produces unsigned MSI and NSIS installers. Windows may display a publisher or SmartScreen warning until a trusted code-signing certificate is configured. Release artifacts include SHA-256 checksums so users can verify downloaded files independently.

## Release rule

A release tag must exactly match the version in `package.json` (for example, `v1.1.1`). The release workflow builds Windows installers, creates SHA-256 checksums, and publishes the artifacts to GitHub Releases. The public landing page then discovers the installer from the latest release.
