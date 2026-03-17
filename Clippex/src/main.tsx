import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Settings from "./Settings";
import "./styles.css";

const windowLabel = (window as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label || "main";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {windowLabel === "settings" ? <Settings /> : <App />}
  </React.StrictMode>
);
