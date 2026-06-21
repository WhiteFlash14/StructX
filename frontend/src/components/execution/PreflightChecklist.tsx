"use client";

export type CheckItem = {
  ok: boolean;
  label: string;
  detail?: string;
};

type Props = {
  items: CheckItem[];
};

export function PreflightChecklist({ items }: Props) {
  return (
    <ul className="preflight">
      {items.map((c) => (
        <li key={c.label} className={c.ok ? "preflight-item ok" : "preflight-item bad"}>
          <span className="preflight-icon" aria-hidden>
            {c.ok ? "✓" : "!"}
          </span>
          <span className="preflight-body">
            <strong>{c.label}</strong>
            {c.detail && <span className="preflight-detail">{c.detail}</span>}
          </span>
        </li>
      ))}
    </ul>
  );
}
