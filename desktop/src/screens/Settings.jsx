import { useState } from "react";
import { useStore, tryAct, FORMATS } from "../store.js";
import { Panel, Btn, Check, Text, Seg, Dot, Brand, Confirm, I } from "../ui.jsx";

const SECTIONS = [["general", "General"], ["engines", "Engines"], ["channels", "Channels"], ["captions", "Captions"], ["posters", "Posters"], ["mcp", "Claude and terminal"]];

export default function Settings({ go }) {
  const s = useStore();
  const st = s.settings;
  const [sec, setSec] = useState("channels");
  const [busy, setBusy] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const [askDisconnect, setAskDisconnect] = useState(null);
  const [removeT, setRemoveT] = useState(null);
  const patch = (p, msg) => tryAct("settings", { patch: p }, msg);
  const yt = st.youtube || {};
  const cs = st.captionStyle || {};
  const Row = ({ label, children, hint }) => <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13.5 }}><span className="muted" style={{ width: 150, flexShrink: 0 }}>{label}</span>{children}{hint && <span className="hint">{hint}</span>}</div>;
  const W = { width: 300 };

  return (
    <div className="main">
      <div className="panel" style={{ width: 200, flexShrink: 0 }}>
        <div className="panel-head">Settings</div>
        {SECTIONS.map(([k, l]) => <div key={k} className={`nav-item ${sec === k ? "on" : ""}`} style={{ padding: "7px 10px" }} onClick={() => setSec(k)}>{l}</div>)}
        <span className="grow" />
        <div className="panel-foot" style={{ flexDirection: "column", alignItems: "stretch", gap: 3, fontSize: 13, color: "var(--muted)" }}><span>Cuttar 0.1</span><span className="ell">{s.paths.dataDir}</span></div>
      </div>
      <div className="col grow">
        {sec === "general" && (
          <Panel title="General">
            <Row label="Output folder"><Text style={W} value={st.outDir} onCommit={(v) => patch({ outDir: v })} /><Btn small onClick={() => tryAct("open", { path: st.outDir })}>Open</Btn></Row>
            <Row label="Max reel length"><input className="input" style={{ width: 70 }} type="number" value={st.maxReelS} onChange={(e) => patch({ maxReelS: +e.target.value })} /><span className="hint">seconds. Claude and the trimmer both stop here.</span></Row>
            <Row label="Keep reels scoring"><input className="input" style={{ width: 70 }} type="number" min="1" max="10" value={st.minScore} onChange={(e) => patch({ minScore: +e.target.value })} /><span className="hint">and above</span></Row>
            <Row label="Auto-approve at"><input className="input" style={{ width: 70 }} type="number" min="0" max="10" value={st.autoApproveScore} onChange={(e) => patch({ autoApproveScore: +e.target.value })} /><span className="hint">0 is off. Autopilot and new sources use it.</span></Row>
            <Row label="Find reels automatically"><Check label="after every transcript" checked={st.autoDetect} onChange={(v) => patch({ autoDetect: v })} /></Row>
            <Row label="Polish captions"><Check label="Claude fixes clear errors word for word before the reel search" checked={st.polishCaptions} onChange={(v) => patch({ polishCaptions: v })} /><span className="hint">Verb forms, misheard words, names. At most 5% of the words.</span></Row>
            <Row label="Render formats">{FORMATS.map(([v, l]) => <Check key={v} label={l} checked={(st.formats || []).includes(v)} onChange={(on) => patch({ formats: on ? [...new Set([...(st.formats || []), v])] : (st.formats || []).filter((f) => f !== v) })} />)}</Row>
            <Row label="Export quality"><select className="input" style={{ width: 180 }} value={st.renderQuality || "best"} onChange={(e) => patch({ renderQuality: e.target.value })}><option value="best">Best, slow</option><option value="good">Good</option><option value="fast">Fast, hardware</option></select><span className="hint">Best takes a minute or two per reel. Fast is near instant and a bit softer.</span></Row>
            <Row label="Parallel jobs"><input className="input" style={{ width: 70 }} type="number" min="1" max="4" value={st.parallelJobs} onChange={(e) => patch({ parallelJobs: +e.target.value })} /><span className="hint">Applies after a restart. One transcript at a time.</span></Row>
            <Row label="Channel name"><Text style={W} value={st.channelName} onCommit={(v) => patch({ channelName: v })} placeholder="shown as the watermark" /></Row>
            <Row label="End card"><Check label="Append the closing card to every render" checked={st.endCard?.enabled} onChange={(v) => patch({ endCard: { enabled: v } })} /><input className="input num" type="number" min="0.5" max="8" step="0.5" style={{ width: 64 }} value={st.endCard?.seconds ?? 2.5} onChange={(e) => patch({ endCard: { seconds: +e.target.value || 2.5 } })} /><span className="hint">seconds. Cards come from a poster.{st.endCard?.paths?.["9x16"] ? "" : " None set yet."}</span></Row>
            <Row label="Glossary"><Text style={W} value={(st.glossary || []).join(", ")} onCommit={(v) => patch({ glossary: v.split(",").map((x) => x.trim()).filter(Boolean) })} /><span className="hint">names and terms whisper should spell right</span></Row>
          </Panel>
        )}
        {sec === "engines" && (
          <>
            <Panel title="Transcription" right={<span style={{ display: "flex", gap: 6, alignItems: "center", fontWeight: 500 }}><Dot c={s.tools.whisper ? "mint" : "coral"} />{s.tools.whisper || "not installed"}</span>}>
              <Row label="Model"><select className="input" style={W} value={st.whisperModel} onChange={(e) => patch({ whisperModel: e.target.value })}>{["mlx-community/whisper-large-v3-turbo", "mlx-community/whisper-large-v3-mlx", "mlx-community/whisper-medium-mlx", "mlx-community/whisper-small-mlx"].map((m) => <option key={m} value={m}>{m.split("/").pop()}</option>)}</select><span className="hint">large-v3-turbo: 1.6 GB, ~12× realtime, all languages</span></Row>
              <Row label="Fallback after OOM"><select className="input" style={W} value={st.whisperFallback} onChange={(e) => patch({ whisperFallback: e.target.value })}>{["", "mlx-community/whisper-medium-mlx", "mlx-community/whisper-small-mlx"].map((m) => <option key={m} value={m}>{m ? m.split("/").pop() : "none"}</option>)}</select></Row>
              {!s.tools.whisper && <Row label="Install"><Btn primary disabled={busy} onClick={async () => { setBusy(true); await tryAct("install_whisper", {}, "Installed"); setBusy(false); }}>{busy ? "Installing…" : "uv tool install mlx-whisper"}</Btn><span className="hint">needs uv {s.tools.uv ? "✓" : "(brew install uv)"}</span></Row>}
            </Panel>
            <Panel title="Reel finder">
              <Row label="Finder"><Seg value={st.brain} onChange={(v) => patch({ brain: v })} options={[["claude", "Claude CLI"], ["ollama", "Ollama (offline)"]]} /></Row>
              <Row label="claude binary"><Text style={W} value={st.claudeBin} onCommit={(v) => patch({ claudeBin: v })} /><span style={{ display: "flex", gap: 6, alignItems: "center" }} className="hint"><Dot c={s.tools.claude ? "mint" : "coral"} />{s.tools.claude || "not found on PATH"}</span></Row>
              <Row label="Ollama model"><Text style={W} value={st.ollamaModel} onCommit={(v) => patch({ ollamaModel: v })} /><span style={{ display: "flex", gap: 6, alignItems: "center" }} className="hint"><Dot c={s.tools.ollama ? "mint" : "muted"} />{s.tools.ollama || "ollama not found"}</span></Row>
              <Row label="Caption language"><select className="input" style={{ width: 180 }} value={st.translateTo} onChange={(e) => patch({ translateTo: e.target.value })}><option value="">The spoken language</option><option value="en">English</option><option value="nl">Dutch</option><option value="ar">Arabic</option><option value="ur">Urdu</option><option value="tr">Turkish</option></select><span className="hint">The spoken words with the current word lit, or one translation by Claude.</span></Row>
            </Panel>
            <Panel title="Tools found">
              <div className="grid2" style={{ fontSize: 13.5 }}>{[["ffmpeg", "ffmpeg"], ["ytdlp", "yt-dlp"], ["whisper", "mlx_whisper"], ["claude", "claude"], ["ollama", "ollama"], ["uv", "uv"]].map(([k, l]) => <span key={k} style={{ display: "flex", gap: 6, alignItems: "center" }}><Dot c={s.tools[k] ? "mint" : "coral"} />{l} <span className="muted ell">{s.tools[k] || "missing"}</span></span>)}</div>
              {s.tools.ffmpeg && !s.tools.ffmpegAss && <span className="coral" style={{ fontSize: 13.5 }}>This ffmpeg cannot burn captions. Install ffmpeg-full from Homebrew.</span>}
              <span className="hint">Missing tools: brew install ffmpeg-full yt-dlp uv, then uv tool install mlx-whisper.</span>
            </Panel>
          </>
        )}
        {sec === "channels" && (
          <>
            <Panel title="Post to" sub="where reels and posters go" right={<span style={{ display: "flex", gap: 4 }}>{[["youtube", "YouTube"], ["instagram", "Instagram"], ["tiktok", "TikTok"], ["facebook", "Facebook"], ["folder", "Folder"]].map(([k, l]) => <Btn key={k} small onClick={() => tryAct("add_target", { kind: k }, `${l} added`)}>+ {l}</Btn>)}</span>}>
              {!(s.targets || []).length && <span className="hint">Add a place to post. YouTube uploads by itself. The others get the file and caption ready.</span>}
              {(s.targets || []).map((t) => {
                const patchT = (p) => tryAct("update_target", { id: t.id, patch: p });
                const linked = t.kind === "youtube" ? !!t.refreshToken : t.kind === "folder" ? !!t.path : false;
                return (
                  <div key={t.id} className="insp-group" style={{ border: "1px solid var(--rule)", borderRadius: 6, padding: 10, opacity: t.enabled ? 1 : 0.6 }}>
                    <div className="insp-head" style={{ borderBottom: 0, paddingBottom: 0 }}><Brand id={t.kind === "folder" ? "youtube" : t.kind} mono={!linked} /><Text value={t.name} onCommit={(v) => patchT({ name: v })} style={{ width: 220, height: 24 }} /><span className="grow" /><Check label="on" checked={t.enabled} onChange={(v) => patchT({ enabled: v })} /><a href="#" className="muted" onClick={(e) => { e.preventDefault(); setRemoveT(t); }}>remove</a></div>
                    {t.kind === "youtube" && <Row label="Account">{t.refreshToken
                      ? <span style={{ display: "flex", alignItems: "center", gap: 8 }}><span style={{ color: "var(--mint)", display: "inline-flex" }}>{I.check}</span><span>Connected as <b>{t.channelTitle || t.name}</b></span><a href="#" className="muted" onClick={(e) => { e.preventDefault(); setAskDisconnect(t); }}>disconnect</a></span>
                      : <><Btn primary disabled={busy} onClick={async () => { setBusy(true); await tryAct("target_connect", { id: t.id }, "YouTube connected"); setBusy(false); }}>{busy ? "Waiting for Google…" : "Connect Google account"}</Btn><span className="hint">Your browser opens. Sign in with the account that owns the channel.</span></>}</Row>}
                    {t.kind === "folder" && <Row label="Folder"><Text style={W} value={t.path} onCommit={(v) => patchT({ path: v })} placeholder="/Users/you/Dropbox/Reels" /><span className="hint">The file, its cover and a caption text land here at post time.</span></Row>}
                    {["instagram", "tiktok", "facebook"].includes(t.kind) && <span className="hint">Needs an approved developer app. At post time the file and caption are ready in the reel folder.</span>}
                    <Row label="Post at"><Text style={{ width: 120 }} value={(t.hours || []).join(", ")} onCommit={(v) => patchT({ hours: v.split(/[\s,]+/).map(Number).filter((n) => Number.isInteger(n) && n >= 0 && n < 24) })} placeholder="17, 20" /><span className="hint">Local hours, one reel each. Empty: only by hand.</span><Check label="No confirm needed" checked={!!t.autoSchedule} onChange={(v) => patchT({ autoSchedule: v })} /></Row>
                    <Row label="Format"><select className="input" style={{ width: 150 }} value={t.format || ""} onChange={(e) => patchT({ format: e.target.value })}><option value="">{`Default (${t.kind === "tiktok" ? "TikTok" : t.kind === "instagram" || t.kind === "facebook" ? "Reels" : "Shorts"})`}</option>{FORMATS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select></Row>
                    {t.kind === "youtube" && <>
                      <Row label="Privacy"><Seg value={t.privacy || "public"} onChange={(v) => patchT({ privacy: v })} options={[["public", "Public"], ["unlisted", "Unlisted"], ["private", "Private"]]} /><span className="hint">Scheduled uploads stay private until their time.</span></Row>
                      <Row label="Category id"><Text style={{ width: 70 }} value={t.categoryId} onCommit={(v) => patchT({ categoryId: v })} /><span className="hint">27 = Education, 29 = Nonprofits</span></Row>
                      <Row label="Title suffix"><Text style={W} value={t.titleSuffix} onCommit={(v) => patchT({ titleSuffix: v })} /></Row>
                      <Row label="Description footer"><Text style={W} value={t.descriptionFooter} onCommit={(v) => patchT({ descriptionFooter: v })} /><span className="hint">{"{source_url}"} is replaced</span></Row>
                    </>}
                  </div>
                );
              })}
              {askDisconnect && <Confirm text={`Disconnect ${askDisconnect.name}? Its scheduled posts wait until you connect again.`} yes="Disconnect" no="Keep" onYes={() => { tryAct("target_disconnect", { id: askDisconnect.id }, "Disconnected"); setAskDisconnect(null); }} onNo={() => setAskDisconnect(null)} />}
              {removeT && <Confirm text={`Remove ${removeT.name}? Posts already planned for it stay on the calendar and fail.`} yes="Remove" no="Keep" onYes={() => { tryAct("remove_target", { id: removeT.id }, "Removed"); setRemoveT(null); }} onNo={() => setRemoveT(null)} />}
              <Row label=""><a href="#" className="muted" onClick={(e) => { e.preventDefault(); setAdvanced(!advanced); }}>{advanced ? "Hide advanced" : "Advanced"}</a></Row>
              {advanced && <>
                <Row label="OAuth client id"><Text style={W} value={yt.clientId} onCommit={(v) => patch({ youtube: { clientId: v } })} placeholder="…apps.googleusercontent.com" /><span className="hint">Built in, shared by every YouTube target. Change only for your own Google Cloud project.</span></Row>
                <Row label="OAuth client secret"><Text style={W} type="password" value={yt.clientSecret} onCommit={(v) => patch({ youtube: { clientSecret: v } })} /></Row>
              </>}
            </Panel>
          </>
        )}
        {sec === "captions" && (
          <Panel title="Captions" sub="the default template">
            <Row label="Default template"><select className="input" style={W} value={st.captionStyle?.name} onChange={(e) => { const t = (st.captionTemplates || []).find((x) => x.name === e.target.value); if (t) patch({ captionStyle: t }); }}>{(st.captionTemplates || []).map((x) => <option key={x.name} value={x.name}>{x.name}</option>)}</select><Btn onClick={() => go("style")}>Edit templates</Btn></Row>
            <span className="hint">Each template sets the captions and the title. Pick one per reel in its inspector.</span>
          </Panel>
        )}
        {sec === "posters" && (
          <Panel title="Posters">
            <Row label="Speaker photo (Higgsfield media id)"><Text style={W} value={st.posterReferenceMedia} onCommit={(v) => patch({ posterReferenceMedia: v })} placeholder="f6a6e81a-…" /><span className="hint">Upload the photo in Claude Code and paste its media id.</span></Row>
            <Row label="WhatsApp link"><Text style={W} value={st.whatsappLink} onCommit={(v) => patch({ whatsappLink: v })} placeholder="https://chat.whatsapp.com/…" /><span className="hint">default QR for new posters</span></Row>
            <Row label="Palettes used"><Text area rows={4} style={W} value={(st.posterHistory || []).join("\n")} onCommit={(v) => patch({ posterHistory: v.split("\n").map((x) => x.trim()).filter(Boolean) })} /><span className="hint">one per line · Claude avoids these</span></Row>
            <span className="hint">Claude draws them with your Higgsfield connection. Three variants cost about nine credits.</span>
          </Panel>
        )}
        {sec === "mcp" && (
          <>
            <Panel title="Claude Code" sub="lets Claude drive Cuttar">
              <div className="kv"><span>Endpoint</span><span className="mono">{s.mcp?.url || "off"}</span><span>Token</span><span className="mono ell">{s.mcp?.token}</span></div>
              <span className="label">Run this once:</span>
              <div className="log" style={{ userSelect: "text" }}>{s.mcp ? `claude mcp add --transport http cuttar ${s.mcp.url} --header "Authorization: Bearer ${s.mcp.token}"` : ""}</div>
              <span className="hint">Then ask Claude Code: "add this lecture and approve everything scoring 8".</span>
            </Panel>
            <Panel title="Terminal" sub="the same actions as commands">
              <div className="log" style={{ userSelect: "text" }}>{`cuttar add https://youtube.com/watch?v=… language=nl
cuttar status
cuttar approve_above sourceId=s-… score=8
cuttar render sourceId=s-…
cuttar autofill days=14
cuttar headless        # run without a window (launchd)`}</div>
              <span className="hint">Data folder: {s.paths.dataDir}</span>
            </Panel>
          </>
        )}
      </div>
    </div>
  );
}
