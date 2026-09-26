import { Component, useEffect, useState } from "react";
import { useStore, act, refresh } from "./store.js";
import { Btn, I } from "./ui.jsx";
import Library from "./screens/Library.jsx";
import Transcript from "./screens/Transcript.jsx";
import Reels from "./screens/Reels.jsx";
import Inbox from "./screens/Inbox.jsx";
import Queue from "./screens/Queue.jsx";
import Publish from "./screens/Publish.jsx";
import Posters from "./screens/Posters.jsx";
import Settings from "./screens/Settings.jsx";
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
        <b className="coral">This screen hit a bug.</b><br />
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
              {id === "queue" && running > 0 && <span className="n mint">{running}</span>}
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
      {s.toast && <div style={{ position: "fixed", right: 16, bottom: 16, padding: "8px 12px", borderRadius: 4, background: s.toast.kind === "err" ? "#3B1F2A" : "var(--head)", border: `1px solid ${s.toast.kind === "err" ? "var(--coral)" : s.toast.kind === "ok" ? "var(--mint)" : "var(--rule)"}`, fontSize: 12.5, maxWidth: 420, zIndex: 20 }}>{s.toast.msg}</div>}
    </div>
  );
}
