import { useEffect, useState } from "react";
import { useStore, tryAct, fileUrl, fmtLong, ago, STAGES } from "../store.js";
import { Panel, Btn, Dot, Bar, I, Brand, FileMark } from "../ui.jsx";

export default function Library({ nav, go, query, setAdding }) {
  const s = useStore();
  const [filter, setFilter] = useState("all");
  const [fts, setFts] = useState(null);
  const sel = s.sources.find((x) => x.id === nav.sourceId) || null;

  const cands = (id) => s.candidates.filter((c) => c.sourceId === id);
  const jobOf = (id) => s.jobs.find((j) => j.refId === id && (j.status === "running" || j.status === "queued"));

  // Title search locally; transcript search through the core's FTS (debounced by the effect below).
  useEffect(() => {
    if (!query.trim()) { setFts(null); return; }
    const t = setTimeout(async () => setFts(await tryAct("search", { query }) || []), 200);
    return () => clearTimeout(t);
  }, [query]);

  const shown = s.sources.filter((x) => {
    if (query.trim() && !x.title.toLowerCase().includes(query.toLowerCase()) && !(fts || []).includes(x.id)) return false;
    if (filter === "review") return cands(x.id).some((c) => !c.approved && !c.discarded) && x.stage === "review";
    if (filter === "progress") return !!jobOf(x.id);
    if (filter === "published") return s.posts.some((p) => p.status === "posted" && cands(x.id).some((c) => c.id === p.candidateId));
    if (filter === "failed") return x.stage === "failed";
    if (filter.startsWith("ch:")) return x.channelId === filter.slice(3);
    if (filter === "kind:youtube") return x.kind === "youtube";
    if (filter === "kind:file") return x.kind === "file";
    return true;
  }).sort((a, b) => (b.updatedAt || "").localeCompare(a.updatedAt || ""));

  const count = (f) => s.sources.filter((x) => f(x)).length;
  const langs = Object.entries(s.sources.reduce((m, x) => { const l = (x.langOverride || x.language || "?").toUpperCase(); m[l] = (m[l] || 0) + 1; return m; }, {}));

  return (
    <div className="main">
      <div className="panel" style={{ width: 220, flexShrink: 0 }}>
        <div className="panel-head"><span className="grow">Sources</span><Btn small icon primary onClick={() => setAdding(true)} aria-label="Add source">{I.plus}</Btn></div>
        <div className="scroll" style={{ padding: "6px 0" }}>
          {s.channels.length > 0 && <div className="nav-group">Watched channels</div>}
          {s.channels.map((c) => <div key={c.id} className={`nav-item ${filter === "ch:" + c.id ? "on" : ""}`} onClick={() => setFilter("ch:" + c.id)}><Brand id="youtube" mono={!c.enabled} style={{ opacity: c.enabled ? 1 : 0.5 }} /><span className="grow ell">{c.name}</span><span className="muted">{count((x) => x.channelId === c.id)}</span></div>)}
          <div className="nav-group">Added by hand</div>
          <div className={`nav-item ${filter === "kind:youtube" ? "on" : ""}`} onClick={() => setFilter("kind:youtube")}><Brand id="youtube" /><span className="grow">YouTube links</span><span className="muted">{count((x) => x.kind === "youtube")}</span></div>
          <div className={`nav-item ${filter === "kind:file" ? "on" : ""}`} onClick={() => setFilter("kind:file")}><FileMark /><span className="grow">Local files</span><span className="muted">{count((x) => x.kind === "file")}</span></div>
          <div className="nav-group">Filters</div>
          {[["all", "Everything", count(() => true)], ["review", "Needs review", count((x) => x.stage === "review" && cands(x.id).some((c) => !c.approved && !c.discarded))], ["progress", "In progress", count((x) => !!jobOf(x.id))], ["published", "Published", count((x) => s.posts.some((p) => p.status === "posted" && cands(x.id).some((c) => c.id === p.candidateId)))], ["failed", "Failed", count((x) => x.stage === "failed")]].map(([k, l, n]) => (
            <div key={k} className={`nav-item ${filter === k ? "on" : ""}`} onClick={() => setFilter(k)}><span className="grow">{l}</span><span className={k === "review" ? "pink" : k === "failed" && n ? "coral" : "muted"}>{n}</span></div>
          ))}
          {langs.length > 0 && <div className="nav-group">Language</div>}
          <div style={{ display: "flex", flexWrap: "wrap", gap: 4, padding: "2px 10px 6px 18px" }}>{langs.map(([l, n]) => <span key={l} className="tag">{l} {n}</span>)}</div>
        </div>
        <span className="grow" />
        <div className="panel-foot" style={{ flexDirection: "column", alignItems: "stretch", gap: 4, fontSize: 12, color: "var(--muted)" }}>
          <span className="ell">{s.paths.outDir}</span>
          <span>{s.sources.length} sources · {s.candidates.length} reels · {s.posters.length} posters</span>
        </div>
      </div>

      <div className="col grow">
        <div className="panel grow">
          <div className="panel-head"><span>Library</span><span className="sub">{shown.length} sources</span><span className="grow" /><Btn small onClick={() => sel && tryAct("run", { id: sel.id })} disabled={!sel}>Run next stage on selected</Btn></div>
          <div className="row head" style={{ gridTemplateColumns: "48px minmax(0,1fr) 46px 200px 90px 80px" }}><span /><span>Name</span><span>Lang</span><span>Stage</span><span>Reels</span><span>Updated</span></div>
          <div className="rows">
            {shown.length === 0 && <div className="empty">Nothing here yet. Add a YouTube link, watch a channel, or drop a file (⌘N).</div>}
            {shown.map((x) => {
              const [c, label] = STAGES[x.stage] || ["muted", x.stage];
              const j = jobOf(x.id);
              const n = cands(x.id).length, ok = cands(x.id).filter((c) => c.approved).length;
              return (
                <div key={x.id} className={`row ${sel?.id === x.id ? "on" : ""}`} style={{ gridTemplateColumns: "48px minmax(0,1fr) 46px 200px 90px 80px" }} onClick={() => go("library", { sourceId: x.id, view: "reels" })} onDoubleClick={() => go(x.stage === "review" || n ? "reels" : "library", { sourceId: x.id, view: n ? "reels" : "transcript" })}>
                  {x.thumbPath ? <img className="thumb" src={fileUrl(x.thumbPath)} width={44} height={25} alt="" /> : <span className="thumb" style={{ width: 44, height: 25, display: "inline-flex", alignItems: "center", justifyContent: "center", color: "var(--muted)" }}>{x.kind === "file" ? <FileMark size={16} /> : <Brand id="youtube" size={16} />}</span>}
                  <span className="ell">{x.title} <span className="muted">· {fmtLong(x.duration)}</span></span>
                  <span>{(x.langOverride || x.language || "auto").toUpperCase()}</span>
                  <span style={{ display: "flex", alignItems: "center", gap: 6 }}><Dot c={c} /><span className={c === "coral" ? "coral" : ""}>{j ? (j.status === "queued" ? "Queued · " : "") + (j.message || label) : x.stage === "review" ? `Review · ${ok} of ${n} ticked` : x.stage === "failed" ? "Failed" : label}</span>{j && j.status === "running" && j.progress > 0 && <Bar v={j.progress} />}</span>
                  <span className="num">{n ? <>{n} <span className={ok ? "mint" : "muted"}>{ok}</span></> : <span className="muted">—</span>}</span>
                  <span className="muted">{ago(x.updatedAt)}</span>
                </div>
              );
            })}
          </div>
        </div>
        <div style={{ height: 180, flexShrink: 0, display: "flex", gap: 10 }}>
          <Panel title="Activity" className="grow" body={false}>
            <div className="scroll" style={{ padding: "6px 10px", fontFamily: "var(--mono)", fontSize: 12, color: "var(--text-2)", lineHeight: 1.5 }}>
              {[...s.jobs].reverse().slice(0, 40).map((j) => <div key={j.id}><span className="muted">{(j.startedAt || j.createdAt || "").slice(11, 19)}</span> <span className={j.status === "failed" ? "coral" : j.status === "done" ? "mint" : "pink"}>{j.kind.padEnd(13)}</span> {j.label} · {j.message || j.status}</div>)}
            </div>
          </Panel>
          <Panel title="Engines" style={{ width: 300, flexShrink: 0 }}>
            <div className="grid2" style={{ fontSize: 12 }}>
              {[["ffmpeg", s.tools.ffmpeg && !s.tools.ffmpegAss ? "ffmpeg (no libass!)" : "ffmpeg"], ["ytdlp", "yt-dlp"], ["whisper", "whisper (MLX)"], ["claude", "claude CLI"], ["ollama", "ollama"], ["uv", "uv"]].map(([k, l]) => <span key={k} style={{ display: "flex", alignItems: "center", gap: 6 }}><Dot c={s.tools[k] ? "mint" : "muted"} /><span className={s.tools[k] ? "" : "muted"}>{l}</span></span>)}
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}><Brand id="youtube" mono={!s.settings.youtube?.refreshToken} /><span className={s.settings.youtube?.refreshToken ? "" : "muted"}>YouTube · {s.settings.youtube?.channelTitle || "not linked"}</span></span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}><Brand id="tiktok" mono /><Brand id="instagram" mono /><span className="muted">not linked</span></span>
            </div>
            <span className="hint">MCP {s.mcp ? `on ${s.mcp.url}` : "off"} · <a href="#" onClick={(e) => { e.preventDefault(); go("settings"); }}>settings</a></span>
          </Panel>
        </div>
      </div>

      <div className="panel" style={{ width: 300, flexShrink: 0 }}>
        <div className="panel-head"><span className="grow">Info</span>{sel && <><Btn small icon onClick={() => tryAct("open", { path: sel.folder })} aria-label="Reveal in Finder">{I.folder}</Btn><Btn small icon onClick={() => confirm(`Remove "${sel.title}" from the library? Files stay on disk.`) && tryAct("remove_source", { id: sel.id }) && go("library", { sourceId: null })} aria-label="Remove">{I.trash}</Btn></>}</div>
        {!sel ? <div className="empty">Select a source.</div> : <SourceInfo x={sel} s={s} go={go} />}
      </div>
    </div>
  );
}

function SourceInfo({ x, s, go }) {
  const cands = s.candidates.filter((c) => c.sourceId === x.id);
  const renders = s.renders.filter((r) => r.sourceId === x.id && r.status === "done");
  const posts = s.posts.filter((p) => cands.some((c) => c.id === p.candidateId));
  const t = s.jobs.filter((j) => j.refId === x.id);
  const last = (kind) => [...t].reverse().find((j) => j.kind === kind);
  const step = (kind, label, done, extra) => {
    const j = last(kind);
    const c = done ? "mint" : j?.status === "running" || j?.status === "queued" ? "pink" : j?.status === "failed" ? "coral" : "";
    return <div style={{ display: "flex", alignItems: "center", gap: 8 }}><Dot c={c} /><span style={{ width: 74, color: done || c ? "var(--text-2)" : "var(--muted)" }}>{label}</span><span className={c === "coral" ? "coral ell" : "muted ell"}>{j?.status === "failed" ? j.message : j?.status === "running" ? j.message || "running" : j?.status === "queued" ? "queued" : extra || (done ? "done" : "waiting")}</span></div>;
  };
  return (
    <>
      <div className="panel-body" style={{ fontSize: 12.5 }}>
        {x.thumbPath && <img src={fileUrl(x.thumbPath)} alt="" style={{ width: "100%", aspectRatio: "16/9", objectFit: "cover", borderRadius: 4, background: "var(--head)" }} />}
        <b style={{ fontSize: 13 }}>{x.title}</b>
        <div className="kv">
          <span>Channel</span><span className="ell">{x.channel || "—"}</span>
          <span>Source</span><span className="ell">{x.url || x.path}</span>
          <span>Video</span><span>{x.meta?.width ? `${x.meta.width}×${x.meta.height} · ` : ""}{fmtLong(x.duration)}</span>
          <span>Language</span><span>{x.langOverride ? `${x.langOverride.toUpperCase()} (override)` : x.language ? `${x.language.toUpperCase()} (auto)` : "auto"} · <a href="#" onClick={(e) => { e.preventDefault(); go("library", { sourceId: x.id, view: "transcript" }); }}>change</a></span>
          <span>Folder</span><span className="ell">{x.folder}</span>
          {x.error && <><span>Error</span><span className="coral">{x.error}</span></>}
        </div>
        <div style={{ borderTop: "1px solid var(--rule)", paddingTop: 8, display: "flex", flexDirection: "column", gap: 5 }}>
          {step("download", "Download", !!x.videoPath)}
          {step("transcribe", "Transcript", ["transcribed", "detecting", "review", "done"].includes(x.stage) || cands.length > 0)}
          {step("detect", "Find reels", cands.length > 0, cands.length ? `${cands.length} candidates` : "")}
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><Dot c={cands.some((c) => c.approved) ? (cands.every((c) => c.approved || c.discarded) ? "mint" : "pink") : x.stage === "review" ? "pink" : ""} /><span style={{ width: 74, color: "var(--text-2)" }}>Review</span><span className={x.stage === "review" ? "pink" : "muted"}>{cands.length ? `${cands.filter((c) => c.approved).length} of ${cands.length} ticked` : "waiting"}</span></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><Dot c={renders.length ? "mint" : ""} /><span style={{ width: 74, color: "var(--text-2)" }}>Render</span><span className="muted">{renders.length ? `${renders.length} files` : "waiting"}</span></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}><Dot c={posts.some((p) => p.status === "posted") ? "mint" : ""} /><span style={{ width: 74, color: "var(--text-2)" }}>Publish</span><span className="muted">{posts.length ? `${posts.filter((p) => p.status === "posted").length} out · ${posts.filter((p) => p.status !== "posted").length} planned` : "waiting"}</span></div>
        </div>
      </div>
      <span className="grow" />
      <div className="panel-foot">
        <Btn primary className="grow" onClick={() => go("reels", { sourceId: x.id })} disabled={!cands.length}>Review reels</Btn>
        <Btn onClick={() => go("library", { sourceId: x.id, view: "transcript" })} disabled={!x.videoPath}>Transcript</Btn>
        <Btn onClick={() => tryAct("run", { id: x.id, stage: x.videoPath ? (s.transcripts ? "detect" : "transcribe") : "download" }, "Queued")}>{x.stage === "failed" ? "Retry" : "Run"}</Btn>
      </div>
    </>
  );
}
