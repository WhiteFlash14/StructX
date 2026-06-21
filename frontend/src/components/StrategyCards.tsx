"use client";

const cards = [
  {
    title: "Breakout Protection",
    description: "Pays when spot settles outside the chosen center band.",
    status: "Available" as const,
    active: true,
  },
  {
    title: "Crash Insurance",
    description: "Tail-focused downside protection.",
    status: "Coming soon" as const,
  },
  {
    title: "Moonshot Upside",
    description: "Convex upside exposure for breakout regimes.",
    status: "Coming soon" as const,
  },
  {
    title: "Expiry Move Note",
    description: "Defined payout around expiry move buckets.",
    status: "Coming soon" as const,
  },
];

export function StrategyCards() {
  return (
    <section className="strategy-grid">
      {cards.map((card) => (
        <article
          key={card.title}
          className={`strategy-tile ${card.active ? "active" : "muted"}`}
        >
          <span className={`tile-status ${card.active ? "" : "soon"}`}>
            {card.status}
          </span>
          <h3>{card.title}</h3>
          <p>{card.description}</p>
        </article>
      ))}
    </section>
  );
}
