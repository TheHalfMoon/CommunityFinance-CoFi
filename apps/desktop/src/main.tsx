import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { applyDocumentLocale, getInitialLocale } from "./i18n";
import "./styles.css";

applyDocumentLocale(getInitialLocale());

const root = document.getElementById("root");

if (!root) {
  throw new Error("CoFi root element is missing");
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
