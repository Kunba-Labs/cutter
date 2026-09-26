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
  gear: <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M4 6h9M19 6h1M4 12h1M11 12h9M4 18h11M21 18h-1" /><circle cx="16" cy="6" r="2" /><circle cx="8" cy="12" r="2" /><circle cx="18" cy="18" r="2" /></svg>,
  search: <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round"><circle cx="11" cy="11" r="7" /><path d="M20 20l-3.5-3.5" /></svg>,
  folder: <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>,
  trash: <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" /></svg>,
  yt: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="2" y="5" width="20" height="14" rx="4" /><path d="M10 9l5 3-5 3z" /></svg>,
  file: <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>,
};
