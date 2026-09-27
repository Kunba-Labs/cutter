/* One store: the core's snapshot, refreshed on `library-changed`; every
   write is `act(action, args)` = invoke("dispatch"). In a plain browser
   (vite without Tauri) the store stays empty and says so. */
import { useSyncExternalStore } from "react";

export const tauri = typeof window !== "undefined" && !!window.__TAURI_INTERNALS__;

let state = { sources: [], channels: [], inbox: [], candidates: [], renders: [], posts: [], posters: [], jobs: [], settings: {}, tools: {}, paths: {}, ready: false, error: null };
const listeners = new Set();
const emit = () => listeners.forEach((l) => l());

export function useStore(selector = (s) => s) {
  return useSyncExternalStore((l) => (listeners.add(l), () => listeners.delete(l)), () => selector(state), () => selector(state));
}
export const getState = () => state;

let invoke, convertFileSrc;

export async function act(action, args = {}) {
  if (!tauri) throw new Error("Open this in the Cuttar app");
  return invoke("dispatch", { action, args });
}

export const fileUrl = (p) => (p && convertFileSrc ? convertFileSrc(p) : p);

export async function refresh() {
  try {
    const s = await act("snapshot");
    state = { ...state, ...s, ready: true, error: null };
  } catch (e) {
    state = { ...state, error: String(e) };
  }
  emit();
}

export function toast(msg, kind = "info") {
  state = { ...state, toast: { msg: String(msg), kind, at: Date.now() } };
  emit();
  setTimeout(() => {
    if (state.toast && Date.now() - state.toast.at >= 3900) {
      state = { ...state, toast: null };
      emit();
    }
  }, 4000);
}

/// An action with the error surfaced instead of swallowed.
export async function tryAct(action, args, okMsg) {
  try {
    const r = await act(action, args);
    if (okMsg) toast(okMsg, "ok");
    return r;
  } catch (e) {
    toast(e?.message || e, "err");
    return null;
  }
}

async function init() {
  if (!tauri) {
    state = { ...state, ready: true, error: "Not inside the Cuttar app: run `bin/dev` (Tauri) instead of the bare Vite page." };
    emit();
    return;
  }
  ({ invoke, convertFileSrc } = await import("@tauri-apps/api/core"));
  const { listen } = await import("@tauri-apps/api/event");
  window.open = (url) => invoke("open_url", { url: String(url) });
  document.addEventListener("click", (e) => {
    const a = e.target.closest?.("a[href^=http]");
    if (a) { e.preventDefault(); window.open(a.href); }
  }, true);
  await refresh();
  let t;
  await listen("library-changed", () => {
    clearTimeout(t);
    t = setTimeout(refresh, 120);
  });
}
init();

export const fmt = (s) => {
  if (s == null || isNaN(s)) return "–";
  const t = Math.max(0, s);
  const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
  return h ? `${h}:${String(m).padStart(2, "0")}:${String(Math.floor(sec)).padStart(2, "0")}` : `${m}:${sec.toFixed(1).padStart(4, "0")}`;
};
export const fmtLong = (s) => {
  if (s == null) return "–";
  const t = Math.round(s), h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
  return h ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}` : `${m}:${String(sec).padStart(2, "0")}`;
};
export const ago = (iso) => {
  if (!iso) return "";
  const d = (Date.now() - new Date(iso).getTime()) / 1000;
  if (d < 60) return "just now";
  if (d < 3600) return `${Math.floor(d / 60)} min ago`;
  if (d < 86400) return `${Math.floor(d / 3600)} h ago`;
  return `${Math.floor(d / 86400)} d ago`;
};
export const STAGES = { added: ["muted", "Added"], downloading: ["pink", "Downloading"], downloaded: ["mint", "Downloaded"], transcribing: ["pink", "Transcribing"], transcribed: ["mint", "Transcribed"], detecting: ["pink", "Finding reels"], review: ["pink", "Review"], done: ["mint", "Done"], failed: ["coral", "Failed"] };
export const LANGS = [["auto", "Auto-detect"], ["nl", "Dutch"], ["en", "English"], ["ur", "Urdu"], ["ar", "Arabic"], ["tr", "Turkish"], ["fr", "French"], ["de", "German"], ["id", "Indonesian"], ["ms", "Malay"], ["bn", "Bengali"], ["hi", "Hindi"], ["fa", "Persian"], ["so", "Somali"]];
export const CATS = ["fact", "statement", "hook", "story", "dua", "reminder", "qa"];
export const FORMATS = [["shorts", "Shorts"], ["reels", "Reels"], ["tiktok", "TikTok"], ["feed", "Feed 4:5"], ["landscape", "16:9"]];

/* Preview playback speed, shared by the Reels and Transcript players. */
let speed = 1;
try { speed = Number(localStorage.getItem("cuttar.speed")) || 1; } catch {}
const speedListeners = new Set();
export function useSpeed() {
  return useSyncExternalStore((l) => (speedListeners.add(l), () => speedListeners.delete(l)), () => speed, () => speed);
}
export function setSpeed(v) {
  speed = v;
  try { localStorage.setItem("cuttar.speed", String(v)); } catch {}
  speedListeners.forEach((l) => l());
}
