import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { usePlayer } from "./store/playerStore";

// Dev-only handle so the presentation layer can be inspected and driven without a
// Spotify account. `import.meta.env.DEV` is false in the release bundle, so this
// block is dead-code-eliminated from production builds.
if (import.meta.env.DEV) {
  (window as unknown as { __sonic?: unknown }).__sonic = {
    store: usePlayer,
    setState: (partial: Record<string, unknown>) =>
      usePlayer.setState(partial as never),
  };
}

/**
 * Entry point.
 *
 * `document.documentElement` gets an explicit transparent background so the
 * WebView2 surface composites against the desktop rather than white.
 */
const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("Sonic: #root is missing from index.html");

/**
 * Last-resort error surface.
 *
 * If the bundle throws before React commits (or a render crashes), a transparent
 * window would otherwise look like "Sonic is invisible". Painting the failure on
 * a dark card makes the cause obvious instead of silent.
 */
function showFatal(message: string) {
  const host = document.getElementById("root");
  if (!host) return;
  host.innerHTML = "";
  const card = document.createElement("div");
  card.setAttribute(
    "style",
    [
      "position:fixed",
      "inset:0",
      "display:flex",
      "align-items:center",
      "justify-content:center",
      "padding:24px",
      "background:rgba(10,10,14,0.92)",
      "color:#fff",
      "font:12px/1.5 -apple-system,Segoe UI,Roboto,sans-serif",
      "border-radius:26px",
      "box-sizing:border-box",
    ].join(";"),
  );
  card.innerHTML =
    '<div style="max-width:330px;text-align:center">' +
    '<div style="font-size:13px;font-weight:600;margin-bottom:6px">Sonic could not start</div>' +
    '<div style="color:#a1a1aa;font-size:11px;word-break:break-word"></div>' +
    "</div>";
  card.lastElementChild!.textContent = message;
  host.appendChild(card);
}

window.addEventListener("error", (event) => {
  showFatal(event.message || String(event.error));
});
window.addEventListener("unhandledrejection", (event) => {
  showFatal(String(event.reason));
});

// Ensure the transparent window is actually transparent at the document level.
document.documentElement.style.background = "transparent";
document.body.style.background = "transparent";

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);