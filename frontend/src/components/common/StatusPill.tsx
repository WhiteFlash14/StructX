"use client";

type Tone = "live" | "soon" | "ok" | "warn" | "danger" | "neutral";

type Props = {
  label: string;
  tone?: Tone;
  dot?: boolean;
};

export function StatusPill({ label, tone = "neutral", dot = false }: Props) {
  return (
    <span className={`status-pill tone-${tone}`}>
      {dot && <span className="status-dot" aria-hidden />}
      {label}
    </span>
  );
}
