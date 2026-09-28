import { useState } from "react";
import { useStore, tryAct, fmtLong, ago } from "../store.js";
import { Panel, Btn, Check, Text, Dot, Bar, I, Brand, useSize, Grip, Confirm } from "../ui.jsx";

export default function Inbox({ go, setAdding }) {
  // window.confirm never shows in the app's webview: the app's own dialog asks instead.
  const [askStop, setAskStop] = useState(null);
  const [sz0, setsz0] = useSize("inbox.left", 240);
  const s = useStore();
  const [selId, setSelId] = useState(null);
  const ch = s.channels.find((c) => c.id === selId) || s.channels[0];
  const items = s.inbox.filter((i) => !ch || i.channelId === ch.id).sort((a, b) => (b.uploadedAt || b.foundAt).localeCompare(a.uploadedAt || a.foundAt));
  const fresh = items.filter((i) => i.status === "new");
  const patch = (p) => ch && tryAct("update_channel", { id: ch.id, patch: p });
  const jobOf = (i) => i.sourceId && s.jobs.find((j) => j.refId === i.sourceId && (j.status === "running" || j.status === "queued"));
  const srcOf = (i) => s.sources.find((x) => x.id === i.sourceId);

  return (
    <div className="main">
      <div className="col" style={{ width: sz0, flexShrink: 0 }}>
        <div className="panel" style={{ flexShrink: 0 }}>
          <div className="panel-head"><span className="grow">Watched channels</span><Btn small icon primary onClick={() => setAdding(true)} aria-label="Watch a channel">{I.plus}</Btn></div>
          {s.channels.length === 0 && <div className="empty">No channels yet.</div>}
          {s.channels.map((c) => <div key={c.id} className={`nav-item ${ch?.id === c.id ? "on" : ""}`} style={{ padding: "6px 10px" }} onClick={() => setSelId(c.id)}><Brand id="youtube" mono={!c.enabled} style={{ opacity: c.enabled ? 1 : 0.5 }} /><span className="grow ell">{c.name}</span><span className={s.inbox.some((i) => i.channelId === c.id && i.status === "new") ? "pink" : "muted"}>{s.inbox.filter((i) => i.channelId === c.id && i.status === "new").length || ""}</span></div>)}
        </div>
        {ch && (
          <Panel title={`Rules · ${ch.name}`} className="grow" right={<><Btn small icon danger onClick={() => setAskStop(ch)} aria-label="Remove">{I.trash}</Btn>{askStop && <Confirm text={`Stop watching ${askStop.name}?`} yes="Stop watching" no="Keep" onYes={() => { tryAct("remove_channel", { id: askStop.id }); setAskStop(null); }} onNo={() => setAskStop(null)} />}</>}>
            <Check label="Enabled" checked={ch.enabled} onChange={(v) => patch({ enabled: v })} />
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13.5 }}><span className="muted">Check every</span><span>{ch.intervalH} h</span></div>
            <input type="range" className="slider" min="1" max="48" value={ch.intervalH} onChange={(e) => patch({ intervalH: +e.target.value })} />
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 13.5 }}><span className="muted">Minimum length</span><span>{Math.round(ch.minLenS / 60)} min</span></div>
            <input type="range" className="slider" min="0" max="3600" step="60" value={ch.minLenS} onChange={(e) => patch({ minLenS: +e.target.value })} />
            <label className="field"><span>Title must match (regex)</span><Text value={ch.titleRegex} onCommit={(v) => patch({ titleRegex: v })} placeholder="tafsir|seerah" /></label>
            <label className="field"><span>Only uploads after</span><Text value={ch.since} onCommit={(v) => patch({ since: v })} placeholder="2026-09-01" /></label>
            <div style={{ borderTop: "1px solid var(--rule)", paddingTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
              <Check label="Transcribe + find reels automatically" checked={ch.autoProcess} onChange={(v) => patch({ autoProcess: v })} />
              <Check label="Auto-approve score ≥ 8 and render" checked={ch.autoApproveScore > 0} onChange={(v) => patch({ autoApproveScore: v ? 8 : 0 })} />
              <Check label="Schedule without my review" checked={ch.autoSchedule} onChange={(v) => patch({ autoSchedule: v })} />
            </div>
            <span className="hint">{ch.url}<br />Last checked {ch.lastCheck ? ago(ch.lastCheck) : "never"}{ch.lastError && <span className="coral"> · {ch.lastError}</span>}<br />Nothing downloads until a rule or you say so.</span>
          </Panel>
        )}
      </div>
      <Grip value={sz0} set={setsz0} dir={1} reset={240} />
      <div className="col grow">
        <div className="panel" style={{ flexShrink: 0, maxHeight: "55%" }}>
          <div className="panel-head"><span>New uploads</span><span className="sub">{fresh.length}</span><span className="grow" /><Btn small onClick={() => tryAct("check_channel", ch ? { id: ch.id } : {}, "Checking")} disabled={!s.channels.length}>Check now</Btn><Btn small primary disabled={!fresh.length} onClick={async () => { for (const i of fresh) await tryAct("process_inbox", { id: i.id }); go("library", { sourceId: null }); }}>Process all</Btn></div>
          <div className="rows">
            {!fresh.length && <div className="empty">Nothing new{ch ? ` on ${ch.name}` : ""}.</div>}
            {fresh.map((i) => (
              <div key={i.id} className="row" style={{ gridTemplateColumns: "104px minmax(0,1fr) 200px", padding: "8px 10px", cursor: "default" }}>
                <img className="thumb" src={`https://i.ytimg.com/vi/${i.videoId}/mqdefault.jpg`} width={104} height={58} alt="" />
                <span style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}><b className="ell" style={{ fontSize: 14 }}>{i.title}</b><span className="muted">{s.channels.find((c) => c.id === i.channelId)?.name} · {i.uploadedAt ? ago(i.uploadedAt) : "date unknown"} · {fmtLong(i.duration)}</span></span>
                <span style={{ display: "flex", gap: 4, justifyContent: "flex-end" }}><Btn small primary onClick={async () => { const r = await tryAct("process_inbox", { id: i.id }, "Added to the library"); if (r) go("library", { sourceId: r.id, view: "reels" }); }}>Process</Btn><Btn small onClick={() => tryAct("skip_inbox", { id: i.id })}>Skip</Btn></span>
              </div>
            ))}
          </div>
        </div>
        <div className="panel grow">
          <div className="panel-head"><span className="grow">History</span><span className="sub">{items.length - fresh.length} seen</span></div>
          <div className="row head" style={{ gridTemplateColumns: "90px minmax(0,1fr) 110px 160px 60px" }}><span>Result</span><span>Name</span><span>When</span><span>Reason</span><span /></div>
          <div className="rows">
            {items.filter((i) => i.status !== "new").map((i) => { const j = jobOf(i); const src = srcOf(i); const st = i.status === "processed" ? (src?.stage === "failed" ? ["coral", "Failed"] : j ? ["pink", j.message || "Running"] : ["mint", "Processed"]) : i.status === "below_min" ? ["muted", "Below minimum"] : i.status === "filtered" ? ["muted", "Title filter"] : ["muted", "Skipped"]; return (
              <div key={i.id} className="row" style={{ gridTemplateColumns: "90px minmax(0,1fr) 110px 160px 60px", cursor: src ? "pointer" : "default" }} onClick={() => src && go("library", { sourceId: src.id, view: "reels" })}>
                <span className={st[0]} style={{ display: "flex", alignItems: "center", gap: 6 }}><Dot c={st[0]} />{st[1]}</span>
                <span className="ell">{i.title}</span>
                <span className="muted">{i.uploadedAt ? ago(i.uploadedAt) : ago(i.foundAt)}</span>
                <span className="muted ell">{src ? `${s.candidates.filter((c) => c.sourceId === src.id).length} reels` : i.status === "below_min" ? `${fmtLong(i.duration)} long` : ""}{j?.progress > 0 && <Bar v={j.progress} />}</span>
                <span>{i.status !== "processed" && <Btn small onClick={(e) => { e.stopPropagation(); tryAct("process_inbox", { id: i.id }, "Added"); }}>Process</Btn>}</span>
              </div>
            ); })}
          </div>
        </div>
      </div>
    </div>
  );
}
