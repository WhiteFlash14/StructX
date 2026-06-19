// Skeleton primitives for the new frontend.
//
// We expose two pieces:
//   * <Skeleton/> — a single shimmering block, configurable shape and size.
//   * <SkeletonText/> — a stack of skeleton lines, useful for paragraphs.
//
// Animations are CSS-only and honor prefers-reduced-motion via the global
// guard in _landing-shared.tsx (which clamps all animation-duration to ~1ms
// when the user has opted out).

"use client";

import type { CSSProperties } from "react";

type SkeletonProps = {
  /** Pixel/% width, defaults to 100%. */
  width?: string | number;
  /** Pixel height, defaults to a 14px text-line height. */
  height?: string | number;
  /** Radius in px, defaults to 8. Pass 999 for pill, 50% for circle. */
  radius?: string | number;
  /** Extra inline style — useful for margins. */
  style?: CSSProperties;
  /** Extra className for layout (e.g. grid placement). */
  className?: string;
  /** Render as a circle (sets radius = 50% and uses width as both axes). */
  circle?: boolean;
};

export function Skeleton({
  width = "100%",
  height = 14,
  radius = 8,
  style,
  className,
  circle = false,
}: SkeletonProps) {
  const w = typeof width === "number" ? `${width}px` : width;
  const h = typeof height === "number" ? `${height}px` : height;
  const r = circle
    ? "50%"
    : typeof radius === "number"
      ? `${radius}px`
      : radius;
  return (
    <span
      className={`ui-skel ${className ?? ""}`}
      style={{
        width: circle ? h : w,
        height: h,
        borderRadius: r,
        ...style,
      }}
      aria-hidden
    />
  );
}

/**
 * Stack of N skeleton lines, where the last line is shorter for natural
 * paragraph rhythm. Lines have small vertical gaps.
 */
export function SkeletonText({
  lines = 3,
  lastWidth = "60%",
  lineHeight = 14,
  gap = 8,
}: {
  lines?: number;
  lastWidth?: string;
  lineHeight?: number;
  gap?: number;
}) {
  return (
    <span
      className="ui-skel-text"
      style={{ display: "grid", gap: `${gap}px` }}
      aria-hidden
    >
      {Array.from({ length: lines }).map((_, i) => (
        <Skeleton
          key={i}
          height={lineHeight}
          width={i === lines - 1 ? lastWidth : "100%"}
        />
      ))}
    </span>
  );
}
