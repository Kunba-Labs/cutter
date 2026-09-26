import { useEffect, useMemo, useRef, useState } from "react";
import { useStore, act, tryAct, fileUrl, fmt, CATS, FORMATS } from "../store.js";
import { Panel, Btn, Seg, Field, Check, Text, I, Cat, CAT_NAMES } from "../ui.jsx";
import { captionCss, hookCss } from "./Style.jsx";

const SPECS = { shorts: [9, 16], reels: [9, 16], tiktok: [9, 16], feed: [4, 5], landscape: [16, 9] };

export default function Reels({ nav, go }) {
  const s = useStore();
  const sources = s.sources.filter((x) => s.candidates.some((c) => c.sourceId === x.id));
  const sourceId = nav.sourceId && s.candidates.some((c) => c.sourceId === nav.sourceId) ? nav.sourceId : sources[0]?.id;
  const x = s.sources.find((v) => v.id === sourceId);
  const cands = useMemo(() => s.candidates.filter((c) => c.sourceId === sourceId).sort((a, b) => a.start - b.start), [s.candidates, sourceId]);
  const [selId, setSelId] = useState(null);
  const c = cands.find((v) => v.id === selId) || cands[0];
  const [format, setFormat] = useState("shorts");
  const [safe, setSafe] = useState(true);
  const [minScore, setMinScore] = useState(0);
  const [detail, setDetail] = useState(null);
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [hold, setHold] = useState(false);
  const [applyScope, setApplyScope] = useState("source");
  const [applyKeys, setApplyKeys] = useState(["style", "hookStyle", "captionPct"]);
  const aspectKey = format === "landscape" ? "16x9" : format === "feed" ? "4x5" : "9x16";
  const video = useRef(null);

  useEffect(() => { if (sourceId) act("source", { id: sourceId }).then(setDetail).catch(() => setDetail(null)); }, [sourceId, x?.updatedAt]);
  useEffect(() => { if (c && video.current) { video.current.currentTime = c.start; } }, [c?.id]);
  useEffect(() => {
    let raf;
    const tick = () => {
      const v = video.current;
      if (v && c) {
        setT(v.currentTime);
        if (!v.paused && v.currentTime >= c.end) {
          const ec = s.settings.endCard;
          if (ec?.enabled && ec.paths?.[aspectKey]) {
            // Hold the closing card for its duration, then loop.
            v.pause(); setHold(true);
            setTimeout(() => { setHold(false); v.currentTime = c.start; v.play(); }, (ec.seconds || 2.5) * 1000);
          } else {
            v.currentTime = c.start;
          }
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [c?.start, c?.end]);
  useEffect(() => {
    const onKey = (e) => {
      if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
      if (e.key === " ") { e.preventDefault(); toggle(); }
      if (e.key === "ArrowDown" || e.key === "ArrowRight") { e.preventDefault(); const i = cands.findIndex((v) => v.id === c?.id); setSelId(cands[Math.min(cands.length - 1, i + 1)]?.id); }
      if (e.key === "ArrowUp" || e.key === "ArrowLeft") { e.preventDefault(); const i = cands.findIndex((v) => v.id === c?.id); setSelId(cands[Math.max(0, i - 1)]?.id); }
      if (e.key === "a" && c) tryAct("approve", { ids: [c.id], approved: !c.approved });
      if (e.key === "[" && c) patch({ start: Math.max(0, t) });
      if (e.key === "]" && c) patch({ end: t });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!x || !c) return <div className="empty">No reel candidates yet.<br />Add a source and let Claude read the transcript, or mark a reel from the Transcript screen.</div>;

  const segs = detail?.transcript?.segments || [];
  const patch = (p) => tryAct("update_candidate", { id: c.id, patch: p });
  const tpl = (s.settings.captionTemplates || []).find((x) => x.name === c.style) || s.settings.captionStyle || {};
  const hookTpl = (s.settings.captionTemplates || []).find((x) => x.name === c.hookStyle) || tpl;
  const toggle = () => { const v = video.current; if (!v) return; if (v.paused) { if (v.currentTime < c.start || v.currentTime > c.end) v.currentTime = c.start; v.play(); setPlaying(true); } else { v.pause(); setPlaying(false); } };
  const seek = (time) => { if (video.current) video.current.currentTime = Math.min(c.end, Math.max(c.start, time)); };
  const [aw, ah] = SPECS[format] || [9, 16];
  const stageRef = useRef(null);
  const [stage, setStage] = useState({ w: 600, h: 560 });
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setStage({ w: e.contentRect.width, h: e.contentRect.height }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  // The frame fills the stage: as tall as it can be, or as wide for landscape.
  const frameH = Math.max(240, Math.floor(Math.min(stage.h - 24, ((stage.w - 200) * ah) / aw)));
  const frameW = Math.round((frameH * aw) / ah);
  const k = frameH / 533; // type and margins scale with the frame
  const srcAspect = (x.meta?.width || 1920) / (x.meta?.height || 1080);
  const vidW = frameH * srcAspect;
  const [drag, setDrag] = useState(null); // { startX, startY, x, y } while the crop window is being dragged
  // What was just dragged stays put until the core has saved it (the snapshot lags a beat).
  const [pending, setPending] = useState(null);
  useEffect(() => { setPending(null); }, [c.crop?.x, c.crop?.y, c.crop?.z, c.captionPct, c.id]);
  const cx = drag ? drag.x : pending?.x ?? c.crop?.x ?? 0.5;
  const cy = drag ? drag.y : pending?.y ?? c.crop?.y ?? 0.5;
  const cz = pending?.z ?? c.crop?.z ?? 1;
  // The picture is scaled so the crop window (frame) shows 1/cz of the largest fitting window.
  const zoomW = vidW * cz, zoomH = frameH * cz;
  const left = Math.min(0, Math.max(frameW - zoomW, frameW / 2 - cx * zoomW));
  const top = Math.min(0, Math.max(frameH - zoomH, frameH / 2 - cy * zoomH));
  // Drag the picture inside the frame to move the crop, one to one from the grab point, both axes.
  const onDown = (e) => { if (e.button !== 0) return; e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setDrag({ startX: e.clientX, startY: e.clientY, x0: cx, y0: cy, x: cx, y: cy, moved: false }); };
  const onMove = (e) => { if (!drag) return; e.preventDefault(); const dx = (e.clientX - drag.startX) / zoomW; const dy = (e.clientY - drag.startY) / zoomH; setDrag({ ...drag, x: Math.min(1, Math.max(0, drag.x0 - dx)), y: Math.min(1, Math.max(0, drag.y0 - dy)), moved: drag.moved || Math.abs(e.clientX - drag.startX) > 3 || Math.abs(e.clientY - drag.startY) > 3 }); };
  const onUp = () => { if (!drag) return; if (drag.moved) { setPending((p) => ({ ...(p || {}), x: drag.x, y: drag.y })); patch({ crop: { ...(c.crop || {}), x: +drag.x.toFixed(3), y: +drag.y.toFixed(3) } }); } else toggle(); setDrag(null); };
  const setZoom = (z) => { setPending((p) => ({ ...(p || {}), z })); patch({ crop: { ...(c.crop || {}), x: cx, y: cy, z: +z.toFixed(2) } }); };
  // Drag the caption block up or down: a per-reel position, the template's otherwise.
  const [capDrag, setCapDrag] = useState(null);
  const capPct = capDrag?.pct ?? pending?.cap ?? c.captionPct ?? tpl.positionPct ?? 26;
  const onCapDown = (e) => { e.stopPropagation(); e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setCapDrag({ y0: e.clientY, h: frameH, pct0: capPct, pct: capPct, moved: false }); };
  const onCapMove = (e) => { if (!capDrag) return; e.stopPropagation(); const pct = Math.round(Math.min(70, Math.max(4, capDrag.pct0 + ((capDrag.y0 - e.clientY) / capDrag.h) * 100))); setCapDrag({ ...capDrag, pct, moved: true }); };
  const onCapUp = (e) => { if (!capDrag) return; e.stopPropagation(); if (capDrag.moved) { setPending((p) => ({ ...(p || {}), cap: capDrag.pct })); patch({ captionPct: capDrag.pct }); } setCapDrag(null); };
  // Caption preview: the words of the current segment, current word highlighted.
  const cur = segs.find((g) => t >= g.start && t < g.end);
  const words = !cur ? [] : cur.words?.length ? cur.words : cur.text.split(" ").map((w, i, a) => ({ w, s: cur.start + ((cur.end - cur.start) * i) / a.length, e: cur.start + ((cur.end - cur.start) * (i + 1)) / a.length }));
  const per = tpl.wordsPerLine || 4;
  const wi = words.findIndex((w) => t >= w.s && t < w.e);
  const line = wi >= 0 ? words.slice(Math.floor(wi / per) * per, Math.floor(wi / per) * per + per) : [];
  const trans = c.translation?.find((g) => t >= g.start && t < g.end);
  // One caption language: the translation replaces the spoken words when a language is set and the reel has one.
  const translated = !!s.settings.translateTo && s.settings.translateTo !== (detail?.transcript?.language || "") && (c.translation?.length || 0) > 0;
  const inWords = segs.filter((g) => g.end > c.start - 5 && g.start < c.end + 5).flatMap((g) => g.words?.length ? g.words : [{ w: g.text, s: g.start, e: g.end }]);
  const snapEnd = () => { const g = segs.find((g) => g.start <= c.end && g.end >= c.end) || segs.filter((g) => g.end <= c.end).pop(); if (g) patch({ end: Math.min(g.end + 0.2, c.start + s.settings.maxReelS) }); };
  const renders = s.renders.filter((r) => r.candidateId === c.id && r.status === "done");
  const ticked = cands.filter((v) => v.approved).length;
  const dur = c.end - c.start;

  return (
    <div className="main">
      <div className="panel" style={{ width: 320, flexShrink: 0 }}>
        <div className="panel-head"><span>Candidates</span><span className="sub">{cands.length} · {ticked} ticked</span><span className="grow" /><Btn small onClick={() => tryAct("approve_above", { sourceId, score: 8 }, "Ticked everything scoring 8+")}>Tick ≥ 8</Btn></div>
        <div style={{ padding: "8px 10px", borderBottom: "1px solid var(--rule)", display: "flex", flexDirection: "column", gap: 6, fontSize: 13 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>Source</span><select className="input" style={{ height: 22 }} value={sourceId} onChange={(e) => { go("reels", { sourceId: e.target.value }); setSelId(null); }}>{sources.map((v) => <option key={v.id} value={v.id}>{v.title}</option>)}</select></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>Min score</span><input type="range" className="slider" min="0" max="10" value={minScore} onChange={(e) => setMinScore(+e.target.value)} /><span style={{ width: 16 }}>{minScore}</span></div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 10, color: "var(--sec)" }}>{CATS.map((k) => { const n = cands.filter((v) => v.category === k).length; return n ? <span key={k} style={{ display: "flex", alignItems: "center", gap: 4 }} title={CAT_NAMES[k]}><Cat id={k} /><span className="num">{n}</span></span> : null; })}</div>
        </div>
        <div className="cands">
          {cands.filter((v) => v.score >= minScore).map((v) => (
            <div key={v.id} className={`cand ${v.id === c.id ? "on" : ""} ${v.discarded ? "off" : ""}`} onClick={() => setSelId(v.id)} title={`${CAT_NAMES[v.category] || v.category} · ${v.why || ""}`}>
              <input type="checkbox" checked={v.approved} onChange={(e) => tryAct("approve", { ids: [v.id], approved: e.target.checked })} onClick={(e) => e.stopPropagation()} style={{ accentColor: "var(--accent)", margin: 0 }} />
              <span className="score">{v.score}</span>
              <span className="ell">{v.title || "(untitled)"}</span>
              <span className="muted" style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "flex-end" }}><Cat id={v.category} />{s.renders.some((r) => r.candidateId === v.id && r.status === "done") && <span className="dot mint" title="rendered" />}<span className="num" style={{ width: 44, textAlign: "right" }}>{(v.end - v.start).toFixed(1)} s</span></span>
            </div>
          ))}
        </div>
        <span className="grow" />
        <div className="panel-foot">
          <Btn primary className="grow" disabled={!ticked} onClick={async () => { const r = await tryAct("render", { sourceId }, `Rendering ${ticked} reels`); if (r) go("queue"); }}>Render {ticked} ticked · {ticked * (s.settings.formats?.length || 3)} files</Btn>
          <Btn onClick={() => tryAct("run", { id: sourceId, stage: "detect" }, "Claude is reading again")}>Find again</Btn>
        </div>
      </div>

      <div className="col grow">
        <div className="panel grow">
          <div className="panel-head"><span>Preview</span><Seg value={format} onChange={setFormat} options={FORMATS} /><Check label="safe zones" checked={safe} onChange={setSafe} /><span className="grow" /><span className="sub">zoom</span><input type="range" className="slider" style={{ width: 120 }} min="1" max="2.5" step="0.05" value={cz} onChange={(e) => setZoom(+e.target.value)} title="Zoom the crop window" /><span className="num sub" style={{ width: 36 }}>{cz.toFixed(2)}×</span></div>
          <div className="stage" ref={stageRef}>
            <div className="frame" style={{ width: frameW, height: frameH, cursor: zoomW > frameW + 1 || zoomH > frameH + 1 ? (drag ? "grabbing" : "grab") : "default" }} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
              {x.videoPath && <video ref={video} src={fileUrl(x.videoPath)} style={{ left, top, width: zoomW, height: zoomH, pointerEvents: "none" }} muted={false} />}
              {hold && s.settings.endCard?.paths?.[aspectKey] && <img src={fileUrl(s.settings.endCard.paths[aspectKey])} alt="" style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: "cover" }} />}
              {hookTpl.hook !== false && c.hook && t - c.start < (hookTpl.hookSeconds || 2.5) && <div className="hook" style={{ top: frameH * ((hookTpl.hookPct ?? 8) / 100) }}><span style={hookCss(hookTpl, k)}>{c.hook}</span></div>}
              {(translated ? !!trans : line.length > 0) && <div className="cap" style={{ bottom: frameH * (capPct / 100 + (format === "tiktok" ? 0.08 : 0)) - 10, alignItems: tpl.align === "left" ? "flex-start" : "center", pointerEvents: "auto", cursor: capDrag ? "grabbing" : "ns-resize" }} onPointerDown={onCapDown} onPointerMove={onCapMove} onPointerUp={onCapUp} onPointerCancel={onCapUp} title="drag up or down to move the captions"><span style={{ ...captionCss(tpl, k), fontSize: (tpl.size || 42) * (frameH / 1920) }}>{translated ? trans.text : line.map((w, i) => <span key={i} style={t >= w.s && t < w.e && (tpl.highlight || "").toLowerCase() !== (tpl.textColor || "#ffffff").toLowerCase() ? { color: tpl.highlight } : undefined}>{w.w} </span>)}</span></div>}
              {safe && aw < ah && <><div className="safe" style={{ left: 0, right: 0, top: 0, height: frameH * 0.11, borderWidth: "0 0 1px 0" }} /><div className="safe" style={{ left: 0, right: 0, bottom: 0, height: frameH * (format === "tiktok" ? 0.2 : 0.14), borderWidth: "1px 0 0 0" }} /><div className="safe" style={{ right: 0, top: frameH * 0.45, width: 56 * k, height: frameH * 0.4, borderWidth: "0 0 0 1px" }} /></>}
              <span style={{ position: "absolute", right: 8, bottom: 8, fontSize: 12, background: "var(--bg)", padding: "1px 5px", borderRadius: 3 }} className="num">{fmt(Math.max(0, t - c.start))} / {fmt(dur)}</span>
            </div>
            <div style={{ position: "absolute", right: 16, top: 12, display: "flex", flexDirection: "column", gap: 6, fontSize: 13, color: "var(--muted)", alignItems: "flex-end" }}><span>source {fmt(c.start)} → {fmt(c.end)}</span><span className={dur > s.settings.maxReelS ? "coral" : "mint"}>{dur.toFixed(1)} s of {s.settings.maxReelS} max</span>{renders.map((r) => <a key={r.id} href="#" onClick={(e) => { e.preventDefault(); tryAct("open", { path: r.path }); }}>{r.format}.mp4 ▸</a>)}</div>
          </div>
          <div className="transport">
            <Btn icon onClick={() => { const i = cands.findIndex((v) => v.id === c.id); setSelId(cands[Math.max(0, i - 1)]?.id); }} aria-label="Previous">{I.prev}</Btn>
            <Btn icon primary onClick={toggle} aria-label="Play">{playing ? I.pause : I.play}</Btn>
            <Btn icon onClick={() => { const i = cands.findIndex((v) => v.id === c.id); setSelId(cands[Math.min(cands.length - 1, i + 1)]?.id); }} aria-label="Next">{I.next}</Btn>
            <span className="muted">loops the reel · drag the picture to move the crop · drag the captions to move them{c.captionPct != null && <> · <a href="#" onClick={(e) => { e.preventDefault(); patch({ captionPct: null }); }}>reset caption position</a></>}</span><span className="grow" /><span className="muted">space play · ↑↓ candidates · [ ] set in/out at playhead · a tick</span>
          </div>
        </div>
        <Panel title="Trim" sub={`in ${fmt(c.start)} · out ${fmt(c.end)} · ${dur.toFixed(1)} s`} right={<><Btn small onClick={snapEnd}>Snap out to sentence</Btn><Btn small onClick={() => patch({ start: Math.max(0, t) })}>In = playhead</Btn><Btn small onClick={() => patch({ end: Math.max(c.start + 3, t) })}>Out = playhead</Btn></>} style={{ height: 150, flexShrink: 0 }}>
          <div className="range" onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); const a = c.start - 5, b = c.end + 5; seek(a + ((e.clientX - r.left) / r.width) * (b - a)); }}>
            {segs.filter((g) => g.end > c.start - 5 && g.start < c.end + 5).map((g, i) => { const a = c.start - 5, b = c.end + 5; return <span key={i} className="seg" style={{ left: `${((Math.max(g.start, a) - a) / (b - a)) * 100}%`, width: `${((Math.min(g.end, b) - Math.max(g.start, a)) / (b - a)) * 100}%`, position: "absolute" }} />; })}
            <span className="sel" style={{ left: `${(5 / (dur + 10)) * 100}%`, width: `${(dur / (dur + 10)) * 100}%` }} />
            <span className="cur" style={{ left: `${((t - c.start + 5) / (dur + 10)) * 100}%` }} />
          </div>
          <div className="words">{inWords.map((w, i) => <span key={i} className={t >= w.s && t < w.e ? "cur" : w.s >= c.start - 0.05 && w.e <= c.end + 0.05 ? "in" : ""} onClick={(e) => { if (e.altKey) patch({ start: w.s }); else if (e.shiftKey) patch({ end: w.e }); else seek(w.s); }} title="click: seek · ⌥click: set in · ⇧click: set out">{w.w}</span>)}</div>
        </Panel>
      </div>

      <div className="panel" style={{ width: 340, flexShrink: 0 }}>
        <div className="panel-head"><span>Reel</span><span className="grow" /><Cat id={c.category} style={{ color: "var(--sec)" }} /><select className="input" style={{ width: 110, height: 22, fontSize: 12, padding: "0 6px" }} value={c.category} onChange={(e) => patch({ category: e.target.value })}>{CATS.map((k) => <option key={k} value={k}>{CAT_NAMES[k]}</option>)}</select><span className="score" style={{ height: 22 }}>score <input type="number" min="1" max="10" value={c.score} onChange={(e) => patch({ score: +e.target.value })} style={{ width: 28, background: "none", border: 0, color: "inherit", font: "inherit", textAlign: "center", padding: 0, marginLeft: 4 }} /></span></div>
        <div className="panel-body" style={{ gap: 12 }}>
          {c.why && <span className="muted" style={{ lineHeight: 1.45, fontStyle: "italic" }}>{c.why}</span>}
          <div className="insp-group">
            <div className="insp-head">Text</div>
            <Field label="Title"><Text value={c.title} onCommit={(v) => patch({ title: v })} /></Field>
            <Field label={`Title on the video · first ${(hookTpl.hookSeconds || 2.5)} s`}><Text value={c.hook} onCommit={(v) => patch({ hook: v })} /></Field>
          </div>
          <div className="insp-group">
            <div className="insp-head">Look<span className="grow" /><a href="#" onClick={(e) => { e.preventDefault(); go("style", { sourceId, candidateId: c.id }); }}>edit templates</a></div>
            <div className="grid2">
              <Field label="Captions"><select className="input" value={c.style || ""} onChange={(e) => patch({ style: e.target.value || null })}><option value="">Default · {s.settings.captionStyle?.name}</option>{(s.settings.captionTemplates || []).map((x) => <option key={x.name} value={x.name}>{x.name}</option>)}</select></Field>
              <Field label="Title"><select className="input" value={c.hookStyle || ""} onChange={(e) => patch({ hookStyle: e.target.value || null })}><option value="">Same as captions</option>{(s.settings.captionTemplates || []).map((x) => <option key={x.name} value={x.name}>{x.name}</option>)}</select></Field>
            </div>
            <span className="hint">Drag the captions or the picture in the preview to place them.{c.captionPct != null && <> Captions at {c.captionPct}% · <a href="#" onClick={(e) => { e.preventDefault(); patch({ captionPct: null }); }}>reset</a>.</>}{(Math.abs((c.crop?.x ?? 0.5) - 0.5) > 0.005 || Math.abs((c.crop?.y ?? 0.5) - 0.5) > 0.005 || (c.crop?.z ?? 1) !== 1) && <> Crop {Math.round((c.crop?.x ?? 0.5) * 100)}% / {Math.round((c.crop?.y ?? 0.5) * 100)}% at {(c.crop?.z ?? 1).toFixed(2)}× · <a href="#" onClick={(e) => { e.preventDefault(); setPending((p) => ({ ...(p || {}), x: 0.5, y: 0.5, z: 1 })); patch({ crop: { x: 0.5, y: 0.5, z: 1 } }); }}>reset</a>.</>}</span>
            <div style={{ display: "flex", flexDirection: "column", gap: 6, background: "var(--bg)", border: "1px solid var(--rule)", borderRadius: 4, padding: "8px 10px" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}><b style={{ fontSize: 13 }}>Apply to the queue</b><span className="grow" /><Seg value={applyScope} onChange={setApplyScope} options={[["source", "This lecture"], ["approved", "Approved"], ["library", "Everything"]]} /></div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "4px 14px" }}>
                {[["style", "Captions template"], ["hookStyle", "Title template"], ["captionPct", "Caption position"], ["crop", "Crop"], ["formats", "Render formats"]].map(([k, l]) => <Check key={k} label={l} checked={applyKeys.includes(k)} onChange={(on) => setApplyKeys(on ? [...applyKeys, k] : applyKeys.filter((x) => x !== k))} />)}
              </div>
              <Btn small primary disabled={!applyKeys.length} onClick={async () => { const n = await tryAct("apply_look", { id: c.id, keys: applyKeys, scope: applyScope }); if (n != null) tryAct("snapshot", {}, `Applied to ${n} reels`); }}>Apply to {applyScope === "source" ? "this lecture's reels" : applyScope === "approved" ? "all approved reels" : "every reel"}</Btn>
            </div>
          </div>
          <div className="insp-group">
            <div className="insp-head">Post</div>
            <Field label="Caption"><Text area rows={4} value={c.caption} onCommit={(v) => patch({ caption: v })} /></Field>
            <Field label={`Hashtags · ${(c.hashtags || []).length}`}><Text value={(c.hashtags || []).join(" ")} onCommit={(v) => patch({ hashtags: v.split(/[\s,#]+/).filter(Boolean) })} /></Field>
          </div>
          <div className="insp-group">
            <div className="insp-head">Render as<span className="grow" /><span className="muted" style={{ fontWeight: 400 }}>{c.formats?.length ? "this reel" : "library default"}</span></div>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "4px 12px" }}>
              {FORMATS.map(([v, l]) => <Check key={v} label={l} checked={(c.formats?.length ? c.formats : s.settings.formats || []).includes(v)} onChange={(on) => { const cur = c.formats?.length ? c.formats : s.settings.formats || []; patch({ formats: on ? [...new Set([...cur, v])] : cur.filter((f) => f !== v) }); }} />)}
            </div>
          </div>
        </div>
        <span className="grow" />
        <div className="panel-foot">
          <Btn primary className="grow" onClick={() => tryAct("approve", { ids: [c.id], approved: !c.approved })}>{c.approved ? "Approved ✓" : "Approve"}</Btn>
          <Btn onClick={async () => { const r = await tryAct("render", { candidateIds: [c.id] }, "Rendering"); if (r) go("queue"); }}>Render now</Btn>
          <Btn danger icon onClick={() => tryAct("discard", { id: c.id })} aria-label="Discard" title={c.discarded ? "Restore" : "Discard"}>{I.trash}</Btn>
        </div>
      </div>
    </div>
  );
}
