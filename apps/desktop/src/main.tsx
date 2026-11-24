// Entry point inside the manager window. The only native calls are the two
// allow-listed commands behind commandBridge.

import { invoke } from "@tauri-apps/api/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import { commandBridge } from "./ipc.ts";
import "./theme/brand.css";
import "./theme/app.css";

const root = document.getElementById("root");
if (root === null) throw new Error("index.html has no #root element");

const bridge = commandBridge((command) => invoke(command));

createRoot(root).render(
  <StrictMode>
    <App bridge={bridge} />
  </StrictMode>,
);
