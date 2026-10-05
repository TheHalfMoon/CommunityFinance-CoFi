# Contributing to CoFi

Thanks for helping improve Community Finance / CoFi.

CoFi is financial infrastructure, so changes should be small, reviewable, deterministic, and evidence-backed.

## Development setup

Requirements:

- Rust 1.85 or newer
- Git

```bash
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-targets --all-features
```

Before opening a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo +1.85.0 check --workspace --all-targets --all-features
```

## Engineering expectations

- Preserve deterministic behavior and exact replay semantics.
- Fail closed before financial mutation.
- Use checked integer arithmetic for money and economic quantities.
- Keep organization scope, currency, identity, and timing boundaries explicit.
- Do not introduce `unsafe`, panic paths, hidden network effects, random identifiers, or caller-controlled accounting shortcuts.
- Add tests for success, replay, conflicts, invalid boundaries, and failed-retry behavior.
- Keep dependency direction explicit and acyclic.

## Pull requests

A strong pull request:

1. solves one bounded problem;
2. explains the invariant or user-visible improvement;
3. includes tests or evidence appropriate to the change;
4. passes formatting, Clippy, workspace tests, and MSRV checks;
5. avoids unrelated cleanup.

Use normal merge commits. Do not rewrite public history to hide review context.

## Commit messages

Prefer short, imperative messages:

```text
feat: enforce canonical payout lineage
fix: reject cross-scope fund transfer
docs: clarify release qualification
```

## Security

Do not report vulnerabilities in a public issue. Follow [SECURITY.md](SECURITY.md).
