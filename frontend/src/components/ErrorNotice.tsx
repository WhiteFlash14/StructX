"use client";

import { useState } from "react";

import type { FriendlyError } from "@/lib/errors";

type Props = {
  error: FriendlyError;
};

export function ErrorNotice({ error }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <div className={`error-notice severity-${error.severity}`}>
      <div className="error-head">
        <strong>{error.title}</strong>
        <span className={`severity-pill severity-${error.severity}`}>
          {error.severity === "blocking"
            ? "Blocking"
            : error.severity === "caution"
              ? "Caution"
              : "Info"}
        </span>
      </div>
      <p className="error-message">{error.message}</p>
      {error.action && <p className="error-action">{error.action}</p>}
      {error.debug && (
        <div className="debug-toggle">
          <button
            type="button"
            className="mini-button"
            onClick={() => setOpen((value) => !value)}
          >
            {open ? "Hide raw details" : "Show raw details"}
          </button>
          {open && <pre className="debug-pre">{error.debug}</pre>}
        </div>
      )}
    </div>
  );
}
