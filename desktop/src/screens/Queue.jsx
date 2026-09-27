import { useState } from "react";
import { useStore, tryAct, ago } from "../store.js";
import { Panel, Btn, Dot, Bar, I, Menu, Confirm } from "../ui.jsx";

const COLS = "14px minmax(0,1.3fr) minmax(0,1fr) 90px 200px 150px 56px";
const GROUPS = [["running", "Running"], ["queued", "Queued"], ["failed", "Failed"], ["done", "Done"], ["cancelled", "Cancelled"]];

export default function Queue() {
  const s = useStore();
  const [selId, setSelId] = useState(null);
  const jobs = [...s.jobs].reverse();
  const j = jobs.find((v) => v.id === selId) || jobs.find((v) => v.status === "running") || jobs[0];
  const src = (v) => s.sources.find((x) => x.id === v.refId) || s.sources.find((x) => s.candidates.some((c) => c.id === v.refId && c.sourceId === x.id));
  const color = { running: "pink", queued: "", failed: "coral", done: "mint", cancelled: "muted" };
  const n = (st) => jobs.filter((v) => v.status === st).length;
  const [menu, setMenu] = useState(null); // { x, y, job }
  const [askStop, setAskStop] = useState(false);
  // The file a job stands for: the finished render, else the lecture's video.
  const fileOf = (v) => (v.kind === "render" && v.status === "done" ? v.message : src(v)?.previewPath || src(v)?.videoPath);
  const folderOf = (v) => fileOf(v) || src(v)?.folder;
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
                  <span className="ell">{v.label}</span>
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
      <div className="col" style={{ width: 380, flexShrink: 0 }}>
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
                {j.kind === "render" && j.status === "done" && <Btn onClick={() => tryAct("open", { path: j.message, reveal: true })}>Reveal in Finder</Btn>}
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
