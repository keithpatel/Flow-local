import { useEffect, useState } from "react";
import { api, type Status } from "./api";

const BARS = 28;

export default function Overlay() {
  const [status, setStatus] = useState<Status>({
    state: "idle",
    transcript: "",
    latency_ms: 0,
    stt_ms: 0,
    cleanup_ms: 0,
    rms: 0,
    paused: false,
  });
  const [waves, setWaves] = useState<number[]>([]);

  useEffect(() => {
    const tick = () => {
      api.getStatus().then(setStatus).catch(() => {});
      api.getWaveform().then(setWaves).catch(() => {});
    };
    tick();
    const timer = window.setInterval(tick, 70);
    return () => window.clearInterval(timer);
  }, []);

  const live = status.state === "listening";
  const processing = status.state === "processing";
  const done = status.state === "done";
  const error = status.state === "error";

  if (!live && !processing && !done && !error) {
    return <div className="h-full w-full" />;
  }

  const recent = waves.slice(-BARS);
  const bars = Array.from({ length: BARS }, (_, i) => recent[i] ?? 0);
  const max = Math.max(0.03, ...bars);

  return (
    <div className="flex h-full w-full items-center justify-center">
      <div
        className={`flex h-11 items-center justify-center gap-[3px] rounded-full border bg-white/90 px-4 shadow-lg backdrop-blur-md transition ${
          error ? "border-red-200 shadow-red-200/40" : "border-slate-200/80 shadow-slate-500/20"
        }`}
      >
        {live &&
          bars.map((v, i) => {
            const h = Math.max(3, Math.pow(Math.min(1, v / max), 0.65) * 28);
            return (
              <span
                key={i}
                className={`w-[3px] shrink-0 rounded-full transition-[height] duration-75 ${
                  v / max > 0.6
                    ? "bg-gradient-to-t from-indigo-500 to-emerald-400"
                    : "bg-indigo-400/80"
                }`}
                style={{ height: `${h}px` }}
              />
            );
          })}

        {processing && (
          <div className="flex items-center gap-1.5">
            {[0, 1, 2].map((i) => (
              <span
                key={i}
                className="h-2 w-2 animate-bounce rounded-full bg-amber-400"
                style={{ animationDelay: `${i * 120}ms` }}
              />
            ))}
          </div>
        )}

        {done && (
          <svg viewBox="0 0 24 24" className="h-5 w-5 text-emerald-500" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round">
            <path d="m5 13 4 4L19 7" />
          </svg>
        )}

        {error && <span className="text-sm font-semibold leading-none text-red-500">!</span>}
      </div>
    </div>
  );
}
