import { useCallback, useEffect, useMemo, useState } from "react";
import {
  api,
  formatTime,
  type AppInfo,
  type HistoryEntry,
  type Mode,
  type Provider,
  type Settings,
  type Status,
} from "./api";

type Tab = "dictation" | "models" | "ai" | "dictionary" | "history" | "settings" | "about";

const EMPTY_STATUS: Status = {
  state: "idle",
  transcript: "",
  latency_ms: 0,
  stt_ms: 0,
  cleanup_ms: 0,
  rms: 0,
  paused: false,
};

function loadWithRetry<T>(load: () => Promise<T>, apply: (value: T) => void, tries = 6): void {
  load()
    .then(apply)
    .catch(() => {
      if (tries > 1) window.setTimeout(() => loadWithRetry(load, apply, tries - 1), 300);
    });
}

const STATE_META: Record<
  Status["state"],
  { label: string; hint: string; dot: string; text: string; glow: string }
> = {
  idle: {
    label: "Ready",
    hint: "Hold Right Ctrl anywhere to dictate",
    dot: "bg-emerald-400",
    text: "text-emerald-600",
    glow: "from-emerald-400/10",
  },
  listening: {
    label: "Listening",
    hint: "Speak now — release Right Ctrl to transcribe",
    dot: "bg-rose-400",
    text: "text-rose-500",
    glow: "from-rose-400/15",
  },
  processing: {
    label: "Transcribing",
    hint: "Running local Whisper on your speech",
    dot: "bg-amber-400",
    text: "text-amber-600",
    glow: "from-amber-400/10",
  },
  done: {
    label: "Pasted",
    hint: "Clean text was pasted into the focused app",
    dot: "bg-indigo-400",
    text: "text-indigo-600",
    glow: "from-indigo-400/10",
  },
  error: {
    label: "Error",
    hint: "Something went wrong — check Debug",
    dot: "bg-red-400",
    text: "text-red-600",
    glow: "from-red-400/10",
  },
  paused: {
    label: "Paused",
    hint: "Dictation is paused — resume to start listening",
    dot: "bg-slate-400",
    text: "text-slate-600",
    glow: "from-slate-400/10",
  },
};

const NAV: { id: Tab; label: string; glyph: string }[] = [
  { id: "dictation", label: "Dictation", glyph: "M12 18v3M8 21h8M5 10a7 7 0 0 1 14 0v4a7 7 0 0 1-14 0v-4Z" },
  { id: "models", label: "Models", glyph: "M4 7h16M4 12h16M4 17h10" },
  { id: "ai", label: "AI Polish", glyph: "M12 3l1.8 4.6L18 9l-4.2 1.4L12 15l-1.8-4.6L6 9l4.2-1.4L12 3Z" },
  { id: "dictionary", label: "Dictionary", glyph: "M4 5h9a3 3 0 0 1 3 3v11H7a3 3 0 0 1-3-3V5Zm12 3h4v11h-4" },
  { id: "history", label: "History", glyph: "M12 8v4l3 2M21 12a9 9 0 1 1-9-9 9 9 0 0 1 9 9Z" },
  { id: "settings", label: "Settings", glyph: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm8-3a8 8 0 0 0-.2-1.7l2-1.5-2-3.4-2.4 1a8 8 0 0 0-2.9-1.7L14 2h-4l-.5 2.7a8 8 0 0 0-2.9 1.7l-2.4-1-2 3.4 2 1.5A8 8 0 0 0 4 12c0 .6.1 1.1.2 1.7l-2 1.5 2 3.4 2.4-1a8 8 0 0 0 2.9 1.7L10 22h4l.5-2.7a8 8 0 0 0 2.9-1.7l2.4 1 2-3.4-2-1.5c.1-.6.2-1.1.2-1.7Z" },
  { id: "about", label: "Debug", glyph: "M12 8v4m0 4h.01M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("dictation");
  const [status, setStatus] = useState<Status>(EMPTY_STATUS);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [waves, setWaves] = useState<number[]>([]);
  const [dict, setDict] = useState<{ k: string; v: string }[]>([]);
  const [snippets, setSnippets] = useState<{ k: string; v: string }[]>([]);
  const [toast, setToast] = useState("");
  const [query, setQuery] = useState("");

  const notify = useCallback((message: string) => {
    setToast(message);
    window.setTimeout(() => setToast(""), 2200);
  }, []);

  const reloadHistory = useCallback(() => {
    api.getHistory().then(setHistory).catch(() => {});
  }, []);

  useEffect(() => {
    loadWithRetry(api.getSettings, setSettings);
    loadWithRetry(api.getAppInfo, setInfo);
    loadWithRetry(api.getHistory, setHistory);
    loadWithRetry(api.getDictionary, (d) => {
      setDict(Object.entries(d).map(([k, v]) => ({ k, v })));
    });
    loadWithRetry(api.getSnippets, (s) => {
      setSnippets(Object.entries(s).map(([k, v]) => ({ k, v })));
    });

    const timer = window.setInterval(() => {
      api.getStatus().then(setStatus).catch(() => {});
    }, 140);
    const waveTimer = window.setInterval(() => {
      api.getWaveform().then(setWaves).catch(() => {});
    }, 80);
    return () => {
      window.clearInterval(timer);
      window.clearInterval(waveTimer);
    };
  }, [reloadHistory]);

  const meta = STATE_META[status.state] ?? STATE_META.idle;

  const patchSettings = (patch: Partial<Settings>) =>
    setSettings((prev) => (prev ? { ...prev, ...patch } : prev));
  const patchLlm = (patch: Partial<Settings["llm"]>) =>
    setSettings((prev) => (prev ? { ...prev, llm: { ...prev.llm, ...patch } } : prev));

  const save = async () => {
    if (!settings) return;
    await api.saveSettings(settings);
    notify("Settings saved");
  };

  const saveTables = async () => {
    const toObject = (rows: { k: string; v: string }[]) =>
      Object.fromEntries(rows.filter((r) => r.k.trim()).map((r) => [r.k.trim(), r.v]));
    await Promise.all([api.saveDictionary(toObject(dict)), api.saveSnippets(toObject(snippets))]);
    notify("Dictionary and snippets saved");
  };

  const togglePaused = async () => {
    const next = await api.setPaused(!status.paused);
    setStatus((s) => ({ ...s, paused: next, state: next ? "paused" : "idle" }));
  };

  const filteredHistory = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter(
      (h) =>
        h.clean_text.toLowerCase().includes(q) || h.raw_text.toLowerCase().includes(q)
    );
  }, [history, query]);

  return (
    <div className="flex h-full">
      <aside className="flex w-60 shrink-0 flex-col border-r border-slate-200/80 bg-white/70 px-4 py-5">
        <div className="mb-7 flex items-center gap-3 px-2">
          <div className="grid h-9 w-9 place-items-center rounded-xl bg-gradient-to-br from-indigo-400 to-emerald-400 shadow-lg shadow-indigo-500/20">
            <svg viewBox="0 0 24 24" className="h-5 w-5 text-ink" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round">
              <path d="M5 10a7 7 0 0 1 14 0v4a7 7 0 0 1-14 0v-4Z" />
              <path d="M12 3v2M9 10v4M12 9v6M15 10v4" />
            </svg>
          </div>
          <div>
            <div className="text-sm font-semibold tracking-tight">FlowLocal</div>
            <div className="text-[11px] text-slate-500">offline dictation</div>
          </div>
        </div>

        <nav className="space-y-1">
          {NAV.map((item) => (
            <button
              key={item.id}
              onClick={() => setTab(item.id)}
              className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-sm transition ${
                tab === item.id
                  ? "bg-indigo-50 text-slate-900 shadow-sm"
                  : "text-slate-500 hover:bg-slate-50 hover:text-slate-900"
              }`}
            >
              <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                <path d={item.glyph} />
              </svg>
              {item.label}
            </button>
          ))}
        </nav>

        <div className="mt-auto space-y-3">
          <div className="rounded-xl border border-slate-200/80 bg-white p-3">
            <div className="flex items-center justify-between text-xs">
              <span className="text-slate-500">Dictation</span>
              <span className={status.paused ? "text-amber-600" : "text-emerald-600"}>
                {status.paused ? "paused" : "active"}
              </span>
            </div>
            <button onClick={togglePaused} className="btn-ghost mt-2 w-full py-1.5 text-xs">
              {status.paused ? "Resume" : "Pause"}
            </button>
          </div>
          <div className="px-2 text-[11px] text-slate-400">v{info?.version ?? "0.1.0"} · 100% local</div>
        </div>
      </aside>

      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-4xl px-8 py-8">
          {tab === "dictation" && (
            <DictationPage
              status={status}
              meta={meta}
              settings={settings}
              info={info}
              waves={waves}
              onStart={() => api.startPtt()}
              onStop={() => api.stopPtt()}
              onHandsFree={() => api.toggleHandsFree()}
              onCancel={() => api.cancel()}
              onCopy={() => api.copyText(status.transcript)}
              onPause={togglePaused}
              notify={notify}
            />
          )}

          {tab === "models" && settings && info && (
            <ModelsPage
              settings={settings}
              info={info}
              onPick={(model) => patchSettings({ active_model: model })}
              onSave={save}
            />
          )}

          {tab === "ai" && settings && (
            <AiPage settings={settings} patchLlm={patchLlm} onSave={save} notify={notify} />
          )}

          {tab === "dictionary" && (
            <DictionaryPage
              dict={dict}
              snippets={snippets}
              setDict={setDict}
              setSnippets={setSnippets}
              onSave={saveTables}
            />
          )}

          {tab === "history" && (
            <HistoryPage
              history={filteredHistory}
              total={history.length}
              query={query}
              setQuery={setQuery}
              onCopy={async (text) => {
                await api.copyText(text);
                notify("Copied to clipboard");
              }}
              onClear={async () => {
                await api.clearHistory();
                reloadHistory();
                notify("History cleared");
              }}
            />
          )}

          {tab === "settings" && settings && info && (
            <SettingsPage settings={settings} patchSettings={patchSettings} info={info} onSave={save} />
          )}

          {tab === "about" && <DebugPage info={info} status={status} onQuit={() => api.quit()} />}
        </div>
      </main>

      <div
        className={`pointer-events-none fixed bottom-6 left-1/2 -translate-x-1/2 rounded-full border border-slate-200 bg-white px-4 py-2 text-xs text-slate-800 shadow-xl transition ${
          toast ? "opacity-100" : "opacity-0"
        }`}
      >
        {toast}
      </div>
    </div>
  );
}

function PageHead({ title, desc }: { title: string; desc: string }) {
  return (
    <div className="mb-6">
      <h1 className="text-xl font-semibold tracking-tight text-slate-900">{title}</h1>
      <p className="mt-1 text-sm text-slate-500">{desc}</p>
    </div>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!on)}
      className={`relative h-6 w-11 rounded-full transition ${on ? "bg-indigo-500" : "bg-slate-300"}`}
    >
      <span
        className={`absolute top-0.5 h-5 w-5 rounded-full bg-white transition-all ${
          on ? "left-[22px]" : "left-0.5"
        }`}
      />
    </button>
  );
}

function DictationPage({
  status,
  meta,
  settings,
  info,
  waves,
  onStart,
  onStop,
  onHandsFree,
  onCancel,
  onCopy,
  onPause,
  notify,
}: {
  status: Status;
  meta: (typeof STATE_META)[Status["state"]];
  settings: Settings | null;
  info: AppInfo | null;
  waves: number[];
  onStart: () => void;
  onStop: () => void;
  onHandsFree: () => void;
  onCancel: () => void;
  onCopy: () => void;
  onPause: () => void;
  notify: (m: string) => void;
}) {
  const isLive = status.state === "listening";
  const recent = waves.slice(-32);
  const waveBars = Array.from({ length: 32 }, (_, i) => recent[i] ?? 0);
  const waveMax = Math.max(0.03, ...waveBars);
  return (
    <>
      <PageHead title="Dictation" desc="Press and hold the hotkey in any app, speak, then release." />

      <div className={`card relative overflow-hidden p-7`}>
        <div className={`pointer-events-none absolute inset-x-0 -top-24 h-48 bg-gradient-to-b ${meta.glow} to-transparent blur-2xl`} />
        <div className="relative flex flex-wrap items-center justify-between gap-4">
          <div>
            <div className="flex items-center gap-2.5">
              <span className={`h-2.5 w-2.5 rounded-full ${meta.dot} ${isLive ? "animate-pulse" : ""}`} />
              <span className={`text-2xl font-semibold tracking-tight ${meta.text}`}>{meta.label}</span>
            </div>
            <p className="mt-1 text-sm text-slate-500">{meta.hint}</p>
          </div>

          <div className="flex h-[52px] items-end gap-[3px]">
            {waveBars.map((v, i) => {
              const h = isLive ? Math.max(4, Math.pow(Math.min(1, v / waveMax), 0.7) * 48) : 4;
              return (
                <span
                  key={i}
                  className={`w-[4px] rounded-full transition-[height] duration-75 ${
                    isLive ? "bg-gradient-to-t from-indigo-500 to-emerald-400" : "bg-slate-200"
                  }`}
                  style={{ height: `${h}px` }}
                />
              );
            })}
          </div>
        </div>

        <div className="relative mt-6 flex flex-wrap gap-2">
          <button onClick={onStart} className="btn-primary" disabled={status.paused || isLive}>
            Start listening
          </button>
          <button onClick={onStop} className="btn-ghost" disabled={!isLive}>
            Stop
          </button>
          <button onClick={onHandsFree} className="btn-ghost">
            Hands-free
          </button>
          <button onClick={onCancel} className="btn-ghost" disabled={!isLive}>
            Cancel (Esc)
          </button>
          <button onClick={onPause} className="btn-ghost ml-auto">
            {status.paused ? "Resume dictation" : "Pause dictation"}
          </button>
        </div>

        <div className="relative mt-7 grid grid-cols-3 gap-3 text-center">
          {[
            { k: "STT", v: `${status.stt_ms} ms` },
            { k: "Cleanup", v: `${status.cleanup_ms} ms` },
            { k: "Total", v: `${status.latency_ms} ms` },
          ].map((s) => (
            <div key={s.k} className="rounded-xl border border-slate-200/80 bg-slate-50 px-3 py-2.5">
              <div className="text-[11px] uppercase tracking-wide text-slate-500">{s.k}</div>
              <div className="mt-0.5 font-mono text-sm text-slate-800">{s.v}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="mt-5 grid gap-5 md:grid-cols-2">
        <div className="card p-5">
          <div className="mb-2 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-slate-800">Last transcript</h2>
            <button
              className="text-xs text-indigo-600 hover:text-indigo-500 disabled:opacity-40"
              disabled={!status.transcript}
              onClick={async () => {
                await onCopy();
                notify("Transcript copied");
              }}
            >
              Copy
            </button>
          </div>
          <p className="selectable min-h-[84px] whitespace-pre-wrap text-sm leading-relaxed text-slate-600">
            {status.transcript || "Nothing yet. Hold the hotkey and say something."}
          </p>
        </div>

        <div className="card p-5">
          <h2 className="mb-3 text-sm font-semibold text-slate-800">How to use</h2>
          <ul className="space-y-2.5 text-sm text-slate-500">
            <li className="flex gap-2.5">
              <Key>{settings?.hotkey?.replace("Right ", "R-") ?? "R-Ctrl"}</Key>
              Hold to talk, release to paste.
            </li>
            <li className="flex gap-2.5">
              <Key>2×</Key>
              Double-tap for hands-free mode.
            </li>
            <li className="flex gap-2.5">
              <Key>Esc</Key>
              Cancel the current recording.
            </li>
            <li className="flex gap-2.5">
              <Key>Mic</Key>
              {settings?.microphone || info?.microphones?.[0] || "System default microphone"}
            </li>
          </ul>
        </div>
      </div>
    </>
  );
}

function Key({ children }: { children: React.ReactNode }) {
  return (
    <span className="inline-flex h-6 min-w-6 shrink-0 items-center justify-center rounded-md border border-slate-200 bg-slate-100 px-1.5 font-mono text-[11px] text-slate-600">
      {children}
    </span>
  );
}

function ModelsPage({
  settings,
  info,
  onPick,
  onSave,
}: {
  settings: Settings;
  info: AppInfo;
  onPick: (m: string) => void;
  onSave: () => void;
}) {
  return (
    <>
      <PageHead title="Models" desc="Local Whisper models. Download more by dropping them into the models folder." />
      <div className="card divide-y divide-slate-200/70">
        {info.models.map((model) => (
          <label key={model} className="flex cursor-pointer items-center gap-4 px-5 py-4">
            <input
              type="radio"
              checked={settings.active_model === model}
              onChange={() => onPick(model)}
              className="h-4 w-4 accent-indigo-500"
            />
            <div className="flex-1">
              <div className="font-mono text-sm text-slate-800">{model}</div>
              <div className="text-xs text-slate-500">
                {model.includes("tiny") ? "fastest · lowest accuracy" : model.includes("base") ? "balanced · recommended" : "higher accuracy"}
              </div>
            </div>
            {settings.active_model === model && (
              <span className="rounded-full bg-emerald-500/10 px-2.5 py-1 text-[11px] text-emerald-600">active</span>
            )}
          </label>
        ))}
      </div>
      <div className="mt-4 flex items-center justify-between gap-4">
        <p className="text-xs text-slate-500">Changing the model is applied the next time FlowLocal starts.</p>
        <button className="btn-primary" onClick={onSave}>
          Save model
        </button>
      </div>

      <div className="mt-5 card p-5">
        <h2 className="mb-3 text-sm font-semibold text-slate-800">Hardware</h2>
        <dl className="grid gap-3 text-sm sm:grid-cols-2">
          <Info label="CPU" value={info.cpu} />
          <Info label="Acceleration" value={info.gpu} />
          <Info label="Engine ready" value={info.engine_ready ? "Yes" : "Missing model"} />
          <Info label="Status" value={info.engine_ready ? "Whisper CLI found" : "Run setup script"} />
        </dl>
      </div>
    </>
  );
}

function AiPage({
  settings,
  patchLlm,
  onSave,
  notify,
}: {
  settings: Settings;
  patchLlm: (p: Partial<Settings["llm"]>) => void;
  onSave: () => void;
  notify: (m: string) => void;
}) {
  const llm = settings.llm;
  return (
    <>
      <PageHead title="AI Polish" desc="Optional second pass that fixes grammar and formatting. Off by default." />
      <div className="card space-y-5 p-5">
        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <span className="label">Provider</span>
            <select
              className="field"
              value={llm.provider}
              onChange={(e) => patchLlm({ provider: e.target.value as Provider })}
            >
              <option value="Disabled">Disabled (Layer 1 only)</option>
              <option value="LocalOllama">Local Ollama</option>
              <option value="OpenAiCompatible">OpenAI-compatible</option>
              <option value="Anthropic">Anthropic</option>
            </select>
          </div>
          <div>
            <span className="label">Mode</span>
            <select
              className="field"
              value={llm.active_mode}
              onChange={(e) => patchLlm({ active_mode: e.target.value as Mode })}
            >
              <option value="Raw">Raw</option>
              <option value="Clean">Clean</option>
              <option value="Email">Email</option>
              <option value="Chat">Message / Chat</option>
              <option value="NotesBullets">Notes / Bullets</option>
              <option value="CodeComments">Code comments</option>
              <option value="Custom">Custom</option>
            </select>
          </div>
          <div>
            <span className="label">Base URL</span>
            <input className="field" value={llm.base_url} onChange={(e) => patchLlm({ base_url: e.target.value })} />
          </div>
          <div>
            <span className="label">Model name</span>
            <input className="field" value={llm.model_name} onChange={(e) => patchLlm({ model_name: e.target.value })} />
          </div>
          <div>
            <span className="label">API key (optional)</span>
            <input
              className="field"
              type="password"
              placeholder="Stored in your settings file"
              value={llm.api_key}
              onChange={(e) => patchLlm({ api_key: e.target.value })}
            />
          </div>
          <div>
            <span className="label">Timeout (ms)</span>
            <input
              className="field"
              type="number"
              min={300}
              step={100}
              value={llm.timeout_ms}
              onChange={(e) => patchLlm({ timeout_ms: Number(e.target.value) })}
            />
          </div>
        </div>

        {llm.active_mode === "Custom" && (
          <div>
            <span className="label">Custom prompt</span>
            <textarea
              className="field min-h-[90px]"
              value={llm.custom_prompt}
              onChange={(e) => patchLlm({ custom_prompt: e.target.value })}
            />
          </div>
        )}

        <div className="flex items-center justify-between border-t border-slate-200/80 pt-4">
          <p className="text-xs text-slate-500">
            If the provider does not answer within the timeout, FlowLocal pastes the Layer 1 text.
          </p>
          <button className="btn-primary" onClick={onSave}>
            Save
          </button>
        </div>
      </div>
      <button className="btn-ghost mt-4" onClick={() => notify("Connection test will be available in a later build")}>
        Test connection
      </button>
    </>
  );
}

function DictionaryPage({
  dict,
  snippets,
  setDict,
  setSnippets,
  onSave,
}: {
  dict: { k: string; v: string }[];
  snippets: { k: string; v: string }[];
  setDict: (rows: { k: string; v: string }[]) => void;
  setSnippets: (rows: { k: string; v: string }[]) => void;
  onSave: () => void;
}) {
  return (
    <>
      <PageHead title="Dictionary & Snippets" desc="Bias the model toward names and jargon, and expand spoken triggers." />
      <Table
        title="Personal dictionary"
        hint="Spoken variant → preferred spelling"
        rows={dict}
        setRows={setDict}
        colA="Heard"
        colB="Replace with"
      />
      <div className="h-5" />
      <Table
        title="Snippets"
        hint="Say the trigger phrase → full text is inserted"
        rows={snippets}
        setRows={setSnippets}
        colA="Trigger"
        colB="Expands to"
      />
      <div className="mt-5 flex justify-end">
        <button className="btn-primary" onClick={onSave}>
          Save all
        </button>
      </div>
    </>
  );
}

function Table({
  title,
  hint,
  rows,
  setRows,
  colA,
  colB,
}: {
  title: string;
  hint: string;
  rows: { k: string; v: string }[];
  setRows: (rows: { k: string; v: string }[]) => void;
  colA: string;
  colB: string;
}) {
  const update = (i: number, patch: Partial<{ k: string; v: string }>) =>
    setRows(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  return (
    <div className="card p-5">
      <div className="mb-3">
        <h2 className="text-sm font-semibold text-slate-800">{title}</h2>
        <p className="text-xs text-slate-500">{hint}</p>
      </div>
      <div className="grid grid-cols-[1fr_1.4fr_auto] gap-2 text-[11px] uppercase tracking-wide text-slate-500">
        <span>{colA}</span>
        <span>{colB}</span>
        <span className="w-14" />
      </div>
      <div className="mt-1.5 space-y-2">
        {rows.map((row, i) => (
          <div key={i} className="grid grid-cols-[1fr_1.4fr_auto] gap-2">
            <input className="field" value={row.k} onChange={(e) => update(i, { k: e.target.value })} />
            <input className="field" value={row.v} onChange={(e) => update(i, { v: e.target.value })} />
            <button
              className="btn-ghost w-14 px-0 text-xs text-rose-500"
              onClick={() => setRows(rows.filter((_, idx) => idx !== i))}
            >
              Remove
            </button>
          </div>
        ))}
        {rows.length === 0 && <p className="py-3 text-sm text-slate-500">No entries yet.</p>}
      </div>
      <button className="btn-ghost mt-3 text-xs" onClick={() => setRows([...rows, { k: "", v: "" }])}>
        + Add entry
      </button>
    </div>
  );
}

function HistoryPage({
  history,
  total,
  query,
  setQuery,
  onCopy,
  onClear,
}: {
  history: HistoryEntry[];
  total: number;
  query: string;
  setQuery: (q: string) => void;
  onCopy: (text: string) => void;
  onClear: () => void;
}) {
  return (
    <>
      <PageHead title="History" desc="Every dictation is stored locally. Nothing leaves this machine." />
      <div className="mb-4 flex gap-2">
        <input
          className="field"
          placeholder="Search transcripts…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button className="btn-ghost shrink-0 text-rose-500" onClick={onClear} disabled={total === 0}>
          Clear all
        </button>
      </div>
      <div className="card divide-y divide-slate-200/70">
        {history.length === 0 && (
          <p className="px-5 py-10 text-center text-sm text-slate-500">No transcripts yet.</p>
        )}
        {history.map((h) => (
          <div key={h.id} className="px-5 py-4">
            <div className="mb-1 flex items-center justify-between text-[11px] text-slate-500">
              <span>
                {formatTime(h.timestamp)} · {h.mode} · {h.latency_ms} ms
              </span>
              <button className="text-indigo-600 hover:text-indigo-500" onClick={() => onCopy(h.clean_text)}>
                Copy
              </button>
            </div>
            <p className="selectable whitespace-pre-wrap text-sm text-slate-600">{h.clean_text}</p>
          </div>
        ))}
      </div>
    </>
  );
}

function SettingsPage({
  settings,
  patchSettings,
  info,
  onSave,
}: {
  settings: Settings;
  patchSettings: (p: Partial<Settings>) => void;
  info: AppInfo;
  onSave: () => void;
}) {
  return (
    <>
      <PageHead title="Settings" desc="Hotkeys, microphone and privacy controls." />
      <div className="card space-y-6 p-6">
        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <span className="label">Push-to-talk hotkey</span>
            <input className="field" value={settings.hotkey} onChange={(e) => patchSettings({ hotkey: e.target.value })} />
            <p className="mt-1 text-[11px] text-slate-500">Take effect on restart.</p>
          </div>
          <div>
            <span className="label">Microphone</span>
            <select
              className="field"
              value={settings.microphone ?? ""}
              onChange={(e) => patchSettings({ microphone: e.target.value || null })}
            >
              <option value="">System default</option>
              {info.microphones.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </div>
          <div>
            <span className="label">Language</span>
            <input className="field" value={settings.default_language} onChange={(e) => patchSettings({ default_language: e.target.value })} />
          </div>
          <div>
            <span className="label">History retention</span>
            <select
              className="field"
              value={settings.history_retention_days}
              onChange={(e) => patchSettings({ history_retention_days: Number(e.target.value) })}
            >
              <option value={0}>Keep forever</option>
              <option value={1}>1 day</option>
              <option value={7}>7 days</option>
              <option value={30}>30 days</option>
            </select>
          </div>
        </div>

        <div className="space-y-4 border-t border-slate-200/80 pt-5">
          <Row
            title="Auto punctuation"
            desc="Spoken punctuation and sentence casing in Layer 1 cleanup."
            control={<Toggle on={settings.auto_punctuation} onChange={(v) => patchSettings({ auto_punctuation: v })} />}
          />
          <Row
            title="Launch at login"
            desc="Start FlowLocal automatically when you sign in."
            control={<Toggle on={settings.launch_at_login} onChange={(v) => patchSettings({ launch_at_login: v })} />}
          />
        </div>

        <div className="flex justify-end border-t border-slate-200/80 pt-5">
          <button className="btn-primary" onClick={onSave}>
            Save settings
          </button>
        </div>
      </div>
    </>
  );
}

function Row({ title, desc, control }: { title: string; desc: string; control: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6">
      <div>
        <div className="text-sm text-slate-800">{title}</div>
        <div className="text-xs text-slate-500">{desc}</div>
      </div>
      {control}
    </div>
  );
}

function DebugPage({ info, status, onQuit }: { info: AppInfo | null; status: Status; onQuit: () => void }) {
  return (
    <>
      <PageHead title="About & Debug" desc="Diagnostics for the local engine." />
      <div className="card p-6">
        <dl className="grid gap-4 text-sm sm:grid-cols-2">
          <Info label="Version" value={info?.version ?? "0.1.0"} />
          <Info label="Model" value={info?.model ?? "—"} />
          <Info label="Hotkey" value={info?.hotkey ?? "Right Ctrl"} />
          <Info label="Engine" value={info?.engine_ready ? "Ready" : "Model missing"} />
          <Info label="CPU" value={info?.cpu ?? "—"} />
          <Info label="Acceleration" value={info?.gpu ?? "—"} />
          <Info label="Last STT" value={`${status.stt_ms} ms`} />
          <Info label="Last cleanup" value={`${status.cleanup_ms} ms`} />
          <Info label="Last total" value={`${status.latency_ms} ms`} />
          <Info label="State" value={status.state} />
        </dl>
      </div>
      <PasteTest />
      <div className="mt-5 flex gap-2">
        <button className="btn-ghost text-rose-500" onClick={onQuit}>
          Quit FlowLocal
        </button>
      </div>
      <p className="mt-4 text-xs leading-relaxed text-slate-500">
        Audio is processed in RAM only. The app makes no network requests unless you enable an AI
        provider. Settings, dictionary, snippets and history are stored as JSON in the local
        <span className="font-mono"> data/ </span> folder.
      </p>
    </>
  );
}

function PasteTest() {
  const [text, setText] = useState("FlowLocal terminal paste test");
  const [msg, setMsg] = useState("");
  return (
    <div className="card mt-5 p-6">
      <h2 className="text-sm font-semibold text-slate-900">Terminal paste test</h2>
      <p className="mt-1 text-xs text-slate-500">
        Click the button, then click into your terminal within 3 seconds.
      </p>
      <div className="mt-3 flex gap-2">
        <input className="field" value={text} onChange={(e) => setText(e.target.value)} />
        <button
          className="btn-primary shrink-0"
          onClick={async () => {
            await api.testPaste(text, 3000);
            setMsg("Pasting in 3 seconds — focus your terminal now.");
          }}
        >
          Test paste
        </button>
      </div>
      {msg && <p className="mt-2 text-xs text-indigo-600">{msg}</p>}
    </div>
  );
}

function Info({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-slate-200/70 bg-slate-50 px-3.5 py-2.5">
      <dt className="text-[11px] uppercase tracking-wide text-slate-500">{label}</dt>
      <dd className="mt-0.5 break-words font-mono text-[13px] text-slate-800">{value}</dd>
    </div>
  );
}
