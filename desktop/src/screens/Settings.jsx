import { useState } from "react";
import { useStore, tryAct, FORMATS } from "../store.js";
import { Panel, Btn, Check, Text, Seg, Dot, Brand } from "../ui.jsx";

const SECTIONS = [["general", "General"], ["engines", "Engines and brain"], ["channels", "Channels"], ["captions", "Captions"], ["posters", "Posters"], ["mcp", "CLI and MCP"]];

export default function Settings() {
  const s = useStore();
  const st = s.settings;
  const [sec, setSec] = useState("channels");
  const [busy, setBusy] = useState(false);
  const patch = (p, msg) => tryAct("settings", { patch: p }, msg);
  const yt = st.youtube || {};
  const cs = st.captionStyle || {};
  const Row = ({ label, children, hint }) => <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12.5 }}><span className="muted" style={{ width: 150, flexShrink: 0 }}>{label}</span>{children}{hint && <span className="hint">{hint}</span>}</div>;
  const W = { width: 300 };

  return (
    <div className="main">
      <div className="panel" style={{ width: 200, flexShrink: 0 }}>
        <div className="panel-head">Settings</div>
        {SECTIONS.map(([k, l]) => <div key={k} className={`nav-item ${sec === k ? "on" : ""}`} style={{ padding: "7px 10px" }} onClick={() => setSec(k)}>{l}</div>)}
        <span className="grow" />
        <div className="panel-foot" style={{ flexDirection: "column", alignItems: "stretch", gap: 3, fontSize: 12, color: "var(--muted)" }}><span>Cuttar 0.1</span><span className="ell">{s.paths.dataDir}</span></div>
      </div>
      <div className="col grow">
        {sec === "general" && (
          <Panel title="General">
            <Row label="Output folder"><Text style={W} value={st.outDir} onCommit={(v) => patch({ outDir: v })} /><Btn small onClick={() => tryAct("open", { path: st.outDir })}>Open</Btn></Row>
            <Row label="Max reel length"><input className="input" style={{ width: 70 }} type="number" value={st.maxReelS} onChange={(e) => patch({ maxReelS: +e.target.value })} /><span className="hint">seconds · the hard limit for Claude and the trimmer</span></Row>
            <Row label="Keep candidates scoring"><input className="input" style={{ width: 70 }} type="number" min="1" max="10" value={st.minScore} onChange={(e) => patch({ minScore: +e.target.value })} /><span className="hint">and above</span></Row>
            <Row label="Auto-approve at"><input className="input" style={{ width: 70 }} type="number" min="0" max="10" value={st.autoApproveScore} onChange={(e) => patch({ autoApproveScore: +e.target.value })} /><span className="hint">0 = off · used by autopilot and new sources</span></Row>
            <Row label="Find reels automatically"><Check label="after every transcript" checked={st.autoDetect} onChange={(v) => patch({ autoDetect: v })} /></Row>
            <Row label="Render formats">{FORMATS.map(([v, l]) => <Check key={v} label={l} checked={(st.formats || []).includes(v)} onChange={(on) => patch({ formats: on ? [...new Set([...(st.formats || []), v])] : (st.formats || []).filter((f) => f !== v) })} />)}</Row>
            <Row label="Parallel jobs"><input className="input" style={{ width: 70 }} type="number" min="1" max="4" value={st.parallelJobs} onChange={(e) => patch({ parallelJobs: +e.target.value })} /><span className="hint">takes effect after a restart · one transcript at a time regardless</span></Row>
            <Row label="Channel name"><Text style={W} value={st.channelName} onCommit={(v) => patch({ channelName: v })} placeholder="shown as the watermark" /></Row>
            <Row label="Glossary"><Text style={W} value={(st.glossary || []).join(", ")} onCommit={(v) => patch({ glossary: v.split(",").map((x) => x.trim()).filter(Boolean) })} /><span className="hint">names and terms whisper should spell right</span></Row>
          </Panel>
        )}
        {sec === "engines" && (
          <>
            <Panel title="Transcription · whisper on MLX" right={<span style={{ display: "flex", gap: 6, alignItems: "center", fontWeight: 500 }}><Dot c={s.tools.whisper ? "mint" : "coral"} />{s.tools.whisper || "not installed"}</span>}>
              <Row label="Model"><select className="input" style={W} value={st.whisperModel} onChange={(e) => patch({ whisperModel: e.target.value })}>{["mlx-community/whisper-large-v3-turbo", "mlx-community/whisper-large-v3-mlx", "mlx-community/whisper-medium-mlx", "mlx-community/whisper-small-mlx"].map((m) => <option key={m} value={m}>{m.split("/").pop()}</option>)}</select><span className="hint">large-v3-turbo: 1.6 GB, ~12× realtime, all languages</span></Row>
              <Row label="Fallback after OOM"><select className="input" style={W} value={st.whisperFallback} onChange={(e) => patch({ whisperFallback: e.target.value })}>{["", "mlx-community/whisper-medium-mlx", "mlx-community/whisper-small-mlx"].map((m) => <option key={m} value={m}>{m ? m.split("/").pop() : "none"}</option>)}</select></Row>
              {!s.tools.whisper && <Row label="Install"><Btn primary disabled={busy} onClick={async () => { setBusy(true); await tryAct("install_whisper", {}, "Installed"); setBusy(false); }}>{busy ? "Installing…" : "uv tool install mlx-whisper"}</Btn><span className="hint">needs uv {s.tools.uv ? "✓" : "(brew install uv)"}</span></Row>}
            </Panel>
            <Panel title="Reel brain">
              <Row label="Brain"><Seg value={st.brain} onChange={(v) => patch({ brain: v })} options={[["claude", "Claude CLI"], ["ollama", "Ollama (offline)"]]} /></Row>
              <Row label="claude binary"><Text style={W} value={st.claudeBin} onCommit={(v) => patch({ claudeBin: v })} /><span style={{ display: "flex", gap: 6, alignItems: "center" }} className="hint"><Dot c={s.tools.claude ? "mint" : "coral"} />{s.tools.claude || "not found on PATH"}</span></Row>
              <Row label="Ollama model"><Text style={W} value={st.ollamaModel} onCommit={(v) => patch({ ollamaModel: v })} /><span style={{ display: "flex", gap: 6, alignItems: "center" }} className="hint"><Dot c={s.tools.ollama ? "mint" : "muted"} />{s.tools.ollama || "ollama not found"}</span></Row>
              <Row label="Translate captions to"><select className="input" style={{ width: 140 }} value={st.translateTo} onChange={(e) => patch({ translateTo: e.target.value })}><option value="">off</option><option value="en">English</option><option value="nl">Dutch</option><option value="ar">Arabic</option><option value="ur">Urdu</option></select><span className="hint">a second caption line, written by the brain while it picks reels</span></Row>
            </Panel>
            <Panel title="Tools found">
              <div className="grid2" style={{ fontSize: 12.5 }}>{[["ffmpeg", "ffmpeg"], ["ytdlp", "yt-dlp"], ["whisper", "mlx_whisper"], ["claude", "claude"], ["ollama", "ollama"], ["uv", "uv"]].map(([k, l]) => <span key={k} style={{ display: "flex", gap: 6, alignItems: "center" }}><Dot c={s.tools[k] ? "mint" : "coral"} />{l} <span className="muted ell">{s.tools[k] || "missing"}</span></span>)}</div>
              {s.tools.ffmpeg && !s.tools.ffmpegAss && <span className="coral" style={{ fontSize: 12.5 }}>This ffmpeg was built without libass, so captions cannot be burned in. `brew reinstall ffmpeg` gives the standard build.</span>}
              <span className="hint">Missing: `brew install ffmpeg yt-dlp uv`, `uv tool install mlx-whisper`, `npm i -g @anthropic-ai/claude-code`.</span>
            </Panel>
          </>
        )}
        {sec === "channels" && (
          <>
            <Panel title={<span style={{ display: "flex", alignItems: "center", gap: 6 }}><Brand id="youtube" />YouTube</span>} sub="Data API v3 · OAuth desktop app · Shorts upload with publishAt" right={<span style={{ display: "flex", gap: 6, alignItems: "center", fontWeight: 500 }}><Dot c={yt.refreshToken ? "mint" : "muted"} />{yt.refreshToken ? `connected · ${yt.channelTitle}` : "not connected"}</span>}>
              <Row label="OAuth client id"><Text style={W} value={yt.clientId} onCommit={(v) => patch({ youtube: { clientId: v } })} placeholder="…apps.googleusercontent.com" /></Row>
              <Row label="OAuth client secret"><Text style={W} type="password" value={yt.clientSecret} onCommit={(v) => patch({ youtube: { clientSecret: v } })} /></Row>
              <Row label="">{yt.refreshToken ? <Btn onClick={() => tryAct("youtube_disconnect", {}, "Disconnected")}>Disconnect</Btn> : <Btn primary disabled={!yt.clientId || !yt.clientSecret || busy} onClick={async () => { setBusy(true); await tryAct("youtube_connect", {}, "YouTube connected"); setBusy(false); }}>{busy ? "Waiting for Google…" : "Connect Google account"}</Btn>}<span className="hint">Google Cloud console → APIs → YouTube Data API v3 enabled → Credentials → OAuth client, type Desktop app. The consent page opens in your browser.</span></Row>
              <Row label="Default privacy"><Seg value={yt.privacy || "public"} onChange={(v) => patch({ youtube: { privacy: v } })} options={[["public", "Public"], ["unlisted", "Unlisted"], ["private", "Private"]]} /><span className="hint">scheduled uploads sit private until publishAt</span></Row>
              <Row label="Category id"><Text style={{ width: 70 }} value={yt.categoryId} onCommit={(v) => patch({ youtube: { categoryId: v } })} /><span className="hint">27 = Education, 29 = Nonprofits</span></Row>
              <Row label="Title suffix"><Text style={W} value={yt.titleSuffix} onCommit={(v) => patch({ youtube: { titleSuffix: v } })} /></Row>
              <Row label="Description footer"><Text style={W} value={yt.descriptionFooter} onCommit={(v) => patch({ youtube: { descriptionFooter: v } })} /><span className="hint">{"{source_url}"} is replaced</span></Row>
            </Panel>
            <Panel title={<span style={{ display: "flex", alignItems: "center", gap: 8 }}><Brand id="tiktok" /><Brand id="instagram" /><Brand id="facebook" />TikTok · Instagram · Facebook</span>} right={<span style={{ display: "flex", gap: 6, alignItems: "center", fontWeight: 500 }}><Dot c="muted" />not linked</span>}>
              <span className="hint">Both need an approved developer app (TikTok Content Posting API, Meta Graph API for Reels). Until then a confirmed post for these channels fails with the path of the rendered file and its caption.txt, ready for a manual upload. The calendar still plans them.</span>
            </Panel>
          </>
        )}
        {sec === "captions" && (
          <Panel title="Caption defaults" sub="applied when a reel has no style of its own">
            <Row label="Preset"><Seg value={cs.preset} onChange={(v) => patch({ captionStyle: { preset: v } })} options={[["karaoke", "Karaoke"], ["clean", "Clean"], ["boxed", "Boxed"], ["outline", "Outline"], ["lower", "Lower third"]]} /></Row>
            <Row label="Size"><input type="range" className="slider" style={{ width: 200 }} min="24" max="72" value={cs.size} onChange={(e) => patch({ captionStyle: { size: +e.target.value } })} /><span>{cs.size} px of 1920</span></Row>
            <Row label="Position from bottom"><input type="range" className="slider" style={{ width: 200 }} min="8" max="50" value={cs.positionPct} onChange={(e) => patch({ captionStyle: { positionPct: +e.target.value } })} /><span>{cs.positionPct}%</span></Row>
            <Row label="Words per line"><input type="range" className="slider" style={{ width: 200 }} min="1" max="8" value={cs.wordsPerLine} onChange={(e) => patch({ captionStyle: { wordsPerLine: +e.target.value } })} /><span>{cs.wordsPerLine}</span></Row>
            <Row label="Highlight colour">{["#3EF2A3", "#F52ACB", "#FFC24D", "#37D2E8", "#FFFFFF"].map((c) => <span key={c} onClick={() => patch({ captionStyle: { highlight: c } })} style={{ width: 16, height: 16, borderRadius: 3, background: c, cursor: "pointer", outline: cs.highlight === c ? "2px solid #fff" : "none" }} />)}</Row>
            <Row label="Font"><Text style={W} value={cs.font} onCommit={(v) => patch({ captionStyle: { font: v } })} /><span className="hint">an installed font name; Arabic and Urdu use Geeza Pro</span></Row>
            <Row label=""><Check label="Translated second line" checked={cs.translation} onChange={(v) => patch({ captionStyle: { translation: v } })} /></Row>
            <Row label=""><Check label="Hook at the top for the first 2.5 s" checked={cs.hook} onChange={(v) => patch({ captionStyle: { hook: v } })} /></Row>
            <Row label=""><Check label="Watermark with the channel name" checked={cs.watermark} onChange={(v) => patch({ captionStyle: { watermark: v } })} /></Row>
          </Panel>
        )}
        {sec === "posters" && (
          <Panel title="Posters">
            <Row label="Speaker photo (Higgsfield media id)"><Text style={W} value={st.posterReferenceMedia} onCommit={(v) => patch({ posterReferenceMedia: v })} placeholder="f6a6e81a-…" /><span className="hint">from a media_upload in Claude Code; keeps his face accurate</span></Row>
            <Row label="WhatsApp link"><Text style={W} value={st.whatsappLink} onCommit={(v) => patch({ whatsappLink: v })} placeholder="https://chat.whatsapp.com/…" /><span className="hint">default QR for new posters</span></Row>
            <Row label="Palettes used"><Text area rows={4} style={W} value={(st.posterHistory || []).join("\n")} onCommit={(v) => patch({ posterHistory: v.split("\n").map((x) => x.trim()).filter(Boolean) })} /><span className="hint">one per line · Claude avoids these</span></Row>
            <span className="hint">Generation runs `claude -p` with the Higgsfield MCP you already have in Claude Code (model gpt_image_2_5, 2:3, high). Three variants ≈ 9 credits. The QR is pasted locally with `tools/add_qr.py` via uv.</span>
          </Panel>
        )}
        {sec === "mcp" && (
          <>
            <Panel title="MCP for Claude" sub="Claude Code drives Cuttar over loopback HTTP">
              <div className="kv"><span>Endpoint</span><span className="mono">{s.mcp?.url || "off"}</span><span>Token</span><span className="mono ell">{s.mcp?.token}</span></div>
              <span className="label">Add it to Claude Code once:</span>
              <div className="log" style={{ userSelect: "text" }}>{s.mcp ? `claude mcp add --transport http cuttar ${s.mcp.url} --header "Authorization: Bearer ${s.mcp.token}"` : ""}</div>
              <span className="hint">Then in any Claude Code session: "add this lecture to Cuttar and approve everything scoring 8", "render the approved reels", "schedule them", "make the poster for Friday".</span>
            </Panel>
            <Panel title="CLI" sub="the same actions from a terminal">
              <div className="log" style={{ userSelect: "text" }}>{`cuttar add https://youtube.com/watch?v=… language=nl
cuttar status
cuttar approve_above sourceId=s-… score=8
cuttar render sourceId=s-…
cuttar autofill days=14
cuttar headless        # run without a window (launchd)`}</div>
              <span className="hint">Install: `cargo install --path cli` or symlink target/release/cuttar into ~/.local/bin. Data dir: {s.paths.dataDir}</span>
            </Panel>
          </>
        )}
      </div>
    </div>
  );
}
