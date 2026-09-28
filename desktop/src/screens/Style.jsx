import { useState } from "react";
import { useStore, tryAct, fileUrl, FONTS, FIXED_FONTS } from "../store.js";
import { Panel, Btn, Check, Text, Seg, I } from "../ui.jsx";

/* libass draws bold at 700 and regular at 400; the preview must ask for the same faces
   (800/500 picked Avenir Next Heavy/Medium here while the render used Bold/Regular). */
const weight = (font, bold) => (bold && !FIXED_FONTS.includes(font) ? 700 : 400);

/* The look of a caption line, shared by the Reels preview and this screen. */
export function captionCss(t, k = 1) {
  const shadow = t.boxed ? "none" : t.outlinePx > 0 ? `0 0 ${Math.max(1, t.outlinePx * 0.5 * k)}px #000, 0 0 ${Math.max(1, t.outlinePx * k)}px #000, 0 ${1.5 * k}px ${t.outlinePx * 1.2 * k}px rgba(0,0,0,.85)` : `0 2px 6px rgba(0,0,0,.75)`;
  return {
    fontFamily: `"${t.font}", "Helvetica Neue", sans-serif`,
    fontWeight: weight(t.font, t.bold),
    color: t.textColor,
    textShadow: shadow,
    textTransform: t.uppercase ? "uppercase" : "none",
    textAlign: t.align === "left" ? "left" : "center",
    background: t.boxed ? "rgba(0,0,0,.6)" : "none",
    padding: t.boxed ? `${3 * k}px ${10 * k}px` : 0,
    borderRadius: t.boxed ? 4 * k : 0,
    lineHeight: 1.15,
  };
}

/* The title at the top, from the same template. */
export function hookCss(t, k = 1) {
  const bg = t.hookBoxed ? t.hookBoxColor || "#000000" : "none";
  return {
    fontFamily: `"${t.hookFont || t.font}", "Helvetica Neue", sans-serif`,
    fontSize: (t.hookSize || 40) * (k * 533 / 1920),
    fontWeight: weight(t.hookFont || t.font, t.hookBold),
    fontStyle: t.hookItalic ? "italic" : "normal",
    color: t.hookColor || "#fff",
    background: bg === "none" ? "none" : bg + "cc",
    padding: t.hookBoxed ? `${4 * k}px ${12 * k}px` : 0,
    borderRadius: t.hookBoxed ? 4 * k : 0,
    textShadow: t.hookBoxed ? "none" : "0 0 2px #000, 0 2px 6px rgba(0,0,0,.85)",
    textTransform: t.hookUppercase ? "uppercase" : "none",
    lineHeight: 1.15,
    textAlign: "center",
    maxWidth: "90%",
  };
}

const SAMPLE = ["sabr", "is", "niet", "wachten", "tot", "het", "voorbij", "is"];

/* The lit word: the highlight colour, and 115 % when the template pops it (libass \\fscx115). */
export const litCss = (t) => {
  const lit = (t.highlight || "").toLowerCase() !== (t.textColor || "#ffffff").toLowerCase();
  return lit || t.wordPop ? { ...(lit ? { color: t.highlight } : {}), ...(t.wordPop ? { fontSize: "1.15em" } : {}) } : undefined;
};

export function Sample({ t, cur = 2, scale = 1, w = 260 }) {
  const per = Math.max(1, t.wordsPerLine || 4);
  const line = SAMPLE.slice(0, per);
  return (
    <span style={{ ...captionCss(t, scale), fontSize: (t.size || 42) * scale * (w / 540), display: "inline-block", maxWidth: "100%" }}>
      {line.map((wd, i) => <span key={i} style={i === cur ? litCss(t) : undefined}>{wd} </span>)}
    </span>
  );
}

export default function Style({ nav, go }) {
  const s = useStore();
  const templates = s.settings.captionTemplates || [];
  const [selName, setSelName] = useState(s.settings.captionStyle?.name || templates[0]?.name);
  const [drag, setDrag] = useState(null);
  const [hdrag, setHdrag] = useState(null);
  const idx = Math.max(0, templates.findIndex((t) => t.name === selName));
  const t = templates[idx] || templates[0];
  const src = s.sources.find((x) => x.id === nav.sourceId) || s.sources.find((x) => x.thumbPath);
  const reel = s.candidates.find((c) => c.id === nav.candidateId);
  if (!t) return <div className="empty">No caption templates in settings.</div>;

  const save = (list, msg) => tryAct("settings", { patch: { captionTemplates: list } }, msg);
  const patch = (p) => save(templates.map((x, i) => (i === idx ? { ...x, ...p } : x)));
  const isDefault = s.settings.captionStyle?.name === t.name;
  const frameH = 560, frameW = 315, k = frameH / 533;
  const Row = ({ label, children }) => <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 110, flexShrink: 0 }}>{label}</span>{children}</div>;
  const Swatches = ({ value, onPick }) => <span style={{ display: "flex", gap: 6, alignItems: "center" }}>{["#FFFFFF", "#3EF2A3", "#F52ACB", "#FFD400", "#37D2E8", "#FF8A5C", "#14061A"].map((c) => <span key={c} onClick={() => onPick(c)} style={{ width: 18, height: 18, borderRadius: 4, background: c, cursor: "pointer", border: "1px solid var(--rule)", outline: value.toLowerCase() === c.toLowerCase() ? "2px solid #fff" : "none" }} />)}<input type="color" value={value} onChange={(e) => onPick(e.target.value)} style={{ width: 26, height: 22, border: 0, background: "none", padding: 0 }} /></span>;

  return (
    <div className="main">
      <div className="panel" style={{ width: 300, flexShrink: 0 }}>
        <div className="panel-head"><a href="#" onClick={(e) => { e.preventDefault(); go("reels"); }} className="sec">Reels</a><span className="muted">/</span><span className="grow">Caption templates</span><Btn small onClick={() => { const n = { ...t, name: `${t.name} copy` }; save([...templates, n], "Duplicated"); setSelName(n.name); }}>Duplicate</Btn></div>
        <div className="scroll" style={{ padding: 8, display: "flex", flexDirection: "column", gap: 6 }}>
          {templates.map((x) => (
            <button key={x.name} type="button" onClick={() => setSelName(x.name)} style={{ display: "flex", alignItems: "center", gap: 10, padding: "6px 8px", borderRadius: 4, background: "var(--bg)", border: x.name === t.name ? "1px solid var(--accent)" : "1px solid var(--rule)", color: "var(--text)", cursor: "pointer", textAlign: "left" }}>
              <span style={{ width: 54, height: 96, borderRadius: 3, background: "#22263A", display: "flex", alignItems: "flex-end", justifyContent: x.align === "left" ? "flex-start" : "center", padding: "0 3px 14px", boxSizing: "border-box", flexShrink: 0, overflow: "hidden" }}><Sample t={x} scale={0.42} w={140} /></span>
              <span style={{ display: "flex", flexDirection: "column", gap: 2 }}><b>{x.name}{s.settings.captionStyle?.name === x.name ? <span className="muted"> · default</span> : ""}</b><span className="muted" style={{ fontSize: 12 }}>{x.font} · {x.uppercase ? "caps · " : ""}{x.boxed ? "boxed · " : x.outlinePx ? `outline ${x.outlinePx} · ` : ""}{x.wordsPerLine} words · {x.size}px</span></span>
            </button>
          ))}
        </div>
        <span className="grow" />
        <div className="panel-foot" style={{ flexDirection: "column", alignItems: "stretch" }}><span className="hint">Looks that work on Shorts, Reels and TikTok. Reels use the default unless you pick one.</span></div>
      </div>

      <div className="panel grow">
        <div className="panel-head"><span>Preview</span><span className="sub">{t.name}{src ? ` on ${src.title}` : ""}</span></div>
        <div className="stage">
          <div className="frame" style={{ width: frameW, height: frameH, background: "#22263A" }}>
            {src?.thumbPath && <img src={fileUrl(src.thumbPath)} alt="" style={{ position: "absolute", top: 0, left: "50%", height: "100%", transform: "translateX(-50%)", opacity: 0.9 }} />}
            {t.hook && <div className="hook" style={{ top: frameH * ((hdrag?.pct ?? t.hookPct ?? 8) / 100), pointerEvents: "auto", cursor: hdrag ? "grabbing" : "ns-resize" }} onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setHdrag({ y0: e.clientY, pct0: t.hookPct ?? 8, pct: t.hookPct ?? 8 }); }} onPointerMove={(e) => { if (!hdrag) return; setHdrag({ ...hdrag, pct: Math.round(Math.min(60, Math.max(2, hdrag.pct0 + ((e.clientY - hdrag.y0) / frameH) * 100))) }); }} onPointerUp={() => { if (!hdrag) return; if (hdrag.pct !== t.hookPct) patch({ hookPct: hdrag.pct }); setHdrag(null); }} onPointerCancel={() => setHdrag(null)} title="drag up or down"><span style={hookCss(t, k)}>Sabr ≠ wachten</span></div>}
            <div className="cap" style={{ bottom: frameH * ((drag?.pct ?? t.positionPct) / 100) - 10, alignItems: t.align === "left" ? "flex-start" : "center", pointerEvents: "auto", cursor: drag ? "grabbing" : "ns-resize" }} onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setDrag({ y0: e.clientY, pct0: t.positionPct, pct: t.positionPct }); }} onPointerMove={(e) => { if (!drag) return; setDrag({ ...drag, pct: Math.round(Math.min(70, Math.max(4, drag.pct0 + ((drag.y0 - e.clientY) / frameH) * 100))) }); }} onPointerUp={() => { if (!drag) return; if (drag.pct !== t.positionPct) patch({ positionPct: drag.pct }); setDrag(null); }} onPointerCancel={() => setDrag(null)} title="drag up or down"><span style={{ ...captionCss(t, k), fontSize: t.size * (frameH / 1920) }}>{SAMPLE.slice(0, Math.max(1, t.wordsPerLine)).map((wd, i) => <span key={i} style={i === 2 ? litCss(t) : undefined}>{wd} </span>)}</span></div>
            {t.watermark && s.settings.channelName && <div style={{ position: "absolute", left: 12, bottom: 12, fontSize: 11 * k, fontWeight: 700, color: "#fff", textShadow: "0 1px 3px #000" }}>{s.settings.channelName}</div>}
          </div>
        </div>
        <div className="transport"><span className="muted">Sample line with the third word lit. Drag it to place it. Long lines wrap.</span></div>
      </div>

      <div className="panel" style={{ width: 340, flexShrink: 0 }}>
        <div className="panel-head"><span className="grow">Template</span>{isDefault && <span className="muted">default</span>}</div>
        <div className="panel-body" style={{ gap: 10 }}>
          <Row label="Name"><Text value={t.name} onCommit={(v) => { const n = v.trim() || t.name; patch({ name: n }); setSelName(n); }} /></Row>
          <Row label="Font"><select className="input" value={t.font} onChange={(e) => patch({ font: e.target.value })} style={{ fontFamily: `"${t.font}"` }}>{(FONTS.includes(t.font) ? FONTS : [t.font, ...FONTS]).map((f) => <option key={f} value={f} style={{ fontFamily: `"${f}"` }}>{f}</option>)}</select></Row>
          <Row label="Size"><input type="range" className="slider" min="24" max="96" value={t.size} onChange={(e) => patch({ size: +e.target.value })} /><input className="input num" type="number" min="16" max="140" style={{ width: 64 }} value={t.size} onChange={(e) => patch({ size: +e.target.value || t.size })} /><span className="muted">px of 1920</span></Row>
          <Row label="From bottom"><input type="range" className="slider" min="4" max="70" value={t.positionPct} onChange={(e) => patch({ positionPct: +e.target.value })} /><input className="input num" type="number" min="4" max="70" style={{ width: 64 }} value={t.positionPct} onChange={(e) => patch({ positionPct: +e.target.value || t.positionPct })} /><span className="muted">%</span></Row>
          <Row label="Words per line"><input type="range" className="slider" min="1" max="8" value={t.wordsPerLine} onChange={(e) => patch({ wordsPerLine: +e.target.value })} /><span className="num" style={{ width: 34 }}>{t.wordsPerLine}</span></Row>
          <Row label="Text"><Swatches value={t.textColor} onPick={(c) => patch({ textColor: c })} /></Row>
          <Row label="Highlight"><Swatches value={t.highlight} onPick={(c) => patch({ highlight: c })} /></Row>
          <Row label="Outline"><input type="range" className="slider" min="0" max="8" value={t.outlinePx} onChange={(e) => patch({ outlinePx: +e.target.value })} disabled={t.boxed} /><span className="num" style={{ width: 34 }}>{t.boxed ? "—" : t.outlinePx}</span></Row>
          <Row label="Align"><Seg value={t.align || "center"} onChange={(v) => patch({ align: v })} options={[["center", "Centre"], ["left", "Left"]]} /></Row>
          <div style={{ display: "flex", flexDirection: "column", gap: 6, paddingLeft: 118 }}>
            <Check label="Box behind the text" checked={t.boxed} onChange={(v) => patch({ boxed: v })} />
            <Check label="Uppercase" checked={t.uppercase} onChange={(v) => patch({ uppercase: v })} />
            <Check label={FIXED_FONTS.includes(t.font) ? "Bold (this font has one weight)" : "Bold"} checked={t.bold && !FIXED_FONTS.includes(t.font)} disabled={FIXED_FONTS.includes(t.font)} onChange={(v) => patch({ bold: v })} />
            <Check label="Current word pops (115 %)" checked={!!t.wordPop} onChange={(v) => patch({ wordPop: v })} />
            <Check label="Hook at the top, first 2.5 s" checked={t.hook} onChange={(v) => patch({ hook: v })} />
            <Check label="Channel name watermark" checked={t.watermark} onChange={(v) => patch({ watermark: v })} />
          </div>
          <span className="hint">Same highlight and text colour means no lit word. Arabic and Urdu use Geeza Pro, in the preview too.</span>
          <div style={{ borderTop: "1px solid var(--rule)", paddingTop: 10, display: "flex", flexDirection: "column", gap: 10 }}>
            <b>Title at the top</b>
            <Row label="Font"><select className="input" value={t.hookFont || ""} onChange={(e) => patch({ hookFont: e.target.value })}><option value="">Same as captions</option>{(!t.hookFont || FONTS.includes(t.hookFont) ? FONTS : [t.hookFont, ...FONTS]).map((f) => <option key={f} value={f} style={{ fontFamily: `"${f}"` }}>{f}</option>)}</select></Row>
            <Row label="Size"><input type="range" className="slider" min="20" max="110" value={t.hookSize} onChange={(e) => patch({ hookSize: +e.target.value })} /><input className="input num" type="number" min="12" max="160" style={{ width: 64 }} value={t.hookSize} onChange={(e) => patch({ hookSize: +e.target.value || t.hookSize })} /></Row>
            <Row label="From top"><input type="range" className="slider" min="2" max="60" value={t.hookPct} onChange={(e) => patch({ hookPct: +e.target.value })} /><input className="input num" type="number" min="2" max="60" style={{ width: 64 }} value={t.hookPct} onChange={(e) => patch({ hookPct: +e.target.value || t.hookPct })} /><span className="muted">%</span></Row>
            <Row label="Shown for"><input type="range" className="slider" min="1" max="10" step="0.5" value={t.hookSeconds} onChange={(e) => patch({ hookSeconds: +e.target.value })} /><span className="num" style={{ width: 40 }}>{t.hookSeconds} s</span></Row>
            <Row label="Text"><Swatches value={t.hookColor || "#FFFFFF"} onPick={(c) => patch({ hookColor: c })} /></Row>
            <Row label="Box"><Swatches value={t.hookBoxColor || "#000000"} onPick={(c) => patch({ hookBoxColor: c, hookBoxed: true })} /></Row>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 12, paddingLeft: 118 }}>
              <Check label="Box" checked={t.hookBoxed} onChange={(v) => patch({ hookBoxed: v })} />
              <Check label="Uppercase" checked={t.hookUppercase} onChange={(v) => patch({ hookUppercase: v })} />
              <Check label="Bold" checked={t.hookBold && !FIXED_FONTS.includes(t.hookFont || t.font)} disabled={FIXED_FONTS.includes(t.hookFont || t.font)} onChange={(v) => patch({ hookBold: v })} />
              <Check label="Italic" checked={t.hookItalic} onChange={(v) => patch({ hookItalic: v })} />
            </div>
          </div>
        </div>
        <span className="grow" />
        <div className="panel-foot" style={{ flexWrap: "wrap" }}>
          <Btn primary onClick={() => tryAct("settings", { patch: { captionStyle: t } }, `${t.name} is the default`)} disabled={isDefault}>Set as default</Btn>
          {reel && <Btn onClick={() => tryAct("update_candidate", { id: reel.id, patch: { style: t.name } }, `${t.name} on "${reel.title}"`)}>Use on this reel</Btn>}
          <span className="grow" />
          <Btn danger icon disabled={templates.length <= 1} onClick={() => { save(templates.filter((_, i) => i !== idx), "Removed"); setSelName(templates[0]?.name); }} aria-label="Delete">{I.trash}</Btn>
        </div>
      </div>
    </div>
  );
}
