"use client";

import type { AppMode } from "@/types/structx";

type Props = {
  mode: AppMode;
  onChange: (mode: AppMode) => void;
};

export function ModeToggle({ mode, onChange }: Props) {
  return (
    <section className="mode-toggle-wrap">
      <div className="mode-toggle">
        <button
          type="button"
          className={mode === "normal" ? "active" : ""}
          onClick={() => onChange("normal")}
        >
          Normal
          <span>Describe your market view</span>
        </button>
        <button
          type="button"
          className={mode === "advanced" ? "active" : ""}
          onClick={() => onChange("advanced")}
        >
          Advanced
          <span>Choose every setting</span>
        </button>
      </div>
    </section>
  );
}
