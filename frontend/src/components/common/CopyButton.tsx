"use client";

import { copyToClipboard } from "@/lib/format";

type Props = {
  value: string;
  label?: string;
  onCopied?: (value: string) => void;
  className?: string;
};

export function CopyButton({ value, label = "Copy", onCopied, className }: Props) {
  return (
    <button
      type="button"
      className={`copy-button ${className ?? ""}`}
      onClick={async () => {
        await copyToClipboard(value);
        onCopied?.(value);
      }}
      title={`Copy ${value}`}
    >
      <svg
        aria-hidden
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
        <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
      </svg>
      <span>{label}</span>
    </button>
  );
}
