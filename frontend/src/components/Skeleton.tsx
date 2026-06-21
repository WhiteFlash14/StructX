"use client";

type Props = {
  lines?: number;
  title?: boolean;
};

export function SkeletonCard({ lines = 3, title = true }: Props) {
  return (
    <section className="panel skeleton-panel">
      {title && <div className="skeleton skeleton-title" />}
      <div className="skeleton skeleton-line" />
      {Array.from({ length: lines }).map((_, idx) => (
        <div key={idx} className="skeleton skeleton-line" />
      ))}
    </section>
  );
}

export function EmptyPreview() {
  return (
    <section className="panel empty-state">
      <div className="panel-header">
        <p className="eyebrow">Get started</p>
        <h2>Preview your first StructX strategy</h2>
      </div>
      <p className="muted">
        Connect a Sui wallet, enter a budget, and compile a payoff. StructX will
        select market, oracle and strikes, then preview the legs and payoff
        before you sign.
      </p>
      <ul className="empty-bullets">
        <li>Your wallet stays in control of every transaction.</li>
        <li>This version runs on Sui Testnet.</li>
        <li>StructX checks the transaction before your wallet opens it.</li>
      </ul>
    </section>
  );
}
