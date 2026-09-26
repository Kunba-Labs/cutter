import { useState } from "react";
import { tryAct, LANGS, useStore } from "../store.js";
import { Btn, Field, Check, Seg, I } from "../ui.jsx";

export default function AddSource({ onClose, go }) {
  const settings = useStore((s) => s.settings);
  const [mode, setMode] = useState("url");
  const [url, setUrl] = useState("");
  const [path, setPath] = useState("");
  const [language, setLanguage] = useState("auto");
  const [transcriptSource, setTS] = useState("whisper");
  const [autoDetect, setAutoDetect] = useState(settings.autoDetect ?? true);
  const [autoApprove, setAutoApprove] = useState(false);
  const [channel, setChannel] = useState({ url: "", name: "", intervalH: 6, minLenS: 600, titleRegex: "", autoProcess: true, autoApproveScore: 0 });
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    setBusy(true);
    let r;
    if (mode === "channel") {
      r = await tryAct("add_channel", channel, "Channel added — checking it now");
      if (r) go("inbox");
    } else {
      const args = { language, transcriptSource, autoDetect, autoApproveScore: autoApprove ? Math.max(8, settings.autoApproveScore || 8) : 0 };
      if (mode === "url") args.url = url.trim(); else args.path = path.trim();
      r = await tryAct("add_source", args, "Added — downloading");
      if (r) go("library", { sourceId: r.id, view: "reels" });
    }
    setBusy(false);
    if (r) onClose();
  };

  return (
    <div className="sheet-bg" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="sheet">
        <div className="panel-head"><span className="grow">Add source</span><Btn small icon onClick={onClose} aria-label="Close">{I.x}</Btn></div>
        <div className="panel-body" style={{ gap: 12, padding: 12 }}>
          <Seg big value={mode} onChange={setMode} options={[["url", "YouTube link"], ["channel", "Watch a channel"], ["file", "Local file"]]} />
          {mode === "url" && <Field label="URL"><input className="input" autoFocus value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://www.youtube.com/watch?v=…" onKeyDown={(e) => e.key === "Enter" && submit()} /></Field>}
          {mode === "file" && <Field label="Path to a video or audio file"><input className="input" autoFocus value={path} onChange={(e) => setPath(e.target.value)} placeholder="/Users/…/khutbah.mov  (drag a file here or paste its path)" onDrop={(e) => { e.preventDefault(); const f = e.dataTransfer.files?.[0]; if (f?.path) setPath(f.path); }} onDragOver={(e) => e.preventDefault()} /></Field>}
          {mode === "channel" && (
            <>
              <Field label="Channel or playlist URL"><input className="input" autoFocus value={channel.url} onChange={(e) => setChannel({ ...channel, url: e.target.value, name: channel.name || e.target.value.split("/").filter(Boolean).pop() || "" })} placeholder="https://www.youtube.com/@channel" /></Field>
              <div className="grid2">
                <Field label="Name"><input className="input" value={channel.name} onChange={(e) => setChannel({ ...channel, name: e.target.value })} /></Field>
                <Field label="Title must match (regex, optional)"><input className="input" value={channel.titleRegex} onChange={(e) => setChannel({ ...channel, titleRegex: e.target.value })} placeholder="tafsir|seerah|khutbah" /></Field>
                <Field label="Check every (hours)"><input className="input" type="number" value={channel.intervalH} onChange={(e) => setChannel({ ...channel, intervalH: +e.target.value })} /></Field>
                <Field label="Minimum length (seconds)"><input className="input" type="number" value={channel.minLenS} onChange={(e) => setChannel({ ...channel, minLenS: +e.target.value })} /></Field>
              </div>
              <Check label="Process new uploads automatically (download, transcribe, find reels)" checked={channel.autoProcess} onChange={(v) => setChannel({ ...channel, autoProcess: v })} />
              <Check label="Auto-approve reels scoring 8 or more" checked={channel.autoApproveScore > 0} onChange={(v) => setChannel({ ...channel, autoApproveScore: v ? 8 : 0 })} />
            </>
          )}
          {mode !== "channel" && (
            <>
              <div className="grid2">
                <Field label="Spoken language"><select className="input" value={language} onChange={(e) => setLanguage(e.target.value)}>{LANGS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select></Field>
                <Field label="Transcript"><select className="input" value={transcriptSource} onChange={(e) => setTS(e.target.value)}><option value="whisper">Local whisper (word timings)</option><option value="captions">YouTube captions</option></select></Field>
              </div>
              <div className="panel" style={{ background: "var(--bg)" }}>
                <div className="panel-body" style={{ gap: 6 }}>
                  <Check label="Find reels with Claude when the transcript is ready" checked={autoDetect} onChange={setAutoDetect} right={`≤ ${settings.maxReelS || 40} s · complete thoughts`} />
                  <Check label="Auto-approve score ≥ 8 and render" checked={autoApprove} onChange={setAutoApprove} right="off: hold for review" />
                </div>
              </div>
            </>
          )}
          <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <span className="hint grow">{settings.outDir}</span>
            <Btn onClick={onClose}>Cancel</Btn>
            <Btn primary disabled={busy || (mode === "url" ? !url.trim() : mode === "file" ? !path.trim() : !channel.url.trim())} onClick={submit}>{I.play} {mode === "channel" ? "Watch" : "Add and run"}</Btn>
          </div>
        </div>
      </div>
    </div>
  );
}
