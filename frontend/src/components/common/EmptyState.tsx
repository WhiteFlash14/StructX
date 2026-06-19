"use client";

import type { ReactNode } from "react";

type Props = {
  title: string;
  body?: string;
  icon?: ReactNode;
  action?: ReactNode;
};

export function EmptyState({ title, body, icon, action }: Props) {
  return (
    <div className="empty-state-card">
      {icon && <div className="empty-icon">{icon}</div>}
      <h3>{title}</h3>
      {body && <p>{body}</p>}
      {action && <div className="empty-action">{action}</div>}
    </div>
  );
}
