import { useState } from "react";
import { useStore, tryAct, ago, FORMAT_BRAND } from "../store.js";
import { Panel, Btn, Dot, Bar, I, Menu, Confirm, useSize, Grip, Brand, FileMark } from "../ui.jsx";

const COLS = "14px minmax(0,1.3fr) minmax(0,1fr) 90px 200px 150px 56px";
const GROUPS = [["running", "Running"], ["queued", "Queued"], ["failed", "Failed"], ["done", "Done"], ["cancelled", "Cancelled"]];

/* What a job makes, as a service mark: a render by its format, a publish by its post's service. */
function JobMark({ v, posts }) {
  const f = v.kind === "render" ? v.args?.format : null;
  const brand = f ? FORMAT_BRAND[f] : v.kind === "publish" ? posts.find((p) => p.id === v.refId)?.channel : null;
  if (brand === "folder") return <span style={{ color: "var(--sec)", display: "flex" }}>{I.folder}</span>;
  if (brand) return <Brand id={brand} size={13} />;
  if (f === "clean") return <FileMark size={13} />;
  const d = KIND_ICONS[v.kind];
  if (d) return <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0, color: "var(--sec)" }} aria-hidden="true">{d}</svg>;
  return <span style={{ width: 13, flexShrink: 0 }} />;
}

/* Marks for the jobs that make no file for a service (stroke icons, 24-unit box). */
const KIND_ICONS = {
  music: <><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></>,
  loudness: <><path d="M11 5 6 9H2v6h4l5 4z" /><path d="M15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14" /></>,
  download: <><path d="M12 3v12M7 10l5 5 5-5" /><path d="M5 21h14" /></>,
  transcribe: <><rect x="9" y="2" width="6" height="12" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v4" /></>,
  detect: <><path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" /><path d="M19 17l.7 1.8 1.8.7-1.8.7L19 22l-.7-1.8-1.8-.7 1.8-.7z" /></>,
  polish: <><path d="M4 20h4L19 9l-4-4L4 16z" /><path d="M13.5 6.5l4 4" /></>,
  punchline: <><path d="M4 11h5v7H4zM4 11c0-4 2-6 5-7M14 11h5v7h-5zM14 11c0-4 2-6 5-7" /></>,
  poster: <><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="M21 15l-5-5L5 21" /></>,
  endcards: <><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="M21 15l-5-5L5 21" /></>,
  waiting_video: <><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></>,
  check_channel: <><path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" /></>,
};

export default function Queue() {
  const [sz0, setsz0] = useSize("queue.right", 380);
  const s = useStore();
  const [selId, setSelId] = useState(null);
  const jobs = [...s.jobs].reverse();
  const j = jobs.find((v) => v.id === selId) || jobs.find((v) => v.status === "running") || jobs[0];
  // The lecture a job belongs to: itself, its reel's, or its post's reel's.
  const src = (v) => { const cid = s.posts.find((p) => p.id === v.refId)?.candidateId || v.refId; return s.sources.find((x) => x.id === v.refId) || s.sources.find((x) => s.candidates.some((c) => c.id === cid && c.sourceId === x.id)); };
  const color = { running: "pink", queued: "", failed: "coral", done: "mint", cancelled: "muted" };
  const n = (st) => jobs.filter((v) => v.status === st).length;
  const [menu, setMenu] = useState(null); // { x, y, job }
  const [askStop, setAskStop] = useState(false);
  // The file a job stands for: the finished render, else the lecture's video.
  // What a job made (or is making): the render's file, the post's file, the poster's folder, the
  // music folder. Only jobs about the lecture itself point at its video.
  const renderOf = (cid, format) => s.renders.filter((r) => r.candidateId === cid && r.format === format).sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
  const outOf = (v) => {
    if (v.kind === "render") return (v.status === "done" && v.message) || renderOf(v.refId, v.args?.format)?.path;
    if (v.kind === "publish") { const p = s.posts.find((x) => x.id === v.refId); return s.renders.find((r) => r.id === p?.renderId)?.path; }
    if (["poster", "endcards", "waiting_video"].includes(v.kind)) return s.posters.find((x) => x.id === v.refId)?.folder;
    if (v.kind === "music") return s.paths.dataDir && `${s.paths.dataDir}/music`;
    return null;
  };
  const fileOf = (v) => (v.kind === "render" || v.kind === "publish" ? outOf(v) : src(v)?.previewPath || src(v)?.videoPath);
  const folderOf = (v) => outOf(v) || src(v)?.videoPath || src(v)?.folder;
  const menuItems = (v) => [
    { label: "Play", disabled: !fileOf(v), onClick: () => tryAct("open", { path: fileOf(v) }) },
    { label: "Open in Finder", disabled: !folderOf(v), onClick: () => tryAct("open", { path: folderOf(v), reveal: true }) },
    ...(v.status === "failed" ? [{ label: "Retry", onClick: () => tryAct("retry_job", { id: v.id }) }] : []),
    ...(v.status === "running" || v.status === "queued" ? [{ label: "Cancel", onClick: () => tryAct("cancel_job", { id: v.id }) }] : []),
    { label: "Clear", danger: true, onClick: () => tryAct("remove_job", { id: v.id }) },
  ];

  return (
    <div className="main">
      {menu && <Menu at={menu} items={menuItems(menu.job)} onClose={() => setMenu(null)} />}
      {askStop && <Confirm text={`Stop ${n("running") + n("queued")} jobs? Running ones stop now. Files already made stay.`} yes="Stop all" no="Keep" onYes={() => { setAskStop(false); tryAct("cancel_jobs", {}, "Stopped"); }} onNo={() => setAskStop(false)} />}
      <div className="panel grow">
        <div className="panel-head"><span>Jobs</span><span className="sub">{s.settings.queuePaused && <b className="coral">paused · </b>}{n("running")} running · {n("queued")} queued · {n("failed")} failed · {n("done")} done</span><span className="grow" /><Btn small onClick={() => jobs.filter((v) => v.status === "failed").forEach((v) => tryAct("retry_job", { id: v.id }))} disabled={!n("failed")}>Retry failed</Btn>{s.settings.queuePaused ? <Btn small primary onClick={() => tryAct("pause_queue", { paused: false }, "Queue running")}>Resume</Btn> : <Btn small onClick={() => tryAct("pause_queue", { paused: true }, "Queue paused. Running jobs finish.")}>Pause</Btn>}<Btn small danger onClick={() => setAskStop(true)} disabled={!n("running") && !n("queued")}>Cancel all</Btn><Btn small onClick={() => tryAct("clear_jobs", {})}>Clear finished</Btn></div>
        <div className="row head" style={{ gridTemplateColumns: COLS }}><span /><span>Job</span><span>Source</span><span>Kind</span><span>Progress</span><span>Status</span><span /></div>
        <div className="rows">
          {!jobs.length && <div className="empty">Nothing queued.</div>}
          {GROUPS.map(([st, label]) => { const g = jobs.filter((v) => v.status === st); if (!g.length) return null; return (
            <div key={st}>
              <div className="group-row"><b>{label}</b><span>{g.length}{st === "running" ? ` of ${s.settings.parallelJobs || 2} at a time` : ""}</span></div>
              {g.slice(0, st === "done" ? 60 : 200).map((v) => (
                <div key={v.id} className={`row ${j?.id === v.id ? "on" : ""}`} style={{ gridTemplateColumns: COLS }} onClick={() => setSelId(v.id)} onContextMenu={(e) => { e.preventDefault(); setSelId(v.id); setMenu({ x: e.clientX, y: e.clientY, job: v }); }}>
                  <Dot c={color[v.status]} />
                  <span style={{ display: "flex", alignItems: "center", gap: 7, minWidth: 0 }}><JobMark v={v} posts={s.posts} /><span className="ell">{v.label}</span></span>
                  <span className="sec ell">{src(v)?.title || v.refId}</span>
                  <span className="sec">{v.kind}</span>
                  <span style={{ display: "flex", alignItems: "center", gap: 6 }}>{v.status === "running" || v.status === "done" ? <><Bar v={v.status === "done" ? 1 : v.progress} mint={v.status === "done"} /><span className="num" style={{ width: 32, textAlign: "right" }}>{Math.round((v.status === "done" ? 1 : v.progress) * 100)}%</span></> : <span className="muted">—</span>}</span>
                  <span className={`${color[v.status]} ell`}>{v.message || v.status}</span>
                  <span style={{ display: "flex", gap: 3, justifyContent: "flex-end" }}>{(v.status === "running" || v.status === "queued") && <Btn small icon onClick={(e) => { e.stopPropagation(); tryAct("cancel_job", { id: v.id }); }} aria-label="Cancel">{I.x}</Btn>}{v.status === "failed" && <Btn small onClick={(e) => { e.stopPropagation(); tryAct("retry_job", { id: v.id }); }}>Retry</Btn>}</span>
                </div>
              ))}
            </div>
          ); })}
        </div>
      </div>
      <Grip value={sz0} set={setsz0} dir={-1} reset={380} />
      <div className="col" style={{ width: sz0, flexShrink: 0 }}>
        <div className="panel grow">
          <div className="panel-head"><span className="grow">Job</span>{j && <span className={color[j.status]} style={{ fontWeight: 500 }}>{j.status}</span>}</div>
          {!j ? <div className="empty">Select a job.</div> : (
            <>
              <div className="panel-body" style={{ flexGrow: 1 }}>
                <b style={{ fontSize: 14 }}>{j.label}</b>
                <div className="kv"><span>Kind</span><span>{j.kind}</span><span>Ref</span><span className="ell">{j.refId}</span><span>Created</span><span>{ago(j.createdAt)}</span>{j.startedAt && <><span>Started</span><span>{j.startedAt.slice(11, 19)}</span></>}{j.finishedAt && <><span>Finished</span><span>{j.finishedAt.slice(11, 19)}</span></>}{Object.keys(j.args || {}).length > 0 && <><span>Args</span><span className="mono ell">{JSON.stringify(j.args)}</span></>}</div>
                {(j.status === "running") && <div style={{ display: "flex", flexDirection: "column", gap: 4 }}><div style={{ display: "flex", justifyContent: "space-between", fontSize: 13 }}><span className="muted">Progress</span><span>{Math.round(j.progress * 100)}%</span></div><span className="bar" style={{ height: 6 }}><i style={{ width: `${Math.round(j.progress * 100)}%` }} /></span></div>}
                <span className={`${color[j.status]}`} style={{ fontSize: 13.5, lineHeight: 1.45 }}>{j.message}</span>
                <span className="label">Log</span>
                <div className="log" style={{ flexGrow: 1, minHeight: 120 }}>{j.log || "—"}</div>
              </div>
              <div className="panel-foot">
                {folderOf(j) && <Btn onClick={() => tryAct("open", { path: folderOf(j), reveal: true })}>Reveal in Finder</Btn>}
                {j.status === "failed" && <Btn primary onClick={() => tryAct("retry_job", { id: j.id })}>Retry</Btn>}
                <span className="grow" />
                {(j.status === "running" || j.status === "queued") && <Btn danger onClick={() => tryAct("cancel_job", { id: j.id })}>Cancel</Btn>}
              </div>
            </>
          )}
        </div>
        <Panel title="This Mac" style={{ flexShrink: 0 }}>
          <div className="kv"><span>Slots</span><span>{s.settings.parallelJobs || 2} at a time, one transcript at a time</span><span>Whisper</span><span className="ell">{s.settings.whisperModel}</span><span>Brain</span><span>{s.settings.brain === "ollama" ? `ollama · ${s.settings.ollamaModel}` : "claude CLI"}</span><span>Output</span><span className="ell">{s.paths.outDir}</span><span>Log</span><span className="ell"><a href="#" onClick={(e) => { e.preventDefault(); tryAct("open", { path: s.paths.log }); }}>{s.paths.log}</a></span></div>
        </Panel>
      </div>
    </div>
  );
}
