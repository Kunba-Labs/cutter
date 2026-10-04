import { useEffect, useRef, useState } from "react";
import { useStore, tryAct, fileUrl, ago } from "../store.js";
import { Panel, Btn, Check, Text, Dot, Seg, I, Brand, CHANNEL_NAMES, useSize, Grip, Confirm } from "../ui.jsx";

const CH = [["youtube", "YouTube Shorts"], ["instagram", "Instagram Reels"], ["tiktok", "TikTok"], ["facebook", "Facebook"]];
const dayKey = (d) => d.toISOString().slice(0, 10);
const localKey = (iso) => { const d = new Date(iso); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };
const hhmm = (iso) => new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
const HOUR = 56; // px per hour on the grid
const GUTTER = 52;

/* What a post is, as shown: a YouTube post uploaded ahead of its time sits private until YouTube
   publishes it, so it reads "scheduled on YouTube" until then and "posted" once it is live. */
const shown = (p) => (p.status === "posted" && p.channel === "youtube" && new Date(p.scheduledAt) > new Date() ? "scheduled" : p.status);
const LABEL = { scheduled: "scheduled on YouTube" };
const tone = (st) => ({ posted: "mint", failed: "coral", confirmed: "pink", scheduled: "tiffany-text" })[st] || "cyan";

export default function Publish({ go }) {
  const [sz0, setsz0] = useSize("publish.left", 260);
  const [sz1, setsz1] = useSize("publish.right", 300);
  const s = useStore();
  const [week, setWeek] = useState(0);
  const [selId, setSelId] = useState(null); // the post in the side panel, and the anchor of a shift-click range
  const [sel, setSel] = useState([]); // every selected post: shift-click for a range, ⌘-click to toggle
  const [view, setView] = useState("week");
  // Dragging with the pointer (the browser's own drag API is taken by the window's file drop).
  // drag = { postId | candidateId, title, x, y, slot: { col, mins } | null, moved }
  const [drag, setDrag] = useState(null);
  const [hover, setHover] = useState(null); // { postId, x, y } while the pointer rests on a post
  const targetName = (p) => (s.targets || []).find((t) => t.id === p.targetId)?.name || CHANNEL_NAMES[p.channel] || p.channel;
  const gridRef = useRef(null);
  useEffect(() => { if (view === "week" && gridRef.current) gridRef.current.scrollTop = 9 * HOUR; }, [view]);
  const slotAt = (x, y) => { const g = gridRef.current; if (!g) return null; const r = g.getBoundingClientRect(); if (x < r.left + GUTTER || x > r.right || y < r.top || y > r.bottom) return null; const col = Math.min(6, Math.floor((x - r.left - GUTTER) / ((r.width - GUTTER) / 7))); const mins = Math.max(0, Math.min(24 * 60 - 15, Math.round(((y - r.top + g.scrollTop) / HOUR) * 4) * 15)); return { col, mins }; };
  const startDrag = (e, item) => { if (e.button !== 0) return; e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setDrag({ ...item, shiftKey: e.shiftKey, metaKey: e.metaKey || e.ctrlKey, x: e.clientX, y: e.clientY, slot: null, moved: false }); };
  const pick = (id, e) => {
    if (e.shiftKey && selId) { const order = [...s.posts].sort((a, b) => a.scheduledAt.localeCompare(b.scheduledAt)).map((p) => p.id); const [i, j] = [order.indexOf(selId), order.indexOf(id)].sort((a, b) => a - b); if (i >= 0) { setSel(order.slice(i, j + 1)); return; } }
    if (e.metaKey) { setSel(sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id]); setSelId(id); return; }
    setSel([id]); setSelId(id);
  };
  const movable = (p) => p && ["planned", "failed"].includes(p.status);
  // The posts a drag of `id` carries: the whole selection when it is part of it.
  const group = (id) => (sel.includes(id) ? sel : [id]).map((x) => s.posts.find((p) => p.id === x)).filter(movable);
  const unschedule = async (ps) => { if (!ps.length) return; await tryAct("batch", { actions: ps.map((p) => ({ action: "unschedule", args: { id: p.id } })) }, `${ps.length} back to unscheduled`); setSel([]); setSelId(null); };
  const moveDrag = (e) => { if (!drag) return; const moved = drag.moved || Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 4; setDrag({ ...drag, x: e.clientX, y: e.clientY, slot: moved ? slotAt(e.clientX, e.clientY) : null, moved }); };
  const planAt = (candidateId, at) => firstTarget && at && tryAct("schedule", { candidateId, targetId: firstTarget.id, at: at.toISOString() }, `Planned for ${at.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} ${hhmm(at.toISOString())}`);
  const endDrag = () => {
    if (!drag) return; const d = drag; setDrag(null);
    if (!d.moved) { if (d.postId) pick(d.postId, d); else if (d.candidateId && firstTarget) planAt(d.candidateId, nextSlot(firstTarget)); return; }
    // Dropped on the Unscheduled panel: off the calendar.
    if (d.postId && document.elementFromPoint(d.x, d.y)?.closest(".unsched")) return unschedule(group(d.postId));
    if (!d.slot) return; const at = new Date(days[d.slot.col]); at.setHours(Math.floor(d.slot.mins / 60), d.slot.mins % 60, 0, 0);
    if (d.postId) { const from = new Date(s.posts.find((p) => p.id === d.postId).scheduledAt); const shift = at - from; tryAct("batch", { actions: group(d.postId).map((p) => ({ action: "update_post", args: { id: p.id, patch: { scheduledAt: new Date(new Date(p.scheduledAt).getTime() + shift).toISOString() } } })) }); }
    else if (d.candidateId) planAt(d.candidateId, at);
  };
  useEffect(() => {
    const key = (e) => { if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) return; if (e.key === "Escape") setSel([]); if ((e.key === "Backspace" || e.key === "Delete") && sel.length) { e.preventDefault(); unschedule(sel.map((x) => s.posts.find((p) => p.id === x)).filter(movable)); } };
    window.addEventListener("keydown", key); return () => window.removeEventListener("keydown", key);
  });
  const dragProps = (item) => ({ onPointerDown: (e) => startDrag(e, item), onPointerMove: moveDrag, onPointerUp: endDrag, onPointerCancel: () => setDrag(null) });
  // The first hour of the target's schedule, from tomorrow on, that has no post yet.
  const nextSlot = (t) => { const hours = t.hours?.length ? t.hours : [17]; for (let d = 1; d < 90; d++) { for (const h of hours) { const at = new Date(); at.setDate(at.getDate() + d); at.setHours(h, 0, 0, 0); const taken = s.posts.some((p) => p.targetId === t.id && Math.abs(new Date(p.scheduledAt) - at) < 3600000); if (!taken) return at; } } return null; };
  const post = s.posts.find((p) => p.id === selId);
  const monday = new Date(); monday.setHours(0, 0, 0, 0); monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7) + week * 7);
  const days = [...Array(7)].map((_, i) => { const d = new Date(monday); d.setDate(d.getDate() + i); return d; });
  // The planned posts of the week on screen, for "Confirm week".
  const weekKeys = days.map((d) => localKey(d.toISOString()));
  const weekPlanned = s.posts.filter((p) => p.status === "planned" && weekKeys.includes(localKey(p.scheduledAt)));
  const [askWeek, setAskWeek] = useState(false);
  const confirmWeek = async () => { setAskWeek(false); let n = 0; for (const p of weekPlanned) if ((await tryAct("confirm", { id: p.id })) != null) n++; tryAct("snapshot", {}, `Confirmed ${n} post${n === 1 ? "" : "s"}`); };
  const todayKey = localKey(new Date().toISOString());
  const cand = (p) => s.candidates.find((c) => c.id === p.candidateId);
  const render = (p) => s.renders.find((r) => r.id === p.renderId);
  const targets = (s.targets || []).filter((t) => t.enabled);
  const linked = (t) => t.kind === "youtube" ? !!t.refreshToken : t.kind === "folder" ? !!t.path : false;
  const firstTarget = targets.find((t) => t.kind === "youtube" && t.refreshToken) || targets[0];
  const unscheduled = s.renders.filter((r) => r.status === "done" && r.format === "shorts" && !s.posts.some((p) => p.candidateId === r.candidateId));
  const patchPost = (p) => post && tryAct("update_post", { id: post.id, patch: p });

  return (
    <div className="main">
      <div className="col" style={{ width: sz0, flexShrink: 0 }}>
        <Panel title="Post to" right={<Btn small onClick={() => go("settings")}>Edit</Btn>} style={{ flexShrink: 0 }}>
          {!targets.length && <span className="hint">No targets yet. Add one in Settings.</span>}
          {targets.map((t) => <div key={t.id} style={{ display: "flex", flexDirection: "column", gap: 2, fontSize: 13.5 }}><div style={{ display: "flex", alignItems: "center", gap: 8 }}><Brand id={t.kind === "folder" ? "youtube" : t.kind} mono={!linked(t)} style={{ opacity: linked(t) ? 1 : 0.55 }} /><b className={`grow ell ${linked(t) ? "" : "sec"}`}>{t.name}</b><span className="muted">{linked(t) ? "" : t.kind === "youtube" ? "not connected" : "by hand"}</span></div>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}><span className="muted" style={{ width: 60 }}>post at</span><Text value={(t.hours || []).join(", ")} onCommit={(v) => tryAct("update_target", { id: t.id, patch: { hours: v.split(/[\s,]+/).map(Number).filter((n) => Number.isInteger(n) && n >= 0 && n < 24) } })} placeholder="17" style={{ width: 90, height: 24 }} /></div></div>)}
          <Btn onClick={async () => { const n = await tryAct("autofill", { days: 14 }); if (n != null) tryAct("snapshot", {}, `${n} posts planned`); }} disabled={!targets.length}>Fill the next 14 days</Btn>
          <span className="hint">One reel per target per listed hour, oldest approved first. Planned posts wait for your confirm.</span>
        </Panel>
        <Panel title={`Unscheduled · ${unscheduled.length}`} className={`grow unsched ${drag?.postId && drag.moved ? "drop-here" : ""}`}>
          {unscheduled.slice(0, 20).map((r) => { const c = s.candidates.find((c) => c.id === r.candidateId); return <div key={r.id} className="ell" style={{ fontSize: 13, color: "var(--text-2)", cursor: "grab", touchAction: "none" }} title="Drag onto the calendar, or click for the next free slot" {...dragProps({ candidateId: r.candidateId, title: c?.title || "" })}>{c?.title || r.candidateId}</div>; })}
          {!unscheduled.length && <span className="hint">Every rendered reel is on the calendar.</span>}
          <span className="hint">Drag a post here, or select it and press Delete, to take it off the calendar.</span>
        </Panel>
      </div>
      <Grip value={sz0} set={setsz0} dir={1} reset={260} />
      <div className="col grow">
        <div className="panel grow">
          {askWeek && <Confirm text={`Confirm ${weekPlanned.length} planned post${weekPlanned.length === 1 ? "" : "s"} for ${days[0].toLocaleDateString([], { day: "numeric", month: "short" })} – ${days[6].toLocaleDateString([], { day: "numeric", month: "short" })}? YouTube ones upload now and go live at their time.`} yes="Confirm all" no="Not yet" onYes={confirmWeek} onNo={() => setAskWeek(false)} />}
          <div className="panel-head"><span>Calendar</span><span className="sub">{days[0].toLocaleDateString([], { day: "numeric", month: "short" })} – {days[6].toLocaleDateString([], { day: "numeric", month: "short", year: "numeric" })}</span><span className="grow" />{sel.length > 1 && <Btn small onClick={() => unschedule(sel.map((x) => s.posts.find((p) => p.id === x)).filter(movable))} title="Back to unscheduled (Delete)">Unschedule · {sel.map((x) => s.posts.find((p) => p.id === x)).filter(movable).length}</Btn>}<Btn small primary disabled={!weekPlanned.length} onClick={() => setAskWeek(true)} title="Confirm every planned post of this week">Confirm week{weekPlanned.length ? ` · ${weekPlanned.length}` : ""}</Btn><Seg value={view} onChange={setView} options={[["week", "Week"], ["list", "List"]]} /><Btn small icon onClick={() => setWeek(week - 1)} aria-label="Previous week">{I.prev}</Btn><Btn small onClick={() => setWeek(0)}>Today</Btn><Btn small icon onClick={() => setWeek(week + 1)} aria-label="Next week">{I.next}</Btn></div>
          {view === "week" ? (
            <div className="cal-wrap">
              <div className="cal-heads"><div style={{ width: GUTTER, flexShrink: 0 }} />{days.map((d) => { const k = localKey(d.toISOString()); return <div key={k} className={`day-head ${k === todayKey ? "today" : ""}`}><span>{d.toLocaleDateString([], { weekday: "short" })}</span><span style={{ fontWeight: 500 }}>{d.getDate()}</span></div>; })}</div>
              <div className="cal-grid" ref={gridRef}>
                <div className="cal-gutter">{[...Array(24)].map((_, h) => <div key={h} className="cal-hour" style={{ height: HOUR }}>{String(h).padStart(2, "0")}:00</div>)}</div>
                {days.map((d, col) => { const k = localKey(d.toISOString()); const ps = s.posts.filter((p) => localKey(p.scheduledAt) === k); return (
                  <div key={k} className={`cal-col ${k === todayKey ? "today" : ""}`} style={{ height: 24 * HOUR }}>
                    {[...Array(24)].map((_, h) => <div key={h} className="cal-line" style={{ top: h * HOUR }} />)}
                    {drag?.slot?.col === col && <div className="cal-drop" style={{ top: (drag.slot.mins / 60) * HOUR, height: HOUR - 4 }}>{`${String(Math.floor(drag.slot.mins / 60)).padStart(2, "0")}:${String(drag.slot.mins % 60).padStart(2, "0")}`}</div>}
                    {ps.map((p) => { const t = new Date(p.scheduledAt); const canMove = movable(p); return <div key={p.id} className={`post ${shown(p)} ${sel.includes(p.id) ? "on" : ""} ${drag?.moved && drag.postId && group(drag.postId).some((g) => g.id === p.id) ? "dragging" : ""}`} style={{ position: "absolute", left: 3, right: 3, top: (t.getHours() * 60 + t.getMinutes()) / 60 * HOUR + 1, height: HOUR - 4, cursor: canMove ? "grab" : "pointer", touchAction: "none" }} onMouseEnter={(e) => !drag && setHover({ postId: p.id, x: e.clientX, y: e.clientY })} onMouseMove={(e) => !drag && hover?.postId === p.id && setHover({ postId: p.id, x: e.clientX, y: e.clientY })} onMouseLeave={() => setHover(null)} {...(canMove ? dragProps({ postId: p.id, title: p.title }) : { onClick: (e) => pick(p.id, { shiftKey: e.shiftKey, metaKey: e.metaKey || e.ctrlKey }) })}><span className="t"><span>{hhmm(p.scheduledAt)}</span><span className="muted" style={{ display: "flex", alignItems: "center", gap: 4 }}><Brand id={p.channel} size={11} />{shown(p) === "posted" ? "✓" : shown(p) === "scheduled" ? "◷" : p.status === "failed" ? "✕" : ""}</span></span><span className="ell">{p.title || "(poster)"}</span></div>; })}
                  </div>
                ); })}
              </div>
              {drag?.moved && <div className="drag-ghost" style={{ left: drag.x + 12, top: drag.y + 12 }}>{drag.postId && group(drag.postId).length > 1 ? `${group(drag.postId).length} posts` : drag.title}</div>}
              {hover && !drag && (() => { const p = s.posts.find((x) => x.id === hover.postId); if (!p) return null; const r = render(p); const left = hover.x + 240 > window.innerWidth ? hover.x - 232 : hover.x + 16; const top = Math.min(hover.y - 40, window.innerHeight - 380); return <div className="post-peek" style={{ left, top }}>{r?.coverPath ? <img src={fileUrl(r.coverPath)} alt="" /> : <div className="post-peek-empty">no render yet</div>}<b className="ell">{p.title || "(poster)"}</b><span className="muted">{new Date(p.scheduledAt).toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} {hhmm(p.scheduledAt)} · {targetName(p)}</span><span className={tone(shown(p))}>{LABEL[shown(p)] || p.status}</span>{r?.info?.width && <span className={r.info.issues?.length ? "coral" : "muted"}>{`${r.info.width}×${r.info.height} · ${Math.round(r.info.fps)} fps · ${(r.info.kbps / 1000).toFixed(1)} Mbps`}{r.info.issues?.length ? ` · ${r.info.issues.join(", ")}` : ""}</span>}</div>; })()}
            </div>
          ) : (
            <div className="rows">{[...s.posts].sort((a, b) => a.scheduledAt.localeCompare(b.scheduledAt)).map((p) => <div key={p.id} className={`row ${sel.includes(p.id) ? "on" : ""}`} style={{ gridTemplateColumns: "190px minmax(0,1fr) 100px 120px", userSelect: "none" }} onClick={(e) => pick(p.id, { shiftKey: e.shiftKey, metaKey: e.metaKey || e.ctrlKey })}><span className="muted" style={{ whiteSpace: "nowrap" }}>{new Date(p.scheduledAt).toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} · {hhmm(p.scheduledAt)}</span><span className="ell">{p.title}</span><span style={{ display: "flex", alignItems: "center", gap: 6 }}><Brand id={p.channel} size={12} />{CHANNEL_NAMES[p.channel]}</span><span className={tone(shown(p))}>{LABEL[shown(p)] || p.status}</span></div>)}</div>
          )}
          <div style={{ height: 26, display: "flex", alignItems: "center", gap: 14, padding: "0 10px", borderTop: "1px solid var(--rule)", fontSize: 12.5, color: "var(--muted)" }}><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--mint)", marginRight: 5 }} />posted</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--tiffany)", marginRight: 5 }} />scheduled on YouTube</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--accent)", marginRight: 5 }} />confirmed</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--cyan)", marginRight: 5 }} />planned, waiting for you</span><span><span className="sq" style={{ display: "inline-block", border: "1px solid var(--coral)", marginRight: 5 }} />failed</span></div>
        </div>
        <Panel title="Deliveries" sub="what went out and what failed" style={{ height: 170, flexShrink: 0 }} body={false}>
          <div className="rows">{s.posts.filter((p) => p.status === "posted" || p.status === "failed").sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 30).map((p) => <div key={p.id} className="row" style={{ gridTemplateColumns: "70px minmax(0,1fr) 110px 110px minmax(0,1fr)", minHeight: 26 }} onClick={() => pick(p.id, {})}><span className="muted">{ago(p.updatedAt)}</span><span className="ell">{p.title}</span><span style={{ display: "flex", alignItems: "center", gap: 6 }}><Brand id={p.channel} size={12} />{CHANNEL_NAMES[p.channel]}</span><span className={tone(shown(p))}>{LABEL[shown(p)] || p.status}</span><span className="mono ell muted">{p.providerId ? (p.channel === "youtube" ? `youtu.be/${p.providerId}` : p.providerId) : p.error}</span></div>)}</div>
        </Panel>
      </div>
      <Grip value={sz1} set={setsz1} dir={-1} reset={300} />
      <div className="panel" style={{ width: sz1, flexShrink: 0 }}>
        <div className="panel-head"><span className="grow">Post</span>{post && <span className={tone(shown(post))} style={{ fontWeight: 500 }}>{shown(post) === "scheduled" ? `on YouTube, live ${new Date(post.scheduledAt).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}` : post.status}</span>}</div>
        {!post ? <div className="empty">Pick a post on the calendar, or click an unscheduled reel to plan it.</div> : (
          <>
            <div className="panel-body">
              <div style={{ display: "flex", gap: 8 }}>{render(post)?.path && <video controls playsInline preload="metadata" poster={render(post)?.coverPath ? fileUrl(render(post).coverPath) : undefined} src={fileUrl(render(post).path)} style={{ width: 150, height: 267, objectFit: "cover", borderRadius: 4, background: "#000", flexShrink: 0 }} />}<div style={{ display: "flex", flexDirection: "column", gap: 3, fontSize: 13.5, minWidth: 0 }}><b>{post.title}</b>{cand(post) && <span className="muted">{(cand(post).end - cand(post).start).toFixed(1)} s · score {cand(post).score} · {cand(post).category}</span>}<span className={render(post) ? "mint" : "coral"}>{render(post) ? `${render(post).format}.mp4 rendered` : "no render yet"}</span>{post.providerId && <a href={post.channel === "youtube" ? `https://youtu.be/${post.providerId}` : "#"}>{post.providerId}</a>}{post.error && <span className="coral">{post.error}</span>}</div></div>
              <label className="field"><span>Title</span><Text value={post.title} onCommit={(v) => patchPost({ title: v })} /></label>
              <label className="field"><span>Caption</span><Text area rows={5} value={post.caption} onCommit={(v) => patchPost({ caption: v })} /></label>
              <div className="grid2">
                <label className="field"><span>Post to</span><select className="input" value={post.targetId || ""} onChange={(e) => { const t = (s.targets || []).find((x) => x.id === e.target.value); if (t) patchPost({ targetId: t.id, channel: t.kind }); }}><option value="">{CHANNEL_NAMES[post.channel] || post.channel}</option>{(s.targets || []).map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}</select></label>
                <label className="field"><span>When (local)</span><input className="input" type="datetime-local" value={new Date(new Date(post.scheduledAt).getTime() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16)} onChange={(e) => e.target.value && patchPost({ scheduledAt: new Date(e.target.value).toISOString() })} /></label>
              </div>
              <span className="hint">{post.channel === "youtube" ? `Uploads now as ${(s.targets || []).find((t) => t.id === post.targetId)?.privacy || "public"}. A future time goes up private and YouTube shows it then.` : post.channel === "folder" ? "The file, cover and caption land in the drop folder at post time." : "Not linked yet. The file and its caption wait in the reel folder."}</span>
            </div>
            <span className="grow" />
            <div className="panel-foot">
              {post.status === "posted" && post.channel === "youtube" && <><span className="muted">Now</span><Seg value={post.privacy || "?"} onChange={(v) => tryAct("set_privacy", { id: post.id, privacy: v }, `Set to ${v}`)} options={[["public", "Public"], ["unlisted", "Unlisted"], ["private", "Private"]]} /></>}
              {post.status !== "posted" && <Btn primary className="grow" onClick={() => tryAct("confirm", { id: post.id }, "Confirmed")} disabled={post.status === "confirmed" || post.status === "posting"}>{post.status === "confirmed" ? "Confirmed" : post.status === "failed" ? "Try again" : "Confirm"}</Btn>}
              {post.status !== "posted" && <Btn onClick={() => tryAct("publish_now", { id: post.id }, "Publishing")}>Post now</Btn>}
              <Btn danger icon onClick={() => unschedule([post])} aria-label="Remove">{I.trash}</Btn>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
