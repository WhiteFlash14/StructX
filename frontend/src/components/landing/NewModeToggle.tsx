// Normal/Advanced toggle scoped to the new frontend (/, /strategies, etc).
// Kept separate from the legacy /app ModeToggle so changes here don't bleed
// into the old workspace UI.

"use client";

export type NewAppMode = "normal" | "advanced";

const TABS: ReadonlyArray<{
  id: NewAppMode;
  label: string;
  icon: React.ReactNode;
}> = [
  {
    id: "normal",
    label: "Normal",
    icon: (
      <svg
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3z" />
      </svg>
    ),
  },
  {
    id: "advanced",
    label: "Advanced",
    icon: (
      <svg
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <rect x="3" y="4" width="7" height="7" rx="1.5" />
        <rect x="14" y="4" width="7" height="7" rx="1.5" />
        <rect x="3" y="14" width="7" height="7" rx="1.5" />
        <rect x="14" y="14" width="7" height="7" rx="1.5" />
      </svg>
    ),
  },
];

export function NewModeToggle({
  mode,
  onChange,
}: {
  mode: NewAppMode;
  onChange: (next: NewAppMode) => void;
}) {
  const activeIndex = TABS.findIndex((t) => t.id === mode);
  return (
    <div
      className="new-mode-toggle"
      role="tablist"
      aria-label="Strategy mode"
    >
      <span
        className="new-mode-thumb"
        style={{ transform: `translateX(${activeIndex * 100}%)` }}
        aria-hidden
      />
      {TABS.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={mode === t.id}
          className={`new-mode-tab ${mode === t.id ? "active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          <span className="new-mode-tab-icon">{t.icon}</span>
          <span className="new-mode-tab-label">{t.label}</span>
        </button>
      ))}
    </div>
  );
}
