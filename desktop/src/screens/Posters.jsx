import { useState } from "react";
import { useStore, tryAct, fileUrl, ago } from "../store.js";
import { Panel, Btn, Check, Text, Seg, Dot, I } from "../ui.jsx";

const TEMPLATES = {
  "weekly-tafsir": { title: "Studiekring · Tafsir Juzz 'Amma", fields: { subtitle: "elke vrijdag", speaker: "Ustadh Tasneem Sadiq al-Qadri", programme: "20.00 - 20.45 Tafsir les · 20.45 Thee & versnaperingen · 21.00 Individuele salawat & dhikr", location: "Locatie: Coolhaven 238a", note: "Een live meeting-link wordt beschikbaar gesteld", qrCaption: "Scan voor onze WhatsApp-groep" } },
  "qa-evening": { title: "Vragenavond", fields: { subtitle: "stel je vragen", programme: "20.00 - 21.30", qrCaption: "Scan voor onze WhatsApp-groep" } },
  "event": { title: "Bijeenkomst", fields: { qrCaption: "Scan voor onze WhatsApp-groep" } },
  "live": { title: "Live", fields: { subtitle: "live op YouTube en Instagram", qrCaption: "Scan voor de link" } },
};
const nextFriday = () => { const d = new Date(); d.setDate(d.getDate() + ((5 - d.getDay() + 7) % 7 || 7)); return d; };
const nl = (d) => d.toLocaleDateString("nl-NL", { weekday: "long", day: "numeric", month: "long" });

export default function Posters({ go }) {
  const s = useStore();
  const [selId, setSelId] = useState(null);
  const p = s.posters.find((v) => v.id === selId) || [...s.posters].reverse()[0];
  const job = p && s.jobs.find((j) => j.refId === p.id && (j.status === "running" || j.status === "queued"));
  const [start, setStart] = useState("");
  const f = p?.fields || {};
  const patchField = (k, v) => p && tryAct("update_poster", { id: p.id, patch: { fields: { [k]: v } } });
  const create = async (t) => {
    const tpl = TEMPLATES[t];
    const r = await tryAct("create_poster", { template: t, title: tpl.title, fields: { ...tpl.fields, date: nl(nextFriday()), qrLink: s.settings.whatsappLink || "" } });
    if (r) setSelId(r.id);
  };
  const startIso = () => { if (start) return new Date(start).toISOString(); const d = nextFriday(); d.setHours(20, 0, 0, 0); return d.toISOString(); };

  return (
    <div className="main">
      <div className="col" style={{ width: 230, flexShrink: 0 }}>
        <div className="panel" style={{ flexShrink: 0, maxHeight: "50%" }}>
          <div className="panel-head"><span className="grow">Events</span></div>
          <div className="scroll">
            {!s.posters.length && <div className="empty">No posters yet. Pick a template below.</div>}
            {[...s.posters].reverse().map((v) => <div key={v.id} className={`nav-item ${p?.id === v.id ? "on" : ""}`} style={{ padding: "6px 10px", flexDirection: "column", alignItems: "stretch", gap: 2 }} onClick={() => setSelId(v.id)}><b className="ell">{v.title}</b><span className={v.status === "exported" ? "mint" : v.status === "failed" ? "coral" : v.status === "variants" || v.status === "chosen" ? "pink" : "muted"} style={{ fontSize: 13 }}>{v.fields?.date} · {v.status}</span></div>)}
          </div>
        </div>
        <Panel title="Templates" body={false} style={{ flexShrink: 0 }}>
          {Object.entries(TEMPLATES).map(([k, t]) => <div key={k} className="nav-item" style={{ padding: "6px 10px" }} onClick={() => create(k)}><span className="grow">{t.title}</span>{I.plus}</div>)}
        </Panel>
        <Panel title="Used before · avoid" className="grow">
          {(s.settings.posterHistory || []).slice(-8).map((h, i) => <span key={i} style={{ fontSize: 13, color: "var(--text-2)" }}>{h}</span>)}
          <span className="hint">Claude picks palettes outside this list; add to it in Settings › Posters or when exporting.</span>
        </Panel>
      </div>
      {!p ? <div className="panel grow"><div className="empty">Choose a template to start a poster.</div></div> : (
        <>
          <div className="col grow">
            <Panel title="Fields" sub={`${p.template} · fixed text from the template, date set to the coming Friday`} style={{ flexShrink: 0 }} right={<Btn small icon danger onClick={() => confirm("Remove this poster?") && tryAct("remove_poster", { id: p.id })} aria-label="Remove">{I.trash}</Btn>}>
              <div className="grid2">
                <label className="field"><span>Title</span><Text value={p.title} onCommit={(v) => tryAct("update_poster", { id: p.id, patch: { title: v } })} /></label>
                <label className="field"><span>Subtitle</span><Text value={f.subtitle} onCommit={(v) => patchField("subtitle", v)} /></label>
                <label className="field"><span>Date (as printed)</span><Text value={f.date} onCommit={(v) => patchField("date", v)} /></label>
                <label className="field"><span>Speaker</span><Text value={f.speaker} onCommit={(v) => patchField("speaker", v)} /></label>
                <label className="field"><span>Programme</span><Text value={f.programme} onCommit={(v) => patchField("programme", v)} /></label>
                <label className="field"><span>Location</span><Text value={f.location} onCommit={(v) => patchField("location", v)} /></label>
                <label className="field"><span>QR link</span><Text value={f.qrLink} onCommit={(v) => patchField("qrLink", v)} placeholder="https://chat.whatsapp.com/…" /></label>
                <label className="field"><span>QR caption</span><Text value={f.qrCaption} onCommit={(v) => patchField("qrCaption", v)} /></label>
                <label className="field" style={{ gridColumn: "1 / -1" }}><span>Extra brief for Claude (style wishes, colours to try, what to avoid)</span><Text value={f.brief} onCommit={(v) => patchField("brief", v)} placeholder="e.g. autumn palette, no lantern this week" /></label>
              </div>
              <div style={{ display: "flex", gap: 6, alignItems: "center" }}><span className="hint grow">{s.settings.posterReferenceMedia ? "Speaker photo reference set" : "No speaker photo reference (Settings › Posters)"} · each variant gets a blank white panel; the QR is pasted and decoded locally.</span><Btn primary disabled={!!job} onClick={() => tryAct("generate_poster", { id: p.id }, "Claude + Higgsfield are drawing")}>{job ? job.message || "Generating…" : p.variants?.length ? "Generate 3 more" : "Generate 3 variants"}</Btn></div>
            </Panel>
            <div className="panel grow">
              <div className="panel-head"><span>Variants</span><span className="sub">{p.variants?.length ? `${p.variants.length} · QR ${p.qrOk ? "pasted and decoded ✓" : "not verified"}` : job ? job.message : "none yet"}</span><span className="grow" /><Btn small onClick={() => tryAct("open", { path: p.folder })}>Open folder</Btn><Btn small onClick={async () => { const path = prompt("Path to a PNG to add as a variant"); if (path) tryAct("add_variant", { id: p.id, path }, "Variant added"); }}>Add file…</Btn></div>
              <div className="scroll">
                {p.error && <div className="empty coral">{p.error}</div>}
                {!p.variants?.length && !p.error && <div className="empty">{job ? "Drawing… this takes a few minutes. Watch the Queue." : "Generate variants, or add PNGs you made elsewhere."}</div>}
                <div className="variants">{(p.variants || []).map((v, i) => <label key={v} className={`variant ${p.chosen === v ? "on" : ""}`} onClick={() => tryAct("choose_variant", { id: p.id, path: v })}><img src={fileUrl(v)} alt="" /><span style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13.5 }}><input type="radio" name="variant" checked={p.chosen === v} onChange={() => {}} style={{ accentColor: "var(--accent)", margin: 0 }} /><b className="grow">Variant {i + 1}</b><span className="muted ell" style={{ maxWidth: 120 }}>{f.palettes?.[i] || ""}</span></span></label>)}</div>
              </div>
            </div>
          </div>
          <div className="panel" style={{ width: 300, flexShrink: 0 }}>
            <div className="panel-head"><span className="grow">Export</span><span className="sub">{p.chosen ? `variant ${p.variants.indexOf(p.chosen) + 1}` : "choose a variant"}</span></div>
            <div className="panel-body">
              {p.chosen && <div style={{ display: "flex", gap: 10 }}><img src={fileUrl(p.chosen)} alt="" style={{ width: 88, height: 132, objectFit: "cover", borderRadius: 3 }} /><div style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 13 }}><span style={{ display: "flex", gap: 6, alignItems: "center" }}><Dot c={p.qrOk ? "mint" : "muted"} />QR {p.qrOk ? "decodes" : "not verified"}</span><span style={{ display: "flex", gap: 6, alignItems: "center" }}><Dot c={p.outputs?.print ? "mint" : "muted"} />print · jpeg · feed · story</span><span style={{ display: "flex", gap: 6, alignItems: "center" }}><Dot c={p.outputs?.["waiting-16x9"] ? "mint" : "muted"} />waiting videos</span></div></div>}
              <label className="field"><span>Palette of the chosen one (remembered, so it is not repeated)</span><Text value={f.chosenPalette ?? (p.chosen ? f.palettes?.[p.variants.indexOf(p.chosen)] : "")} onCommit={(v) => patchField("chosenPalette", v)} placeholder="emerald + gold, mihrab arch" /></label>
              <Btn primary disabled={!p.chosen} onClick={() => tryAct("export_poster", { id: p.id, palette: f.chosenPalette || f.palettes?.[p.variants.indexOf(p.chosen)] || "" }, "Exported: print, JPEG 1600, feed 4:5, story 9:16")}>Export files</Btn>
              {p.outputs && Object.entries(p.outputs).filter(([k]) => !k.startsWith("waiting") && !k.startsWith("break") && !k.startsWith("ended")).map(([k, v]) => <a key={k} href="#" style={{ fontSize: 13 }} onClick={(e) => { e.preventDefault(); tryAct("open", { path: v, reveal: true }); }}>{k} · {String(v).split("/").pop()}</a>)}
              <div style={{ borderTop: "1px solid var(--rule)", paddingTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
                <span className="label">Waiting video · "Begint zo" loop for the stream</span>
                <label className="field"><span>Event starts (local) · default next Friday 20:00</span><input className="input" type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} /></label>
                <span className="hint">16:9 for YouTube Live and 9:16 for Instagram Live, a burned countdown to the start, plus "we zijn zo terug" and "afgelopen" stills. Render on the day so the countdown is right.</span>
                <Btn disabled={!p.chosen} onClick={() => tryAct("waiting_video", { id: p.id, startAt: startIso(), countdown: true, loopS: 600 }, "Rendering the waiting videos")}>Render waiting videos</Btn>
                {p.outputs && Object.entries(p.outputs).filter(([k]) => k.startsWith("waiting") || k.startsWith("break") || k.startsWith("ended")).map(([k, v]) => <a key={k} href="#" style={{ fontSize: 13 }} onClick={(e) => { e.preventDefault(); tryAct("open", { path: v }); }}>{k} ▸</a>)}
              </div>
              <div style={{ borderTop: "1px solid var(--rule)", paddingTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
                <span className="label">Announce</span>
                <Btn disabled={!p.outputs?.story} onClick={() => { const d = new Date(); d.setDate(d.getDate() + 1); d.setHours(10, 0, 0, 0); tryAct("schedule", { posterId: p.id, channel: "instagram", at: d.toISOString(), title: p.title, caption: `${p.title}\n${f.date || ""}\n${f.programme || ""}\n${f.location || ""}` }, "Planned on the calendar"); go("publish"); }}>Add to the calendar</Btn>
              </div>
              <span className="hint">{p.folder}</span>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
