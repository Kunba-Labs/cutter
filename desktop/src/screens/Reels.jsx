import { useEffect, useMemo, useRef, useState } from "react";
import { useStore, act, tryAct, fileUrl, fmt, CATS, FORMATS } from "../store.js";
import { Panel, Btn, Seg, Field, Check, Text, I } from "../ui.jsx";

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
  const video = useRef(null);

  useEffect(() => { if (sourceId) act("source", { id: sourceId }).then(setDetail).catch(() => setDetail(null)); }, [sourceId, x?.updatedAt]);
  useEffect(() => { if (c && video.current) { video.current.currentTime = c.start; } }, [c?.id]);
  useEffect(() => {
    let raf;
    const tick = () => {
      const v = video.current;
      if (v && c) { setT(v.currentTime); if (!v.paused && v.currentTime >= c.end) { v.currentTime = c.start; } }
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
  const cx = drag ? drag.x : c.crop?.x ?? 0.5;
  const cy = drag ? drag.y : c.crop?.y ?? 0.5;
  const left = Math.min(0, Math.max(frameW - vidW, frameW / 2 - cx * vidW));
  // Drag the picture inside the frame to move the crop: dragging left shows more of the right side.
  const onDown = (e) => { if (e.button !== 0) return; e.preventDefault(); setDrag({ startX: e.clientX, startY: e.clientY, x: cx, y: cy, moved: false }); };
  const onMove = (e) => { if (!drag) return; const dx = (e.clientX - drag.startX) / vidW; setDrag({ ...drag, x: Math.min(1, Math.max(0, drag.x - dx)), moved: drag.moved || Math.abs(e.clientX - drag.startX) > 3 }); };
  const onUp = () => { if (!drag) return; if (drag.moved) patch({ crop: { ...(c.crop || {}), x: +drag.x.toFixed(3), y: +drag.y.toFixed(3) } }); else toggle(); setDrag(null); };
  // Caption preview: the words of the current segment, current word highlighted.
  const cur = segs.find((g) => t >= g.start && t < g.end);
  const words = !cur ? [] : cur.words?.length ? cur.words : cur.text.split(" ").map((w, i, a) => ({ w, s: cur.start + ((cur.end - cur.start) * i) / a.length, e: cur.start + ((cur.end - cur.start) * (i + 1)) / a.length }));
  const per = s.settings.captionStyle?.wordsPerLine || 4;
  const wi = words.findIndex((w) => t >= w.s && t < w.e);
  const line = wi >= 0 ? words.slice(Math.floor(wi / per) * per, Math.floor(wi / per) * per + per) : [];
  const trans = c.translation?.find((g) => t >= g.start && t < g.end);
  const inWords = segs.filter((g) => g.end > c.start - 5 && g.start < c.end + 5).flatMap((g) => g.words?.length ? g.words : [{ w: g.text, s: g.start, e: g.end }]);
  const snapEnd = () => { const g = segs.find((g) => g.start <= c.end && g.end >= c.end) || segs.filter((g) => g.end <= c.end).pop(); if (g) patch({ end: Math.min(g.end + 0.2, c.start + s.settings.maxReelS) }); };
  const renders = s.renders.filter((r) => r.candidateId === c.id && r.status === "done");
  const ticked = cands.filter((v) => v.approved).length;
  const dur = c.end - c.start;

  return (
    <div className="main">
      <div className="panel" style={{ width: 320, flexShrink: 0 }}>
        <div className="panel-head"><span>Candidates</span><span className="sub">{cands.length} · {ticked} ticked</span><span className="grow" /><Btn small onClick={() => tryAct("approve_above", { sourceId, score: 8 }, "Ticked everything scoring 8+")}>Tick ≥ 8</Btn></div>
        <div style={{ padding: "8px 10px", borderBottom: "1px solid var(--rule)", display: "flex", flexDirection: "column", gap: 6, fontSize: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>Source</span><select className="input" style={{ height: 22 }} value={sourceId} onChange={(e) => { go("reels", { sourceId: e.target.value }); setSelId(null); }}>{sources.map((v) => <option key={v.id} value={v.id}>{v.title}</option>)}</select></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>Min score</span><input type="range" className="slider" min="0" max="10" value={minScore} onChange={(e) => setMinScore(+e.target.value)} /><span style={{ width: 16 }}>{minScore}</span></div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 3 }}>{CATS.map((k) => { const n = cands.filter((v) => v.category === k).length; return n ? <span key={k} className={`chip ${k}`}>{k} {n}</span> : null; })}</div>
        </div>
        <div className="cands">
          {cands.filter((v) => v.score >= minScore).map((v) => (
            <div key={v.id} className={`cand ${v.id === c.id ? "on" : ""} ${v.discarded ? "off" : ""}`} onClick={() => setSelId(v.id)}>
              <input type="checkbox" checked={v.approved} onChange={(e) => tryAct("approve", { ids: [v.id], approved: e.target.checked })} onClick={(e) => e.stopPropagation()} style={{ accentColor: "var(--accent)", margin: 0 }} />
              <span className="score">{v.score}</span>
              <span style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}><span className="ell">{v.title || "(untitled)"}</span><span className={`${v.category}`} style={{ fontSize: 11.5, color: `var(--${{ fact: "cyan", statement: "amber", hook: "accent-text", story: "violet", dua: "mint", reminder: "sec", qa: "peach" }[v.category] || "sec"})` }}>{v.category}{s.renders.some((r) => r.candidateId === v.id && r.status === "done") ? " · rendered" : ""}</span></span>
              <span className="muted num" style={{ textAlign: "right" }}>{(v.end - v.start).toFixed(1)} s</span>
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
          <div className="panel-head"><span>Preview</span><Seg value={format} onChange={setFormat} options={FORMATS} /><Check label="safe zones" checked={safe} onChange={setSafe} /><span className="grow" /><span className="sub">crop</span><input type="range" className="slider" style={{ width: 120 }} min="0" max="1" step="0.01" value={cx} onChange={(e) => patch({ crop: { ...(c.crop || {}), x: +e.target.value } })} title="Horizontal crop position" /></div>
          <div className="stage" ref={stageRef}>
            <div className="frame" style={{ width: frameW, height: frameH, cursor: vidW > frameW ? (drag ? "grabbing" : "grab") : "default" }} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerLeave={onUp}>
              {x.videoPath && <video ref={video} src={fileUrl(x.videoPath)} style={{ left, width: vidW, pointerEvents: "none" }} muted={false} />}
              {s.settings.captionStyle?.hook !== false && c.hook && t - c.start < 2.5 && <div className="hook" style={{ top: frameH * 0.08 }}><span style={{ fontSize: 16 * k, padding: `${4 * k}px ${10 * k}px` }}>{c.hook}</span></div>}
              {line.length > 0 && <div className="cap" style={{ bottom: frameH * ((s.settings.captionStyle?.positionPct || 26) / 100 + (format === "tiktok" ? 0.08 : 0)) - 10 }}><span className="line" style={{ fontSize: (s.settings.captionStyle?.size || 42) * (frameH / 1920) }}>{line.map((w, i) => <span key={i}>{t >= w.s && t < w.e ? <b>{w.w}</b> : w.w} </span>)}</span>{s.settings.captionStyle?.translation !== false && trans && <span className="trans" style={{ fontSize: 12 * k }}>{trans.text}</span>}</div>}
              {safe && aw < ah && <><div className="safe" style={{ left: 0, right: 0, top: 0, height: frameH * 0.11, borderWidth: "0 0 1px 0" }} /><div className="safe" style={{ left: 0, right: 0, bottom: 0, height: frameH * (format === "tiktok" ? 0.2 : 0.14), borderWidth: "1px 0 0 0" }} /><div className="safe" style={{ right: 0, top: frameH * 0.45, width: 56 * k, height: frameH * 0.4, borderWidth: "0 0 0 1px" }} /></>}
              <span style={{ position: "absolute", right: 8, bottom: 8, fontSize: 11, background: "var(--bg)", padding: "1px 5px", borderRadius: 3 }} className="num">{fmt(Math.max(0, t - c.start))} / {fmt(dur)}</span>
            </div>
            <div style={{ position: "absolute", right: 16, top: 12, display: "flex", flexDirection: "column", gap: 6, fontSize: 12, color: "var(--muted)", alignItems: "flex-end" }}><span>source {fmt(c.start)} → {fmt(c.end)}</span><span className={dur > s.settings.maxReelS ? "coral" : "mint"}>{dur.toFixed(1)} s of {s.settings.maxReelS} max</span>{renders.map((r) => <a key={r.id} href="#" onClick={(e) => { e.preventDefault(); tryAct("open", { path: r.path }); }}>{r.format}.mp4 ▸</a>)}</div>
          </div>
          <div className="transport">
            <Btn icon onClick={() => { const i = cands.findIndex((v) => v.id === c.id); setSelId(cands[Math.max(0, i - 1)]?.id); }} aria-label="Previous">{I.prev}</Btn>
            <Btn icon primary onClick={toggle} aria-label="Play">{playing ? I.pause : I.play}</Btn>
            <Btn icon onClick={() => { const i = cands.findIndex((v) => v.id === c.id); setSelId(cands[Math.min(cands.length - 1, i + 1)]?.id); }} aria-label="Next">{I.next}</Btn>
            <span className="muted">loops the reel · drag the picture to move the crop</span><span className="grow" /><span className="muted">space play · ↑↓ candidates · [ ] set in/out at playhead · a tick</span>
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

      <div className="panel" style={{ width: 320, flexShrink: 0 }}>
        <div className="panel-head"><span className="grow">Reel</span><span className="score">score {c.score}</span><select className="input" style={{ width: 100, height: 20, fontSize: 11.5 }} value={c.category} onChange={(e) => patch({ category: e.target.value })}>{CATS.map((k) => <option key={k} value={k}>{k}</option>)}</select></div>
        <div className="panel-body">
          <Field label="Title"><Text value={c.title} onCommit={(v) => patch({ title: v })} /></Field>
          <Field label="On-screen hook · first 2.5 s"><Text value={c.hook} onCommit={(v) => patch({ hook: v })} /></Field>
          {c.why && <Field label="Why Claude picked it"><span style={{ color: "var(--text-2)", lineHeight: 1.45, background: "var(--bg)", border: "1px solid var(--rule)", borderRadius: 4, padding: "6px 8px", fontSize: 12.5 }}>{c.why}</span></Field>}
          <Field label="Caption"><Text area rows={3} value={c.caption} onCommit={(v) => patch({ caption: v })} /></Field>
          <Field label="Hashtags"><Text value={(c.hashtags || []).join(" ")} onCommit={(v) => patch({ hashtags: v.split(/[\s,#]+/).filter(Boolean) })} /></Field>
          <div className="grid2">
            <Field label="Caption style"><select className="input" value={c.style || ""} onChange={(e) => patch({ style: e.target.value || null })}><option value="">Default ({s.settings.captionStyle?.preset})</option>{["karaoke", "clean", "boxed", "outline", "lower"].map((p) => <option key={p} value={p}>{p}</option>)}</select></Field>
            <Field label="Score"><input className="input" type="number" min="1" max="10" value={c.score} onChange={(e) => patch({ score: +e.target.value })} /></Field>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4, borderTop: "1px solid var(--rule)", paddingTop: 8 }}>
            <span className="label">Render as {c.formats?.length ? "" : "(default from settings)"}</span>
            {FORMATS.map(([v, l]) => <Check key={v} label={l} checked={(c.formats?.length ? c.formats : s.settings.formats || []).includes(v)} onChange={(on) => { const cur = c.formats?.length ? c.formats : s.settings.formats || []; patch({ formats: on ? [...new Set([...cur, v])] : cur.filter((f) => f !== v) }); }} />)}
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
