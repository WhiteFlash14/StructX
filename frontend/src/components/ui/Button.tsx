// Unified Button primitive scoped to the new frontend.
//
// Why this lives at /components/ui: we deliberately do NOT retrofit every
// existing button at once (the brief warns against risky full rewrites).
// New code reaches for this; existing buttons keep their inline styles
// until a follow-up pass replaces them. Variants and states match the
// brief's surface area so the API doesn't change later.

"use client";

import { forwardRef } from "react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant =
  | "primary"
  | "secondary"
  | "ghost"
  | "danger"
  | "success"
  | "icon"
  | "compact";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  loading?: boolean;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
  fullWidth?: boolean;
};

/**
 * Stable-width loading: while `loading` is true the children still occupy
 * their slot (invisible) and the spinner is absolutely positioned, so the
 * button doesn't reflow when the spinner appears.
 */
export const Button = forwardRef<HTMLButtonElement, Props>(function Button(
  {
    variant = "primary",
    loading = false,
    leftIcon,
    rightIcon,
    fullWidth,
    disabled,
    className,
    children,
    ...rest
  },
  ref,
) {
  const isDisabled = disabled || loading;
  return (
    <button
      ref={ref}
      type={rest.type ?? "button"}
      disabled={isDisabled}
      aria-busy={loading || undefined}
      className={[
        "ui-btn",
        `ui-btn-${variant}`,
        fullWidth ? "ui-btn-full" : "",
        loading ? "is-loading" : "",
        className ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      {...rest}
    >
      <span className="ui-btn-inner" aria-hidden={loading || undefined}>
        {leftIcon ? <span className="ui-btn-icon">{leftIcon}</span> : null}
        <span className="ui-btn-label">{children}</span>
        {rightIcon ? <span className="ui-btn-icon">{rightIcon}</span> : null}
      </span>
      {loading ? (
        <span className="ui-btn-spinner" aria-hidden>
          <svg viewBox="0 0 24 24" width="14" height="14">
            <circle
              cx="12"
              cy="12"
              r="9"
              fill="none"
              stroke="currentColor"
              strokeOpacity="0.25"
              strokeWidth="2.5"
            />
            <path
              d="M21 12a9 9 0 0 1-9 9"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
            />
          </svg>
        </span>
      ) : null}
    </button>
  );
});
