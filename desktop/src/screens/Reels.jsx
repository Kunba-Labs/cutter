import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useStore, act, tryAct, fileUrl, fmt, fmtLong, CATS, FORMATS, TRANSITIONS, LOUDNESS, FORMAT_BRAND, reelTrack, useSpeed, setSpeed, toast, audioGain, resumeAudio, dbGain, levelDb, PREVIEW_LUFS } from "../store.js";
import { Speed, Panel, Btn, Seg, Field, Check, Text, I, Cat, CAT_NAMES, Dot, Confirm, useSize, Grip, Menu, Brand, FileMark } from "../ui.jsx";
import { captionCss, hookCss, litCss } from "./Style.jsx";

const SPECS = { shorts: [9, 16], reels: [9, 16], tiktok: [9, 16], feed: [4, 5], landscape: [16, 9] };
// Seconds of lecture shown around the reel in the trim strip, so an edge can be pushed outwards.
const CTX = 15;
const RTL = ["ar", "ur", "fa", "he", "ps", "sd"];
// A word or a line corrected in place in the words strip: Enter keeps it, Escape leaves it, empty drops a word.
const Inline = ({ value, onDone, area }) => {
  const Tag = area ? "textarea" : "input";
  return <Tag autoFocus className={`input inline-edit ${area ? "line" : ""}`} defaultValue={value} size={area ? undefined : Math.max(3, value.length + 1)} rows={area ? 2 : undefined}
    onFocus={(e) => e.currentTarget.select()} onBlur={(e) => onDone(e.currentTarget.value.trim())}
    onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Escape") { e.currentTarget.value = value; e.currentTarget.blur(); } else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); e.currentTarget.blur(); } }} />;
};
const formatName = (f) => (f === "clean" ? "Clean" : FORMATS.find(([v]) => v === f)?.[1] || f);

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
  const [edit, setEdit] = useState(null); // { gi, wi } a word, { gi } a whole line, edited in the words strip
  const [wordMenu, setWordMenu] = useState(null);
  useEffect(() => setEdit(null), [c?.id]);
  useEffect(() => { if (edit) { video.current?.pause(); setPlaying(false); } }, [edit]);
  const refreshSeg = (r, index) => setDetail((d) => d && { ...d, transcript: { ...d.transcript, segments: d.transcript.segments.map((g, i) => (i === index ? r : g)) } });
  const [tab, setTab] = useState("text");
  const [teaserOpen, setTeaserOpen] = useState(false);
  const [applyScope, setApplyScope] = useState("source");
  const [applyKeys, setApplyKeys] = useState(["style", "hookStyle", "captionPct"]);
  const aspectKey = format === "landscape" ? "16x9" : format === "feed" ? "4x5" : "9x16";
  const video = useRef(null);
  // The teaser preview: the punchline, then the reel's transition (`flash` holds its name), then the reel.
  const teaser = useRef(false);
  const [teasing, setTeasing] = useState(false);
  const [flash, setFlash] = useState(false);
  const [logoDrag, setLogoDrag] = useState(null);
  const [logoAspect, setLogoAspect] = useState(1);
  const [askOver, setAskOver] = useState(false);
  // The player reads media CORS-clean so the preview can normalise its sound; false after a refusal.
  const [cors, setCors] = useState(true);
  // Where the player was when a CORS refusal made it remount, so the new element picks up there.
  const resume = useRef(null);
  const noCors = () => { if (!cors) return; resume.current = video.current?.currentTime ?? null; setCors(false); };
  const [leftW, setLeftW] = useSize("reels.left", 320);
  const [rightW, setRightW] = useSize("reels.right", 340, 260, 700);
  const [trimH, setTrimH] = useSize("reels.trim", 178, 120, 480);
  // While the end card (or the music's tail) holds after the reel, the music keeps playing.
  const holding = useRef(false);
  // The reel's background track plays under the preview at its level (no ducking here).
  const music = useRef(null);
  const speed = useSpeed();
  useEffect(() => { if (video.current) video.current.playbackRate = speed; }, [speed]);

  useEffect(() => { if (sourceId) act("source", { id: sourceId }).then(setDetail).catch(() => setDetail(null)); }, [sourceId, x?.updatedAt, s.undoneAt]);
  // Another reel: start at its in point, out of any teaser the last one was paused in.
  useEffect(() => { teaser.current = false; setTeasing(false); if (c && video.current) { video.current.currentTime = c.start; } }, [c?.id]);
  // The start of the whole thing: the teaser when this reel has one switched on, else the reel.
  const fromTop = (v) => {
    const withTeaser = c && c.punchStart != null && c.punchEnd != null && (c.introOn ?? s.settings.intro ?? true);
    teaser.current = !!withTeaser; setTeasing(!!withTeaser);
    v.currentTime = withTeaser ? c.punchStart : c.start;
  };
  useEffect(() => {
    let raf;
    const tick = () => {
      const v = video.current;
      if (v && c) {
        setT(v.currentTime);
        const m = music.current;
        if (m) {
          if (v.paused) { if (!m.paused && !holding.current) m.pause(); }
          else {
            // Follow the video loosely: same speed, and a correction only past a second of drift and
            // never while a seek is still landing. Re-seeking every frame (a seek here takes longer
            // than the old 0.4 s tolerance) kept the track seeking forever: one second of sound, then none.
            if (m.playbackRate !== v.playbackRate) m.playbackRate = v.playbackRate;
            const len = m.duration || 1e9;
            // The music runs on the file's clock: from the teaser's first second, on through the reel.
            const lead = c.punchStart != null && c.punchEnd != null && (c.introOn ?? s.settings.intro ?? true) ? c.punchEnd - c.punchStart : 0;
            const clock = teaser.current ? v.currentTime - c.punchStart : lead + v.currentTime - c.start;
            const want = Math.max(0, clock) % len;
            const off = Math.abs(m.currentTime - want);
            if (!m.seeking && Math.min(off, len - off) > 1) m.currentTime = want;
            if (m.paused) m.play().catch(() => {});
          }
        }
        if (teaser.current && !v.paused && c.punchEnd != null && v.currentTime >= c.punchEnd) {
          teaser.current = false; setTeasing(false);
          const tx = c.transition || s.settings.transition || "swoosh";
          if (tx !== "cut") { setFlash(tx); setTimeout(() => setFlash(false), 450); }
          v.currentTime = c.start;
        } else if (!teaser.current && !v.paused && v.currentTime >= c.end && v.currentTime < c.end + 0.5) {
          // Crossing the out point loops; playing on from beyond it (to hear what follows) does not.
          const ec = s.settings.endCard;
          const card = (c.endCardOn ?? ec?.enabled) && ec?.paths?.[aspectKey];
          // No card but music: the picture fades out while the music plays on, as in the render.
          const tail = !card && music.current ? s.settings.musicTail ?? 3 : 0;
          if (card || tail > 0) {
            // Hold the closing card (or the fading frame) for its duration, then loop.
            // Music on over the card only when the library says so; the tail is there for the music.
            v.pause(); holding.current = !card || ec.music !== false; setHold(card ? "card" : "tail");
            setTimeout(() => { holding.current = false; setHold(false); fromTop(v); v.play(); }, (card ? ec.seconds || 2.5 : tail) * 1000);
          } else {
            fromTop(v);
          }
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [c?.start, c?.end, c?.punchStart, c?.punchEnd, c?.introOn, s.settings.intro, c?.transition, s.settings.transition, c?.endCardOn, s.settings.endCard?.enabled, s.settings.endCard?.music, s.settings.endCard?.seconds, s.settings.endCard?.paths, s.settings.musicTail, aspectKey]);
  useEffect(() => {
    const onKey = (e) => {
      if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return;
      if (e.key === " ") { e.preventDefault(); toggle(); }
      if (e.key === "ArrowDown" || e.key === "ArrowRight") { e.preventDefault(); const i = cands.findIndex((v) => v.id === c?.id); setSelId(cands[Math.min(cands.length - 1, i + 1)]?.id); }
      if (e.key === "ArrowUp" || e.key === "ArrowLeft") { e.preventDefault(); const i = cands.findIndex((v) => v.id === c?.id); setSelId(cands[Math.max(0, i - 1)]?.id); }
      if (e.key === "a" && c) { tryAct("approve", { ids: [c.id], approved: !c.approved }); if (!c.approved) { const i = cands.findIndex((v) => v.id === c.id); const next = cands.slice(i + 1).find((v) => !v.approved && !v.discarded); if (next) setSelId(next.id); } }
      if (e.key === "[" && c) patch({ start: Math.max(0, t) });
      if (e.key === "]" && c) patch({ end: t });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!x || !c) return <div className="empty">No reels yet. Add a lecture, or mark one on the Transcript screen.</div>;

  const segs = detail?.transcript?.segments || [];
  const patch = (p) => tryAct("update_candidate", { id: c.id, patch: p });
  const tpl = (s.settings.captionTemplates || []).find((x) => x.name === c.style) || s.settings.captionStyle || {};
  const hookTpl = (s.settings.captionTemplates || []).find((x) => x.name === c.hookStyle) || tpl;
  // Play from the reel's start (or from outside the strip) begins with the teaser; anywhere else resumes there.
  const toggle = () => { const v = video.current; if (!v) return; resumeAudio(); if (v.paused) { if (!teaser.current && (Math.abs(v.currentTime - c.start) < 0.3 || v.currentTime < c.start - CTX || v.currentTime > c.end + CTX)) fromTop(v); v.play(); setPlaying(true); } else { v.pause(); setPlaying(false); } };
  const seek = (time) => { teaser.current = false; setTeasing(false); if (video.current) video.current.currentTime = Math.min(c.end + CTX, Math.max(Math.max(0, c.start - CTX), time)); };
  const hasPunch = c.punchStart != null && c.punchEnd != null;
  const introOn = hasPunch && (c.introOn ?? s.settings.intro ?? true);
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
  const onCapDown = (e) => {
    e.stopPropagation(); e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId); setCapDrag({ y0: e.clientY, h: frameH, pct0: capPct, pct: capPct, moved: false });
  };
  const onCapMove = (e) => { if (!capDrag) return; e.stopPropagation(); const pct = Math.round(Math.min(70, Math.max(4, capDrag.pct0 + ((capDrag.y0 - e.clientY) / capDrag.h) * 100))); setCapDrag({ ...capDrag, pct, moved: true }); };
  const onCapUp = (e) => { if (!capDrag) return; e.stopPropagation(); if (capDrag.moved) { setPending((p) => ({ ...(p || {}), cap: capDrag.pct })); patch({ captionPct: capDrag.pct }); } setCapDrag(null); };
  // Caption preview: the words of the current segment, current word highlighted.
  const cur = segs.find((g) => t >= g.start && t < g.end);
  const words = !cur ? [] : cur.words?.length ? cur.words : cur.text.split(" ").map((w, i, a) => ({ w, s: cur.start + ((cur.end - cur.start) * i) / a.length, e: cur.start + ((cur.end - cur.start) * (i + 1)) / a.length }));
  const per = tpl.wordsPerLine || 4;
  const wi = words.findIndex((w) => t >= w.s && t < w.e);
  const line = wi >= 0 ? words.slice(Math.floor(wi / per) * per, Math.floor(wi / per) * per + per) : [];
  const trans = c.translation?.find((g) => t >= g.start && t < g.end);
  // The render draws Arabic-script captions in Geeza Pro at 115 %; so does the preview.
  const capLang = s.settings.translateTo && (c.translation?.length || 0) > 0 ? s.settings.translateTo : detail?.transcript?.language || "";
  const capTpl = RTL.includes(capLang) ? { ...tpl, font: "Geeza Pro", size: (tpl.size || 42) * 1.15 } : tpl;
  // One caption language: the translation replaces the spoken words when a language is set and the reel has one.
  const translated = !!s.settings.translateTo && s.settings.translateTo !== (detail?.transcript?.language || "") && (c.translation?.length || 0) > 0;
  const inWords = segs.flatMap((g, gi) => (g.end > c.start - CTX && g.start < c.end + CTX) ? (g.words?.length ? g.words.map((w, wi) => ({ ...w, gi, wi })) : g.text.split(/\s+/).map((w, wi, a) => ({ w, s: g.start + ((g.end - g.start) * wi) / a.length, e: g.start + ((g.end - g.start) * (wi + 1)) / a.length, gi, wi }))) : []);
  const editWord = async (w, v) => { setEdit(null); if (v === w.w) return; const r = await tryAct("edit_word", { id: sourceId, index: w.gi, word: w.wi, text: v }, "Word corrected"); if (r) refreshSeg(r, w.gi); };
  const editLine = async (gi, v) => { setEdit(null); if (v === segs[gi]?.text) return; const r = await tryAct("edit_segment", { id: sourceId, index: gi, text: v }, "Line corrected"); if (r) refreshSeg(r, gi); };
  const snapEnd = () => { const g = segs.find((g) => g.start <= c.end && g.end >= c.end) || segs.filter((g) => g.end <= c.end).pop(); if (g) patch({ end: Math.min(g.end + 0.2, c.start + s.settings.maxReelS) }); };
  // The trim strip spans the reel and CTX seconds either side (never before 0).
  const ra = Math.max(0, c.start - CTX), rb = c.end + CTX;
  const pct = (time) => ((time - ra) / (rb - ra)) * 100;
  const segAt = (time) => segs.findIndex((g) => g.end > time + 0.01);
  // One line more (or less) at either edge; the core refuses a reel past the max.
  const lineBefore = () => { const i = segs.findIndex((g) => g.start >= c.start - 0.05); const g = segs[Math.max(0, (i < 0 ? segs.length : i) - 1)]; if (g && g.start < c.start) patch({ start: g.start }); };
  const lineAfter = () => { const g = segs.find((g) => g.end > c.end + 0.05); if (g) patch({ end: g.end }); };
  const dropFirst = () => { const g = segs.find((g) => g.start >= c.start + 0.3); if (g && c.end - g.start >= 3) patch({ start: g.start }); };
  const dropLast = () => { const g = [...segs].reverse().find((g) => g.end <= c.end - 0.3); if (g && g.end - c.start >= 3) patch({ end: g.end }); };
  const nudge = (edge, d) => patch(edge === "start" ? { start: Math.max(0, +(c.start + d).toFixed(2)) } : { end: +Math.max(c.start + 3, c.end + d).toFixed(2) });
  // The teaser's words, from the transcript between its times.
  const wordsIn = (a, b) => inWords.filter((w) => w.s >= a - 0.05 && w.e <= b + 0.05).map((w) => w.w).join(" ");
  const setPunch = (a, b) => { if (b - a < 0.8) { toast("A teaser needs at least a second", "err"); return; } patch({ punchStart: +a.toFixed(2), punchEnd: +b.toFixed(2), punchline: wordsIn(a, b), introOn: true }); };
  const busyPunch = s.jobs.some((j) => j.kind === "punchline" && j.refId === c.id && (j.status === "queued" || j.status === "running"));
  const track = reelTrack(s.tracks || [], s.settings, c);
  const musicVol = c.musicVolume ?? s.settings.musicVolume ?? 0.3;
  // Normalised preview: the speech at PREVIEW_LUFS whatever the recording, the music set against it.
  // Without a CORS-clean graph (cors false) the elements play as they are, music by its level only.
  const speechL = x.meta?.lufs, trackL = track?.lufs;
  const speechGain = speechL != null ? Math.min(16, dbGain(PREVIEW_LUFS - speechL)) : 1;
  const musicGain = musicVol * (speechL != null && trackL != null ? dbGain(speechL - trackL) : 1) * speechGain;
  if (cors) {
    const vg = audioGain(video.current), mg = audioGain(music.current);
    if (vg) vg.gain.value = speechGain;
    if (mg) mg.gain.value = musicGain;
  } else if (music.current) music.current.volume = Math.min(1, musicGain / speechGain);
  // The logo on this reel: its own place when dragged, else the library's.
  const L = s.settings.logo || {};
  const logoOn = !!L.path && (c.logo?.on ?? L.enabled);
  const lw = frameW * (L.size || 0.16), lh = lw / logoAspect;
  const lx = logoDrag?.x ?? c.logo?.x ?? L.x ?? 0.94, ly = logoDrag?.y ?? c.logo?.y ?? L.y ?? 0.04;
  const onLogoDown = (e) => { e.stopPropagation(); e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setLogoDrag({ x0: e.clientX, y0: e.clientY, sx: lx, sy: ly, x: lx, y: ly }); };
  const onLogoMove = (e) => { if (!logoDrag) return; e.stopPropagation(); const fx = Math.max(1, frameW - lw), fy = Math.max(1, frameH - lh); setLogoDrag({ ...logoDrag, x: Math.min(1, Math.max(0, logoDrag.sx + (e.clientX - logoDrag.x0) / fx)), y: Math.min(1, Math.max(0, logoDrag.sy + (e.clientY - logoDrag.y0) / fy)) }); };
  const onLogoUp = (e) => { if (!logoDrag) return; e.stopPropagation(); if (logoDrag.x !== logoDrag.sx || logoDrag.y !== logoDrag.sy) patch({ logo: { ...(c.logo || {}), x: +logoDrag.x.toFixed(3), y: +logoDrag.y.toFixed(3) } }); setLogoDrag(null); };
  const ticked = cands.filter((v) => v.approved).length;
  const dur = c.end - c.start;

  return (
    <div className="main">
      <div className="panel" style={{ width: leftW, flexShrink: 0 }}>
        <div className="panel-head"><span>Candidates</span><span className="sub">{cands.length} · {ticked} ticked</span><span className="grow" /><Btn small onClick={() => tryAct("approve_above", { sourceId, score: 8 }, "Ticked everything scoring 8+")}>Tick ≥ 8</Btn></div>
        <div style={{ padding: "8px 10px", borderBottom: "1px solid var(--rule)", display: "flex", flexDirection: "column", gap: 6, fontSize: 13 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>Source</span><select className="input" style={{ height: 22 }} value={sourceId} onChange={(e) => { go("reels", { sourceId: e.target.value }); setSelId(null); }}>{sources.map((v) => <option key={v.id} value={v.id}>{v.title}</option>)}</select></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>Min score</span><input type="range" className="slider" min="0" max="10" value={minScore} onChange={(e) => setMinScore(+e.target.value)} /><span style={{ width: 16 }}>{minScore}</span></div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 10, color: "var(--sec)" }}>{CATS.map((k) => { const n = cands.filter((v) => v.category === k).length; return n ? <span key={k} style={{ display: "flex", alignItems: "center", gap: 4 }} title={CAT_NAMES[k]}><Cat id={k} /><span className="num">{n}</span></span> : null; })}</div>
        </div>
        <div className="cands">
          {cands.filter((v) => v.score >= minScore || v.id === c.id).map((v) => (
            <div key={v.id} className={`cand ${v.id === c.id ? "on" : ""} ${v.discarded ? "off" : ""}`} onClick={() => setSelId(v.id)} title={`${CAT_NAMES[v.category] || v.category} · ${v.why || ""}`}>
              <input type="checkbox" checked={v.approved} onChange={(e) => tryAct("approve", { ids: [v.id], approved: e.target.checked })} onClick={(e) => e.stopPropagation()} style={{ accentColor: "var(--accent)", margin: 0 }} />
              <span className="score">{v.score}</span>
              <span className="ell">{v.title || "(untitled)"}</span>
              <span className="muted" style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "flex-end" }}><Cat id={v.category} />{s.renders.some((r) => r.candidateId === v.id && r.status === "done") && <span className="dot mint" title="rendered" />}<span className="num" style={{ width: 44, textAlign: "right" }}>{(v.end - v.start).toFixed(1)} s</span></span>
            </div>
          ))}
        </div>
        <span className="grow" />
        <div className="cand-detail">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Cat id={c.category} style={{ color: "var(--sec)" }} />
            <select className="input" style={{ height: 22, fontSize: 12, padding: "0 6px", width: 120 }} value={c.category} onChange={(e) => patch({ category: e.target.value })}>{CATS.map((k) => <option key={k} value={k}>{CAT_NAMES[k]}</option>)}</select>
            <span className="grow" />
            <span className="score" style={{ height: 22 }}>score <input type="number" min="1" max="10" value={c.score} onChange={(e) => patch({ score: +e.target.value })} style={{ width: 28, background: "none", border: 0, color: "inherit", font: "inherit", textAlign: "center", padding: 0, marginLeft: 4 }} /></span>
          </div>
          {c.why && <span className="muted" style={{ lineHeight: 1.45, fontStyle: "italic" }}>{c.why}</span>}
        </div>
        <div className="panel-foot" style={{ flexWrap: "wrap" }}>
          <Btn primary className="grow" style={{ flexBasis: "100%" }} disabled={!ticked} onClick={async () => { const r = await tryAct("render", { sourceId }, `Rendering ${ticked} reels`); if (r) go("queue"); }}>Render {ticked} ticked · {ticked * ((s.settings.formats?.length || 3) + 1)} files</Btn>
          <select className="input" style={{ width: 76 }} value={s.settings.targetReelS || 60} onChange={(e) => tryAct("settings", { patch: { targetReelS: +e.target.value, maxReelS: Math.round(+e.target.value * 1.5) } })} title="Length the reel finder aims for. A thought that closes sooner stays shorter; one that needs more may run to 1.5×.">{[20, 30, 45, 60, 75, 90].map((n) => <option key={n} value={n}>~{n} s</option>)}</select>
          <Btn onClick={() => tryAct("run", { id: sourceId, stage: "detect" }, "Claude is reading again")}>Find again</Btn>
          <Btn danger onClick={() => setAskOver(true)} title="Delete this lecture's reels and their files, then find new ones">Start over</Btn>
          {askOver && <Confirm text={`Delete all ${cands.length} reels of this lecture, their queued renders and their files, and find new ones at ~${s.settings.targetReelS || 60} s? The video and transcript stay.`} yes="Delete and find again" no="Keep" onYes={async () => { setAskOver(false); const r = await tryAct("clear_reels", { sourceId }); if (r) tryAct("run", { id: sourceId, stage: "detect" }, `${r.reels} reels deleted. Claude is reading again`); }} onNo={() => setAskOver(false)} />}
        </div>
      </div>

      <Grip value={leftW} set={setLeftW} reset={320} />
      <div className="col grow">
        <div className="panel grow">
          <div className="panel-head"><span>Preview</span><Seg value={format} onChange={setFormat} options={FORMATS} /><Check label="safe zones" checked={safe} onChange={setSafe} /><span className="grow" /><Speed value={speed} onChange={setSpeed} /><span className="sub" style={{ marginLeft: 8 }}>zoom</span><input type="range" className="slider" style={{ width: 120 }} min="1" max="2.5" step="0.05" value={cz} onChange={(e) => setZoom(+e.target.value)} title="Zoom the crop window" /><span className="num sub" style={{ width: 36 }}>{cz.toFixed(2)}×</span></div>
          <div className="teaser-bar">
            <Check label="Teaser" checked={introOn} disabled={!hasPunch} onChange={(v) => patch({ introOn: v })} />
            {hasPunch ? <span className="ell" title={c.punchline}>“{c.punchline || "…"}” <span className="muted num">{(c.punchEnd - c.punchStart).toFixed(1)} s</span></span> : <span className="muted ell">No punchline yet. Ask Claude, or right-click a word in the strip.</span>}
            <span className="grow" />
            <Btn small disabled={busyPunch} onClick={() => tryAct("punchline", { id: c.id }, "Claude is picking the punchline")}>{busyPunch ? "Asking…" : hasPunch ? "Ask again" : "Ask Claude"}</Btn>
            <Btn small className={teaserOpen ? "on" : ""} onClick={() => setTeaserOpen(!teaserOpen)} aria-expanded={teaserOpen}>Edit {teaserOpen ? "▴" : "▾"}</Btn>
          </div>
          {teaserOpen && <div className="teaser-bar">
            <Btn small onClick={() => setPunch(t, hasPunch && c.punchEnd > t + 0.8 ? c.punchEnd : t + 3)}>Start = playhead</Btn>
            <Btn small disabled={!hasPunch} onClick={() => setPunch(c.punchStart, t)}>End = playhead</Btn>
            {hasPunch && <span className="muted num">{fmt(c.punchStart)} → {fmt(c.punchEnd)}</span>}
            <span className="grow" />
            <span className="muted">Transition</span>
            <select className="input" style={{ width: 150, height: 22 }} value={c.transition || ""} onChange={(e) => patch({ transition: e.target.value || null })}><option value="">Default · {TRANSITIONS.find(([v]) => v === (s.settings.transition || "swoosh"))?.[1]}</option>{TRANSITIONS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
          </div>}
          <div className="stage" ref={stageRef}>
            <div className={`frame${flash ? ` tx-${flash}` : ""}`} style={{ width: frameW, height: frameH, cursor: zoomW > frameW + 1 || zoomH > frameH + 1 ? (drag ? "grabbing" : "grab") : "default" }} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
              {x.videoPath && <video ref={video} key={`${x.meta?.fileAt || x.id}-${cors}`} crossOrigin={cors ? "anonymous" : undefined} src={fileUrl(x.previewPath || x.videoPath, x.meta?.fileAt)} onLoadedMetadata={(e) => { e.currentTarget.playbackRate = speed; if (resume.current != null) { e.currentTarget.currentTime = resume.current; resume.current = null; } }} onError={(e) => cors ? noCors() : toast(`The player can\u2019t read this video (error ${e.currentTarget.error?.code ?? "?"}). Try Import again in the Library.`, "err")} style={{ left, top, width: zoomW, height: zoomH, pointerEvents: "none" }} muted={false} />}
              {hold === "tail" && <div className="tail-fade" style={{ animationDuration: `${s.settings.musicTail ?? 3}s` }} />}
              {hold === "card" && s.settings.endCard?.paths?.[aspectKey] && <img src={fileUrl(s.settings.endCard.paths[aspectKey])} alt="" style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: "cover" }} />}
              {c.titleOn !== false && hookTpl.hook !== false && c.hook && (teasing || (!introOn && t >= c.start - 0.3 && t - c.start < (hookTpl.hookSeconds || 2.5))) && <div className="hook" style={{ top: frameH * ((hookTpl.hookPct ?? 8) / 100) }}><span style={hookCss(hookTpl, k)}>{c.hook}</span></div>}
              {c.captionsOn !== false && (translated ? !!trans : line.length > 0) && <div className="cap" style={{ bottom: frameH * (capPct / 100 + (format === "tiktok" ? 0.08 : 0)) - 10, alignItems: tpl.align === "left" ? "flex-start" : "center", pointerEvents: "auto", cursor: capDrag ? "grabbing" : "ns-resize" }} onPointerDown={onCapDown} onPointerMove={onCapMove} onPointerUp={onCapUp} onPointerCancel={onCapUp} title="Drag up or down"><span style={{ ...captionCss(capTpl, k), fontSize: (capTpl.size || 42) * (frameH / 1920) }}>{translated ? trans.text : line.map((w, i) => <span key={i} style={t >= w.s && t < w.e ? litCss(tpl) : undefined}>{w.w} </span>)}</span></div>}
              {logoOn && (hold !== "card" || L.onEndCard) && <img src={fileUrl(L.path)} alt="" draggable={false} onLoad={(e) => setLogoAspect(e.currentTarget.naturalWidth / Math.max(1, e.currentTarget.naturalHeight))} onPointerDown={onLogoDown} onPointerMove={onLogoMove} onPointerUp={onLogoUp} onPointerCancel={onLogoUp} title="Drag to place the logo on this reel" style={{ position: "absolute", left: (frameW - lw) * lx, top: (frameH - lh) * ly, width: lw, height: lh, opacity: L.opacity ?? 1, cursor: logoDrag ? "grabbing" : "move", zIndex: 3 }} />}
              {(flash === "flash" || flash === "fade") && <div className={`teaser-flash ${flash}`} />}
              {track && <audio ref={music} key={`${track.id}-${cors}`} crossOrigin={cors ? "anonymous" : undefined} src={fileUrl(track.path)} loop onError={noCors} style={{ display: "none" }} />}
              {teasing && <span className="teaser-badge">teaser</span>}
              {safe && aw < ah && <><div className="safe" style={{ left: 0, right: 0, top: 0, height: frameH * 0.11, borderWidth: "0 0 1px 0" }} /><div className="safe" style={{ left: 0, right: 0, bottom: 0, height: frameH * (format === "tiktok" ? 0.2 : 0.14), borderWidth: "1px 0 0 0" }} /><div className="safe" style={{ right: 0, top: frameH * 0.45, width: 56 * k, height: frameH * 0.4, borderWidth: "0 0 0 1px" }} /></>}
              <span style={{ position: "absolute", right: 8, bottom: 8, fontSize: 12, background: "var(--bg)", padding: "1px 5px", borderRadius: 3 }} className="num">{fmt(Math.max(0, t - c.start))} / {fmt(dur)}</span>
            </div>
            <div style={{ position: "absolute", right: 16, top: 12, display: "flex", flexDirection: "column", gap: 6, fontSize: 13, color: "var(--muted)", alignItems: "flex-end" }}><span>source {fmt(c.start)} → {fmt(c.end)}</span><span className={dur > s.settings.maxReelS ? "coral" : "mint"}>{dur.toFixed(1)} s · aim {s.settings.targetReelS || 60} · max {s.settings.maxReelS}</span></div>
          </div>
          <div className="transport">
            <Btn icon onClick={() => { const i = cands.findIndex((v) => v.id === c.id); setSelId(cands[Math.max(0, i - 1)]?.id); }} aria-label="Previous">{I.prev}</Btn>
            <Btn icon primary onClick={toggle} aria-label="Play">{playing ? I.pause : I.play}</Btn>
            <Btn icon onClick={() => { const i = cands.findIndex((v) => v.id === c.id); setSelId(cands[Math.min(cands.length - 1, i + 1)]?.id); }} aria-label="Next">{I.next}</Btn>
            <input type="range" className="slider scrub grow" min="0" max={dur.toFixed(2)} step="0.05" value={Math.min(dur, Math.max(0, t - c.start))} onChange={(e) => seek(c.start + +e.target.value)} title="Scrub through the reel. It loops; drag the picture, captions or logo to place them." />
            <span className="num muted" style={{ whiteSpace: "nowrap" }}>{fmt(Math.max(0, t - c.start))} / {fmt(dur)}</span>
            <span className="muted" title="Space plays. Arrows switch reels. [ and ] set in and out. A ticks.">⌨︎</span>
          </div>
        </div>
        <Grip value={trimH} set={setTrimH} dir={-1} axis="y" reset={178} />
        <Panel title="Trim" sub={`in ${fmt(c.start)} · out ${fmt(c.end)} · ${dur.toFixed(1)} s`} right={<><Btn small onClick={() => patch({ start: Math.max(0, t) })}>In = playhead</Btn><Btn small onClick={() => patch({ end: Math.max(c.start + 3, t) })}>Out = playhead</Btn><Btn small onClick={snapEnd}>Snap out to sentence</Btn></>} style={{ height: trimH, flexShrink: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13 }}>
            <span className="muted">Start</span><Btn small onClick={lineBefore} title="Start one transcript line earlier">+ line before</Btn><Btn small onClick={() => nudge("start", -1)}>−1 s</Btn><Btn small onClick={() => nudge("start", 1)}>+1 s</Btn><Btn small onClick={dropFirst} title="Start at the next line">− first line</Btn>
            <span className="grow" />
            <span className="muted">End</span><Btn small onClick={dropLast} title="End at the line before">− last line</Btn><Btn small onClick={() => nudge("end", -1)}>−1 s</Btn><Btn small onClick={() => nudge("end", 1)}>+1 s</Btn><Btn small onClick={lineAfter} title="End one transcript line later">+ line after</Btn>
          </div>
          <div className="range" onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); seek(ra + ((e.clientX - r.left) / r.width) * (rb - ra)); }}>
            {segs.filter((g) => g.end > ra && g.start < rb).map((g, i) => <span key={i} className="seg" style={{ left: `${pct(Math.max(g.start, ra))}%`, width: `${pct(Math.min(g.end, rb)) - pct(Math.max(g.start, ra))}%`, position: "absolute" }} />)}
            <span className="sel" style={{ left: `${pct(c.start)}%`, width: `${pct(c.end) - pct(c.start)}%` }} />
            {hasPunch && <span className="punch" style={{ left: `${pct(c.punchStart)}%`, width: `${pct(c.punchEnd) - pct(c.punchStart)}%` }} title={`Teaser: ${c.punchline || ""}`} />}
            <span className="cur" style={{ left: `${pct(t)}%` }} />
          </div>
          <div className="words">{inWords.map((w, i) => {
            if (edit && edit.wi == null && edit.gi === w.gi) return inWords[i - 1]?.gi === w.gi ? null : <Inline key={i} area value={segs[w.gi]?.text || ""} onDone={(v) => editLine(w.gi, v)} />;
            if (edit && edit.gi === w.gi && edit.wi === w.wi) return <Inline key={i} value={w.w} onDone={(v) => editWord(w, v)} />;
            return <span key={i} className={`${inWords[i - 1] && inWords[i - 1].gi !== w.gi ? "ls " : ""}${t >= w.s && t < w.e ? "cur" : w.s >= c.start - 0.05 && w.e <= c.end + 0.05 ? "in" : ""}${hasPunch && w.s >= c.punchStart - 0.05 && w.e <= c.punchEnd + 0.05 ? " punch" : ""}`}
              onClick={(e) => { if (e.metaKey && e.shiftKey) setPunch(c.punchStart ?? w.s, w.e); else if (e.metaKey) setPunch(w.s, Math.max(w.e, c.punchEnd != null && c.punchEnd > w.s ? c.punchEnd : w.e + 2)); else if (e.altKey) patch({ start: w.s }); else if (e.shiftKey) patch({ end: w.e }); else seek(w.s); }}
              onDoubleClick={() => setEdit({ gi: w.gi, wi: w.wi })}
              onContextMenu={(e) => { e.preventDefault(); setWordMenu({ x: e.clientX, y: e.clientY, w }); }}
              title="Click: jump. Double-click: correct the word. Right-click: more.">{w.w}</span>;
          })}</div>
          {wordMenu && <Menu at={wordMenu} onClose={() => setWordMenu(null)} items={[
            { label: "Correct this word", onClick: () => setEdit({ gi: wordMenu.w.gi, wi: wordMenu.w.wi }) },
            { label: "Correct the whole line", onClick: () => setEdit({ gi: wordMenu.w.gi }) },
            { label: "Reel starts here   ⌥-click", onClick: () => patch({ start: wordMenu.w.s }) },
            { label: "Reel ends here   ⇧-click", onClick: () => patch({ end: wordMenu.w.e }) },
            { label: "Teaser starts here   ⌘-click", onClick: () => setPunch(wordMenu.w.s, Math.max(wordMenu.w.e, c.punchEnd != null && c.punchEnd > wordMenu.w.s ? c.punchEnd : wordMenu.w.e + 2)) },
            { label: "Teaser ends here   ⌘⇧-click", disabled: !hasPunch || wordMenu.w.e <= c.punchStart, onClick: () => setPunch(c.punchStart, wordMenu.w.e) },
          ]} />}
        </Panel>
      </div>

      <Grip value={rightW} set={setRightW} dir={-1} reset={340} />
      <div className="panel" style={{ width: rightW, flexShrink: 0 }}>
        <div className="panel-head">{TABS.map(([v, l]) => <button key={v} type="button" className={`tab ${tab === v ? "on" : ""}`} onClick={() => setTab(v)}>{l}</button>)}</div>
        <div className="panel-body" style={{ gap: 12 }}>
          <ReelDetails c={c} s={s} sourceId={sourceId} go={go} tab={tab} hookTpl={hookTpl} applyScope={applyScope} setApplyScope={setApplyScope} applyKeys={applyKeys} setApplyKeys={setApplyKeys} setPending={setPending} aspectKey={aspectKey} />
        </div>
        <span className="grow" />
        <div className="panel-foot" style={{ flexWrap: "wrap" }}>
          <Btn primary className="grow" onClick={async () => { const r = await tryAct("approve", { ids: [c.id], approved: !c.approved }); if (r != null && !c.approved) { const i = cands.findIndex((v) => v.id === c.id); const next = cands.slice(i + 1).find((v) => !v.approved && !v.discarded); if (next) setSelId(next.id); } }}>{c.approved ? "Approved ✓" : "Approve and next"}</Btn>
          <Btn onClick={async () => { const r = await tryAct("render", { candidateIds: [c.id] }, "Rendering"); if (r) go("queue"); }}>Render now</Btn>
          <Btn danger icon onClick={() => tryAct("discard", { id: c.id })} aria-label="Discard" title={c.discarded ? "Restore" : "Discard"}>{I.trash}</Btn>
        </div>
      </div>
    </div>
  );
}

const TABS = [["text", "Text"], ["look", "Look"], ["sound", "Sound"], ["publish", "Publish"]];

// The inspector's tabs. Memoised: during playback the parent re-renders every frame for the
// picture and the current word; this part only when the reel or the store changes.
const ReelDetails = memo(function ReelDetails({ c, s, sourceId, go, tab, hookTpl, applyScope, setApplyScope, applyKeys, setApplyKeys, setPending, aspectKey }) {
  const patch = (p) => tryAct("update_candidate", { id: c.id, patch: p });
  const L = s.settings.logo || {};
  const ec = s.settings.endCard || {};
  const chooseLogo = async () => { const p = await tryAct("choose_file", { kind: "image", prompt: "Choose the logo (PNG with transparency works best)" }); if (p) tryAct("settings", { patch: { logo: { path: p, enabled: true } } }, "Logo set"); };
  const chooseCard = async () => { const p = await tryAct("choose_file", { kind: "image", prompt: "Choose the end card image" }); if (p) tryAct("end_card_image", { path: p, background: ec.background || "#000000" }, "End card set for 9:16, 4:5 and 16:9"); };
  // The colour picker fires while it is dragged: remake the cards once it settles.
  const colorTimer = useRef(null);
  const setCardColor = (hex) => { clearTimeout(colorTimer.current); colorTimer.current = setTimeout(() => tryAct("end_card_image", { background: hex }), 450); };
  const done = s.renders.filter((r) => r.candidateId === c.id && r.status === "done");
  return (
    <>
      {tab === "text" && <div className="insp-group">
        <Field label="Title"><Text value={c.title} onCommit={(v) => patch({ title: v })} /></Field>
        <Check label={`Title on the video, first ${(hookTpl.hookSeconds || 2.5)} s`} checked={c.titleOn !== false} onChange={(v) => patch({ titleOn: v })} />
        <Text value={c.hook} disabled={c.titleOn === false} onCommit={(v) => patch({ hook: v })} placeholder="2 to 5 words" />
        <Field label="Cover line"><Text area rows={2} value={c.coverTitle || ""} onCommit={async (v) => { await patch({ coverTitle: v }); tryAct("covers", { candidateIds: [c.id] }); }} placeholder="Written with the cover pick. Tells the reel's story in one or two lines." /></Field>
        <Btn small style={{ alignSelf: "flex-start" }} onClick={() => tryAct("pick_cover", { id: c.id }, "Choosing a new cover")} title="Picks the cover frame again and renders this reel again.">New cover</Btn>
        <span className="hint">Words are corrected in the strip under the trim: double-click a word, or right-click for the whole line. That changes the lecture's transcript.</span>
      </div>}

      {tab === "look" && <>
        <div className="insp-group">
          <div className="insp-head">Captions and title<span className="grow" /><a href="#" onClick={(e) => { e.preventDefault(); go("style", { sourceId, candidateId: c.id }); }}>edit templates</a></div>
          <div className="grid2">
            <Field label="Captions"><select className="input" value={c.style || ""} onChange={(e) => patch({ style: e.target.value || null })}><option value="">Default · {s.settings.captionStyle?.name}</option>{(s.settings.captionTemplates || []).map((x) => <option key={x.name} value={x.name}>{x.name}</option>)}</select></Field>
            <Field label="Title"><select className="input" value={c.hookStyle || ""} onChange={(e) => patch({ hookStyle: e.target.value || null })}><option value="">Same as captions</option>{(s.settings.captionTemplates || []).map((x) => <option key={x.name} value={x.name}>{x.name}</option>)}</select></Field>
          </div>
          <div style={{ display: "flex", gap: 14, flexWrap: "wrap" }}><Check label="Captions on the video" checked={c.captionsOn !== false} onChange={(v) => patch({ captionsOn: v })} /></div>
          <span className="hint">Drag the captions or the picture to place them.{c.captionPct != null && <> Captions at {c.captionPct}% · <a href="#" onClick={(e) => { e.preventDefault(); patch({ captionPct: null }); }}>reset</a>.</>}{(Math.abs((c.crop?.x ?? 0.5) - 0.5) > 0.005 || Math.abs((c.crop?.y ?? 0.5) - 0.5) > 0.005 || (c.crop?.z ?? 1) !== 1) && <> Crop {Math.round((c.crop?.x ?? 0.5) * 100)}% / {Math.round((c.crop?.y ?? 0.5) * 100)}% at {(c.crop?.z ?? 1).toFixed(2)}× · <a href="#" onClick={(e) => { e.preventDefault(); setPending((p) => ({ ...(p || {}), x: 0.5, y: 0.5, z: 1 })); patch({ crop: { x: 0.5, y: 0.5, z: 1 } }); }}>reset</a>.</>}</span>
        </div>
        <div className="insp-group">
          <div className="insp-head">Logo<span className="grow" />{L.path && <Check label="On this reel" checked={c.logo?.on ?? L.enabled} onChange={(v) => patch({ logo: { ...(c.logo || {}), on: v } })} />}</div>
          {L.path ? <>
            <span className="hint">Drag the logo in the preview to place it on this reel.{c.logo?.x != null && <> <a href="#" onClick={(e) => { e.preventDefault(); tryAct("settings", { patch: { logo: { x: c.logo.x, y: c.logo.y } } }, "Place is the default now"); }}>make this the default place</a> · <a href="#" onClick={(e) => { e.preventDefault(); patch({ logo: { x: null, y: null } }); }}>reset</a>.</>}</span>
            <details className="apply-box">
              <summary>Every reel<span className="muted"> · library setting</span></summary>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}><img src={fileUrl(L.path)} alt="" style={{ height: 28, maxWidth: 80, objectFit: "contain", background: "#0006", borderRadius: 3 }} /><span className="grow" /><Btn small onClick={chooseLogo}>Change…</Btn></div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 48 }}>size</span><input type="range" className="slider" min="0.05" max="0.4" step="0.01" value={L.size ?? 0.16} onChange={(e) => tryAct("settings", { patch: { logo: { size: +e.target.value } } })} /><span className="num muted" style={{ width: 34 }}>{Math.round((L.size ?? 0.16) * 100)}%</span></div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 48 }}>opacity</span><input type="range" className="slider" min="0.2" max="1" step="0.05" value={L.opacity ?? 1} onChange={(e) => tryAct("settings", { patch: { logo: { opacity: +e.target.value } } })} /><span className="num muted" style={{ width: 34 }}>{Math.round((L.opacity ?? 1) * 100)}%</span></div>
              <Check label="Also on the end card" checked={!!L.onEndCard} onChange={(v) => tryAct("settings", { patch: { logo: { onEndCard: v } } })} />
            </details>
          </> : <div style={{ display: "flex", alignItems: "center", gap: 8 }}><Btn small onClick={chooseLogo}>Choose logo…</Btn><span className="hint">PNG with transparency. Goes on every reel; drag to place.</span></div>}
        </div>
        <div className="insp-group">
          <div className="insp-head">End card<span className="grow" />{ec.paths?.[aspectKey] && <Check label="On this reel" checked={c.endCardOn ?? ec.enabled} onChange={(v) => patch({ endCardOn: v })} />}</div>
          {ec.paths?.[aspectKey] ? <div style={{ display: "flex", alignItems: "center", gap: 8 }}><img src={fileUrl(ec.paths[aspectKey])} alt="" style={{ height: 48, borderRadius: 3, border: "1px solid var(--rule)" }} /><span className="muted" style={{ fontSize: 13 }}>{ec.seconds ?? 2.5} s after the reel{ec.enabled ? ", on for every reel" : ", off by default"}</span></div> : <span className="hint">No end card yet.</span>}
          <details className="apply-box" open={!ec.image || undefined}>
            <summary>Every reel<span className="muted"> · library setting</span></summary>
            {ec.image && <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
              <Seg value={ec.background ? "color" : "blur"} onChange={(v) => tryAct("end_card_image", { background: v === "color" ? "#000000" : "" })} options={[["color", "Colour"], ["blur", "Blurred"]]} />
              {ec.background && <input type="color" defaultValue={ec.background} key={ec.background} onChange={(e) => setCardColor(e.target.value)} style={{ width: 30, height: 24, border: 0, background: "none", padding: 0 }} title="Background behind the picture" />}
              <span className="muted">for</span><input className="input num" type="number" min="0.5" max="10" step="0.5" style={{ width: 58 }} value={ec.seconds ?? 2.5} onChange={(e) => tryAct("settings", { patch: { endCard: { seconds: +e.target.value || 2.5 } } })} /><span className="muted">s</span>
            </div>}
            {ec.paths?.[aspectKey] && <Check label="Music plays over the end card" checked={ec.music !== false} onChange={(v) => tryAct("settings", { patch: { endCard: { music: v } } })} />}
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}><Btn small onClick={chooseCard}>Choose image…</Btn><Btn small onClick={() => go("posters")}>Draw from a poster</Btn></div>
          </details>
        </div>
      </>}

      {tab === "sound" && <div className="insp-group">
        {(() => {
          const ready = (s.tracks || []).filter((t) => t.status === "ready");
          const def = s.settings.music || "random";
          const picked = reelTrack(s.tracks || [], s.settings, c);
          return <>
            <Field label={`Music${picked ? ` · ${picked.title}` : ""}`}><select className="input" value={c.music ?? ""} onChange={(e) => patch({ music: e.target.value || null })}>
              <option value="">Default · {def === "none" ? "no music" : "random"}</option><option value="random">Random</option><option value="none">No music</option>
              {ready.map((t) => <option key={t.id} value={t.id}>{t.title}{t.duration ? ` · ${fmtLong(t.duration)}` : ""}</option>)}
            </select></Field>
            {!ready.length && <span className="hint">No tracks yet. Add a YouTube video or playlist under Settings › Music.</span>}
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 70 }}>Music level</span><input type="range" className="slider" min="0" max="1" step="0.05" value={c.musicVolume ?? s.settings.musicVolume ?? 0.3} onChange={(e) => patch({ musicVolume: +e.target.value })} /><span className="num muted" style={{ width: 96 }} title="How far under the speaker the music sits (before it ducks while he speaks)">{levelDb(c.musicVolume ?? s.settings.musicVolume ?? 0.3)} dB{c.musicVolume == null ? " default" : ""}</span>{c.musicVolume != null && <a href="#" onClick={(e) => { e.preventDefault(); patch({ musicVolume: null }); }}>reset</a>}</div>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 70 }}>Loudness</span><select className="input" value={c.loudness ?? ""} onChange={(e) => patch({ loudness: e.target.value === "" ? null : +e.target.value })}><option value="">Default · {LOUDNESS.find(([v]) => v === (s.settings.loudness ?? -11))?.[1] || `${s.settings.loudness} LUFS`}</option>{LOUDNESS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select></div>
            <span className="hint">The music ducks under the speaker in the render. <a href="#" onClick={(e) => { e.preventDefault(); go("settings"); }}>Music sources</a>.</span>
          </>;
        })()}
      </div>}

      {tab === "publish" && <>
        <div className="insp-group">
          <Field label="Caption"><Text area rows={4} value={c.caption} onCommit={(v) => patch({ caption: v })} /></Field>
          <Field label={`Hashtags · ${(c.hashtags || []).length}`}><Text value={(c.hashtags || []).join(" ")} onCommit={(v) => patch({ hashtags: v.split(/[\s,#]+/).filter(Boolean) })} /></Field>
          {(s.targets || []).filter((t) => t.enabled).length > 0 && <Field label="Post to"><div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>{(s.targets || []).filter((t) => t.enabled).map((t) => { const all = (s.targets || []).filter((x) => x.enabled).map((x) => x.id); const on = !c.targets?.length || c.targets.includes(t.id); return <Check key={t.id} label={t.name} checked={on} onChange={(v) => { const cur = c.targets?.length ? c.targets.filter((id) => all.includes(id)) : all; const next = v ? [...new Set([...cur, t.id])] : cur.filter((id) => id !== t.id); patch({ targets: next.length === all.length ? [] : next }); }} />; })}</div></Field>}
        </div>
        <div className="insp-group">
          <div className="insp-head">Render as<span className="grow" /><span className="muted" style={{ fontWeight: 400 }}>{c.formats?.length ? "this reel" : "library default"} + clean cut</span></div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "4px 12px" }}>
            {FORMATS.map(([v, l]) => <Check key={v} label={l} checked={(c.formats?.length ? c.formats : s.settings.formats || []).includes(v)} onChange={(on) => { const cur = c.formats?.length ? c.formats : s.settings.formats || []; patch({ formats: on ? [...new Set([...cur, v])] : cur.filter((f) => f !== v) }); }} />)}
          </div>
        </div>
        <div className="insp-group">
          <div className="insp-head">Files</div>
          {!done.length && <span className="hint">Nothing rendered yet.</span>}
          {done.map((r) => { const i = r.info || {}; const bad = i.issues?.length; return <div key={r.id} style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13 }}><Dot c={bad ? "coral" : i.width ? "mint" : ""} />{FORMAT_BRAND[r.format] ? <Brand id={FORMAT_BRAND[r.format]} size={12} /> : <FileMark size={12} />}<b style={{ width: 62 }}>{formatName(r.format)}</b><span className={`ell ${bad ? "coral" : "muted"}`}>{i.width ? `${i.width}×${i.height} · ${Math.round(i.fps)} fps · ${(i.kbps / 1000).toFixed(1)} Mbps · ${i.seconds} s${bad ? ` · ${i.issues.join(", ")}` : ""}` : "not measured"}</span><a href="#" className="muted" onClick={(e) => { e.preventDefault(); tryAct("open", { path: r.path }); }}>play</a></div>; })}
        </div>
      </>}

      <details className="apply-box">
        <summary>Copy settings to other reels<span className="muted"> · {applyKeys.length} chosen</span></summary>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 50 }}>To</span><Seg value={applyScope} onChange={setApplyScope} options={[["source", "Lecture"], ["approved", "Approved"], ["library", "All"]]} /></div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "4px 14px" }}>
          {[["music", "Music + level"], ["loudness", "Loudness"], ["style", "Captions template"], ["hookStyle", "Title template"], ["captionsOn", "Captions on/off"], ["titleOn", "Title on/off"], ["captionPct", "Caption position"], ["crop", "Crop"], ["intro", "Teaser on/off + transition"], ["logo", "Logo on/off + place"], ["endCard", "End card on/off"], ["formats", "Render formats"]].map(([k, l]) => <Check key={k} label={l} checked={applyKeys.includes(k)} onChange={(on) => setApplyKeys(on ? [...applyKeys, k] : applyKeys.filter((x) => x !== k))} />)}
        </div>
        <Btn small primary style={{ alignSelf: "flex-start" }} disabled={!applyKeys.length} onClick={async () => { const n = await tryAct("apply_look", { id: c.id, keys: applyKeys, scope: applyScope }); if (n != null) tryAct("snapshot", {}, `Copied to ${n} reels`); }}>Copy to {applyScope === "source" ? "this lecture's reels" : applyScope === "approved" ? "all approved reels" : "every reel"}</Btn>
      </details>
    </>
  );
});
