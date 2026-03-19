import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Settings from "./Settings";
import "./styles.css";

function getWindowLabel(): string {
  try {
    const internals = (window as any).__TAURI_INTERNALS__;
    if (internals?.metadata?.currentWindow?.label) {
      return internals.metadata.currentWindow.label;
    }
  } catch {}
  // URL parametresi ile de kontrol et
  const params = new URLSearchParams(window.location.search);
  return params.get("window") || "main";
}

const windowLabel = getWindowLabel();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {windowLabel === "settings" ? <Settings /> : <App />}
  </React.StrictMode>
);
