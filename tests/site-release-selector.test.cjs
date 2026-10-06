const assert = require("node:assert/strict");

const {
  assetArchitecture,
  formatSize,
  pickInstaller,
  platformLabel,
  scoreAsset,
} = require("../site/app.js");

const windowsMsi = {
  name: "CoFi-Setup-x64.msi",
  browser_download_url: "https://example.test/CoFi-Setup-x64.msi",
  size: 50 * 1024 * 1024,
};

assert.equal(scoreAsset(windowsMsi, "windows"), 45);
assert.equal(scoreAsset(windowsMsi, "macos"), 0);
assert.equal(assetArchitecture(windowsMsi.name), "x64");
assert.equal(formatSize(windowsMsi.size), "50.0 MB");
assert.equal(platformLabel("windows"), "Windows");

assert.equal(pickInstaller([], "windows"), undefined);
assert.equal(pickInstaller([windowsMsi], "windows"), windowsMsi);

const ambiguousWindowsAssets = [
  windowsMsi,
  {
    name: "CoFi-Setup-arm64.msi",
    browser_download_url: "https://example.test/CoFi-Setup-arm64.msi",
    size: 50 * 1024 * 1024,
  },
];

assert.equal(
  pickInstaller(ambiguousWindowsAssets, "windows"),
  undefined,
  "architecture-specific installers must not be guessed when ambiguous",
);

const genericWindowsAssets = [
  ...ambiguousWindowsAssets,
  {
    name: "CoFi-Setup.msi",
    browser_download_url: "https://example.test/CoFi-Setup.msi",
    size: 50 * 1024 * 1024,
  },
];

assert.equal(
  pickInstaller(genericWindowsAssets, "windows")?.name,
  "CoFi-Setup.msi",
  "a single generic installer is safe to select",
);

assert.equal(
  pickInstaller(
    [
      {
        name: "CoFi.dmg",
        browser_download_url: "https://example.test/CoFi.dmg",
        size: 60 * 1024 * 1024,
      },
    ],
    "macos",
  )?.name,
  "CoFi.dmg",
);

assert.equal(
  pickInstaller(
    [
      {
        name: "CoFi.AppImage",
        browser_download_url: "https://example.test/CoFi.AppImage",
        size: 70 * 1024 * 1024,
      },
    ],
    "linux",
  )?.name,
  "CoFi.AppImage",
);

console.log("site-release-selector: PASS");
