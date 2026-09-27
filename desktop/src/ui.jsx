import { useEffect, useState } from "react";

export const Panel = ({ title, sub, right, children, style, className = "", body = true }) => (
  <div className={`panel ${className}`} style={style}>
    {title !== undefined && (
      <div className="panel-head">
        <span>{title}</span>
        {sub && <span className="sub">{sub}</span>}
        <span className="grow" />
        {right}
      </div>
    )}
    {body ? <div className="panel-body">{children}</div> : children}
  </div>
);

export const Btn = ({ primary, small, danger, icon, className = "", ...p }) => (
  <button type="button" className={`btn ${primary ? "primary" : ""} ${small ? "small" : ""} ${danger ? "danger" : ""} ${icon ? "icon" : ""} ${className}`} {...p} />
);

export const Seg = ({ value, onChange, options, big }) => (
  <div className={`seg ${big ? "big" : ""}`}>
    {options.map(([v, label]) => (
      <button key={v} type="button" className={v === value ? "on" : ""} onClick={() => onChange(v)}>{label}</button>
    ))}
  </div>
);

export const Field = ({ label, children }) => (
  <label className="field"><span>{label}</span>{children}</label>
);

export const Check = ({ label, checked, onChange, right }) => (
  <label className="check"><input type="checkbox" checked={!!checked} onChange={(e) => onChange(e.target.checked)} /><span className="grow">{label}</span>{right && <span className="muted">{right}</span>}</label>
);

export const Dot = ({ c }) => <span className={`dot ${c || ""}`} />;

/* A text input that commits on blur/Enter (so typing doesn't spam the core). */
export function Text({ value, onCommit, area, ...p }) {
  const [v, setV] = useState(value ?? "");
  useEffect(() => setV(value ?? ""), [value]);
  const commit = () => { if (v !== (value ?? "")) onCommit(v); };
  const Tag = area ? "textarea" : "input";
  return <Tag className="input" value={v} onChange={(e) => setV(e.target.value)} onBlur={commit} onKeyDown={(e) => { if (e.key === "Enter" && !area) e.target.blur(); }} {...p} />;
}

export const Bar = ({ v, mint }) => <span className={`bar ${mint ? "mint" : ""}`}><i style={{ width: `${Math.round((v || 0) * 100)}%` }} /></span>;

export const I = {
  play: <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M6 4l14 8-14 8z" /></svg>,
  pause: <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M6 4h4v16H6zM14 4h4v16h-4z" /></svg>,
  prev: <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M18 4v16L6 12z" /></svg>,
  next: <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M6 4v16l12-8z" /></svg>,
  plus: <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"><path d="M12 5v14M5 12h14" /></svg>,
  x: <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>,
  check: <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="M20 6L9 17l-5-5" /></svg>,
  gear: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>,
  search: <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round"><circle cx="11" cy="11" r="7" /><path d="M20 20l-3.5-3.5" /></svg>,
  folder: <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>,
  trash: <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" /></svg>,
  yt: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="5" width="20" height="14" rx="4" /><path d="M10 9l5 3-5 3z" /></svg>,
  file: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>,
};

/* Brand marks (simple-icons, CC0). `Brand.youtube` etc.; `mono` draws them in the current text colour. */
const brandPaths = {
  youtube: "M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z",
  instagram: "M7.0301.084c-1.2768.0602-2.1487.264-2.911.5634-.7888.3075-1.4575.72-2.1228 1.3877-.6652.6677-1.075 1.3368-1.3802 2.127-.2954.7638-.4956 1.6365-.552 2.914-.0564 1.2775-.0689 1.6882-.0626 4.947.0062 3.2586.0206 3.6671.0825 4.9473.061 1.2765.264 2.1482.5635 2.9107.308.7889.72 1.4573 1.388 2.1228.6679.6655 1.3365 1.0743 2.1285 1.38.7632.295 1.6361.4961 2.9134.552 1.2773.056 1.6884.069 4.9462.0627 3.2578-.0062 3.668-.0207 4.9478-.0814 1.28-.0607 2.147-.2652 2.9098-.5633.7889-.3086 1.4578-.72 2.1228-1.3881.665-.6682 1.0745-1.3378 1.3795-2.1284.2957-.7632.4966-1.636.552-2.9124.056-1.2809.0692-1.6898.063-4.948-.0063-3.2583-.021-3.6668-.0817-4.9465-.0607-1.2797-.264-2.1487-.5633-2.9117-.3084-.7889-.72-1.4568-1.3876-2.1228C21.2982 1.33 20.628.9208 19.8378.6165 19.074.321 18.2017.1197 16.9244.0645 15.6471.0093 15.236-.005 11.977.0014 8.718.0076 8.31.0215 7.0301.0839m.1402 21.6932c-1.17-.0509-1.8053-.2453-2.2287-.408-.5606-.216-.96-.4771-1.3819-.895-.422-.4178-.6811-.8186-.9-1.378-.1644-.4234-.3624-1.058-.4171-2.228-.0595-1.2645-.072-1.6442-.079-4.848-.007-3.2037.0053-3.583.0607-4.848.05-1.169.2456-1.805.408-2.2282.216-.5613.4762-.96.895-1.3816.4188-.4217.8184-.6814 1.3783-.9003.423-.1651 1.0575-.3614 2.227-.4171 1.2655-.06 1.6447-.072 4.848-.079 3.2033-.007 3.5835.005 4.8495.0608 1.169.0508 1.8053.2445 2.228.408.5608.216.96.4754 1.3816.895.4217.4194.6816.8176.9005 1.3787.1653.4217.3617 1.056.4169 2.2263.0602 1.2655.0739 1.645.0796 4.848.0058 3.203-.0055 3.5834-.061 4.848-.051 1.17-.245 1.8055-.408 2.2294-.216.5604-.4763.96-.8954 1.3814-.419.4215-.8181.6811-1.3783.9-.4224.1649-1.0577.3617-2.2262.4174-1.2656.0595-1.6448.072-4.8493.079-3.2045.007-3.5825-.006-4.848-.0608M16.953 5.5864A1.44 1.44 0 1 0 18.39 4.144a1.44 1.44 0 0 0-1.437 1.4424M5.8385 12.012c.0067 3.4032 2.7706 6.1557 6.173 6.1493 3.4026-.0065 6.157-2.7701 6.1506-6.1733-.0065-3.4032-2.771-6.1565-6.174-6.1498-3.403.0067-6.156 2.771-6.1496 6.1738M8 12.0077a4 4 0 1 1 4.008 3.9921A3.9996 3.9996 0 0 1 8 12.0077",
  tiktok: "M12.525.02c1.31-.02 2.61-.01 3.91-.02.08 1.53.63 3.09 1.75 4.17 1.12 1.11 2.7 1.62 4.24 1.79v4.03c-1.44-.05-2.89-.35-4.2-.97-.57-.26-1.1-.59-1.62-.93-.01 2.92.01 5.84-.02 8.75-.08 1.4-.54 2.79-1.35 3.94-1.31 1.92-3.58 3.17-5.91 3.21-1.43.08-2.86-.31-4.08-1.03-2.02-1.19-3.44-3.37-3.65-5.71-.02-.5-.03-1-.01-1.49.18-1.9 1.12-3.72 2.58-4.96 1.66-1.44 3.98-2.13 6.15-1.72.02 1.48-.04 2.96-.04 4.44-.99-.32-2.15-.23-3.02.37-.63.41-1.11 1.04-1.36 1.75-.21.51-.15 1.07-.14 1.61.24 1.64 1.82 3.02 3.5 2.87 1.12-.01 2.19-.66 2.77-1.61.19-.33.4-.67.41-1.06.1-1.79.06-3.57.07-5.36.01-4.03-.01-8.05.02-12.07z",
  facebook: "M9.101 23.691v-7.98H6.627v-3.667h2.474v-1.58c0-4.085 1.848-5.978 5.858-5.978.401 0 .955.042 1.468.103a8.68 8.68 0 0 1 1.141.195v3.325a8.623 8.623 0 0 0-.653-.036 26.805 26.805 0 0 0-.733-.009c-.707 0-1.259.096-1.675.309a1.686 1.686 0 0 0-.679.622c-.258.42-.374.995-.374 1.752v1.297h3.919l-.386 2.103-.287 1.564h-3.246v8.245C19.396 23.238 24 18.179 24 12.044c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.628 3.874 10.35 9.101 11.647Z"
};
const brandColors = {"youtube": "#FF0000", "instagram": "#E4405F", "tiktok": "#F4F4F5", "facebook": "#1877F2"};
export const Brand = ({ id, size = 14, mono = false, style }) => {
  const p = brandPaths[id];
  if (!p) return <span className="sq" style={{ background: "var(--violet)", width: size * 0.6, height: size * 0.6, ...style }} />;
  return <svg width={size} height={size} viewBox="0 0 24 24" fill={mono ? "currentColor" : brandColors[id]} style={{ flexShrink: 0, ...style }} aria-hidden="true"><path d={p} /></svg>;
};
export const FileMark = ({ size = 14, style }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" style={{ flexShrink: 0, color: "var(--violet)", ...style }} aria-hidden="true"><path d="M6 3h8l5 5v13H6z" /><path d="M14 3v5h5" /><path d="M9 17l2-2 2 2 3-3" /></svg>
);
export const CHANNEL_NAMES = { youtube: "YouTube", instagram: "Instagram", tiktok: "TikTok", facebook: "Facebook" };

/* In-app confirm, used instead of the native dialog. */
export const Confirm = ({ text, yes = "Discard", no = "Keep editing", onYes, onNo }) => (
  <div className="sheet-bg" style={{ zIndex: 30 }} onMouseDown={(e) => e.target === e.currentTarget && onNo()}>
    <div className="sheet" style={{ width: 380 }}>
      <div className="panel-body" style={{ padding: 16, gap: 14 }}>
        <span style={{ fontSize: 14.5 }}>{text}</span>
        <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}><Btn onClick={onNo} autoFocus>{no}</Btn><Btn danger onClick={onYes}>{yes}</Btn></div>
      </div>
    </div>
  </div>
);

/* Category marks: monochrome line icons, no coloured words. */
const catPaths = {
  fact: <><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7.5v.5" /></>,
  statement: <><path d="M9 7H5a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1h3v3l3-3V8a1 1 0 0 0-1-1zM20 7h-4a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1h3v3l3-3V8a1 1 0 0 0-1-1z" /></>,
  hook: <><path d="M12 3v11a4 4 0 0 0 8 0v-1M12 3a2 2 0 1 0 0 .01" /></>,
  story: <><path d="M4 5a2 2 0 0 1 2-2h6v16H6a2 2 0 0 0-2 2zM12 3h6a2 2 0 0 1 2 2v16a2 2 0 0 0-2-2h-6z" /></>,
  dua: <><path d="M7 20V9a2 2 0 0 1 4 0v5M17 20V9a2 2 0 0 0-4 0v5M7 20a3 3 0 0 0 10 0" /></>,
  reminder: <><path d="M6 16V11a6 6 0 0 1 12 0v5l2 2H4zM10 20a2 2 0 0 0 4 0" /></>,
  qa: <><circle cx="12" cy="12" r="9" /><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.4-1 .9-1 1.7M12 17v.5" /></>,
};
export const CAT_NAMES = { fact: "Fact", statement: "Statement", hook: "Hook", story: "Story", dua: "Du'a", reminder: "Reminder", qa: "Q&A" };
export const Cat = ({ id, size = 14, style }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0, ...style }} aria-label={CAT_NAMES[id] || id}><title>{CAT_NAMES[id] || id}</title>{catPaths[id] || catPaths.statement}</svg>
);

/* Playback speed for previews: a small segmented control, remembered per session. */
export const SPEEDS = [1, 1.25, 1.5, 1.75, 2, 2.5, 3];
export const Speed = ({ value, onChange }) => (
  <div className="seg" title="Playback speed">
    {SPEEDS.map((v) => <button key={v} type="button" className={v === value ? "on" : ""} onClick={() => onChange(v)}>{v}×</button>)}
  </div>
);

/** Right-click menu. `at` is { x, y }; items are { label, onClick, danger, disabled }. Closes on any click, Escape or scroll. */
export function Menu({ at, items, onClose }) {
  useEffect(() => {
    const off = () => onClose();
    const key = (e) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("pointerdown", off); window.addEventListener("keydown", key); window.addEventListener("scroll", off, true); window.addEventListener("blur", off);
    return () => { window.removeEventListener("pointerdown", off); window.removeEventListener("keydown", key); window.removeEventListener("scroll", off, true); window.removeEventListener("blur", off); };
  }, [onClose]);
  const x = Math.min(at.x, window.innerWidth - 200), y = Math.min(at.y, window.innerHeight - items.length * 28 - 12);
  return <div className="menu" style={{ left: x, top: y }} onPointerDown={(e) => e.stopPropagation()}>{items.map((it, i) => <button key={i} className={`menu-item ${it.danger ? "danger" : ""}`} disabled={it.disabled} onClick={() => { onClose(); it.onClick(); }}>{it.label}</button>)}</div>;
}
