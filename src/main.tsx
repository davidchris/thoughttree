import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { setBackendTransport, TauriTransport } from "./lib/transport";

setBackendTransport(new TauriTransport());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
