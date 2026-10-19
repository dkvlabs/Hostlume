import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import { CompactMonitor, isCompactMode } from "@/features/compact/CompactMonitor";
import "@/styles/globals.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("missing #root element");
}

// The compact floating monitor (phase 06 §2.6 fallback) runs in its own
// frameless always-on-top window, flagged by an initialization script.
createRoot(container).render(
  <StrictMode>
    {isCompactMode() ? <CompactMonitor /> : <App />}
  </StrictMode>
);
