import { useState } from "react";
import { useStore, tryAct, fileUrl, ago } from "../store.js";
import { Panel, Btn, Check, Text, Dot, Seg, I } from "../ui.jsx";

const CH = [["youtube", "YouTube Shorts"], ["instagram", "Instagram Reels"], ["tiktok", "TikTok"], ["facebook", "Facebook"]];
const dayKey = (d) => d.toISOString().slice(0, 10);
const localKey = (iso) => { const d = new Date(iso); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };
const hhmm = (iso) => new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

export default function Publish({ go }) {
  const s = useStore();
  const [week, setWeek] = useState(0);
  const [selId, setSelId] = useState(null);
  const [view, setView] = useState("week");
  const post = s.posts.find((p) => p.id === selId);
  const monday = new Date(); monday.setHours(0, 0, 0, 0); monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7) + week * 7);
  const days = [...Array(7)].map((_, i) => { const d = new Date(monday); d.setDate(d.getDate() + i); return d; });
  const todayKey = localKey(new Date().toISOString());
  const cand = (p) => s.candidates.find((c) => c.id === p.candidateId);
  const render = (p) => s.renders.find((r) => r.id === p.renderId);
  const linked = (c) => c === "youtube" ? !!s.settings.youtube?.refreshToken : false;
  const unscheduled = s.renders.filter((r) => r.status === "done" && r.format === "shorts" && !s.posts.some((p) => p.candidateId === r.candidateId));
  const patchPost = (p) => post && tryAct("update_post", { id: post.id, patch: p });
  const cadence = s.settings.cadence || {};

  return (
    <div className="main">
      <div className="col" style={{ width: 260, flexShrink: 0 }}>
        <Panel title="Channels" right={<Btn small onClick={() => go("settings")}>Link</Btn>} style={{ flexShrink: 0 }}>
          {CH.map(([id, label]) => <div key={id} style={{ display: "flex", flexDirection: "column", gap: 2, fontSize: 11.5 }}><div style={{ display: "flex", alignItems: "center", gap: 8 }}><Dot c={linked(id) ? "mint" : "muted"} /><b className={`grow ${linked(id) ? "" : "sec"}`}>{label}{id === "youtube" && s.settings.youtube?.channelTitle ? ` · ${s.settings.youtube.channelTitle}` : ""}</b></div><span className="muted" style={{ paddingLeft: 15 }}>{linked(id) ? `at ${(cadence[id] || []).map((h) => `${h}:00`).join(", ") || "no cadence"} · publishAt` : id === "youtube" ? "connect in Settings" : "export + caption.txt for now"}</span></div>)}
        </Panel>
        <Panel title="Cadence" style={{ flexShrink: 0 }}>
          {CH.map(([id, label]) => <div key={id} style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 11.5 }}><span className="muted" style={{ width: 90 }}>{label.split(" ")[0]}</span><Text value={(cadence[id] || []).join(", ")} onCommit={(v) => tryAct("settings", { patch: { cadence: { ...cadence, [id]: v.split(/[\s,]+/).map(Number).filter((n) => n >= 0 && n < 24) } } })} placeholder="17, 19" /><span className="muted">h</span></div>)}
          <Btn onClick={async () => { const n = await tryAct("autofill", { days: 14 }); if (n != null) tryAct("snapshot", {}, `${n} posts planned`); }}>Fill the next 14 days</Btn>
          <span className="hint">One reel per channel per hour listed, oldest approved first. Planned posts wait for your confirm; YouTube uploads as private with publishAt.</span>
        </Panel>
        <Panel title={`Unscheduled · ${unscheduled.length}`} className="grow">
          {unscheduled.slice(0, 20).map((r) => { const c = s.candidates.find((c) => c.id === r.candidateId); return <div key={r.id} className="ell" style={{ fontSize: 11, color: "var(--text-2)", cursor: "pointer" }} onClick={() => { const at = new Date(); at.setDate(at.getDate() + 1); at.setHours(cadence.youtube?.[0] ?? 17, 0, 0, 0); tryAct("schedule", { candidateId: r.candidateId, channel: "youtube", at: at.toISOString() }, "Planned for tomorrow"); }}>{c?.title || r.candidateId}</div>; })}
          {!unscheduled.length && <span className="hint">Every rendered reel is on the calendar.</span>}
        </Panel>
      </div>
      <div className="col grow">
        <div className="panel grow">
          <div className="panel-head"><span>Calendar</span><span className="sub">{days[0].toLocaleDateString([], { day: "numeric", month: "short" })} – {days[6].toLocaleDateString([], { day: "numeric", month: "short", year: "numeric" })}</span><span className="grow" /><Seg value={view} onChange={setView} options={[["week", "Week"], ["list", "List"]]} /><Btn small icon onClick={() => setWeek(week - 1)} aria-label="Previous week">{I.prev}</Btn><Btn small onClick={() => setWeek(0)}>Today</Btn><Btn small icon onClick={() => setWeek(week + 1)} aria-label="Next week">{I.next}</Btn></div>
          {view === "week" ? (
            <div className="cal">
              {days.map((d) => { const k = localKey(d.toISOString()); const ps = s.posts.filter((p) => localKey(p.scheduledAt) === k).sort((a, b) => a.scheduledAt.localeCompare(b.scheduledAt)); return (
                <div key={k} className={`day ${k === todayKey ? "today" : ""}`}>
                  <div className="day-head"><span>{d.toLocaleDateString([], { weekday: "short" })}</span><span style={{ fontWeight: 500 }}>{d.getDate()}</span></div>
                  <div className="day-body">{ps.map((p) => <div key={p.id} className={`post ${p.status} ${post?.id === p.id ? "on" : ""}`} onClick={() => setSelId(p.id)}><span className="t"><span>{hhmm(p.scheduledAt)}</span><span className="muted">{p.channel === "youtube" ? "YT" : p.channel === "instagram" ? "IG" : p.channel === "tiktok" ? "TT" : "FB"}{p.status === "posted" ? " ✓" : p.status === "failed" ? " ✕" : ""}</span></span><span className="ell">{p.title || "(poster)"}</span></div>)}</div>
                </div>
              ); })}
            </div>
          ) : (
            <div className="rows">{[...s.posts].sort((a, b) => a.scheduledAt.localeCompare(b.scheduledAt)).map((p) => <div key={p.id} className={`row ${post?.id === p.id ? "on" : ""}`} style={{ gridTemplateColumns: "130px minmax(0,1fr) 100px 120px" }} onClick={() => setSelId(p.id)}><span className="muted">{new Date(p.scheduledAt).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}</span><span className="ell">{p.title}</span><span>{p.channel}</span><span className={p.status === "posted" ? "mint" : p.status === "failed" ? "coral" : p.status === "confirmed" ? "pink" : "cyan"}>{p.status}</span></div>)}</div>
          )}
          <div style={{ height: 26, display: "flex", alignItems: "center", gap: 14, padding: "0 10px", borderTop: "1px solid var(--rule)", fontSize: 10.5, color: "var(--muted)" }}><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--mint)", marginRight: 5 }} />posted</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--accent)", marginRight: 5 }} />confirmed</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--cyan)", marginRight: 5 }} />planned, not confirmed</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--coral)", marginRight: 5 }} />failed</span></div>
        </div>
        <Panel title="Deliveries" sub="what went out, and what did not" style={{ height: 170, flexShrink: 0 }} body={false}>
          <div className="rows">{s.posts.filter((p) => p.status === "posted" || p.status === "failed").sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 30).map((p) => <div key={p.id} className="row" style={{ gridTemplateColumns: "70px minmax(0,1fr) 110px 110px minmax(0,1fr)", minHeight: 26 }} onClick={() => setSelId(p.id)}><span className="muted">{ago(p.updatedAt)}</span><span className="ell">{p.title}</span><span>{p.channel}</span><span className={p.status === "posted" ? "mint" : "coral"}>{p.status}</span><span className="mono ell muted">{p.providerId ? (p.channel === "youtube" ? `youtu.be/${p.providerId}` : p.providerId) : p.error}</span></div>)}</div>
        </Panel>
      </div>
      <div className="panel" style={{ width: 300, flexShrink: 0 }}>
        <div className="panel-head"><span className="grow">Post</span>{post && <span className={post.status === "posted" ? "mint" : post.status === "failed" ? "coral" : "pink"} style={{ fontWeight: 500 }}>{post.status}</span>}</div>
        {!post ? <div className="empty">Pick a post on the calendar, or click an unscheduled reel to plan it.</div> : (
          <>
            <div className="panel-body">
              <div style={{ display: "flex", gap: 8 }}>{render(post)?.coverPath && <img src={fileUrl(render(post).coverPath)} alt="" style={{ width: 64, height: 114, objectFit: "cover", borderRadius: 3, background: "var(--head)" }} />}<div style={{ display: "flex", flexDirection: "column", gap: 3, fontSize: 11.5, minWidth: 0 }}><b>{post.title}</b>{cand(post) && <span className="muted">{(cand(post).end - cand(post).start).toFixed(1)} s · score {cand(post).score} · {cand(post).category}</span>}<span className={render(post) ? "mint" : "coral"}>{render(post) ? `${render(post).format}.mp4 rendered` : "no render yet"}</span>{post.providerId && <a href={post.channel === "youtube" ? `https://youtu.be/${post.providerId}` : "#"}>{post.providerId}</a>}{post.error && <span className="coral">{post.error}</span>}</div></div>
              <label className="field"><span>Title</span><Text value={post.title} onCommit={(v) => patchPost({ title: v })} /></label>
              <label className="field"><span>Caption</span><Text area rows={5} value={post.caption} onCommit={(v) => patchPost({ caption: v })} /></label>
              <div className="grid2">
                <label className="field"><span>Channel</span><select className="input" value={post.channel} onChange={(e) => patchPost({ channel: e.target.value })}>{CH.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select></label>
                <label className="field"><span>When (local)</span><input className="input" type="datetime-local" value={new Date(new Date(post.scheduledAt).getTime() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16)} onChange={(e) => e.target.value && patchPost({ scheduledAt: new Date(e.target.value).toISOString() })} /></label>
              </div>
              <span className="hint">{post.channel === "youtube" ? `Uploads now as ${s.settings.youtube?.privacy || "public"}; when the time is in the future it goes up private with publishAt.` : "Not linked: the render and caption.txt are in the reel folder for a manual upload."}</span>
            </div>
            <span className="grow" />
            <div className="panel-foot">
              {post.status !== "posted" && <Btn primary className="grow" onClick={() => tryAct("confirm", { id: post.id }, "Confirmed")} disabled={post.status === "confirmed" || post.status === "posting"}>{post.status === "confirmed" ? "Confirmed" : post.status === "failed" ? "Try again" : "Confirm"}</Btn>}
              {post.status !== "posted" && <Btn onClick={() => tryAct("publish_now", { id: post.id }, "Publishing")}>Post now</Btn>}
              <Btn danger icon onClick={() => tryAct("unschedule", { id: post.id }) && setSelId(null)} aria-label="Remove">{I.trash}</Btn>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
