import { tryAct, LANGS } from "../store.js";
import { Btn, Bar, Dot, Field, I } from "../ui.jsx";

const LANG_NAME = Object.fromEntries(LANGS);
const STEPS = ["Download", "Transcribe", "Find reels", "Ready"];
const STEP_OF = { added: 0, downloading: 0, downloaded: 0, transcribing: 1, transcribed: 1, detecting: 2, review: 3, done: 3 };
// A failed lecture says only "failed": the step is the one whose job failed last.
const JOB_STEP = { download: 0, redownload: 0, transcribe: 1, polish: 1, detect: 2 };

/* One line on what a template does to a reel, for the pickers. */
export function templateSummary(t, s) {
  const r = t.reel || {};
  const caps = r.captionsOn === false ? "no captions" : `${r.style || "default"} captions`;
  const track = (s.tracks || []).find((x) => x.id === r.music);
  const music = r.music === "none" ? "no music" : track ? track.title : `${r.music || "default"} music`;
  const names = (r.targets || []).map((id) => (s.targets || []).find((x) => x.id === id)?.name).filter(Boolean);
  const post = r.targets?.[0] === "none" ? "posts nowhere" : r.targets?.length ? `posts to ${names.join(", ") || "removed targets"}` : "posts everywhere";
  return [caps, music, post, t.autoApproveScore > 0 ? `approves ≥ ${t.autoApproveScore}` : "you approve"].join(" · ");
}

/* A lecture on its way in: where it is, the language heard (override it), the template its reels get.
   Floats bottom right; minimised it is one line in the status bar, a click opens it again. */
export default function ImportProgress({ s, sourceId, min, setMin, onClose, go }) {
  const src = s.sources.find((x) => x.id === sourceId);
  if (!src) return null;
  // Only the import's own jobs: renders and covers of its reels come after "ready".
  const jobs = s.jobs.filter((j) => j.refId === src.id && j.kind in JOB_STEP && (j.status === "running" || j.status === "queued"));
  const job = jobs.find((j) => j.status === "running") || jobs[0];
  const reels = s.candidates.filter((c) => c.sourceId === src.id && !c.discarded).length;
  // Without "find reels" a transcript is the end of the road.
  const failed = src.stage === "failed";
  const lastFail = failed && s.jobs.filter((j) => j.refId === src.id && j.status === "failed" && j.kind in JOB_STEP).sort((a, b) => (a.finishedAt < b.finishedAt ? 1 : -1))[0];
  const step = failed ? JOB_STEP[lastFail?.kind] ?? 0 : src.stage === "transcribed" && !jobs.length ? 3 : STEP_OF[src.stage] ?? 0;
  const ready = step === 3 && !jobs.length;
  const pct = ready ? 1 : Math.min(1, (Math.min(step, 2) + (job?.status === "running" ? job.progress || 0 : 0)) / 3);
  const line = failed ? `${STEPS[step]} failed` : ready || step === 3 ? `Ready · ${reels} reel${reels === 1 ? "" : "s"}` : `${STEPS[step]}${job?.status === "running" ? ` ${Math.round((job.progress || 0) * 100)}%` : ""}`;

  if (min) return (
    <button type="button" className="import-pop min" onClick={() => setMin(false)} title="Show the import">
      <Dot c={failed ? "coral" : ready ? "mint" : "pink"} /><span className="ell grow">{src.title}</span><span className={failed ? "coral" : "muted"}>{line}</span><span className="muted">▴</span>
    </button>
  );

  const heard = src.language && LANG_NAME[src.language];
  return (
    <div className="import-pop">
      <div className="panel-head"><span className="grow ell">{src.title}</span><Btn small icon onClick={() => setMin(true)} aria-label="Minimise" title="Minimise">—</Btn><Btn small icon onClick={onClose} aria-label="Close">{I.x}</Btn></div>
      <div className="panel-body" style={{ gap: 10, padding: 12 }}>
        <div className="import-steps">
          {STEPS.map((l, i) => <span key={l} className={i < step || (ready && i === 3) ? "mint" : i === step ? (failed ? "coral" : "pink") : "muted"}><Dot c={i < step || ready ? "mint" : i === step ? (failed ? "coral" : "pink") : ""} />{l}</span>)}
        </div>
        <Bar v={pct} mint={ready} />
        <span className={`ell ${failed ? "coral" : "muted"}`} style={{ fontSize: 13 }}>{failed ? src.error : ready ? `${reels} reel${reels === 1 ? "" : "s"} found${src.autoApproveScore > 0 ? `, scoring ${src.autoApproveScore}+ approved and rendering` : ""}.` : job?.message || job?.label || "Waiting in the queue"}</span>
        <div className="grid2">
          <Field label={src.langOverride ? "Language · set by you" : heard ? `Language · heard ${heard}` : "Language · not heard yet"}>
            <select className="input" value={src.langOverride || "auto"} onChange={(e) => tryAct("set_language", { id: src.id, language: e.target.value }, src.videoPath ? "Transcribing again" : "Language set")}>
              {LANGS.map(([v, l]) => <option key={v} value={v}>{v === "auto" && heard ? `Auto · ${heard}` : l}</option>)}
            </select>
          </Field>
          <Field label="Template">
            <select className="input" value={src.templateId || ""} onChange={async (e) => { const ids = await tryAct("set_source_template", { id: src.id, templateId: e.target.value || null }); const stale = (ids || []).filter((id) => s.renders.some((r) => r.candidateId === id && r.status === "done") || s.jobs.some((j) => j.kind === "render" && j.refId === id && (j.status === "running" || j.status === "queued"))); if (stale.length) tryAct("render", { candidateIds: stale }, `Rendering ${stale.length} reels again`); }}>
              <option value="">No template</option>
              {(s.templates || []).map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
            </select>
          </Field>
        </div>
        {(() => { const t = (s.templates || []).find((x) => x.id === src.templateId); return <span className="hint">{t ? templateSummary(t, s) : (s.templates || []).length ? "New reels take the last copied settings; reels already found keep theirs." : "Save a reel's settings as a template under Reels to start new lectures from it."}</span>; })()}
        {src.videoPath && <span className="hint">Changing the language transcribes the lecture again.</span>}
        <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
          {(reels > 0 || ready) && <Btn small primary onClick={() => { go("reels", { sourceId: src.id }); setMin(true); }}>Open reels</Btn>}
        </div>
      </div>
    </div>
  );
}
