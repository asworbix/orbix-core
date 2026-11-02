import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import RunMonitor from "./components/RunMonitor";
import PairOverlay from "./components/PairOverlay";
import QuickBar from "./components/QuickBar";
import ErrorBoundary from "./components/ErrorBoundary";
import "./styles/globals.css";

// Tag the document with the host platform so platform-aware CSS selectors
// (e.g. window-control buttons on Windows, traffic-light gutter on macOS) can
// switch on `[data-platform="..."]`.
const ua = navigator.userAgent;
const platform = /Windows/i.test(ua) ? "win"
  : /Macintosh|Mac OS X/i.test(ua) ? "mac"
  : /Linux/i.test(ua) ? "linux"
  : "unknown";
document.documentElement.dataset.platform = platform;

// The Run monitor window loads the same bundle with a query flag and
// renders only the monitor - no second entry point to maintain.
const windowKind = new URLSearchParams(window.location.search).get("window") ?? "";
const isMonitor = ["monitor", "agent-monitor"].includes(windowKind);
// The pair-mode drag overlay: same bundle, draws only the bridge.
const isPairOverlay = windowKind === "pair-overlay";
// The quick bar: Bob from anywhere, behind a global shortcut.
const isQuickBar = windowKind === "bar";

// A root ErrorBoundary so a throw anywhere outside a panel shows a recovery
// card instead of a blank white window - the panels have their own boundary,
// but nothing caught the shell itself.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary fallbackLabel="Orbix Core">
      {isQuickBar ? <QuickBar /> : isPairOverlay ? <PairOverlay /> : isMonitor ? <RunMonitor /> : <App />}
    </ErrorBoundary>
  </React.StrictMode>
);
