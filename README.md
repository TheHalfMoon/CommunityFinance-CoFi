# CoFi landing site

This directory contains the zero-dependency public landing page for the CoFi desktop app.

## Scope

- Product: CoFi desktop app.
- Website: marketing + download landing only.
- Download destination: GitHub Releases.
- If exactly one installer matches the visitor's platform, the primary CTA links directly to it.
- If no installer exists or architecture is ambiguous, the CTA falls back to the latest release page instead of guessing.
- No web application or authenticated dashboard is implemented here.
- The dashboard shown on the landing page is a visual product preview.

## Local preview

From the repository root:

```powershell
py -m http.server 4173 -d site
```

Then open `http://127.0.0.1:4173`.

## Verification

```powershell
node --check site/app.js
node tests/site-release-selector.test.cjs
```

## Design

The canonical CoFi mark is black/white only. The landing page uses the approved light lavender/mist visual system and responsive layouts for desktop, tablet, and mobile.
