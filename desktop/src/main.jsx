import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.jsx";
import "./app.css";
import { tauri, tryAct, toast } from "./store.js";

if (import.meta.env.DEV) {
  import("react-grab");
}

// No WebKit context menu (Reload, Inspect…) outside text fields, and no dragging images or links around.
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input, textarea, [data-selectable]")) e.preventDefault(); });
document.addEventListener("dragstart", (e) => { if (!e.target.closest("input, textarea, [draggable='true']")) e.preventDefault(); });
// A file dropped anywhere becomes a source; the webview must never just open it.
document.addEventListener("dragover", (e) => { if (e.dataTransfer?.types?.includes("Files")) e.preventDefault(); });
document.addEventListener("drop", (e) => { if (e.dataTransfer?.types?.includes("Files")) e.preventDefault(); });
const VIDEO = /\.(mp4|mov|m4v|mkv|webm|avi|mts|m4a|mp3|wav)$/i;
if (tauri) {
  import("@tauri-apps/api/webview").then(({ getCurrentWebview }) => getCurrentWebview().onDragDropEvent((ev) => {
    if (ev.payload.type !== "drop") return;
    const files = (ev.payload.paths || []).filter((p) => VIDEO.test(p));
    if (!files.length) { toast("Drop a video file", "err"); return; }
    files.forEach((p) => tryAct("add_source", { path: p }, `Added ${p.split("/").pop()}. Transcribing.`));
  }));
}

createRoot(document.getElementById("root")).render(
  <StrictMode>
    <App />
  </StrictMode>
);
