import { Component, useEffect, useState } from "react";
import { useStore, act, refresh, undo } from "./store.js";
import { Btn, I } from "./ui.jsx";
import Library from "./screens/Library.jsx";
import Transcript from "./screens/Transcript.jsx";
import Reels from "./screens/Reels.jsx";
import Inbox from "./screens/Inbox.jsx";
import Queue from "./screens/Queue.jsx";
import Publish from "./screens/Publish.jsx";
import Posters from "./screens/Posters.jsx";
import Settings from "./screens/Settings.jsx";
import Style from "./screens/Style.jsx";
import AddSource from "./screens/AddSource.jsx";

/* A screen that throws shows the error and a way back instead of a blank window. */
class Boundary extends Component {
  state = { error: null };
  static getDerivedStateFromError(error) { return { error }; }
  componentDidUpdate(prev) { if (prev.resetKey !== this.props.resetKey && this.state.error) this.setState({ error: null }); }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="panel grow"><div className="empty" style={{ userSelect: "text" }}>
        <b className="coral">This screen broke.</b><br />
        <span className="mono">{String(this.state.error?.message || this.state.error)}</span><br /><br />
        <Btn onClick={this.props.onHome}>Back to the library</Btn>
      </div></div>
    );
  }
}

const TABS = [["library", "Library"], ["inbox", "Inbox"], ["reels", "Reels"], ["posters", "Posters"], ["publish", "Publish"], ["queue", "Queue"]];

export default function App() {
  const s = useStore();
  const [nav, setNav] = useState({ tab: "library", sourceId: null, view: "reels" });
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState("");

  const go = (tab, extra = {}) => setNav((n) => ({ ...n, tab, ...extra }));
  const inboxNew = s.inbox.filter((i) => i.status === "new").length;
  const review = s.candidates.filter((c) => !c.approved && !c.discarded).length;
  const running = s.jobs.filter((j) => j.status === "running" || j.status === "queued").length;
  const autopilot = s.channels.some((c) => c.enabled);

  useEffect(() => {
    const onKey = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "n") { e.preventDefault(); setAdding(true); }
      if ((e.metaKey || e.ctrlKey) && e.key === ",") { e.preventDefault(); go("settings"); }
      if ((e.metaKey || e.ctrlKey) && e.key === "r") { e.preventDefault(); refresh(); }
      // ⌘Z / ⇧⌘Z undo and redo the last change to the library; inside a text field they stay the field's own.
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z" && !["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName)) { e.preventDefault(); undo(e.shiftKey); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!s.ready) return <div className="empty">Opening the library…</div>;
  if (s.error && !s.sources.length) return <div className="empty">{s.error}</div>;

  const props = { nav, go, query, setAdding };
  const screen = {
    library: nav.sourceId && nav.view === "transcript" ? <Transcript {...props} /> : <Library {...props} />,
    reels: <Reels {...props} />,
    inbox: <Inbox {...props} />,
    posters: <Posters {...props} />,
    publish: <Publish {...props} />,
    queue: <Queue {...props} />,
    settings: <Settings {...props} />,
    style: <Style {...props} />,
  }[nav.tab];

  return (
    <div className="app">
      <div className="topbar">
        <button type="button" className={`brand ${s.jobs.some((j) => j.status === "running") ? "busy" : ""}`} onClick={() => go("library", { sourceId: null })} title="Library">Cuttar</button>
        <div className="tabs">
          {TABS.map(([id, label]) => (
            <button key={id} type="button" className={`tab ${nav.tab === id ? "on" : ""}`} onClick={() => go(id)}>
              {label}
              {id === "inbox" && inboxNew > 0 && <span className="n">{inboxNew}</span>}
              {id === "reels" && review > 0 && <span className="n">{review}</span>}
              {id === "queue" && running > 0 && <>{s.jobs.some((j) => j.status === "running") && <i className="spin" />}<span className="n mint">{running}</span></>}
            </button>
          ))}
        </div>
        <span className="grow" />
        <label className="search">{I.search}<input type="search" placeholder="Search titles and transcripts" value={query} onChange={(e) => setQuery(e.target.value)} /></label>
        <span className="status"><span className={`dot ${autopilot ? "mint" : "muted"}`} />{autopilot ? `Autopilot · ${s.channels.filter((c) => c.enabled).length} channels` : "Autopilot off"}</span>
        <Btn primary small onClick={() => setAdding(true)}>{I.plus} Add</Btn>
        <Btn icon small className={nav.tab === "settings" ? "primary" : ""} onClick={() => go("settings")} aria-label="Settings">{I.gear}</Btn>
      </div>
      <Boundary resetKey={`${nav.tab}:${nav.sourceId}:${nav.view}`} onHome={() => go("library", { sourceId: null, view: "reels" })}>{screen}</Boundary>
      {adding && <AddSource onClose={() => setAdding(false)} go={go} />}
      <QueueMeter jobs={s.jobs} paused={s.settings.queuePaused} onOpen={() => go("queue")} />
      {s.toast && <div style={{ position: "fixed", right: 16, bottom: 40, padding: "8px 12px", borderRadius: 4, background: s.toast.kind === "err" ? "#3B1F2A" : "var(--head)", border: `1px solid ${s.toast.kind === "err" ? "var(--coral)" : s.toast.kind === "ok" ? "var(--mint)" : "var(--rule)"}`, fontSize: 13.5, maxWidth: 420, zIndex: 20 }}>{s.toast.msg}</div>}
    </div>
  );
}

/* The whole queue at a glance, bottom right while anything is queued or running: done / total of the
   run (what is waiting, and what finished since the oldest of it was queued), one bar, and the time
   left at the pace so far. */
function QueueMeter({ jobs, paused, onOpen }) {
  const active = jobs.filter((j) => j.status === "queued" || j.status === "running");
  if (!active.length) return <div className="statusbar"><span className="grow" /><span className="queue-meter idle">Queue idle</span></div>;
  const from = active.reduce((m, j) => (j.createdAt < m ? j.createdAt : m), active[0].createdAt);
  // A finished job belongs to this run when it ended after the oldest waiting job was queued.
  const batch = jobs.filter((j) => j.status !== "cancelled" && (j.createdAt >= from || (j.finishedAt && j.finishedAt >= from)));
  const finished = batch.filter((j) => j.status === "done" || j.status === "failed").length;
  const units = finished + batch.filter((j) => j.status === "running").reduce((n, j) => n + (j.progress || 0), 0);
  const started = batch.map((j) => j.startedAt).filter(Boolean).sort()[0];
  const secs = started ? (Date.now() - new Date(started).getTime()) / 1000 : 0;
  const left = units > 0.2 && secs > 10 ? ((batch.length - units) * secs) / units : null;
  const eta = paused ? "paused" : left == null ? "" : left < 90 ? `${Math.max(1, Math.round(left))} s left` : left < 5400 ? `${Math.round(left / 60)} min left` : `${(left / 3600).toFixed(1)} h left`;
  const failed = batch.filter((j) => j.status === "failed").length;
  return (
    <div className="statusbar">
      <span className="muted ell">{active.find((j) => j.status === "running")?.label || active[0].label}</span>
      <span className="grow" />
      <button type="button" className="queue-meter" onClick={onOpen} title="Open the queue">
        <span className="num qm-count">{finished}/{batch.length}</span>
        <span className="qm-bar"><i style={{ width: `${Math.min(100, (units / batch.length) * 100)}%` }} /></span>
        <span className={`num qm-eta ${failed ? "coral" : "muted"}`}>{failed ? `${failed} failed` : eta}</span>
      </button>
    </div>
  );
}
