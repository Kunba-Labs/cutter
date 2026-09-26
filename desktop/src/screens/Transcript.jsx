import { useEffect, useRef, useState } from "react";
import { useStore, act, tryAct, fileUrl, fmt, fmtLong, LANGS } from "../store.js";
import { Panel, Btn, I } from "../ui.jsx";

export default function Transcript({ nav, go }) {
  const s = useStore();
  const x = s.sources.find((v) => v.id === nav.sourceId);
  const [detail, setDetail] = useState(null);
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const video = useRef(null);
  const cands = s.candidates.filter((c) => c.sourceId === nav.sourceId);

  useEffect(() => { if (nav.sourceId) act("source", { id: nav.sourceId }).then(setDetail).catch(() => setDetail(null)); }, [nav.sourceId, x?.stage, x?.updatedAt]);
  useEffect(() => {
    let raf;
    const tick = () => { if (video.current) setT(video.current.currentTime); raf = requestAnimationFrame(tick); };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);
  if (!x) return <div className="empty">No source selected.</div>;
  const segs = detail?.transcript?.segments || [];
  const cur = segs.findIndex((g) => t >= g.start && t < g.end);
  const seek = (time) => { if (video.current) { video.current.currentTime = time; } };
  const toggle = () => { const v = video.current; if (!v) return; if (v.paused) { v.play(); setPlaying(true); } else { v.pause(); setPlaying(false); } };
  const candAt = (g) => cands.find((c) => g.start >= c.start - 0.01 && g.end <= c.end + 0.01);
  const words = (g) => g.words?.length ? g.words.map((w, i) => <span key={i}>{t >= w.s && t < w.e ? <mark>{w.w}</mark> : w.w} </span>) : g.text;
  const lang = x.langOverride || detail?.transcript?.language || x.language || "";

  return (
    <div className="main">
      <div className="col grow">
        <div className="panel grow">
          <div className="panel-head"><a href="#" onClick={(e) => { e.preventDefault(); go("library", { sourceId: x.id, view: "reels" }); }} className="sec">Library</a><span className="muted">/</span><span className="ell">{x.title}</span><span className="grow" /><span className="sub">{x.meta?.width ? `${x.meta.height}p · ` : ""}{fmtLong(x.duration)} · {lang.toUpperCase()}</span></div>
          <div className="stage">
            {x.videoPath ? <video ref={video} src={fileUrl(x.videoPath)} style={{ maxWidth: "100%", maxHeight: "100%" }} onClick={toggle} /> : <div className="empty">Not downloaded yet.</div>}
            {cur >= 0 && <div className="cap" style={{ bottom: 32 }}><span className="line" style={{ fontSize: 22 }}>{words(segs[cur])}</span></div>}
          </div>
          <div className="transport">
            <span className="num" style={{ fontWeight: 600 }}>{fmt(t)} <span className="muted">/ {fmtLong(x.duration)}</span></span>
            <Btn icon onClick={() => seek(t - 5)} aria-label="Back 5 s">{I.prev}</Btn>
            <Btn icon primary onClick={toggle} aria-label="Play">{playing ? I.pause : I.play}</Btn>
            <Btn icon onClick={() => seek(t + 5)} aria-label="Forward 5 s">{I.next}</Btn>
            <span className="grow" />
            <Btn small onClick={() => tryAct("add_candidate", { sourceId: x.id, start: Math.max(0, t), end: t + 30, title: "Handpicked" }, "Reel marked from here (30 s)")}>Mark reel here</Btn>
            <Btn small primary onClick={() => cands.length ? go("reels", { sourceId: x.id }) : tryAct("run", { id: x.id, stage: "detect" }, "Finding reels")} disabled={!segs.length}>{cands.length ? `Reels · ${cands.length}` : "Find reels"}</Btn>
          </div>
        </div>
        <Panel title="Timeline" sub="click to seek · pink = a reel candidate" style={{ height: 96, flexShrink: 0 }}>
          <div className="range" style={{ height: 40 }} onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); seek(((e.clientX - r.left) / r.width) * (x.duration || 1)); }}>
            {cands.map((c) => <span key={c.id} className="sel" style={{ left: `${(c.start / (x.duration || 1)) * 100}%`, width: `${((c.end - c.start) / (x.duration || 1)) * 100}%`, borderWidth: 0 }} />)}
            <span className="cur" style={{ left: `${(t / (x.duration || 1)) * 100}%`, background: "#fff", width: 2 }} />
          </div>
          <div className="tl">{[0, .2, .4, .6, .8, 1].map((f) => <span key={f}>{fmtLong((x.duration || 0) * f)}</span>)}</div>
        </Panel>
      </div>
      <div className="col" style={{ width: 540, flexShrink: 0 }}>
        <div className="panel grow">
          <div className="panel-head"><span className="grow">Transcript</span><span className="sub">{detail?.transcript ? `${detail.transcript.engine} · ${segs.length} segments` : "none yet"}</span><Btn small onClick={() => detail?.transcript && tryAct("open", { path: x.folder + "/transcript.srt" })} disabled={!detail?.transcript}>Open .srt</Btn></div>
          <div className="scroll" style={{ padding: "6px 0", fontSize: 11.5 }}>
            {!segs.length && <div className="empty">{x.stage === "transcribing" ? "Transcribing…" : "No transcript yet."}</div>}
            {segs.map((g, i) => { const c = candAt(g); return (
              <div key={i} className={`transcript-line ${i === cur ? "on" : ""}`} onClick={() => seek(g.start)} style={c ? { boxShadow: "inset 3px 0 0 var(--accent)" } : undefined}>
                <span className="t">{fmtLong(g.start)}</span>
                <span>{i === cur ? words(g) : g.text}{c && i === segs.findIndex((h) => candAt(h)?.id === c.id) && <span className="pink" style={{ fontSize: 10.5, fontWeight: 600 }}> ■ {c.title} · score {c.score}</span>}</span>
              </div>
            ); })}
          </div>
        </div>
        <Panel title="Language and engine" style={{ flexShrink: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11.5 }}><span className="muted" style={{ width: 90 }}>Detected</span><span>{x.language ? `${(LANGS.find(([v]) => v === x.language) || [0, x.language])[1]}` : "not yet"}</span></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11.5 }}><span className="muted" style={{ width: 90 }}>Override</span><select className="input" style={{ width: 200 }} value={x.langOverride || "auto"} onChange={(e) => tryAct("set_language", { id: x.id, language: e.target.value, retranscribe: true }, "Transcribing again")}>{LANGS.map(([v, l]) => <option key={v} value={v}>{v === "auto" ? "Keep detected" : l}</option>)}</select><span className="hint">changes transcribe again</span></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11.5 }}><span className="muted" style={{ width: 90 }}>Engine</span><select className="input" style={{ width: 200 }} value={x.transcriptSource} onChange={(e) => tryAct("update_source", { id: x.id, patch: { transcriptSource: e.target.value } })}><option value="whisper">whisper {s.settings.whisperModel?.split("/").pop()} (MLX)</option><option value="captions">YouTube captions</option></select><Btn small onClick={() => tryAct("run", { id: x.id, stage: "transcribe" }, "Transcribing")}>Transcribe again</Btn></div>
          <div style={{ display: "flex", alignItems: "flex-start", gap: 8, fontSize: 11.5 }}><span className="muted" style={{ width: 90, paddingTop: 3 }}>Glossary</span><span style={{ display: "flex", flexWrap: "wrap", gap: 3 }}>{(s.settings.glossary || []).map((g) => <span key={g} className="tag">{g}</span>)}<a href="#" className="tag" onClick={(e) => { e.preventDefault(); go("settings"); }}>+ edit</a></span></div>
        </Panel>
      </div>
    </div>
  );
}
