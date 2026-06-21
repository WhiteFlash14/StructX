"use client";

import { useState } from "react";

type Section = {
  label: string;
  content: string | null | undefined;
};

type Props = {
  title?: string;
  sections: Section[];
};

export function DebugDetails({ title = "Advanced debug details", sections }: Props) {
  const [open, setOpen] = useState(false);
  const populated = sections.filter((s) => s.content && s.content.trim().length > 0);
  if (!populated.length) return null;

  return (
    <section className="debug-details">
      <button
        type="button"
        className="debug-toggle-button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {open ? "▾" : "▸"} {title}
      </button>
      {open && (
        <div className="debug-body">
          {populated.map((section) => (
            <div key={section.label} className="debug-section">
              <p className="debug-label">{section.label}</p>
              <pre className="debug-pre">{section.content}</pre>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
