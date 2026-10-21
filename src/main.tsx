import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { OVERLAY_WINDOW } from "./constants";
import Overlay from "./Overlay";
import "./styles.css";

const isOverlay = getCurrentWindow().label === OVERLAY_WINDOW;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isOverlay ? <Overlay /> : <App />}</React.StrictMode>,
);
