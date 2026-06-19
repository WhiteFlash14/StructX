"use client";

type Props = {
  lines?: number;
  title?: boolean;
};

export function SkeletonCard({ lines = 3, title = true }: Props) {
  return (
    <div className="panel skeleton-panel">
      {title && <div className="skeleton skeleton-title" />}
      <div className="skeleton skeleton-line" />
      {Array.from({ length: lines }).map((_, idx) => (
        <div key={idx} className="skeleton skeleton-line" />
      ))}
    </div>
  );
}

export function SkeletonGrid({ count = 4 }: { count?: number }) {
  return (
    <div className="strategy-grid">
      {Array.from({ length: count }).map((_, idx) => (
        <div className="strategy-card skeleton-card" key={idx}>
          <div className="skeleton skeleton-pill" />
          <div className="skeleton skeleton-title" />
          <div className="skeleton skeleton-line" />
          <div className="skeleton skeleton-line short" />
        </div>
      ))}
    </div>
  );
}
