#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CANONICAL_REPOSITORY = "https://github.com/TheHalfMoon/CommunityFinance-CoFi"
CANONICAL_SECURITY = f"{CANONICAL_REPOSITORY}/security"
ALLOWED_GITHUB_REPOSITORIES = {
    ("TheHalfMoon", "CommunityFinance-CoFi"),
    ("devagrawal09", "jev-review"),
    ("rust-lang", "crates.io-index"),
}
SKIPPED_GENERATED_FILES = {"Cargo.lock", "package-lock.json"}
GITHUB_REPOSITORY_URL = re.compile(
    r"https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)"
)


def fail(message: str) -> None:
    raise SystemExit(f"repository identity check failed: {message}")


def read_text(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def tracked_files() -> list[Path]:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z"],
        check=True,
        capture_output=True,
    )
    return [
        ROOT / raw.decode("utf-8")
        for raw in result.stdout.split(b"\0")
        if raw
    ]


def normalized_repo(owner: str, repository: str) -> tuple[str, str]:
    if repository.endswith(".git"):
        repository = repository[:-4]
    return owner, repository


def check_external_repository_links() -> None:
    violations: list[str] = []
    for path in tracked_files():
        if path.name in SKIPPED_GENERATED_FILES:
            continue
        try:
            content = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue

        relative = path.relative_to(ROOT).as_posix()
        for line_number, line in enumerate(content.splitlines(), start=1):
            for match in GITHUB_REPOSITORY_URL.finditer(line):
                repository = normalized_repo(match.group(1), match.group(2))
                if repository not in ALLOWED_GITHUB_REPOSITORIES:
                    violations.append(
                        f"{relative}:{line_number}: unexpected GitHub repository "
                        f"{repository[0]}/{repository[1]}"
                    )

    if violations:
        fail("\n".join(violations))


def check_workspace_identity() -> None:
    root_manifest = tomllib.loads(read_text("Cargo.toml"))
    workspace = root_manifest.get("workspace", {})
    workspace_package = root_manifest.get("workspace", {}).get("package", {})

    if workspace_package.get("repository") != CANONICAL_REPOSITORY:
        fail("workspace repository metadata is not canonical")

    members = workspace.get("members", [])
    if not members:
        fail("workspace has no members")
    if any(not member.startswith("crates/cofi-") for member in members):
        fail("every workspace crate path must use the cofi- namespace")

    for manifest_path in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        name = manifest.get("package", {}).get("name", "")
        if not name.startswith("cofi-"):
            fail(f"{manifest_path.relative_to(ROOT)} package name is not CoFi-native: {name}")


def check_desktop_identity() -> None:
    package = json.loads(read_text("apps/desktop/package.json"))
    if package.get("name") != "cofi-desktop":
        fail("desktop package name must be cofi-desktop")

    desktop_manifest = tomllib.loads(read_text("apps/desktop/src-tauri/Cargo.toml"))
    desktop_package = desktop_manifest.get("package", {})
    if desktop_package.get("name") != "cofi-desktop":
        fail("desktop Rust package name must be cofi-desktop")
    if desktop_package.get("repository") != CANONICAL_REPOSITORY:
        fail("desktop repository metadata is not canonical")

    tauri = json.loads(read_text("apps/desktop/src-tauri/tauri.conf.json"))
    if tauri.get("productName") != "CoFi":
        fail("desktop productName must be CoFi")
    if tauri.get("identifier") != "com.communityfinance.cofi":
        fail("desktop application identifier is not canonical")
    if tauri.get("version") != package.get("version"):
        fail("desktop package and Tauri versions differ")


def check_public_surfaces() -> None:
    security_config = read_text(".github/ISSUE_TEMPLATE/config.yml")
    if CANONICAL_SECURITY not in security_config:
        fail("security contact link does not target the canonical repository")

    readme = read_text("README.md")
    if f"git clone {CANONICAL_REPOSITORY}.git" not in readme:
        fail("README clone command is not canonical")

    site_app = read_text("site/app.js")
    if (
        f"{CANONICAL_REPOSITORY}/releases/latest" not in site_app
        or "api.github.com/repos/TheHalfMoon/CommunityFinance-CoFi/releases/latest"
        not in site_app
    ):
        fail("landing-page release endpoints are not canonical")

    notice = read_text("NOTICE")
    if "Community Finance / CoFi" not in notice or "TheHalfMoon" not in notice:
        fail("NOTICE does not carry the canonical CoFi identity")


def main() -> int:
    check_workspace_identity()
    check_desktop_identity()
    check_public_surfaces()
    check_external_repository_links()
    print("COFI_REPOSITORY_IDENTITY=PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
