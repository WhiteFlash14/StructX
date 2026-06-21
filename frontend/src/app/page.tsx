import Link from "next/link";

import {
  LandingHeader,
  LandingFooter,
  LandingStyles,
} from "@/app/_landing-shared";

export const metadata = {
  title: "StructX",
  description:
    "Choose how you think BTC will move, review the possible outcomes, and open the strategy from your Sui wallet.",
};

export default function LandingPage() {
  return (
    <main className="landing">
      <LandingHeader showWallet={false} showLaunchApp />
      <Hero />
      <HowItWorks />
      <AsymmetricReturns />
      <CallToAction />
      <LandingFooter />
      <LandingStyles />
    </main>
  );
}

function Hero() {
  return (
    <section className="hero" id="top">
      <div className="hero-grid">
        <div className="hero-text">
          <p className="eyebrow">BTC STRATEGIES ON DEEPBOOK PREDICT</p>
          <h1>
            Build around your BTC view,
            <br />
            <span className="accent">then see the payoff clearly.</span>
          </h1>
          <p className="hero-sub">
            Tell StructX how you think BTC will move or choose a strategy
            yourself. You can review the price and every payoff scenario
            before opening it from your wallet.
          </p>
        </div>
        <div className="hero-visual" aria-hidden>
          <span className="hero-bigmark">
            <svg viewBox="0 0 200 200" fill="none">
              <path
                d="M40 60l60-30 60 30-60 30-60-30z"
                stroke="currentColor"
                strokeWidth="6"
                strokeLinejoin="round"
              />
              <path
                d="M40 100l60 30 60-30"
                stroke="currentColor"
                strokeWidth="6"
                strokeLinejoin="round"
              />
              <path
                d="M40 140l60 30 60-30"
                stroke="currentColor"
                strokeWidth="6"
                strokeLinejoin="round"
              />
            </svg>
          </span>
          <div className="hero-mock">
            <div className="hero-mock-head">
              <span className="hero-mock-dot dot-r" />
              <span className="hero-mock-dot dot-y" />
              <span className="hero-mock-dot dot-g" />
              <span className="hero-mock-title">structx.app</span>
            </div>
            <div className="hero-mock-body">
              <div className="hero-mock-row">
                <span className="hero-mock-pill teal">Live</span>
                <strong>Breakout Protection</strong>
              </div>
              <PayoffMiniChart />
              <div className="hero-mock-foot">
                <div>
                  <span>Premium</span>
                  <strong>2.00 dUSDC</strong>
                </div>
                <div>
                  <span>Max payout</span>
                  <strong>4.09 dUSDC</strong>
                </div>
                <div className="audit-ok">
                  <span className="dot" /> Audit ok
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function PayoffMiniChart() {
  const bars = [
    { tone: "win", h: 78 },
    { tone: "win", h: 44 },
    { tone: "loss", h: 18 },
    { tone: "win", h: 44 },
    { tone: "win", h: 78 },
  ];
  return (
    <div className="payoff-mini">
      {bars.map((b, i) => (
        <div
          key={i}
          className={`payoff-mini-bar tone-${b.tone}`}
          style={{ height: `${b.h}%` }}
        />
      ))}
    </div>
  );
}

function HowItWorks() {
  const steps = [
    {
      title: "Share your view",
      body: "Describe the move you expect, or choose a strategy from the library.",
    },
    {
      title: "Preview the payoff",
      body: "See the live premium, the positions being opened, and what each expiry outcome would pay.",
    },
    {
      title: "Sign from your wallet",
      body: "StructX checks the full transaction first, then your wallet asks you to review and approve it.",
    },
  ];
  return (
    <section className="section" id="how-it-works">
      <div className="section-head center">
        <p className="eyebrow accent-text">HOW IT WORKS</p>
        <h2>From your market view to an open position.</h2>
      </div>
      <ol className="howto-list">
        {steps.map((s, i) => (
          <li key={s.title} className="howto-step">
            <span className="howto-index">{i + 1}</span>
            <h3>{s.title}</h3>
            <p>{s.body}</p>
          </li>
        ))}
      </ol>
    </section>
  );
}

function AsymmetricReturns() {
  return (
    <section className="section">
      <div className="split-section reverse">
        <div className="split-text">
          <p className="eyebrow accent-text">DEFINED PAYOFF</p>
          <h2>See the full payoff before you sign.</h2>
          <p className="section-sub">
            Your maximum loss is the premium shown in the preview. The payoff
            table shows what you receive in each price range, so you know the
            shape of the trade before your wallet opens.
          </p>
        </div>
        <div className="split-visual">
          <PayoffShowcase />
        </div>
      </div>
    </section>
  );
}

function PayoffShowcase() {
  const bars = [
    { label: "BTC < 63K", h: 92, tone: "win" },
    { label: "63K to 64K", h: 52, tone: "win" },
    { label: "64K to 65K", h: 14, tone: "loss" },
    { label: "65K to 66K", h: 52, tone: "win" },
    { label: "BTC > 66K", h: 92, tone: "win" },
  ];
  return (
    <div className="payoff-card">
      <div className="payoff-card-head">
        <div>
          <p className="mock-label">Scenario payoff</p>
          <strong>BTC · Breakout Protection</strong>
        </div>
        <span className="mock-pill green">Preview</span>
      </div>
      <div className="payoff-bars">
        {bars.map((b) => (
          <div key={b.label} className="payoff-bar-col">
            <div
              className={`payoff-bar tone-${b.tone}`}
              style={{ height: `${b.h}%` }}
            />
            <span className="payoff-bar-label">{b.label}</span>
          </div>
        ))}
      </div>
      <div className="payoff-card-foot">
        <div>
          <span>Premium</span>
          <strong>2.00</strong>
        </div>
        <div>
          <span>Max payout</span>
          <strong>4.09</strong>
        </div>
        <div>
          <span>Net</span>
          <strong className="pos">+2.04</strong>
        </div>
      </div>
    </div>
  );
}

function CallToAction() {
  return (
    <section className="section cta-section" id="safety">
      <div className="cta-card">
        <p className="eyebrow accent-text">BUILD YOUR FIRST STRATEGY</p>
        <h2>StructX is live on Sui Testnet.</h2>
        <p className="section-sub">
          Describe the move you have in mind and StructX will match it to a
          strategy. The preview shows the exact structure, and your wallet
          stays in control of the transaction from start to finish.
        </p>
        <div className="hero-ctas center">
          <Link href="/strategies/breakout-protection" className="btn btn-primary">
            Try Breakout Protection
          </Link>
          <Link href="/strategies" className="btn btn-outline">
            Browse all strategies
          </Link>
        </div>
      </div>
    </section>
  );
}
