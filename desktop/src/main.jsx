import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.jsx";
import "./app.css";

if (import.meta.env.DEV) {
  import("react-grab");
}

// No WebKit context menu (Reload, Inspect…) outside text fields, and no dragging images or links around.
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input, textarea, [data-selectable]")) e.preventDefault(); });
document.addEventListener("dragstart", (e) => { if (!e.target.closest("input, textarea")) e.preventDefault(); });

createRoot(document.getElementById("root")).render(
  <StrictMode>
    <App />
  </StrictMode>
);
