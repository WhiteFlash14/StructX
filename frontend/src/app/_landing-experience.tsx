"use client";

// Interactive landing experience for StructX.
//
// Motion here is hand-built, no animation library:
//   - reveals: a single IntersectionObserver toggles an `.in` class so CSS
//     handles the staggered entrances.
//   - the "assembly" band: a rAF-throttled scroll loop writes progress vars
//     (--p, --pa..--pc, --b0..--b4) onto a pinned section; CSS reads them to
//     drift the primitives together and build the payoff bar by bar.
//   - the hero payoff scrubs as you move across it, and sweeps on its own
//     when idle so the page feels alive while you're just reading.
// Everything checks prefers-reduced-motion and falls back to a static,
// fully-revealed state.

import Link from "next/link";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";

import { STRATEGY_CATALOG } from "@/lib/strategyCatalog";

const PREMIUM = 2.0;

type Bucket = {
  label: string;
  region: string;
  gross: number;
  h: number; // bar height as a percentage of the chart area
  loss?: boolean;
};

const BUCKETS: Bucket[] = [
  { label: "BTC under 63K", region: "Downside tail", gross: 4.09, h: 92 },
  { label: "63K to 64K", region: "Lower range", gross: 3.1, h: 56 },
  { label: "64K to 65K", region: "Center", gross: 0, h: 14, loss: true },
  { label: "65K to 66K", region: "Upper range", gross: 3.1, h: 56 },
  { label: "BTC over 66K", region: "Upside tail", gross: 4.09, h: 92 },
];

function prefersReduced(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function clamp01(n: number) {
  return n < 0 ? 0 : n > 1 ? 1 : n;
}

function stage(p: number, a: number, b: number) {
  return clamp01((p - a) / (b - a));
}

export function LandingExperience() {
  const rootRef = useRef<HTMLDivElement | null>(null);

  // ---- reveals + pinned assembly scroll engine -------------------------
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const reduce = prefersReduced();

    const revealEls = Array.from(
      root.querySelectorAll<HTMLElement>("[data-reveal]"),
    );
    let io: IntersectionObserver | null = null;
    if (reduce) {
      revealEls.forEach((el) => el.classList.add("in"));
    } else {
      io = new IntersectionObserver(
        (entries) => {
          for (const en of entries) {
            if (en.isIntersecting) {
              en.target.classList.add("in");
              io?.unobserve(en.target);
            }
          }
        },
        { threshold: 0.16, rootMargin: "0px 0px -7% 0px" },
      );
      revealEls.forEach((el) => io?.observe(el));
    }

    const pins = Array.from(root.querySelectorAll<HTMLElement>("[data-pin]"));
    const setPinVars = (pin: HTMLElement, p: number) => {
      pin.style.setProperty("--p", p.toFixed(4));
      pin.style.setProperty("--pa", stage(p, 0.02, 0.34).toFixed(4));
      pin.style.setProperty("--pb", stage(p, 0.3, 0.74).toFixed(4));
      pin.style.setProperty("--pc", stage(p, 0.74, 0.98).toFixed(4));
      for (let i = 0; i < 5; i++) {
        const a = 0.32 + i * 0.06;
        pin.style.setProperty(`--b${i}`, stage(p, a, a + 0.22).toFixed(4));
      }
    };

    let ticking = false;
    let raf = 0;
    const update = () => {
      ticking = false;
      const vh = window.innerHeight;
      for (const pin of pins) {
        const rect = pin.getBoundingClientRect();
        const total = pin.offsetHeight - vh;
        const scrolled = Math.min(Math.max(-rect.top, 0), Math.max(total, 1));
        setPinVars(pin, total > 0 ? scrolled / total : 0);
      }
    };
    const onScroll = () => {
      if (!ticking) {
        ticking = true;
        raf = requestAnimationFrame(update);
      }
    };

    if (reduce) {
      pins.forEach((pin) => setPinVars(pin, 1));
    } else if (pins.length) {
      update();
      window.addEventListener("scroll", onScroll, { passive: true });
      window.addEventListener("resize", onScroll);
    }

    return () => {
      io?.disconnect();
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <div className="lx" ref={rootRef}>
      <Hero />
      <Marquee />
      <AssemblyBand />
      <StatsBand />
      <HowItWorks />
      <Safety />
      <FinalCta />
      <ExperienceStyles />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Hero: headline + interactive payoff that scrubs on hover and sweeps on idle.
// ---------------------------------------------------------------------------
function Hero() {
  const [active, setActive] = useState(2);
  const [engaged, setEngaged] = useState(false);
  const chartRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (engaged || prefersReduced()) return;
    const id = window.setInterval(() => {
      setActive((b) => (b + 1) % BUCKETS.length);
    }, 1700);
    return () => window.clearInterval(id);
  }, [engaged]);

  const pickFromX = useCallback((clientX: number) => {
    const el = chartRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const ratio = (clientX - rect.left) / rect.width;
    const idx = Math.min(
      BUCKETS.length - 1,
      Math.max(0, Math.floor(ratio * BUCKETS.length)),
    );
    setActive(idx);
  }, []);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      setEngaged(true);
      setActive((b) => Math.max(0, b - 1));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      setEngaged(true);
      setActive((b) => Math.min(BUCKETS.length - 1, b + 1));
    }
  };

  const bucket = BUCKETS[active];
  const net = bucket.gross - PREMIUM;

  return (
    <section className="lx-hero" id="top">
      <div className="lx-hero-grid">
        <div className="lx-hero-copy">
          <p className="lx-eyebrow" data-reveal>
            <span className="lx-eyebrow-dot" aria-hidden />
            BTC strategies on DeepBook Predict
          </p>
          <h1 className="lx-hero-title" data-reveal>
            Build around your BTC view,{" "}
            <span className="lx-accent">then see the payoff clearly.</span>
          </h1>
          <p className="lx-hero-sub" data-reveal>
            Tell StructX how you think BTC will move by expiry, or pick a
            strategy yourself. You get the price and every payoff outcome up
            front, then open it from your own wallet.
          </p>
          <div className="lx-hero-ctas" data-reveal>
            <Link href="/strategies" className="lx-btn lx-btn-primary">
              Open the app
            </Link>
            <Link
              href="/strategies/breakout-protection"
              className="lx-btn lx-btn-outline"
            >
              See Breakout Protection
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M5 12h14M13 6l6 6-6 6" />
              </svg>
            </Link>
          </div>
        </div>

        <div className="lx-hero-visual" data-reveal>
          <div className="lx-glow" aria-hidden />
          <div className="lx-card">
            <div className="lx-card-head">
              <span className="lx-dot lx-dot-r" />
              <span className="lx-dot lx-dot-y" />
              <span className="lx-dot lx-dot-g" />
              <span className="lx-card-host">structx.app</span>
              <span className="lx-card-live">
                <span className="lx-card-live-dot" /> Live
              </span>
            </div>

            <div className="lx-card-title-row">
              <strong>Breakout Protection</strong>
              <span className="lx-card-tag">Example payoff</span>
            </div>

            <div
              ref={chartRef}
              className="lx-chart"
              role="group"
              aria-label="Payoff by BTC price at expiry. Use the left and right arrows to step through the price ranges."
              tabIndex={0}
              onPointerMove={(e) => {
                setEngaged(true);
                pickFromX(e.clientX);
              }}
              onPointerLeave={() => setEngaged(false)}
              onPointerDown={(e) => {
                setEngaged(true);
                pickFromX(e.clientX);
              }}
              onKeyDown={onKey}
              onBlur={() => setEngaged(false)}
            >
              {BUCKETS.map((b, i) => (
                <div
                  key={b.label}
                  className={[
                    "lx-bar-col",
                    i === active ? "is-active" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                >
                  <div
                    className={`lx-bar ${b.loss ? "is-loss" : ""}`}
                    style={{ height: `${b.h}%` }}
                  />
                </div>
              ))}
              <div
                className="lx-chart-marker"
                aria-hidden
                style={{
                  left: `${((active + 0.5) / BUCKETS.length) * 100}%`,
                }}
              />
            </div>

            <div className="lx-readout">
              <div className="lx-readout-region">
                <span className="lx-readout-label">If BTC settles</span>
                <strong>{bucket.label}</strong>
                <span className="lx-readout-sub">{bucket.region}</span>
              </div>
              <div className="lx-readout-nums">
                <div>
                  <span className="lx-readout-label">Pays</span>
                  <strong className="lx-mono">{bucket.gross.toFixed(2)}</strong>
                </div>
                <div>
                  <span className="lx-readout-label">Net</span>
                  <strong
                    className={`lx-mono ${net >= 0 ? "lx-pos" : "lx-neg"}`}
                  >
                    {net >= 0 ? "+" : ""}
                    {net.toFixed(2)}
                  </strong>
                </div>
              </div>
            </div>

            <div className="lx-card-foot">
              <span>Premium 2.00 dUSDC</span>
              <span className="lx-card-foot-ok">
                <span className="lx-card-live-dot" /> Loss capped at premium
              </span>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Marquee of real strategy names. Continuous, pauses on hover.
// ---------------------------------------------------------------------------
function Marquee() {
  const names = STRATEGY_CATALOG.map((s) => s.displayName);
  const row = [...names, ...names];
  return (
    <div className="lx-marquee" aria-hidden>
      <div className="lx-marquee-fade lx-marquee-fade-l" />
      <div className="lx-marquee-fade lx-marquee-fade-r" />
      <div className="lx-marquee-track">
        {row.map((n, i) => (
          <span key={`${n}-${i}`} className="lx-marquee-item">
            <span className="lx-marquee-dot" />
            {n}
          </span>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Signature: three primitives drift together and the payoff builds on scroll.
// ---------------------------------------------------------------------------
function AssemblyBand() {
  return (
    <section className="lx-band">
      <div className="lx-band-dots" aria-hidden />
      <div className="lx-pin" data-pin>
        <div className="lx-pin-inner">
          <div className="lx-band-head">
            <p className="lx-eyebrow lx-eyebrow-light">
              <span className="lx-eyebrow-dot" aria-hidden />
              The build
            </p>
            <h2>
              Three primitives. <span className="lx-accent">One payoff.</span>
            </h2>
            <p className="lx-band-sub">
              Every StructX strategy is made from the same parts: a down
              position, a range, and an up position. Stack them and the payoff
              takes whatever shape your view calls for.
            </p>
          </div>

          <div className="lx-assembly">
            <div className="lx-prims" aria-hidden>
              <span className="lx-prim lx-prim-down">
                <PrimGlyph kind="down" />
                Down
              </span>
              <span className="lx-prim lx-prim-range">
                <PrimGlyph kind="range" />
                Range
              </span>
              <span className="lx-prim lx-prim-up">
                <PrimGlyph kind="up" />
                Up
              </span>
            </div>

            <div className="lx-assembly-chart" aria-hidden>
              {BUCKETS.map((b, i) => (
                <div key={b.label} className="lx-acol">
                  <div
                    className={`lx-abar ${b.loss ? "is-loss" : ""}`}
                    style={
                      {
                        height: `${b.h}%`,
                        ["--bi" as string]: `var(--b${i})`,
                      } as React.CSSProperties
                    }
                  />
                  <span className="lx-acol-label">{b.label}</span>
                </div>
              ))}
            </div>

            <div className="lx-assembly-tags" aria-hidden>
              <span className="lx-atag lx-atag-l">Wins on the tails</span>
              <span className="lx-atag lx-atag-c">Defined loss in the middle</span>
              <span className="lx-atag lx-atag-r">Wins on the tails</span>
            </div>
          </div>

          <p className="lx-band-foot" aria-hidden>
            <span className="lx-mono">Breakout Protection</span>
            <span className="lx-band-foot-sep" />
            <span className="lx-mono">Premium 2.00</span>
            <span className="lx-band-foot-sep" />
            <span className="lx-mono">Max payout 4.09 dUSDC</span>
          </p>
        </div>
      </div>
    </section>
  );
}

function PrimGlyph({ kind }: { kind: "down" | "range" | "up" }) {
  const common = {
    width: 16,
    height: 16,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
  if (kind === "down")
    return (
      <svg {...common} aria-hidden>
        <path d="M3 6h8v12h10" />
      </svg>
    );
  if (kind === "up")
    return (
      <svg {...common} aria-hidden>
        <path d="M3 18h10V6h8" />
      </svg>
    );
  return (
    <svg {...common} aria-hidden>
      <path d="M3 18h5V8h8v10h5" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Honest stats: every number is a real product fact, counts up in view.
// ---------------------------------------------------------------------------
function StatsBand() {
  const stats = [
    { end: 11, suffix: "", label: "Ready-made strategies" },
    { end: 3, suffix: "", label: "Predict primitives" },
    { end: 5, suffix: "", label: "Payoff regions in Breakout" },
    { end: 100, suffix: "%", label: "Non-custodial. Your wallet signs." },
  ];
  return (
    <section className="lx-stats" data-reveal>
      <div className="lx-stats-grid">
        {stats.map((s) => (
          <div key={s.label} className="lx-stat">
            <strong className="lx-stat-num lx-mono">
              <CountUp end={s.end} suffix={s.suffix} />
            </strong>
            <span className="lx-stat-label">{s.label}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function CountUp({
  end,
  suffix = "",
  duration = 1100,
}: {
  end: number;
  suffix?: string;
  duration?: number;
}) {
  const ref = useRef<HTMLSpanElement | null>(null);
  const [val, setVal] = useState(0);
  const fired = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const reduce = prefersReduced();
    const io = new IntersectionObserver(
      (entries) => {
        for (const en of entries) {
          if (en.isIntersecting && !fired.current) {
            fired.current = true;
            io.disconnect();
            if (reduce) {
              setVal(end);
              return;
            }
            const start = performance.now();
            const tick = (now: number) => {
              const t = Math.min(1, (now - start) / duration);
              const eased = 1 - Math.pow(1 - t, 3);
              setVal(Math.round(eased * end));
              if (t < 1) requestAnimationFrame(tick);
            };
            requestAnimationFrame(tick);
          }
        }
      },
      { threshold: 0.6 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [end, duration]);

  return (
    <span ref={ref}>
      {val}
      {suffix}
    </span>
  );
}

// ---------------------------------------------------------------------------
// How it works: three steps with a connector line that draws in on reveal.
// ---------------------------------------------------------------------------
function HowItWorks() {
  const steps = [
    {
      title: "Share your view",
      body: "Tell StructX how you think BTC will move by expiry, or pick a strategy from the library yourself.",
    },
    {
      title: "Preview the whole payoff",
      body: "See the live premium, the exact positions being opened, and what every price outcome would pay.",
    },
    {
      title: "Sign from your wallet",
      body: "StructX checks the full transaction first. Your wallet asks you to review and approve, and stays in control.",
    },
  ];
  return (
    <section className="lx-section" id="how-it-works">
      <div className="lx-section-head" data-reveal>
        <p className="lx-eyebrow lx-eyebrow-accent">How it works</p>
        <h2>From a market view to an open position.</h2>
      </div>
      <ol className="lx-steps">
        <span className="lx-steps-line" data-reveal aria-hidden />
        {steps.map((s, i) => (
          <li
            key={s.title}
            className="lx-step"
            data-reveal
            style={{ ["--d" as string]: `${i * 90}ms` } as React.CSSProperties}
          >
            <span className="lx-step-index lx-mono">{`0${i + 1}`}</span>
            <h3>{s.title}</h3>
            <p>{s.body}</p>
          </li>
        ))}
      </ol>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Defined payoff + what StructX promises.
// ---------------------------------------------------------------------------
function Safety() {
  const facts = [
    {
      title: "Your wallet stays in control",
      body: "StructX never holds your funds. You review and sign the transaction, and it runs from your wallet.",
    },
    {
      title: "Loss is capped at the premium",
      body: "The premium in the preview is the most you can lose. The payoff table shows what each outcome returns.",
    },
    {
      title: "Settled on the price at expiry",
      body: "Strategies pay on where BTC finishes. A quick wick across a level and back does not decide the trade.",
    },
  ];
  return (
    <section className="lx-section" id="safety">
      <div className="lx-split">
        <div className="lx-split-text" data-reveal>
          <p className="lx-eyebrow lx-eyebrow-accent">Defined payoff</p>
          <h2>See the full payoff before you sign.</h2>
          <p className="lx-section-sub">
            No surprises after the wallet prompt. You know your maximum loss,
            your maximum payout, and the shape of the trade across every price
            range before anything is signed.
          </p>
          <ul className="lx-facts">
            {facts.map((f) => (
              <li key={f.title} className="lx-fact" data-reveal>
                <span className="lx-fact-mark" aria-hidden>
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M5 12l5 5 9-10" />
                  </svg>
                </span>
                <div>
                  <strong>{f.title}</strong>
                  <p>{f.body}</p>
                </div>
              </li>
            ))}
          </ul>
        </div>

        <div className="lx-split-visual" data-reveal>
          <div className="lx-payoff-card">
            <div className="lx-payoff-card-head">
              <div>
                <span className="lx-readout-label">Scenario payoff</span>
                <strong>BTC · Breakout Protection</strong>
              </div>
              <span className="lx-card-tag">Preview</span>
            </div>
            <div className="lx-payoff-rows">
              {BUCKETS.map((b) => {
                const net = b.gross - PREMIUM;
                return (
                  <div key={b.label} className="lx-payoff-row">
                    <span className="lx-payoff-row-range">{b.label}</span>
                    <span className="lx-payoff-row-bar">
                      <span
                        className={`lx-payoff-row-fill ${b.loss ? "is-loss" : ""}`}
                        style={{ width: `${b.h}%` }}
                      />
                    </span>
                    <span
                      className={`lx-payoff-row-net lx-mono ${net >= 0 ? "lx-pos" : "lx-neg"}`}
                    >
                      {net >= 0 ? "+" : ""}
                      {net.toFixed(2)}
                    </span>
                  </div>
                );
              })}
            </div>
            <div className="lx-payoff-card-foot">
              <div>
                <span className="lx-readout-label">Premium</span>
                <strong className="lx-mono">2.00</strong>
              </div>
              <div>
                <span className="lx-readout-label">Max payout</span>
                <strong className="lx-mono">4.09</strong>
              </div>
              <div>
                <span className="lx-readout-label">Best net</span>
                <strong className="lx-mono lx-pos">+2.09</strong>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function FinalCta() {
  return (
    <section className="lx-section lx-cta-section">
      <div className="lx-cta-card" data-reveal>
        <p className="lx-eyebrow lx-eyebrow-accent">Build your first strategy</p>
        <h2>StructX is live on Sui Testnet.</h2>
        <p className="lx-section-sub">
          Describe the move you have in mind and StructX matches it to a
          structure. The preview shows the exact legs, and your wallet stays in
          control from start to finish.
        </p>
        <div className="lx-hero-ctas lx-center">
          <Link
            href="/strategies/breakout-protection"
            className="lx-btn lx-btn-primary"
          >
            Try Breakout Protection
          </Link>
          <Link href="/strategies" className="lx-btn lx-btn-outline">
            Browse all strategies
          </Link>
        </div>
      </div>
    </section>
  );
}

function ExperienceStyles() {
  return (
    <style href="sx-landing-experience" precedence="default">
      {EXPERIENCE_CSS}
    </style>
  );
}

const EXPERIENCE_CSS = `
.lx { --lx-ink: #0b1d36; }
.lx *, .lx *::before, .lx *::after { box-sizing: border-box; }
.lx-mono {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-variant-numeric: tabular-nums;
}
.lx-accent { color: var(--sx-teal-dark); }

/* Reveal primitive: opacity + lift, settled by .in. The global
   prefers-reduced-motion rule flattens the transition to ~0ms. */
.lx [data-reveal] {
  opacity: 0;
  transform: translateY(16px);
  transition: opacity 0.6s cubic-bezier(0.22, 1, 0.36, 1),
    transform 0.6s cubic-bezier(0.22, 1, 0.36, 1);
  transition-delay: var(--d, 0ms);
}
.lx [data-reveal].in {
  opacity: 1;
  transform: none;
}

/* ===== Buttons ===== */
.lx-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 48px;
  padding: 0 24px;
  border-radius: 999px;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.01em;
  border: 1px solid transparent;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease,
    transform 0.12s ease, box-shadow 0.15s ease;
}
.lx-btn svg { transition: transform 0.2s ease; }
.lx-btn-primary {
  background: var(--sx-teal);
  color: #fff;
  box-shadow: 0 12px 26px rgba(33, 196, 163, 0.28);
}
.lx-btn-primary:hover { background: var(--sx-teal-dark); transform: translateY(-1px); }
.lx-btn-outline {
  background: var(--sx-surface);
  color: var(--sx-navy);
  border-color: var(--sx-border-strong);
}
.lx-btn-outline:hover {
  border-color: var(--sx-navy-muted);
  transform: translateY(-1px);
}
.lx-btn-outline:hover svg { transform: translateX(3px); }

/* ===== Eyebrow ===== */
.lx-eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  margin: 0 0 22px;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--sx-navy-muted);
}
.lx-eyebrow-accent { color: var(--sx-teal-dark); }
.lx-eyebrow-light { color: #7fb6c9; }
.lx-eyebrow-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--sx-teal);
  box-shadow: 0 0 0 4px rgba(33, 196, 163, 0.16);
}

/* ===== Hero ===== */
.lx-hero {
  max-width: 1180px;
  margin: 0 auto;
  padding: 40px 28px 56px;
}
.lx-hero-grid {
  display: grid;
  grid-template-columns: 1.04fr 0.96fr;
  gap: 64px;
  align-items: center;
}
.lx-hero-title {
  margin: 0;
  font-size: clamp(42px, 6vw, 78px);
  line-height: 1.03;
  letter-spacing: -0.035em;
  font-weight: 700;
  color: var(--sx-navy);
}
.lx-hero-sub {
  margin: 26px 0 0;
  max-width: 500px;
  color: var(--sx-navy-muted);
  font-size: 18px;
  line-height: 1.6;
}
.lx-hero-ctas {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 34px;
}
.lx-hero-ctas.lx-center { justify-content: center; }

/* Hero visual */
.lx-hero-visual {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 420px;
}
.lx-glow {
  position: absolute;
  inset: -6% -2% -2%;
  background:
    radial-gradient(46% 40% at 72% 30%, rgba(33, 196, 163, 0.22), transparent 70%),
    radial-gradient(50% 46% at 30% 78%, rgba(135, 182, 220, 0.28), transparent 72%);
  filter: blur(10px);
  animation: lx-breathe 9s ease-in-out infinite;
  pointer-events: none;
}
@keyframes lx-breathe {
  0%, 100% { opacity: 0.75; transform: scale(1); }
  50% { opacity: 1; transform: scale(1.05); }
}
.lx-card {
  position: relative;
  width: min(100%, 430px);
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 22px;
  padding: 20px;
  box-shadow: 0 30px 80px rgba(16, 40, 74, 0.13);
  z-index: 1;
}
.lx-card-head {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-bottom: 18px;
}
.lx-dot { width: 10px; height: 10px; border-radius: 50%; }
.lx-dot-r { background: #ff5f57; }
.lx-dot-y { background: #febc2e; }
.lx-dot-g { background: #28c840; }
.lx-card-host {
  margin-left: 6px;
  font-size: 12px;
  color: var(--sx-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
}
.lx-card-live {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: 999px;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  font-size: 11px;
  font-weight: 700;
}
.lx-card-live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--sx-teal);
  animation: lx-pulse 2.2s ease-in-out infinite;
}
@keyframes lx-pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.45; transform: scale(0.82); }
}
.lx-card-title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}
.lx-card-title-row strong { font-size: 16px; color: var(--sx-navy); }
.lx-card-tag {
  font-size: 10.5px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--sx-muted);
  background: var(--sx-surface-soft);
  padding: 4px 9px;
  border-radius: 999px;
}

/* Hero interactive chart */
.lx-chart {
  position: relative;
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  align-items: end;
  gap: 8px;
  height: 152px;
  padding: 10px;
  background: var(--sx-surface-soft);
  border-radius: 14px;
  cursor: crosshair;
  outline: none;
  touch-action: pan-y;
}
.lx-chart:focus-visible {
  box-shadow: 0 0 0 3px rgba(16, 40, 74, 0.18);
}
.lx-bar-col {
  position: relative;
  display: flex;
  align-items: flex-end;
  height: 100%;
  z-index: 1;
}
.lx-bar {
  width: 100%;
  border-radius: 7px 7px 4px 4px;
  background: var(--sx-teal-soft);
  transition: background 0.18s ease, transform 0.18s ease;
}
.lx-bar.is-loss { background: rgba(239, 68, 68, 0.2); }
.lx-bar-col.is-active .lx-bar {
  background: var(--sx-teal);
  transform: scaleY(1.02);
}
.lx-bar-col.is-active .lx-bar.is-loss { background: var(--sx-danger); }
.lx-chart-marker {
  position: absolute;
  top: 8px;
  bottom: 8px;
  width: 2px;
  margin-left: -1px;
  border-radius: 2px;
  background: var(--sx-navy);
  opacity: 0.16;
  transition: left 0.22s cubic-bezier(0.4, 0, 0.2, 1);
  z-index: 0;
}

.lx-readout {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-top: 16px;
  padding-top: 14px;
}
.lx-readout-region { display: grid; gap: 1px; }
.lx-readout-region strong { font-size: 15px; color: var(--sx-navy); }
.lx-readout-sub { font-size: 11.5px; color: var(--sx-muted); }
.lx-readout-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--sx-muted);
  font-weight: 600;
}
.lx-readout-nums { display: flex; gap: 22px; }
.lx-readout-nums > div { display: grid; gap: 1px; text-align: right; }
.lx-readout-nums strong { font-size: 17px; }
.lx-pos { color: var(--sx-teal-dark); }
.lx-neg { color: var(--sx-danger); }
.lx-card-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px dashed var(--sx-border);
  font-size: 12px;
  color: var(--sx-muted);
}
.lx-card-foot-ok {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--sx-teal-dark);
  font-weight: 600;
}

/* ===== Marquee ===== */
.lx-marquee {
  position: relative;
  overflow: hidden;
  margin-top: 14px;
  padding: 22px 0;
  border-top: 1px solid var(--sx-border);
  border-bottom: 1px solid var(--sx-border);
  -webkit-mask-image: linear-gradient(90deg, transparent, #000 9%, #000 91%, transparent);
  mask-image: linear-gradient(90deg, transparent, #000 9%, #000 91%, transparent);
}
.lx-marquee-fade { display: none; }
.lx-marquee-track {
  display: inline-flex;
  align-items: center;
  gap: 0;
  white-space: nowrap;
  animation: lx-marquee 36s linear infinite;
  will-change: transform;
}
.lx-marquee:hover .lx-marquee-track { animation-play-state: paused; }
@keyframes lx-marquee {
  to { transform: translateX(-50%); }
}
.lx-marquee-item {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  padding: 0 28px;
  font-size: 15px;
  font-weight: 500;
  color: var(--sx-navy-muted);
  letter-spacing: -0.01em;
}
.lx-marquee-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--sx-teal);
  opacity: 0.6;
}

/* ===== Assembly band (dark) ===== */
.lx-band {
  position: relative;
  background: var(--lx-ink);
  color: #eaf1f8;
  margin-top: 40px;
  overflow: clip;
}
.lx-band-dots {
  position: absolute;
  inset: 0;
  background-image: radial-gradient(rgba(255, 255, 255, 0.07) 1px, transparent 1.4px);
  background-size: 26px 26px;
  mask-image: radial-gradient(120% 80% at 50% 30%, #000 30%, transparent 78%);
  -webkit-mask-image: radial-gradient(120% 80% at 50% 30%, #000 30%, transparent 78%);
  pointer-events: none;
}
.lx-pin {
  position: relative;
  height: 250vh;
}
.lx-pin-inner {
  position: sticky;
  top: 0;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 30px;
  max-width: 1080px;
  margin: 0 auto;
  padding: 80px 28px;
}
.lx-band-head { max-width: 660px; }
.lx-band-head h2 {
  margin: 0;
  font-size: clamp(34px, 4.6vw, 56px);
  line-height: 1.06;
  letter-spacing: -0.03em;
  font-weight: 700;
}
.lx-band-sub {
  margin: 18px 0 0;
  max-width: 540px;
  color: #9fb4c9;
  font-size: 17px;
  line-height: 1.6;
}

.lx-assembly {
  position: relative;
  margin-top: 6px;
}
.lx-prims {
  display: flex;
  justify-content: center;
  gap: clamp(16px, 9vw, 120px);
  margin-bottom: 26px;
}
.lx-prim {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  padding: 10px 18px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: #d6e3f0;
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.01em;
  opacity: var(--pa, 0);
  will-change: transform, opacity;
}
.lx-prim svg { color: var(--sx-teal); }
/* Spread the three chips apart, then let --pa draw them to center. */
.lx-prim-down { transform: translateX(calc((1 - var(--pa, 0)) * -150px)); }
.lx-prim-up { transform: translateX(calc((1 - var(--pa, 0)) * 150px)); }
.lx-prim-range { transform: translateY(calc((1 - var(--pa, 0)) * -26px)); }

.lx-assembly-chart {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  align-items: end;
  gap: clamp(8px, 1.4vw, 18px);
  height: clamp(170px, 26vh, 240px);
  max-width: 760px;
  margin: 0 auto;
}
.lx-acol {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  height: 100%;
  gap: 12px;
}
.lx-abar {
  width: 100%;
  border-radius: 9px 9px 5px 5px;
  background: linear-gradient(180deg, var(--sx-teal), #18a98c);
  transform: scaleY(var(--bi, 0));
  transform-origin: bottom;
  box-shadow: 0 10px 30px rgba(33, 196, 163, 0.2);
}
.lx-abar.is-loss {
  background: linear-gradient(180deg, #f0726f, var(--sx-danger));
  box-shadow: 0 10px 30px rgba(239, 68, 68, 0.22);
}
.lx-acol-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  color: #7e94aa;
  white-space: nowrap;
  opacity: var(--pb, 0);
}
.lx-assembly-tags {
  position: relative;
  display: flex;
  justify-content: space-between;
  max-width: 760px;
  margin: 22px auto 0;
  opacity: var(--pc, 0);
  transform: translateY(calc((1 - var(--pc, 0)) * 8px));
}
.lx-atag {
  font-size: 12.5px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.lx-atag-l, .lx-atag-r { color: var(--sx-teal); }
.lx-atag-c { color: #f0a0a0; }

.lx-band-foot {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 16px;
  flex-wrap: wrap;
  margin: 8px 0 0;
  color: #8aa0b6;
  font-size: 12.5px;
  opacity: var(--pc, 0);
}
.lx-band-foot-sep {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.2);
}

/* ===== Stats ===== */
.lx-stats {
  max-width: 1180px;
  margin: 0 auto;
  padding: 88px 28px;
}
.lx-stats-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 18px;
}
.lx-stat {
  display: grid;
  gap: 8px;
  padding: 28px 24px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 20px;
}
.lx-stat-num {
  font-size: clamp(40px, 5vw, 56px);
  font-weight: 600;
  line-height: 1;
  color: var(--sx-navy);
  letter-spacing: -0.02em;
}
.lx-stat-label {
  font-size: 13.5px;
  color: var(--sx-navy-muted);
  line-height: 1.4;
  max-width: 200px;
}

/* ===== Generic section ===== */
.lx-section {
  max-width: 1180px;
  margin: 0 auto;
  padding: 60px 28px;
}
.lx-section-head { max-width: 700px; margin-bottom: 48px; }
.lx-section-head.center { margin-inline: auto; text-align: center; }
.lx-section h2 {
  margin: 0;
  font-size: clamp(30px, 4.2vw, 48px);
  line-height: 1.08;
  letter-spacing: -0.028em;
  font-weight: 700;
  color: var(--sx-navy);
}
.lx-section-sub {
  margin: 18px 0 0;
  max-width: 540px;
  color: var(--sx-navy-muted);
  font-size: 17px;
  line-height: 1.6;
}

/* ===== Steps ===== */
.lx-steps {
  position: relative;
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 18px;
}
.lx-steps-line {
  position: absolute;
  top: 38px;
  left: 12%;
  right: 12%;
  height: 2px;
  background: linear-gradient(90deg, var(--sx-teal), var(--sx-blue-soft));
  transform: scaleX(0);
  transform-origin: left;
  opacity: 0;
  transition: transform 0.9s cubic-bezier(0.22, 1, 0.36, 1) 0.1s, opacity 0.4s ease;
}
.lx-steps-line.in { transform: scaleX(1); opacity: 0.5; }
.lx-step {
  position: relative;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 22px;
  padding: 26px;
  display: grid;
  gap: 9px;
}
.lx-step-index {
  display: inline-grid;
  place-items: center;
  width: 40px;
  height: 40px;
  border-radius: 12px;
  background: var(--sx-navy);
  color: #fff;
  font-size: 14px;
  font-weight: 600;
}
.lx-step h3 { margin: 6px 0 0; font-size: 18px; font-weight: 700; color: var(--sx-navy); }
.lx-step p { margin: 0; color: var(--sx-navy-muted); font-size: 14.5px; line-height: 1.55; }

/* ===== Split / safety ===== */
.lx-split {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 64px;
  align-items: center;
}
.lx-facts { list-style: none; margin: 28px 0 0; padding: 0; display: grid; gap: 16px; }
.lx-fact { display: flex; gap: 14px; align-items: flex-start; }
.lx-fact-mark {
  flex: 0 0 auto;
  display: inline-grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 9px;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  margin-top: 1px;
}
.lx-fact strong { display: block; font-size: 15px; color: var(--sx-navy); margin-bottom: 3px; }
.lx-fact p { margin: 0; font-size: 14px; color: var(--sx-navy-muted); line-height: 1.55; }

.lx-payoff-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-teal-soft);
  border-radius: 24px;
  padding: 26px;
  box-shadow: 0 28px 70px rgba(16, 40, 74, 0.1);
  display: grid;
  gap: 18px;
}
.lx-payoff-card-head { display: flex; align-items: center; justify-content: space-between; }
.lx-payoff-card-head strong { display: block; font-size: 15px; color: var(--sx-navy); margin-top: 2px; }
.lx-payoff-rows { display: grid; gap: 10px; }
.lx-payoff-row {
  display: grid;
  grid-template-columns: 92px 1fr 56px;
  align-items: center;
  gap: 14px;
}
.lx-payoff-row-range { font-size: 12px; color: var(--sx-navy-muted); white-space: nowrap; }
.lx-payoff-row-bar {
  height: 12px;
  background: var(--sx-surface-soft);
  border-radius: 999px;
  overflow: hidden;
}
.lx-payoff-row-fill {
  display: block;
  height: 100%;
  border-radius: 999px;
  background: var(--sx-teal);
}
.lx-payoff-row-fill.is-loss { background: var(--sx-danger); }
.lx-payoff-row-net { font-size: 13px; text-align: right; }
.lx-payoff-card-foot {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
  padding-top: 14px;
  border-top: 1px dashed var(--sx-border);
}
.lx-payoff-card-foot > div { display: grid; gap: 2px; }
.lx-payoff-card-foot strong { font-size: 19px; color: var(--sx-navy); }

/* ===== Final CTA ===== */
.lx-cta-section { padding-bottom: 120px; }
.lx-cta-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-teal-soft);
  border-radius: 30px;
  padding: 72px 40px;
  text-align: center;
  box-shadow: 0 28px 70px rgba(16, 40, 74, 0.09);
}
.lx-cta-card h2 { margin: 0 auto 14px; max-width: 540px; }
.lx-cta-card .lx-section-sub { margin: 0 auto 28px; }

/* ===== Responsive ===== */
@media (max-width: 980px) {
  .lx-hero { padding: 28px 22px 40px; }
  .lx-hero-grid { grid-template-columns: 1fr; gap: 40px; }
  .lx-hero-visual { min-height: 0; }
  .lx-stats-grid { grid-template-columns: repeat(2, 1fr); }
  .lx-split { grid-template-columns: 1fr; gap: 36px; }
  .lx-section { padding: 48px 22px; }
  .lx-pin { height: 220vh; }
  .lx-pin-inner { padding: 64px 22px; gap: 24px; }
}
@media (max-width: 600px) {
  .lx-steps { grid-template-columns: 1fr; }
  .lx-steps-line { display: none; }
  .lx-stats-grid { grid-template-columns: 1fr 1fr; }
  .lx-readout-nums { gap: 14px; }
  .lx-prims { gap: 10px; }
  .lx-prim { padding: 8px 13px; font-size: 13px; }
  .lx-acol-label { font-size: 8.5px; }
  .lx-cta-card { padding: 48px 22px; }
}
`;