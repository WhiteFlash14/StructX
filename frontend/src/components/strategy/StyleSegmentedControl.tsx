"use client";

import type { StrategyStyle } from "@/types/structx";

const OPTIONS: { id: StrategyStyle; label: string; sub: string }[] = [
  {
    id: "tail-heavy",
    label: "Conservative",
    sub: "More budget on far-tail protection",
  },
  {
    id: "balanced",
    label: "Balanced",
    sub: "Tail vs. moderate breakout",
  },
  {
    id: "higher-hit-rate",
    label: "Aggressive",
    sub: "More budget on closer ranges",
  },
];

type Props = {
  value: StrategyStyle;
  onChange: (value: StrategyStyle) => void;
};

export function StyleSegmentedControl({ value, onChange }: Props) {
  return (
    <div className="seg-wrap">
      <div className="seg" role="radiogroup" aria-label="Strategy style">
        {OPTIONS.map((opt) => {
          const active = value === opt.id;
          return (
            <button
              key={opt.id}
              type="button"
              role="radio"
              aria-checked={active}
              className={`seg-item ${active ? "active" : ""}`}
              onClick={() => onChange(opt.id)}
            >
              {opt.label}
            </button>
          );
        })}
      </div>
      <p className="seg-help">
        {OPTIONS.find((o) => o.id === value)?.sub}
      </p>
    </div>
  );
}
