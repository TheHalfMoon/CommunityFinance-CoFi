const RELEASE_API =
  "https://api.github.com/repos/TheHalfMoon/CommunityFinance-CoFi/releases/latest";
const RELEASE_FALLBACK =
  "https://github.com/TheHalfMoon/CommunityFinance-CoFi/releases/latest";

function detectPlatform() {
  const platform = navigator.userAgentData?.platform ?? navigator.platform ?? "";
  const normalized = platform.toLowerCase();

  if (normalized.includes("win")) return "windows";
  if (normalized.includes("mac")) return "macos";
  if (normalized.includes("linux")) return "linux";
  return "unknown";
}

function scoreAsset(asset, platform) {
  const name = asset.name.toLowerCase();

  if (platform === "windows") {
    if (name.endsWith(".msix")) return 50;
    if (name.endsWith(".msi")) return 45;
    if (name.endsWith(".exe")) return 40;
    if (name.endsWith(".zip") && /windows|win32|win64|x64/.test(name)) return 20;
  }

  if (platform === "macos") {
    if (name.endsWith(".dmg")) return 50;
    if (name.endsWith(".pkg")) return 45;
    if (name.endsWith(".zip") && /mac|macos|darwin/.test(name)) return 20;
  }

  if (platform === "linux") {
    if (name.endsWith(".appimage")) return 50;
    if (name.endsWith(".deb")) return 45;
    if (name.endsWith(".rpm")) return 40;
    if (
      (name.endsWith(".tar.gz") || name.endsWith(".tgz")) &&
      /linux|x86_64|amd64/.test(name)
    ) {
      return 20;
    }
  }

  return 0;
}

function assetArchitecture(name) {
  const normalized = name.toLowerCase();

  if (/arm64|aarch64/.test(normalized)) return "arm64";
  if (/x86_64|amd64|x64/.test(normalized)) return "x64";
  if (/x86|win32|i[3-6]86/.test(normalized)) return "x86";
  return "generic";
}

function pickInstaller(assets, platform) {
  const candidates = [...assets]
    .map((asset) => ({
      asset,
      score: scoreAsset(asset, platform),
      architecture: assetArchitecture(asset.name),
    }))
    .filter(({ score }) => score > 0)
    .sort((a, b) => b.score - a.score);

  if (candidates.length === 0) return undefined;

  const highestScore = candidates[0].score;
  const best = candidates.filter(({ score }) => score === highestScore);

  if (best.length === 1) return best[0].asset;

  const generic = best.filter(({ architecture }) => architecture === "generic");
  return generic.length === 1 ? generic[0].asset : undefined;
}

function formatSize(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "";
  const megabytes = bytes / 1024 / 1024;
  return `${megabytes.toFixed(megabytes >= 100 ? 0 : 1)} MB`;
}

function platformLabel(platform) {
  if (platform === "windows") return "Windows";
  if (platform === "macos") return "macOS";
  if (platform === "linux") return "Linux";
  return "Desktop";
}

function updateDownloadLinks({ href, label, compactLabel, ariaLabel }) {
  document.querySelectorAll("[data-download-link]").forEach((link) => {
    link.href = href;
    link.setAttribute("aria-label", ariaLabel);
  });

  document.querySelectorAll("[data-download-label]").forEach((element) => {
    element.textContent = element.hasAttribute("data-compact-label")
      ? compactLabel
      : label;
  });
}

async function hydrateReleaseDownload() {
  const note = document.querySelector("[data-download-note]");
  const platform = detectPlatform();
  const label = platformLabel(platform);

  try {
    const response = await fetch(RELEASE_API, {
      headers: { Accept: "application/vnd.github+json" },
    });

    if (!response.ok) {
      throw new Error(`GitHub API returned ${response.status}`);
    }

    const release = await response.json();
    const installer = pickInstaller(release.assets ?? [], platform);

    if (installer) {
      updateDownloadLinks({
        href: installer.browser_download_url,
        label: `Download for ${label}`,
        compactLabel: "Download",
        ariaLabel: `Download CoFi for ${label}`,
      });

      if (note) {
        const size = formatSize(installer.size);
        note.textContent = [installer.name, size, release.tag_name]
          .filter(Boolean)
          .join(" · ");
      }

      return;
    }

    updateDownloadLinks({
      href: release.html_url || RELEASE_FALLBACK,
      label: "View latest release",
      compactLabel: "Release",
      ariaLabel: "View the latest CoFi release on GitHub",
    });

    if (note) {
      note.textContent =
        `CoFi ${release.tag_name ?? "latest"} is published, but a ${label} desktop installer is not available yet.`;
    }
  } catch {
    updateDownloadLinks({
      href: RELEASE_FALLBACK,
      label: "View latest release",
      compactLabel: "Release",
      ariaLabel: "View the latest CoFi release on GitHub",
    });

    if (note) {
      note.textContent =
        "Open the latest GitHub release to see currently available CoFi downloads.";
    }
  }
}

function initMobileMenu() {
  const menuButton = document.querySelector(".menu-button");
  const mobileMenu = document.querySelector("#mobile-menu");

  if (!menuButton || !mobileMenu) return;

  menuButton.addEventListener("click", () => {
    const isOpen = menuButton.getAttribute("aria-expanded") === "true";
    menuButton.setAttribute("aria-expanded", String(!isOpen));
    mobileMenu.hidden = isOpen;
  });

  mobileMenu.addEventListener("click", (event) => {
    if (event.target instanceof HTMLAnchorElement) {
      menuButton.setAttribute("aria-expanded", "false");
      mobileMenu.hidden = true;
    }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !mobileMenu.hidden) {
      mobileMenu.hidden = true;
      menuButton.setAttribute("aria-expanded", "false");
      menuButton.focus();
    }
  });
}

if (typeof document !== "undefined") {
  initMobileMenu();
  hydrateReleaseDownload();
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    assetArchitecture,
    formatSize,
    pickInstaller,
    platformLabel,
    scoreAsset,
  };
}
