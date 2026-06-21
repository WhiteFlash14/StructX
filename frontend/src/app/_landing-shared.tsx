// Shared landing chrome: header, footer, brand mark, and global landing styles.
// Used by `/` (landing) and `/strategies`. Server-component safe.

import Link from "next/link";

import { HeaderConnect } from "@/components/landing/HeaderConnect";
import Image from "next/image";

export function BrandMark({ size = 44 }: { size?: number }) {
  // Flat teal square (small corner radius) with a bold "X" cross — Aleph-style monogram.
  return (
    <span
      className="brand-mark"
      aria-hidden
      style={{ width: size, height: size }}
    >
      <svg viewBox="0 0 44 44" width={size} height={size} fill="none">
        <rect width="44" height="44" rx="6" fill="currentColor" />
        <path
          d="M14 14l16 16M30 14l-16 16"
          stroke="#ffffff"
          strokeWidth="3.2"
          strokeLinecap="round"
        />
      </svg>
    </span>
  );
}

export function LandingHeader({
  showWallet = true,
  showLaunchApp = false,
}: {
  showWallet?: boolean;
  showLaunchApp?: boolean;
} = {}) {
  return (
    <header className="landing-header">
      <div className="landing-header-inner">
        {/*
          Two siblings, not nested: the StructX brand goes to "/", the
          "Built on DeepBook" link goes to DeepBook's X. Nesting the external
          <a> inside the <Link> would require an onClick to stop bubbling,
          which forces the file into a Client Component.
        */}
        <div className="landing-brand-wrap">
          <Link href="/" className="landing-brand" aria-label="StructX home">
            <Image
              src="/structx_x_logo_transparent_1x1.png"
              alt=""
              width={42}
              height={42}
              className="landing-brand-logo"
              priority
            />
            <span className="landing-brand-text">StructX</span>
          </Link>
          <a
            href="https://x.com/DeepBookonSui"
            target="_blank"
            rel="noreferrer noopener"
            className="landing-brand-poweredby"
            aria-label="Built on DeepBook"
          >
            <span className="landing-brand-poweredby-label">Built on</span>
            <img
              src="/deepbook-predict.png"
              alt=""
              width={14}
              height={14}
              className="landing-brand-poweredby-mark"
              aria-hidden
            />
            <span className="landing-brand-poweredby-name">DeepBook</span>
          </a>
        </div>
        {showLaunchApp ? (
          <div className="landing-header-cta">
            <Link
              href="/strategies"
              className="btn btn-primary compact landing-launch-btn"
            >
              Launch app
            </Link>
          </div>
        ) : showWallet ? (
          <div className="landing-header-cta">
            <HeaderConnect />
          </div>
        ) : null}
      </div>
    </header>
  );
}

export function LandingFooter() {
  return (
    <footer className="landing-footer">
      <div className="landing-footer-inner">
        <div className="landing-footer-meta">
          <div className="landing-footer-brand-block">
            <div className="landing-footer-brand">
              <Image
                src="/structx_x_logo_transparent_1x1.png"
                alt=""
                width={22}
                height={22}
                className="landing-brand-logo"
                priority
              />
              <strong>StructX</strong>
              <span className="landing-footer-year">© {new Date().getFullYear()}</span>
            </div>
          </div>
          <nav className="landing-footer-links" aria-label="Footer">
            <Link href="/strategies">Strategies</Link>
            <Link href="/#how-it-works">How it works</Link>
            <Link href="/#safety">Safety</Link>
            <a
              href="https://docs.sui.io/onchain-finance/deepbook-predict"
              target="_blank"
              rel="noreferrer noopener"
            >
              Docs
            </a>
          </nav>
        </div>
      </div>
    </footer>
  );
}

export function LandingStyles() {
  return (
    <style>{LANDING_CSS}</style>
  );
}

const LANDING_CSS = `
:root {
  --sx-bg: #f5f7fb;
  --sx-surface: #ffffff;
  --sx-surface-soft: #eef3f7;
  --sx-navy: #10284a;
  --sx-navy-muted: #405a78;
  --sx-muted: #7c8ba0;
  --sx-border: #dce6ef;
  --sx-border-strong: #c2d2e0;
  --sx-teal: #21c4a3;
  --sx-teal-dark: #0e9f83;
  --sx-teal-soft: #def8f1;
  --sx-blue-soft: #d9e7f8;
  --sx-warning: #f59e0b;
  --sx-danger: #ef4444;
}
html, body {
  margin: 0;
  padding: 0;
  min-height: 100%;
  background: var(--sx-bg);
  overscroll-behavior-y: none;
}

/* ===== Global accessibility + motion guard ============================
   prefers-reduced-motion: any user who's opted in to reduced motion at the
   OS level gets durations clamped to ~1ms and animations cancelled. That
   covers every transition/animation defined later in this file without
   having to thread an OS preference through React.
   focus-visible: every interactive element gets a clear keyboard ring.
   Mouse clicks don't trigger it (focus-visible only). */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}
.landing :is(button, a, [role="button"], [role="tab"], select):focus-visible {
  outline: 2px solid var(--sx-navy);
  outline-offset: 2px;
  border-radius: 8px;
}
/* Text fields match :focus-visible on a plain mouse click too, so a bright
   ring fires every time someone clicks in. Keep a quiet, on-brand ring for
   the bare cases; styled wrappers (search, form controls) suppress it and
   show the state on the container instead. */
.landing :is(input, textarea):focus-visible {
  outline: 2px solid var(--sx-navy);
  outline-offset: 1px;
  border-radius: 8px;
}

/* ===== Mode swap on /strategies =====
   Used by StrategiesView wrapping the conditional panel in a key=mode
   container so React unmounts/remounts on toggle and triggers this fade.
   180ms is the brief-recommended sweet spot for section enter. */
@keyframes mode-swap-in {
  from { opacity: 0; transform: translateY(4px); }
  to   { opacity: 1; transform: translateY(0); }
}
.landing .mode-swap {
  animation: mode-swap-in 180ms cubic-bezier(0.4, 0, 0.2, 1) both;
}

/* Staggered entrance for strategy cards — the first 8 cards animate in with
   a 30ms cascade so the Advanced grid doesn't pop all at once. Anything
   past 8 just fades in normally so we don't penalize long lists. */
@keyframes strategy-card-in {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0); }
}
.landing .strategy-cards .strategy-card {
  animation: strategy-card-in 220ms cubic-bezier(0.4, 0, 0.2, 1) both;
}
.landing .strategy-cards .strategy-card:nth-child(1) { animation-delay: 0ms; }
.landing .strategy-cards .strategy-card:nth-child(2) { animation-delay: 30ms; }
.landing .strategy-cards .strategy-card:nth-child(3) { animation-delay: 60ms; }
.landing .strategy-cards .strategy-card:nth-child(4) { animation-delay: 90ms; }
.landing .strategy-cards .strategy-card:nth-child(5) { animation-delay: 120ms; }
.landing .strategy-cards .strategy-card:nth-child(6) { animation-delay: 150ms; }
.landing .strategy-cards .strategy-card:nth-child(7) { animation-delay: 180ms; }
.landing .strategy-cards .strategy-card:nth-child(8) { animation-delay: 210ms; }

/* ===== UI Button primitive ===== */
.ui-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 40px;
  padding: 0 16px;
  border: 1px solid transparent;
  border-radius: 14px;
  font: inherit;
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.005em;
  line-height: 1;
  cursor: pointer;
  user-select: none;
  -webkit-tap-highlight-color: transparent;
  transition:
    background-color 120ms cubic-bezier(0.4, 0, 0.2, 1),
    color 120ms cubic-bezier(0.4, 0, 0.2, 1),
    border-color 120ms cubic-bezier(0.4, 0, 0.2, 1),
    box-shadow 120ms cubic-bezier(0.4, 0, 0.2, 1),
    transform 80ms cubic-bezier(0.4, 0, 0.2, 1);
}
.ui-btn:hover:not(:disabled) { transform: translateY(-1px); }
.ui-btn:active:not(:disabled) { transform: translateY(0) scale(0.99); }
.ui-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
  transform: none;
}
.ui-btn-full { width: 100%; }
.ui-btn-inner {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  transition: opacity 120ms cubic-bezier(0.4, 0, 0.2, 1);
}
.ui-btn.is-loading .ui-btn-inner { opacity: 0; }
.ui-btn-icon {
  display: inline-grid;
  place-items: center;
  flex: 0 0 auto;
}
.ui-btn-spinner {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: currentColor;
}
.ui-btn-spinner svg {
  animation: ui-btn-spin 0.8s linear infinite;
}
@keyframes ui-btn-spin { to { transform: rotate(360deg); } }

/* Variants */
.ui-btn-primary {
  background: var(--sx-navy);
  color: var(--sx-surface);
  box-shadow: 0 8px 22px rgba(16, 40, 74, 0.12);
}
.ui-btn-primary:hover:not(:disabled) { background: #0b1d36; }
.ui-btn-secondary {
  background: var(--sx-surface);
  color: var(--sx-navy);
  border-color: var(--sx-border-strong);
}
.ui-btn-secondary:hover:not(:disabled) {
  background: var(--sx-surface-soft);
  border-color: var(--sx-navy-muted);
}
.ui-btn-ghost {
  background: transparent;
  color: var(--sx-navy);
}
.ui-btn-ghost:hover:not(:disabled) { background: var(--sx-surface-soft); }
.ui-btn-danger {
  background: var(--sx-danger);
  color: #fff;
  box-shadow: 0 8px 22px rgba(239, 68, 68, 0.22);
}
.ui-btn-danger:hover:not(:disabled) { background: #d63a3a; }
.ui-btn-success {
  background: var(--sx-teal-dark);
  color: var(--sx-surface);
  box-shadow: 0 8px 22px rgba(14, 159, 131, 0.22);
}
.ui-btn-success:hover:not(:disabled) { background: #0c8a72; }
.ui-btn-icon {
  height: 36px;
  width: 36px;
  padding: 0;
  border-radius: 999px;
  background: var(--sx-surface);
  color: var(--sx-navy);
  border-color: var(--sx-border);
}
.ui-btn-icon:hover:not(:disabled) {
  background: var(--sx-surface-soft);
  border-color: var(--sx-border-strong);
}
.ui-btn-compact {
  height: 30px;
  padding: 0 12px;
  font-size: 12.5px;
  border-radius: 10px;
  background: var(--sx-surface);
  color: var(--sx-navy);
  border-color: var(--sx-border-strong);
}
.ui-btn-compact:hover:not(:disabled) {
  background: var(--sx-surface-soft);
}

/* ===== UI Skeleton primitive =====
   Solid base block with a moving gradient highlight that sweeps L → R.
   Animation runs at 1.4s — slow enough to feel calm, fast enough to read
   as "live". The global prefers-reduced-motion rule collapses the
   keyframe animation to ~1ms, leaving a static muted block. */
.ui-skel {
  display: inline-block;
  background: var(--sx-surface-soft);
  position: relative;
  overflow: hidden;
  vertical-align: top;
  flex: 0 0 auto;
}
.ui-skel::after {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(
    90deg,
    transparent 0%,
    rgba(255, 255, 255, 0.65) 50%,
    transparent 100%
  );
  transform: translateX(-100%);
  animation: ui-skel-shimmer 1.4s linear infinite;
}
@keyframes ui-skel-shimmer {
  to { transform: translateX(100%); }
}
.ui-skel-text { width: 100%; }

/* Skeleton-flavored wb-cards inside the workbench preview column.
   The wb-card-in entrance still applies; we just override the inner head
   so the skeleton's eyebrow placeholder doesn't fight the real layout. */
.wb-card.ui-skel-card .wb-card-head {
  margin-bottom: 14px;
}
.ui-skel-rows {
  display: grid;
  gap: 12px;
}
.ui-skel-row {
  display: grid;
  grid-template-columns: 64px 1fr auto auto;
  gap: 14px;
  align-items: center;
  padding: 6px 0;
  border-bottom: 1px solid var(--sx-border);
}
.ui-skel-row:last-child { border-bottom: 0; }
.ui-skel-row.scenario {
  grid-template-columns: 1fr auto auto;
}
.landing *,
.landing *::before,
.landing *::after {
  box-sizing: border-box;
}
.landing {
  background: var(--sx-bg);
  color: var(--sx-navy);
  font-family: var(--font-sans), "Inter", ui-sans-serif, system-ui,
    -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  font-feature-settings: "ss01" 1, "cv11" 1;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  text-rendering: optimizeLegibility;
  line-height: 1.55;
  min-height: 100dvh;
}
.landing,
.landing input,
.landing select,
.landing textarea,
.landing button {
  font-family: var(--font-sans), "Inter", ui-sans-serif, system-ui,
    -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
}
.landing a { color: inherit; text-decoration: none; }

/* ===== Brand mark ===== */
.brand-mark {
  display: inline-grid;
  place-items: center;
  color: var(--sx-teal);
  border-radius: 9px;
  flex: 0 0 auto;
}

/* ===== Header ===== */
.landing-header {
  position: sticky;
  top: 0;
  z-index: 30;
  background: rgba(245, 247, 251, 0.72);
  backdrop-filter: saturate(140%) blur(14px);
  -webkit-backdrop-filter: saturate(140%) blur(14px);
  /* No drawn divider. The translucent blur is enough to separate
   * the sticky header from the content scrolling underneath. */
}
.landing-header-inner {
  max-width: 1180px;
  margin: 0 auto;
  padding: 20px 28px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 28px;
}
.landing-brand {
  display: inline-flex;
  align-items: center;
  gap: 12px;
}
.landing-brand-text {
  font-family: var(--font-plex-mono), ui-monospace, "IBM Plex Mono",
    SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 32px;
  font-weight: 500;
  letter-spacing: -0.01em;
  color: var(--sx-navy);
  line-height: 1;
}
.landing-brand-wrap {
  display: inline-flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
}
.landing-brand-poweredby {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  /* Align under the StructX wordmark, nudged a bit further right so the
     DeepBook logo sits visually inside the wordmark, not at its left edge. */
  margin-left: 68px;
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.02em;
  color: var(--sx-navy-muted);
  text-decoration: none;
  line-height: 1;
}
.landing-brand-poweredby:hover {
  color: var(--sx-navy);
}
.landing-brand-poweredby-label {
  color: var(--sx-muted);
  font-weight: 500;
}
.landing-brand-poweredby-mark {
  display: block;
  border-radius: 3px;
}
.landing-brand-poweredby-name {
  font-weight: 600;
  letter-spacing: -0.005em;
}
.landing-nav {
  display: inline-flex;
  gap: 28px;
  margin-left: 24px;
  flex: 1;
}
.landing-nav a {
  font-size: 14px;
  color: var(--sx-navy-muted);
  font-weight: 500;
}
.landing-nav a:hover {
  color: var(--sx-navy);
}
.landing-header-cta {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 10px;
}
.landing-connect-wrap button,
.landing-connect-wrap [role="button"] {
  background: #cdebf6 !important;
  color: var(--sx-navy) !important;
  border: 1px solid transparent !important;
  border-radius: 999px !important;
  font-weight: 600 !important;
  font-size: 13.5px !important;
  letter-spacing: -0.005em !important;
  padding: 10px 18px !important;
  line-height: 1.2 !important;
  font-family: inherit !important;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.04) !important;
  transition: background 0.12s ease, color 0.12s ease,
    box-shadow 0.12s ease, transform 0.06s ease !important;
}
.landing-connect-wrap button:hover,
.landing-connect-wrap [role="button"]:hover {
  background: #b5e1f0 !important;
  color: var(--sx-navy) !important;
  box-shadow:
    0 1px 0 rgba(16, 40, 74, 0.04),
    0 6px 16px rgba(16, 40, 74, 0.06) !important;
}
.landing-connect-wrap button:active,
.landing-connect-wrap [role="button"]:active {
  transform: translateY(1px) !important;
}

/* ===== Profile dropdown (replaces ConnectButton when wallet connected) ===== */
.profile-dropdown-root {
  position: relative;
  display: inline-block;
}
/* Override the cyan ConnectButton skin for the profile pill so it reads as
   an identity widget, not a primary CTA. The .landing-connect-wrap button
   rule above is !important, so this one has to be too. */
.landing-connect-wrap .profile-pill {
  display: inline-flex !important;
  align-items: center !important;
  gap: 8px !important;
  background: var(--sx-surface) !important;
  color: var(--sx-navy) !important;
  border: 1px solid var(--sx-border) !important;
  border-radius: 999px !important;
  padding: 5px 10px 5px 6px !important;
  font-weight: 500 !important;
  font-size: 13px !important;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease !important;
  box-shadow: none !important;
}
.landing-connect-wrap .profile-pill:hover {
  background: var(--sx-surface-soft) !important;
  border-color: var(--sx-border-strong) !important;
  box-shadow: none !important;
}
.profile-pill-addr {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 12.5px;
  color: var(--sx-navy);
  letter-spacing: 0;
}
.profile-pill-chev {
  color: var(--sx-navy-muted);
  transition: transform 0.15s ease;
}
.profile-pill-chev.open {
  transform: rotate(180deg);
}

.profile-menu {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  min-width: 260px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
  box-shadow: 0 18px 60px rgba(8, 18, 36, 0.16);
  padding: 12px;
  z-index: 60;
  display: grid;
  gap: 6px;
}
.profile-menu-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 8px 12px;
  border-bottom: 1px solid var(--sx-border);
}
.profile-menu-head-text {
  display: grid;
  gap: 2px;
  min-width: 0;
  flex: 1;
}
.profile-menu-head-text strong {
  font-size: 13px;
  color: var(--sx-navy);
  font-weight: 600;
}
.profile-copy {
  align-self: flex-start;
  background: transparent;
  border: 0;
  padding: 0;
  margin-top: 2px;
  color: var(--sx-teal-dark);
  font-size: 11.5px;
  font-weight: 600;
  cursor: pointer;
  letter-spacing: 0.005em;
}
.profile-copy:hover { text-decoration: underline; }
.profile-menu-section {
  display: grid;
  gap: 2px;
  padding-top: 6px;
}
.profile-menu-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  background: transparent;
  border: 0;
  padding: 10px 10px;
  border-radius: 10px;
  color: var(--sx-navy);
  font-size: 13.5px;
  font-weight: 500;
  cursor: pointer;
  text-align: left;
  text-decoration: none;
  letter-spacing: -0.005em;
}
.profile-menu-item:hover {
  background: var(--sx-surface-soft);
  color: var(--sx-navy);
}
.profile-menu-item.danger { color: var(--sx-danger); }
.profile-menu-item.danger:hover { background: #fee2e2; }
.profile-menu-icon {
  display: inline-grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  background: var(--sx-surface-soft);
  color: var(--sx-navy-muted);
}
.profile-menu-item.danger .profile-menu-icon {
  color: var(--sx-danger);
  background: #fee2e2;
}
.profile-menu-divider {
  height: 1px;
  background: var(--sx-border);
  margin: 4px 0;
}
.landing-launch-btn {
  padding: 11px 20px;
  font-size: 14px;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.04);
}
.landing-launch-btn:hover {
  box-shadow:
    0 1px 0 rgba(16, 40, 74, 0.04),
    0 8px 18px rgba(16, 40, 74, 0.08);
}

/* ===== Buttons (pill-shaped) ===== */
.btn {
  cursor: pointer;
  font-weight: 600;
  font-size: 14px;
  border-radius: 999px;
  padding: 12px 22px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: background 0.12s ease, border-color 0.12s ease,
    color 0.12s ease, transform 0.06s ease;
  border: 1px solid transparent;
}
.btn.compact {
  padding: 9px 16px;
  font-size: 13px;
}
.btn-primary {
  background: var(--sx-teal);
  color: white;
}
.btn-primary:hover { background: var(--sx-teal-dark); }
.btn-outline {
  background: transparent;
  color: var(--sx-navy);
  border: 1px solid var(--sx-border-strong);
}
.btn-outline:hover {
  background: var(--sx-surface);
  border-color: var(--sx-navy-muted);
}

/* ===== Hero ===== */
.hero {
  max-width: 1180px;
  margin: 0 auto;
  padding: 36px 28px 120px;
}
.hero-grid {
  display: grid;
  grid-template-columns: 1.1fr 1fr;
  gap: 80px;
  align-items: center;
}
.eyebrow {
  margin: 0 0 18px;
  color: var(--sx-navy-muted);
  font-size: 12px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  font-weight: 700;
}
.accent-text { color: var(--sx-teal-dark); }
.hero h1 {
  margin: 0;
  font-size: clamp(40px, 6.5vw, 84px);
  letter-spacing: -0.035em;
  line-height: 1.04;
  font-weight: 700;
}
.hero h1 .accent { color: var(--sx-teal-dark); }
.hero-sub {
  margin: 28px 0 36px;
  color: var(--sx-navy-muted);
  font-size: 18px;
  max-width: 520px;
  line-height: 1.55;
}
.hero-ctas {
  display: inline-flex;
  gap: 12px;
  flex-wrap: wrap;
}
.hero-ctas.center { justify-content: center; }
.powered-by {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  margin-top: 28px;
  padding: 8px 14px 8px 12px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 999px;
  font-size: 12.5px;
  color: var(--sx-navy-muted);
  letter-spacing: -0.005em;
  transition:
    background 0.12s ease,
    border-color 0.12s ease;
}
.powered-by strong {
  color: var(--sx-navy);
  font-weight: 600;
}
.powered-by:hover {
  background: var(--sx-surface-soft);
  border-color: var(--sx-border-strong);
}
.powered-by-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: var(--sx-teal);
  box-shadow: 0 0 0 3px rgba(33, 196, 163, 0.18);
  flex: 0 0 auto;
}

/* ===== Hero visual ===== */
.hero-visual {
  position: relative;
  min-height: 440px;
  display: grid;
  place-items: center;
}
.hero-bigmark {
  position: absolute;
  inset: 0;
  color: var(--sx-border-strong);
  opacity: 0.32;
  display: grid;
  place-items: center;
}
.hero-bigmark svg {
  width: min(100%, 560px);
  height: auto;
}
.hero-mock {
  position: relative;
  width: min(100%, 420px);
  background: var(--sx-surface);
  border: 1px solid var(--sx-teal-soft);
  border-radius: 24px;
  padding: 22px;
  box-shadow: 0 24px 70px rgba(16, 40, 74, 0.08);
  z-index: 1;
}
.hero-mock.small { width: min(100%, 380px); }
.hero-mock-head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 16px;
}
.hero-mock-dot { width: 10px; height: 10px; border-radius: 50%; }
.hero-mock-dot.dot-r { background: #ff5f57; }
.hero-mock-dot.dot-y { background: #febc2e; }
.hero-mock-dot.dot-g { background: #28c840; }
.hero-mock-title { margin-left: 8px; font-size: 12px; color: var(--sx-muted); }
.hero-mock-body { display: grid; gap: 14px; }
.hero-mock-row { display: flex; align-items: center; gap: 8px; }
.hero-mock-row strong { font-size: 15px; }
.hero-mock-meta { margin-left: auto; color: var(--sx-muted); font-size: 12px; }
.hero-mock-pill.teal {
  padding: 3px 9px;
  font-size: 11px;
  font-weight: 700;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  border-radius: 999px;
}
.hero-mock-foot {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 8px;
  padding-top: 8px;
  border-top: 1px dashed var(--sx-border);
}
.hero-mock-foot > div { display: grid; }
.hero-mock-foot span {
  font-size: 10px;
  text-transform: uppercase;
  color: var(--sx-muted);
  letter-spacing: 0.08em;
}
.hero-mock-foot strong { font-size: 13px; }
.audit-ok {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--sx-teal-dark);
  font-weight: 700;
  font-size: 12px;
}
.audit-ok .dot {
  width: 7px;
  height: 7px;
  background: var(--sx-teal);
  border-radius: 50%;
}

/* Mini payoff chart in hero */
.payoff-mini {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  align-items: end;
  gap: 6px;
  height: 110px;
  padding: 8px 6px;
  background: var(--sx-surface-soft);
  border-radius: 12px;
}
.payoff-mini-bar {
  background: var(--sx-teal-soft);
  border-radius: 6px;
  min-height: 6px;
}
.payoff-mini-bar.tone-loss { background: rgba(239, 68, 68, 0.18); }

/* ===== Sections ===== */
.section {
  max-width: 1180px;
  margin: 0 auto;
  padding: 140px 28px;
}
.section-head {
  margin-bottom: 56px;
  max-width: 720px;
}
.section-head.center {
  margin: 0 auto 56px;
  text-align: center;
}
.section h2 {
  font-size: clamp(32px, 4.5vw, 52px);
  letter-spacing: -0.025em;
  margin: 0;
  line-height: 1.08;
  font-weight: 700;
}
.section-sub {
  margin: 18px 0 0;
  color: var(--sx-navy-muted);
  font-size: 17px;
  max-width: 600px;
  line-height: 1.6;
}

/* ===== Strategy cards (used on /strategies) ===== */
.strategies-hero {
  max-width: 1180px;
  margin: 0 auto;
  padding: 56px 28px 40px;
}
.strategies-eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  padding: 6px 12px 6px 10px;
  border-radius: 999px;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.strategies-eyebrow-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--sx-teal-dark);
}
.strategies-hero h1 {
  margin: 22px 0 0;
  font-size: clamp(40px, 5.4vw, 64px);
  letter-spacing: -0.035em;
  line-height: 1.05;
  font-weight: 600;
  color: var(--sx-navy);
  max-width: 920px;
}
.strategies-hero h1 .accent {
  color: var(--sx-teal-dark);
  font-weight: 600;
}
.strategies-sub {
  margin: 22px 0 0;
  max-width: 640px;
  color: var(--sx-navy-muted);
  font-size: 17px;
  line-height: 1.55;
  letter-spacing: -0.005em;
}
.strategies-meta {
  display: inline-flex;
  align-items: center;
  gap: 14px;
  margin-top: 28px;
  padding: 10px 16px;
  border: 1px solid var(--sx-border);
  border-radius: 999px;
  background: var(--sx-surface);
  color: var(--sx-navy-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.005em;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
}
.strategies-meta strong {
  color: var(--sx-navy);
  font-weight: 600;
  margin-right: 4px;
}
.strategies-meta .dot {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--sx-border-strong);
}
.strategies-grid-section { padding-top: 24px; padding-bottom: 96px; }

/* ===== Strategies clean list ===== */
.strategies-list-container {
  max-width: 960px;
  margin: 0 auto;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 24px;
  overflow: hidden;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
}
.strategies-list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.strategies-list-item {
  margin: 0;
  border-top: 1px solid var(--sx-border);
}
.strategies-list-item:first-child { border-top: 0; }
.strategies-row {
  display: flex;
  align-items: flex-start;
  gap: 22px;
  padding: 24px 28px;
  background: var(--sx-surface);
  transition: background 0.18s ease;
}
.strategies-row:hover { background: var(--sx-surface-soft); }
.strategies-row-glyph {
  width: 44px;
  height: 44px;
  border-radius: 10px;
  background: var(--sx-surface-soft);
  color: var(--sx-navy);
  display: grid;
  place-items: center;
  font-weight: 600;
  font-size: 17px;
  flex: 0 0 44px;
  letter-spacing: -0.01em;
  margin-top: 2px;
}
.strategies-row.accent-blue .strategies-row-glyph {
  background: var(--sx-blue-soft);
  color: #1d4ed8;
}
.strategies-row.accent-emerald .strategies-row-glyph {
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
}
.strategies-row.accent-violet .strategies-row-glyph {
  background: #ede9fe;
  color: #6d28d9;
}
.strategies-row.accent-amber .strategies-row-glyph {
  background: #fef3c7;
  color: #b45309;
}
.strategies-row-text { flex: 1; display: grid; gap: 8px; min-width: 0; }
.strategies-row-desc {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 14px;
  line-height: 1.5;
  letter-spacing: -0.005em;
  max-width: 580px;
}
.strategies-row-tags {
  display: inline-flex;
  gap: 6px;
  flex-wrap: wrap;
  margin-top: 4px;
}
.strategies-row-tag {
  font-size: 11px;
  font-weight: 500;
  color: var(--sx-navy-muted);
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  padding: 3px 9px;
  border-radius: 999px;
  line-height: 1.3;
}
.strategies-row:hover .strategies-row-tag {
  background: var(--sx-surface);
}
.strategies-row-title {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-size: 17px;
  line-height: 1.2;
}
.strategies-row-title strong {
  font-weight: 600;
  letter-spacing: -0.015em;
  color: var(--sx-navy);
}
.strategies-row-status {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  padding: 4px 9px;
  border-radius: 999px;
  line-height: 1;
}
.strategies-row-status.live {
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
}
.strategies-row-status.beta {
  background: #fef3c7;
  color: #b45309;
}
.strategies-row-tags {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--sx-muted);
}
.strategies-row-tag {
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  padding: 2px 8px;
  border-radius: 999px;
  color: var(--sx-navy-muted);
  font-weight: 500;
}
.strategies-row-dot { color: var(--sx-border-strong); }
.strategies-row-budget { font-weight: 500; }
.strategies-row-arrow {
  color: var(--sx-muted);
  flex: 0 0 auto;
  margin-top: 13px;
  transition: transform 0.18s ease, color 0.18s ease;
}
.strategies-row:hover .strategies-row-arrow {
  color: var(--sx-teal-dark);
  transform: translateX(4px);
}

/* ===== Detail page ===== */
.detail-shell {
  max-width: 1360px;
  margin: 0 auto;
  padding: 40px 28px 120px;
}
.detail-back {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--sx-navy-muted);
  font-size: 15px;
  font-weight: 500;
  margin-bottom: 24px;
}
.detail-back:hover { color: var(--sx-navy); }
.detail-head {
  display: grid;
  gap: 18px;
  margin-bottom: 48px;
}
.detail-head h1 {
  margin: 0;
  font-size: clamp(34px, 4.5vw, 52px);
  letter-spacing: -0.03em;
  line-height: 1.04;
  font-weight: 600;
}
.detail-status-row {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.detail-summary {
  color: var(--sx-navy-muted);
  font-size: 16.5px;
  max-width: 720px;
  line-height: 1.6;
  margin: 0;
  letter-spacing: -0.005em;
}
.detail-summary-meta {
  display: block;
  margin-top: 10px;
  color: var(--sx-muted);
  font-size: 13px;
  letter-spacing: 0;
}
.detail-meta {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin-top: 8px;
}
.detail-meta-cell {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
  padding: 14px 16px;
  display: grid;
  gap: 6px;
}
.detail-meta-cell span {
  display: block;
  color: var(--sx-muted);
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  font-weight: 600;
  line-height: 1;
}
.detail-meta-cell strong {
  display: block;
  font-size: 13.5px;
  font-weight: 500;
  color: var(--sx-navy);
  line-height: 1.4;
}

/* Workbench */
.workbench {
  display: grid;
  grid-template-columns: minmax(360px, 420px) minmax(0, 1fr);
  gap: 28px;
  align-items: start;
}
.workbench-form {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 20px;
  padding: 22px;
  display: grid;
  gap: 16px;
  position: sticky;
  top: 92px;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
  min-width: 0;
  width: 100%;
}
.workbench-form > * { min-width: 0; }
.workbench-form > header {
  display: grid;
  gap: 4px;
  padding-bottom: 4px;
}
.workbench-form h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.workbench-form-sub {
  margin: 0;
  color: var(--sx-muted);
  font-size: 12.5px;
  line-height: 1.5;
}
.wb-field {
  display: grid;
  gap: 8px;
}
.wb-label {
  font-size: 11px;
  color: var(--sx-navy-muted);
  text-transform: uppercase;
  letter-spacing: 0.09em;
  font-weight: 600;
  line-height: 1;
}
.wb-input,
.wb-select {
  width: 100%;
  background: var(--sx-surface);
  color: var(--sx-navy);
  border: 1px solid var(--sx-border);
  border-radius: 12px;
  padding: 11px 14px;
  outline: none;
  font-size: 14px;
  font-family: inherit;
  font-variant-numeric: tabular-nums;
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
.wb-input:focus,
.wb-select:focus {
  border-color: var(--sx-teal);
  box-shadow: 0 0 0 3px rgba(33, 196, 163, 0.16);
}
.wb-input.mono {
  font-family: var(--font-plex-mono), ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12.5px;
  letter-spacing: -0.01em;
}
.wb-input.with-suffix {
  padding-right: 64px;
}
.wb-input-wrap {
  position: relative;
  display: block;
  width: 100%;
  min-width: 0;
}
.wb-input-suffix {
  position: absolute;
  right: 14px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--sx-muted);
  font-size: 12px;
  font-weight: 500;
  pointer-events: none;
}
/* Themed Select inside a workbench field: match the .wb-input look. */
.wb-field .sx-select {
  width: 100%;
}
.wb-field .sx-select-trigger {
  width: 100%;
  height: 42px;
  border-radius: 12px;
  padding: 0 12px 0 14px;
  font-size: 14px;
}
.wb-seg {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 3px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  border-radius: 10px;
  padding: 3px;
}
.wb-seg-item {
  all: unset;
  cursor: pointer;
  text-align: center;
  padding: 9px 6px;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--sx-navy-muted);
  border-radius: 8px;
  transition: background 0.12s ease, color 0.12s ease, box-shadow 0.12s ease;
}
.wb-seg-item:hover { color: var(--sx-navy); }
.wb-seg-item.active {
  background: var(--sx-surface);
  color: var(--sx-navy);
  font-weight: 600;
  box-shadow:
    0 1px 2px rgba(16, 40, 74, 0.06),
    0 0 0 1px var(--sx-border);
}

/* ===== Style customize control ===== */
.wb-style-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}
.wb-style-note {
  margin-left: auto;
  font-size: 11px;
  color: var(--sx-muted);
  text-transform: uppercase;
  letter-spacing: 0.09em;
  font-weight: 600;
  line-height: 1;
}
.wb-style-toggle {
  all: unset;
  cursor: pointer;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--sx-teal-dark);
  letter-spacing: -0.005em;
  padding: 4px 0;
}
.wb-style-toggle:hover { color: var(--sx-navy); }
.wb-style-toggle.on { color: var(--sx-navy); }
.wb-style-default {
  margin: 0;
  font-size: 12.5px;
  color: var(--sx-muted);
  line-height: 1.4;
}
.wb-advanced-panel {
  display: grid;
  gap: 16px;
  padding: 16px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
}
.wb-grid-2,
.wb-grid-3 {
  display: grid;
  gap: 12px;
}
.wb-grid-2 {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}
.wb-grid-3 {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}
.wb-sliders {
  display: grid;
  gap: 12px;
  padding: 12px 14px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  border-radius: 12px;
}
.wb-slider-row {
  display: grid;
  gap: 6px;
}
.wb-slider-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 11.5px;
}
.wb-slider-head span {
  color: var(--sx-navy-muted);
  font-weight: 500;
  letter-spacing: -0.005em;
}
.wb-slider-head strong {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--sx-navy);
  font-variant-numeric: tabular-nums;
  min-width: 22px;
  text-align: right;
}
.wb-bias-labels {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 11px;
  color: var(--sx-navy-muted);
  font-weight: 500;
  letter-spacing: -0.005em;
}
.wb-slider {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 4px;
  background: var(--sx-border);
  border-radius: 999px;
  outline: none;
  cursor: pointer;
  margin: 0;
  padding: 0;
}
.wb-slider::-webkit-slider-runnable-track {
  height: 4px;
  background: var(--sx-border);
  border-radius: 999px;
}
.wb-slider::-moz-range-track {
  height: 4px;
  background: var(--sx-border);
  border-radius: 999px;
}
.wb-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--sx-teal);
  border: 2px solid var(--sx-surface);
  box-shadow: 0 1px 2px rgba(16, 40, 74, 0.18);
  cursor: pointer;
  margin-top: -6px;
  transition: transform 0.12s ease;
}
.wb-slider::-webkit-slider-thumb:hover { transform: scale(1.12); }
.wb-slider::-moz-range-thumb {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--sx-teal);
  border: 2px solid var(--sx-surface);
  box-shadow: 0 1px 2px rgba(16, 40, 74, 0.18);
  cursor: pointer;
  transition: transform 0.12s ease;
}
.wb-slider:focus::-webkit-slider-thumb {
  box-shadow:
    0 1px 2px rgba(16, 40, 74, 0.18),
    0 0 0 4px rgba(33, 196, 163, 0.18);
}
.wb-slider:focus::-moz-range-thumb {
  box-shadow:
    0 1px 2px rgba(16, 40, 74, 0.18),
    0 0 0 4px rgba(33, 196, 163, 0.18);
}
.wb-divider {
  height: 1px;
  background: var(--sx-border);
  margin: 4px 0;
}
.wb-advanced-toggle {
  all: unset;
  cursor: pointer;
  color: var(--sx-navy-muted);
  font-size: 12.5px;
  font-weight: 500;
  font-family: inherit;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 0;
}
.wb-advanced-toggle:hover { color: var(--sx-navy); }
.wb-advanced-toggle::before {
  content: "+";
  font-weight: 600;
  font-size: 14px;
  width: 14px;
  display: inline-grid;
  place-items: center;
}
.wb-advanced-toggle[aria-expanded="true"]::before { content: "−"; }
.wb-wallet {
  background: var(--sx-surface-soft);
  border-radius: 12px;
  padding: 12px 14px;
  display: grid;
  gap: 8px;
}
.wb-wallet-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 13px;
  min-height: 22px;
}
.wb-wallet-row span {
  color: var(--sx-muted);
  font-size: 11.5px;
  font-weight: 500;
}
.wb-wallet-row strong {
  font-family: var(--font-plex-mono), ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
  color: var(--sx-navy);
}
.wb-wallet-row strong.pos { color: var(--sx-teal-dark); font-weight: 600; }
.wb-wallet-row .net-bad { color: var(--sx-danger); font-weight: 600; }
.wb-help {
  font-size: 11px;
  color: var(--sx-muted);
  margin: 0;
}

/* PredictManager discover / auto-create status block */
.wb-discover {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--sx-border);
  background: var(--sx-surface);
}
.wb-discover.phase-checking,
.wb-discover.phase-creating { background: var(--sx-surface-soft); }
.wb-discover.phase-found,
.wb-discover.phase-created {
  background: var(--sx-teal-soft);
  border-color: rgba(33, 196, 163, 0.3);
}
.wb-discover.phase-error {
  background: #fef2f2;
  border-color: rgba(239, 68, 68, 0.3);
}
.wb-discover-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-top: 6px;
  flex-shrink: 0;
  background: var(--sx-muted);
}
.wb-discover-dot.dot-checking,
.wb-discover-dot.dot-creating {
  background: var(--sx-navy-muted);
  animation: wb-pulse 1.2s ease-in-out infinite;
}
.wb-discover-dot.dot-found,
.wb-discover-dot.dot-created { background: var(--sx-teal-dark); }
.wb-discover-dot.dot-error { background: var(--sx-danger); }
@keyframes wb-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}
.wb-discover-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}
.wb-discover-text strong {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--sx-navy);
}
.wb-discover-text span {
  font-size: 12px;
  color: var(--sx-navy-muted);
  line-height: 1.4;
}
.wb-discover-retry {
  align-self: flex-start;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border-strong);
  border-radius: 8px;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: -0.005em;
  color: var(--sx-navy);
  cursor: pointer;
}
.wb-discover-retry:hover { background: var(--sx-surface-soft); }
.wb-discover-id {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 10px;
  padding: 8px 12px;
  border-radius: 10px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
}
.wb-discover-id-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: var(--sx-muted);
  font-weight: 600;
}
.wb-discover-id-value {
  font-size: 12px;
  color: var(--sx-navy);
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.wb-discover-change {
  background: transparent;
  border: 0;
  color: var(--sx-teal-dark);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  padding: 4px 6px;
}
.wb-discover-change:hover { text-decoration: underline; }

.wb-button-row {
  display: grid;
  gap: 8px;
}

/* Preview column */
.workbench-preview {
  display: grid;
  gap: 16px;
  min-width: 0;
}
.wb-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 20px;
  padding: 24px 26px;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
  /* Compile / audit results stream into the workbench one wb-card at a time.
     A tiny enter animation keeps them from popping in cold. The global
     reduced-motion rule above clamps this to 1ms for users who opt out. */
  animation: wb-card-in 220ms cubic-bezier(0.4, 0, 0.2, 1) both;
}
@keyframes wb-card-in {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0); }
}
.wb-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 18px;
}
.wb-card-head h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.wb-card-head span {
  color: var(--sx-muted);
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  font-weight: 500;
}
.wb-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}
.wb-stat {
  background: var(--sx-surface-soft);
  border-radius: 14px;
  padding: 16px 16px 14px;
  display: grid;
  gap: 10px;
  min-height: 86px;
}
.wb-stat label {
  display: block;
  font-size: 10.5px;
  color: var(--sx-muted);
  text-transform: uppercase;
  letter-spacing: 0.09em;
  font-weight: 600;
  line-height: 1.1;
}
.wb-stat strong {
  display: block;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
  font-variant-numeric: tabular-nums;
  line-height: 1.1;
  color: var(--sx-navy);
}
.wb-stat.pos strong { color: var(--sx-teal-dark); }
.wb-stat.neg strong { color: var(--sx-danger); }

.wb-bars {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 14px;
  align-items: end;
  height: 220px;
  margin: 4px 0 10px;
  padding: 0 4px;
}
.wb-bar-col {
  display: grid;
  align-items: end;
  gap: 12px;
  grid-template-rows: 1fr auto;
  height: 100%;
}
.wb-bar {
  border-radius: 10px;
  background: var(--sx-teal);
  min-height: 6px;
  align-self: end;
  transition: filter 0.12s ease;
}
.wb-bar.tone-loss { background: var(--sx-danger); }
.wb-bar-label {
  text-align: center;
  font-size: 11px;
  color: var(--sx-muted);
  letter-spacing: 0;
  font-variant-numeric: tabular-nums;
  font-weight: 500;
}

.wb-empty {
  background: var(--sx-surface);
  border: 1px dashed var(--sx-border-strong);
  border-radius: 24px;
  padding: 48px 24px;
  text-align: center;
  color: var(--sx-navy-muted);
}
.wb-empty h3 {
  margin: 0 0 6px;
  color: var(--sx-navy);
  font-size: 16px;
  font-weight: 700;
}
.wb-empty p { margin: 0; font-size: 14px; }

.wb-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13.5px;
}
.wb-table th {
  text-align: left;
  font-size: 10.5px;
  color: var(--sx-muted);
  text-transform: uppercase;
  letter-spacing: 0.09em;
  padding: 10px 14px;
  border-bottom: 1px solid var(--sx-border);
  font-weight: 600;
}
.wb-table td {
  padding: 12px 14px;
  border-bottom: 1px solid var(--sx-border);
  color: var(--sx-navy);
  vertical-align: middle;
  font-weight: 500;
}
.wb-table tr:last-child td { border-bottom: 0; }
.wb-table .mono {
  font-family: var(--font-plex-mono), ui-monospace, SFMono-Regular, Menlo,
    monospace;
  font-size: 12.5px;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
  font-weight: 500;
}
.wb-table .pos { color: var(--sx-teal-dark); font-weight: 600; }
.wb-table .neg { color: var(--sx-danger); font-weight: 600; }
.wb-table tbody tr {
  transition: background 0.08s ease;
}
.wb-table tbody tr:hover {
  background: var(--sx-surface-soft);
}

.wb-kind {
  display: inline-flex;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.02em;
  background: var(--sx-surface-soft);
  color: var(--sx-navy-muted);
}
.wb-kind.down { background: #fee2e2; color: var(--sx-danger); }
.wb-kind.up { background: var(--sx-teal-soft); color: var(--sx-teal-dark); }
.wb-kind.range { background: #ede9fe; color: #6d28d9; }

.wb-alert {
  border-radius: 12px;
  padding: 14px 16px;
  display: grid;
  gap: 4px;
  font-size: 13.5px;
  line-height: 1.5;
  border: 1px solid;
  letter-spacing: -0.005em;
}
.wb-alert.info {
  background: #eff6ff;
  border-color: #bfdbfe;
  color: #1e3a8a;
}
.wb-alert.warn {
  background: #fff7ed;
  border-color: #fed7aa;
  color: #9a3412;
}
.wb-alert.danger {
  background: #fef2f2;
  border-color: #fecaca;
  color: #991b1b;
}
.wb-alert strong { font-weight: 700; }
.wb-alert small {
  display: block;
  font-size: 12px;
  opacity: 0.85;
  margin-top: 2px;
}

.wb-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 11px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.02em;
  background: var(--sx-surface-soft);
  color: var(--sx-navy-muted);
  border: 1px solid var(--sx-border);
  line-height: 1;
}
.wb-pill.live { background: var(--sx-teal-soft); color: var(--sx-teal-dark); border-color: transparent; }
.wb-pill.beta { background: #fef3c7; color: #b45309; border-color: transparent; }
.wb-pill.danger { background: #fee2e2; color: var(--sx-danger); border-color: transparent; }
.wb-pill .dot {
  width: 7px; height: 7px; border-radius: 999px; background: currentColor;
}

.wb-secondary {
  background: var(--sx-surface);
  color: var(--sx-navy);
  border: 1px solid var(--sx-border-strong);
  border-radius: 999px;
  padding: 11px 16px;
  font-size: 13px;
  font-weight: 500;
  letter-spacing: -0.005em;
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease,
    color 0.12s ease;
  font-family: inherit;
}
.wb-secondary:hover:not(:disabled) {
  background: var(--sx-surface-soft);
  border-color: var(--sx-navy-muted);
}
.wb-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.wb-primary {
  background: var(--sx-teal);
  color: white;
  border: 0;
  border-radius: 999px;
  padding: 12px 18px;
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.005em;
  cursor: pointer;
  transition: background 0.12s ease, transform 0.06s ease;
  font-family: inherit;
}
.wb-primary:hover:not(:disabled) { background: var(--sx-teal-dark); }
.wb-primary:active:not(:disabled) { transform: translateY(1px); }
.wb-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.wb-connect-wrap [data-testid="connect-button"],
.wb-connect-wrap button {
  background: var(--sx-navy) !important;
  color: white !important;
  border-radius: 999px !important;
  font-weight: 500 !important;
  padding: 8px 14px !important;
  border: 0 !important;
  font-size: 12.5px !important;
  letter-spacing: -0.005em !important;
  line-height: 1.2 !important;
  font-family: inherit !important;
  transition: background 0.12s ease !important;
}
.wb-connect-wrap button:hover { background: #1f3a64 !important; }

@media (max-width: 980px) {
  .workbench { grid-template-columns: 1fr; }
  .workbench-form { position: static; }
  .wb-stats { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .detail-meta { grid-template-columns: 1fr; }
  .wb-grid-3 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
@media (max-width: 640px) {
  .strategies-row { padding: 14px 16px; gap: 12px; }
  .strategies-row-glyph { width: 36px; height: 36px; flex-basis: 36px; }
  .strategies-row-tags { flex-wrap: wrap; }
  .wb-stats { grid-template-columns: 1fr; }
  .wb-bars { grid-template-columns: repeat(5, 1fr); height: 160px; }
  .wb-grid-2,
  .wb-grid-3 { grid-template-columns: 1fr; }
  .wb-style-head {
    align-items: flex-start;
  }
  .wb-style-note {
    width: 100%;
    margin-left: 0;
  }
}
.strategy-cards {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
}
.strategy-card {
  position: relative;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 20px;
  padding: 22px 22px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  text-decoration: none;
  color: inherit;
  overflow: hidden;
  transition:
    border-color 0.18s ease,
    box-shadow 0.22s ease,
    transform 0.22s cubic-bezier(0.4, 0, 0.2, 1);
}
.strategy-card.is-featured {
  border-color: var(--sx-border-strong);
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
}
.strategy-card:hover {
  border-color: var(--sx-border-strong);
  box-shadow: 0 18px 44px rgba(16, 40, 74, 0.09);
  transform: translateY(-2px);
}
.strategy-card:hover .strategy-card-rule {
  opacity: 1;
}

/* Accent rule across the very top of the card — subtle, only on hover. */
.strategy-card-rule {
  position: absolute;
  inset: 0 0 auto 0;
  height: 2px;
  opacity: 0.6;
  transition: opacity 0.18s ease;
}
.strategy-card.accent-blue .strategy-card-rule { background: #1d4ed8; }
.strategy-card.accent-emerald .strategy-card-rule { background: var(--sx-teal-dark); }
.strategy-card.accent-violet .strategy-card-rule { background: #6d28d9; }
.strategy-card.accent-amber .strategy-card-rule { background: #b45309; }

.strategy-card-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.strategy-card-glyph {
  width: 40px;
  height: 40px;
  border-radius: 12px;
  background: var(--sx-surface-soft);
  color: var(--sx-navy);
  display: grid;
  place-items: center;
}
.strategy-card.accent-blue .strategy-card-glyph {
  background: var(--sx-blue-soft);
  color: #1d4ed8;
}
.strategy-card.accent-emerald .strategy-card-glyph {
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
}
.strategy-card.accent-violet .strategy-card-glyph {
  background: #ede9fe;
  color: #6d28d9;
}
.strategy-card.accent-amber .strategy-card-glyph {
  background: #fef3c7;
  color: #b45309;
}

.strategy-card-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.04em;
  padding: 4px 9px 4px 7px;
  border-radius: 999px;
  border: 1px solid var(--sx-border);
  background: var(--sx-surface);
  color: var(--sx-navy-muted);
  text-transform: none;
}
.strategy-card-status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--sx-muted);
}
.strategy-card-status.live {
  border-color: rgba(33, 196, 163, 0.35);
  color: var(--sx-teal-dark);
  background: var(--sx-teal-soft);
}
.strategy-card-status.live .strategy-card-status-dot {
  background: var(--sx-teal-dark);
}
.strategy-card-status.beta {
  border-color: rgba(180, 83, 9, 0.25);
  color: #b45309;
  background: #fef3c7;
}
.strategy-card-status.beta .strategy-card-status-dot {
  background: #b45309;
}

.strategy-card-body {
  display: grid;
  gap: 6px;
}
.strategy-card-title {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  letter-spacing: -0.022em;
  line-height: 1.15;
  color: var(--sx-navy);
}
.strategy-card-desc {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 14px;
  line-height: 1.55;
  letter-spacing: -0.003em;
}
.strategy-card-tags {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.strategy-card-tag {
  /* Hard-pinned dimensions so every chip — across every card — renders to
     the same width and box. Earlier the first-card chips drifted because
     padding + line-height combined unevenly with the font fallback. */
  display: inline-flex;
  align-items: center;
  height: 24px;
  padding: 0 10px;
  font-size: 11.5px;
  font-weight: 500;
  letter-spacing: -0.005em;
  line-height: 1;
  color: var(--sx-navy-muted);
  background: var(--sx-surface-soft);
  border: 1px solid transparent;
  border-radius: 999px;
  text-transform: none;
  white-space: nowrap;
  box-sizing: border-box;
}

.strategy-card-meta {
  display: grid;
  grid-template-columns: 1.1fr 0.9fr 0.7fr;
  gap: 12px;
  padding-top: 14px;
  border-top: 1px solid var(--sx-border);
  align-items: end;
}
.strategy-card-meta-cell {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}
.strategy-card-meta-cell > span:first-child {
  display: block;
  color: var(--sx-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-weight: 500;
}
.strategy-card-meta-cell > strong {
  display: block;
  font-size: 13px;
  color: var(--sx-navy);
  font-weight: 500;
  letter-spacing: -0.005em;
  line-height: 1.2;
}
.strategy-card-meta-cell > strong.mono {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  font-weight: 500;
}

/* ===== Strategy card: payoff thumbnail ===== */
.strategy-card-thumb {
  display: block;
  margin: 2px 0 2px;
  padding: 8px 6px 4px;
  color: var(--sx-navy);
  border-radius: 12px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
}
.strategy-card.accent-blue .strategy-card-thumb {
  color: #1d4ed8;
  background: var(--sx-blue-soft);
  border-color: rgba(29, 78, 216, 0.18);
}
.strategy-card.accent-emerald .strategy-card-thumb {
  color: var(--sx-teal-dark);
  background: var(--sx-teal-soft);
  border-color: rgba(14, 159, 131, 0.22);
}
.strategy-card.accent-violet .strategy-card-thumb {
  color: #6d28d9;
  background: #ede9fe;
  border-color: rgba(109, 40, 217, 0.18);
}
.strategy-card.accent-amber .strategy-card-thumb {
  color: #b45309;
  background: #fef3c7;
  border-color: rgba(180, 83, 9, 0.2);
}
.strategy-card-thumb svg {
  display: block;
  width: 100%;
  height: 44px;
}
.strategy-card-thumb-axis {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 4px;
  padding: 0 2px;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 9.5px;
  color: var(--sx-muted);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  opacity: 0.82;
}
.strategy-card-thumb-arrow {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 11px;
  opacity: 0.85;
}

/* ===== Strategy card: featured pill ===== */
.strategy-card-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 9px;
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.01em;
  border-radius: 999px;
  border: 1px solid var(--sx-border);
  background: var(--sx-surface);
  color: var(--sx-navy-muted);
  white-space: nowrap;
}
.strategy-card-pill.is-featured {
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  border-color: rgba(33, 196, 163, 0.35);
}

/* ===== Strategy card: tags row pushes meta+CTA to the bottom ===== */
.strategy-card-tags { margin-top: auto; }

/* Accent-tinted border on hover so the whole card reads as a button. */
.strategy-card.accent-blue:hover { border-color: rgba(29, 78, 216, 0.45); }
.strategy-card.accent-emerald:hover { border-color: rgba(14, 159, 131, 0.5); }
.strategy-card.accent-violet:hover { border-color: rgba(109, 40, 217, 0.45); }
.strategy-card.accent-amber:hover { border-color: rgba(180, 83, 9, 0.5); }
.strategy-card.accent-blue:hover .strategy-card-title { color: #1d4ed8; }
.strategy-card.accent-emerald:hover .strategy-card-title { color: var(--sx-teal-dark); }
.strategy-card.accent-violet:hover .strategy-card-title { color: #6d28d9; }
.strategy-card.accent-amber:hover .strategy-card-title { color: #b45309; }
.strategy-card-title {
  transition: color 0.18s ease;
}

/* ===== Risk meter (three-dot indicator on cards) ===== */
.strategy-risk-meter {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.strategy-risk-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--sx-border-strong);
  flex: 0 0 7px;
}
.strategy-risk-dot.on {
  background: var(--sx-navy);
}
.strategy-card.accent-blue .strategy-risk-dot.on { background: #1d4ed8; }
.strategy-card.accent-emerald .strategy-risk-dot.on { background: var(--sx-teal-dark); }
.strategy-card.accent-violet .strategy-risk-dot.on { background: #6d28d9; }
.strategy-card.accent-amber .strategy-risk-dot.on { background: #b45309; }
.strategy-risk-label {
  margin-left: 4px;
  font-size: 11.5px;
  font-weight: 500;
  color: var(--sx-navy);
  letter-spacing: -0.005em;
  line-height: 1;
  white-space: nowrap;
}

/* ===== Strategies toolbar (search + filters + sort) ===== */
.strategies-toolbar {
  display: grid;
  gap: 12px;
  margin-bottom: 18px;
}
.strategies-toolbar-top {
  display: flex;
  align-items: center;
  gap: 12px;
}
.strategies-toolbar-search {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  height: 46px;
  padding: 0 8px 0 16px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
  color: var(--sx-navy-muted);
  flex: 1 1 auto;
  min-width: 0;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}
.strategies-toolbar-search:hover {
  border-color: var(--sx-border-strong);
}
/* The container owns the focus state. The inner input's own ring is
   suppressed below so we never paint the harsh bright outline a text input
   triggers on plain mouse click. */
.strategies-toolbar-search:focus-within {
  border-color: var(--sx-navy);
  box-shadow: 0 0 0 4px rgba(16, 40, 74, 0.07);
}
.strategies-toolbar-search svg {
  flex: 0 0 auto;
  color: var(--sx-muted);
}
.strategies-toolbar-search input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  font-size: 14px;
  color: var(--sx-navy);
  letter-spacing: -0.005em;
  font-family: inherit;
  height: 100%;
}
.strategies-toolbar-search input:focus,
.strategies-toolbar-search input:focus-visible {
  outline: none;
}
.strategies-toolbar-search input::placeholder {
  color: var(--sx-muted);
}
.strategies-toolbar-search input::-webkit-search-decoration,
.strategies-toolbar-search input::-webkit-search-cancel-button,
.strategies-toolbar-search input::-webkit-search-results-button,
.strategies-toolbar-search input::-webkit-search-results-decoration {
  appearance: none;
  display: none;
}
.strategies-toolbar-clear {
  display: inline-grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 999px;
  background: var(--sx-surface-soft);
  color: var(--sx-muted);
  cursor: pointer;
  flex: 0 0 auto;
  transition: background 0.15s ease, color 0.15s ease;
}
.strategies-toolbar-clear:hover {
  background: var(--sx-blue-soft);
  color: var(--sx-navy);
}
.strategies-toolbar-clear:focus-visible {
  outline: 2px solid var(--sx-navy);
  outline-offset: 2px;
}

.strategies-filters {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
  overflow-x: auto;
  scrollbar-width: none;
}
.strategies-filters::-webkit-scrollbar { display: none; }
.strategies-filter {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 14px;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: var(--sx-navy-muted);
  font-family: inherit;
  font-size: 13px;
  font-weight: 500;
  letter-spacing: -0.005em;
  cursor: pointer;
  transition: background 0.13s ease, color 0.13s ease;
  line-height: 1;
  white-space: nowrap;
  flex: 0 0 auto;
}
.strategies-filter:hover {
  color: var(--sx-navy);
  background: rgba(16, 40, 74, 0.045);
}
.strategies-filter.active {
  background: var(--sx-navy);
  color: var(--sx-surface);
}
.strategies-filter.active:hover {
  background: #1a3057;
}
.strategies-filter:focus-visible {
  outline: 2px solid var(--sx-navy);
  outline-offset: 2px;
}
.strategies-filter-count {
  display: inline-grid;
  place-items: center;
  min-width: 18px;
  height: 16px;
  padding: 0 5px;
  border-radius: 999px;
  background: rgba(16, 40, 74, 0.06);
  color: var(--sx-navy-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  font-weight: 600;
  line-height: 1;
  transition: background 0.13s ease, color 0.13s ease;
}
.strategies-filter:hover .strategies-filter-count {
  background: rgba(16, 40, 74, 0.09);
  color: var(--sx-navy);
}
.strategies-filter.active .strategies-filter-count {
  background: rgba(255, 255, 255, 0.18);
  color: var(--sx-surface);
}

.strategies-sort {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex: 0 0 auto;
}
.strategies-sort-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: var(--sx-muted);
  font-weight: 600;
}

/* ===== Strategies result bar (above the grid) ===== */
.strategies-result-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
  padding: 0 4px;
  color: var(--sx-navy-muted);
  font-size: 13px;
  letter-spacing: -0.005em;
}
.strategies-result-count strong {
  color: var(--sx-navy);
  font-weight: 600;
}
.strategies-result-clear {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 12px;
  border: 0;
  background: transparent;
  color: var(--sx-navy-muted);
  font-family: inherit;
  font-weight: 500;
  font-size: 12.5px;
  letter-spacing: -0.005em;
  cursor: pointer;
  border-radius: 999px;
  transition: background 0.13s ease, color 0.13s ease;
}
.strategies-result-clear svg {
  flex: 0 0 auto;
  transition: transform 0.25s ease;
}
.strategies-result-clear:hover {
  background: var(--sx-surface);
  color: var(--sx-navy);
}
.strategies-result-clear:hover svg {
  transform: rotate(-90deg);
}
.strategies-result-clear:focus-visible {
  outline: 2px solid rgba(33, 196, 163, 0.55);
  outline-offset: 2px;
}

/* ===== Strategies empty state ===== */
.strategies-empty {
  display: grid;
  justify-items: center;
  text-align: center;
  gap: 10px;
  padding: 72px 28px;
  background: var(--sx-surface);
  border: 1px dashed var(--sx-border-strong);
  border-radius: 24px;
  color: var(--sx-navy-muted);
}
.strategies-empty-glyph {
  display: inline-grid;
  place-items: center;
  width: 56px;
  height: 56px;
  border-radius: 50%;
  background: var(--sx-surface-soft);
  color: var(--sx-navy);
}
.strategies-empty-title {
  margin: 6px 0 0;
  color: var(--sx-navy);
  font-size: 19px;
  font-weight: 600;
  letter-spacing: -0.018em;
}
.strategies-empty-sub {
  margin: 0;
  font-size: 14px;
  color: var(--sx-navy-muted);
  max-width: 380px;
  line-height: 1.55;
}
.strategies-empty-cta {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
  height: 40px;
  padding: 0 22px;
  background: var(--sx-navy);
  color: var(--sx-surface);
  border: 0;
  border-radius: 999px;
  font-family: inherit;
  font-size: 13.5px;
  font-weight: 600;
  letter-spacing: -0.005em;
  cursor: pointer;
  transition: background 0.15s ease, transform 0.1s ease;
}
.strategies-empty-cta svg {
  transition: transform 0.25s ease;
}
.strategies-empty-cta:hover {
  background: #1a3057;
}
.strategies-empty-cta:hover svg {
  transform: rotate(-90deg);
}
.strategies-empty-cta:active {
  transform: translateY(1px);
}
.strategies-empty-cta:focus-visible {
  outline: 2px solid rgba(33, 196, 163, 0.55);
  outline-offset: 2px;
}

/* ===== How it works ===== */
.howto-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 18px;
}
.howto-step {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 24px;
  padding: 28px;
  display: grid;
  gap: 10px;
}
.howto-index {
  display: inline-grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 999px;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  font-weight: 800;
  font-size: 13px;
}
.howto-step h3 {
  margin: 4px 0 0;
  font-size: 17px;
  font-weight: 700;
}
.howto-step p {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 14px;
  line-height: 1.55;
}

/* ===== Split section ===== */
.split-section {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 96px;
  align-items: center;
}
.split-section.reverse .split-visual { order: 2; }
.split-section.reverse .split-text { order: 1; }
.split-text h2 { margin-bottom: 18px; }
.bullet-list {
  margin: 20px 0 0;
  padding: 0 0 0 18px;
  color: var(--sx-navy-muted);
  font-size: 14px;
  line-height: 1.75;
}

/* Payoff card showcase */
.payoff-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-teal-soft);
  border-radius: 24px;
  padding: 26px;
  box-shadow: 0 24px 70px rgba(16, 40, 74, 0.08);
  display: grid;
  gap: 18px;
}
.payoff-card-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.payoff-bars {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 12px;
  height: 220px;
  align-items: end;
}
.payoff-bar-col {
  display: grid;
  grid-template-rows: 1fr auto;
  align-items: end;
  gap: 10px;
  height: 100%;
}
.payoff-bar {
  border-radius: 10px;
  background: var(--sx-teal);
}
.payoff-bar.tone-loss { background: var(--sx-danger); }
.payoff-bar-label {
  text-align: center;
  font-size: 10px;
  color: var(--sx-muted);
  letter-spacing: 0.06em;
}
.payoff-card-foot {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 10px;
  padding-top: 10px;
  border-top: 1px dashed var(--sx-border);
}
.payoff-card-foot > div { display: grid; }
.payoff-card-foot span {
  font-size: 10px;
  color: var(--sx-muted);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.payoff-card-foot strong { font-size: 18px; }
.payoff-card-foot strong.pos { color: var(--sx-teal-dark); }
.mock-label {
  margin: 0;
  font-size: 11px;
  text-transform: uppercase;
  color: var(--sx-muted);
  letter-spacing: 0.08em;
}
.mock-pill.green {
  padding: 3px 9px;
  font-size: 11px;
  font-weight: 700;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  border-radius: 999px;
}

/* ===== Normal / Advanced toggle (new frontend) ===== */
.new-mode-toggle-wrap {
  margin-top: 32px;
}
.new-mode-toggle {
  position: relative;
  display: inline-grid;
  grid-template-columns: 1fr 1fr;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 999px;
  padding: 4px;
  gap: 0;
  min-width: 360px;
  box-shadow:
    0 1px 0 rgba(16, 40, 74, 0.03),
    inset 0 1px 0 rgba(255, 255, 255, 0.6);
}
/* The sliding navy "thumb" — the active tab is just the surface under it. */
.new-mode-thumb {
  position: absolute;
  top: 4px;
  bottom: 4px;
  left: 4px;
  width: calc(50% - 4px);
  border-radius: 999px;
  background: var(--sx-navy);
  transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  pointer-events: none;
  will-change: transform;
}
.new-mode-tab {
  position: relative;
  z-index: 1;
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 10px 18px;
  border-radius: 999px;
  border: 0;
  background: transparent;
  cursor: pointer;
  text-align: left;
  color: var(--sx-navy-muted);
  transition: color 0.18s ease;
  min-width: 0;
}
.new-mode-tab:hover {
  color: var(--sx-navy);
}
.new-mode-tab.active {
  color: var(--sx-surface);
}
.new-mode-tab-icon {
  display: inline-grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 999px;
  background: var(--sx-surface-soft);
  color: var(--sx-navy);
  flex: 0 0 auto;
  transition: background 0.18s ease, color 0.18s ease;
}
.new-mode-tab.active .new-mode-tab-icon {
  background: rgba(255, 255, 255, 0.12);
  color: var(--sx-surface);
}
.new-mode-tab-label {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.01em;
  line-height: 1;
}
/* Toggle is now smaller and single-line — adjust min-width to match. */
.new-mode-toggle { min-width: 260px; }
.new-mode-tab { padding: 9px 18px; }

/* ===== Normal Mode panel — production grade ===== */
.normal-panel {
  max-width: 1180px;
  margin: 0 auto;
}
.normal-panel-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.05fr) minmax(0, 1fr);
  gap: 22px;
  align-items: start;
}
.normal-panel-form {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 24px;
  padding: 28px 28px 24px;
  display: grid;
  gap: 18px;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
}
/* Two-stage step header: numbered step + heading/copy */
.normal-step {
  display: grid;
  grid-template-columns: 28px 1fr;
  gap: 12px;
  align-items: start;
}
.normal-step-secondary {
  margin-top: 4px;
}
.normal-step-num {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 999px;
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  font-size: 11.5px;
  font-weight: 700;
  letter-spacing: 0.01em;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
}
.normal-step-num.small {
  width: 20px;
  height: 20px;
  font-size: 10.5px;
}
.normal-step h2 {
  margin: 0 0 4px;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
  color: var(--sx-navy);
  line-height: 1.2;
}
.normal-step h3 {
  margin: 0;
  font-size: 14.5px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--sx-navy);
  align-self: center;
}
.normal-panel-sub {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 14px;
  line-height: 1.5;
}

/* Quick-intent chips with leading icon — these read as preset prompts. */
.normal-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.normal-chip {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  font-weight: 500;
  padding: 8px 12px;
  border-radius: 999px;
  background: var(--sx-bg);
  border: 1px solid var(--sx-border);
  color: var(--sx-navy);
  cursor: pointer;
  letter-spacing: -0.005em;
  transition: border-color 0.15s ease, background 0.15s ease, color 0.15s ease;
}
.normal-chip:hover {
  border-color: var(--sx-border-strong);
}
.normal-chip.active {
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  border-color: rgba(33, 196, 163, 0.4);
}
.normal-chip-icon {
  display: inline-grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border-radius: 6px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  color: var(--sx-navy-muted);
}
.normal-chip.active .normal-chip-icon {
  background: rgba(255, 255, 255, 0.5);
  color: var(--sx-teal-dark);
  border-color: rgba(33, 196, 163, 0.45);
}

.normal-field {
  display: grid;
  gap: 6px;
}
.normal-field-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: var(--sx-muted);
  font-weight: 600;
}
.normal-field textarea,
.normal-field input,
.normal-field select {
  font: inherit;
  font-size: 14px;
  padding: 11px 13px;
  border: 1px solid var(--sx-border);
  border-radius: 12px;
  background: var(--sx-bg);
  color: var(--sx-navy);
  width: 100%;
  box-sizing: border-box;
  outline: none;
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
.normal-field textarea {
  resize: vertical;
  line-height: 1.55;
}
.normal-field textarea:focus,
.normal-field input:focus,
.normal-field select:focus {
  border-color: var(--sx-teal-dark);
  box-shadow: 0 0 0 3px rgba(33, 196, 163, 0.15);
}
.normal-field select {
  appearance: none;
  -webkit-appearance: none;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%237c8ba0' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='M6 9l6 6 6-6'/></svg>");
  background-repeat: no-repeat;
  background-position: right 12px center;
  padding-right: 32px;
}
.normal-row {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 12px;
}
.normal-suffix {
  position: relative;
  display: flex;
  align-items: center;
}
.normal-suffix input {
  padding-right: 56px;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-variant-numeric: tabular-nums;
}
.normal-suffix > span {
  position: absolute;
  right: 13px;
  font-size: 12px;
  color: var(--sx-muted);
  font-weight: 500;
  pointer-events: none;
}

/* Primary CTA — full-width, with subtle motion on the trailing arrow. */
.normal-generate {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: var(--sx-navy);
  color: var(--sx-surface);
  border: 0;
  border-radius: 999px;
  padding: 13px 22px;
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.005em;
  cursor: pointer;
  transition: background 0.15s ease, transform 0.06s ease;
  box-shadow: 0 8px 22px rgba(16, 40, 74, 0.12);
}
.normal-generate:hover:not(:disabled) {
  background: #0b1d36;
}
.normal-generate:hover:not(:disabled) svg {
  transform: translateX(3px);
}
.normal-generate:active:not(:disabled) {
  transform: translateY(1px);
}
.normal-generate:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.normal-generate svg {
  transition: transform 0.15s ease;
}
.normal-generate-spinner {
  width: 14px;
  height: 14px;
  border-radius: 999px;
  border: 2px solid rgba(255, 255, 255, 0.35);
  border-top-color: var(--sx-surface);
  animation: normal-spin 0.7s linear infinite;
}
@keyframes normal-spin {
  to { transform: rotate(360deg); }
}

.normal-disclaimer {
  margin: 0;
  font-size: 12px;
  color: var(--sx-muted);
  line-height: 1.55;
}

/* ===== Output column ===== */
.normal-panel-output {
  display: grid;
  gap: 12px;
  align-content: start;
  position: sticky;
  top: 24px;
}
.normal-empty {
  background: var(--sx-surface);
  border: 1px dashed var(--sx-border);
  border-radius: 24px;
  padding: 36px 28px;
  text-align: center;
  color: var(--sx-navy-muted);
  display: grid;
  gap: 12px;
  justify-items: center;
}
.normal-empty-illustration {
  display: flex;
  gap: 6px;
  align-items: flex-end;
  width: 100%;
  max-width: 240px;
  height: 80px;
  margin-bottom: 4px;
}
.normal-empty-bar {
  flex: 1 1 0;
  border-radius: 6px;
  background: linear-gradient(180deg, var(--sx-teal-soft), var(--sx-surface-soft));
  opacity: 0.85;
}
.normal-empty-illustration.is-loading .normal-empty-bar {
  animation: normal-bar-pulse 1.2s ease-in-out infinite;
}
@keyframes normal-bar-pulse {
  0%, 100% { opacity: 0.45; }
  50% { opacity: 1; }
}
.normal-empty h3 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  color: var(--sx-navy);
  letter-spacing: -0.015em;
}
.normal-empty p {
  margin: 0;
  font-size: 13.5px;
  line-height: 1.55;
  max-width: 340px;
}

.normal-error {
  background: #fef2f2;
  border: 1px solid rgba(239, 68, 68, 0.25);
  border-radius: 16px;
  padding: 14px 16px;
  display: grid;
  gap: 4px;
}
.normal-error strong {
  color: var(--sx-danger);
  font-size: 13px;
  letter-spacing: -0.005em;
}
.normal-error p {
  margin: 0;
  color: var(--sx-navy);
  font-size: 13px;
  line-height: 1.55;
}

.normal-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 20px;
  padding: 22px;
  display: grid;
  gap: 14px;
}
.normal-card-recommend {
  border-color: rgba(33, 196, 163, 0.35);
  box-shadow:
    0 0 0 1px rgba(33, 196, 163, 0.06),
    0 14px 36px rgba(16, 40, 74, 0.06);
}
.normal-card h3 {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.02em;
  color: var(--sx-navy);
}
.normal-card-eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--sx-muted);
  font-weight: 600;
}
.normal-meta {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 18px;
  margin: 0;
  font-size: 13px;
  padding: 12px 14px;
  background: var(--sx-bg);
  border-radius: 12px;
  border: 1px solid var(--sx-border);
}
.normal-meta dt {
  color: var(--sx-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  align-self: center;
}
.normal-meta dd {
  margin: 0;
  color: var(--sx-navy);
  font-weight: 500;
  align-self: center;
}
.normal-meta dd.capitalize {
  text-transform: capitalize;
}
.normal-reason {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 13px;
  line-height: 1.55;
  padding-left: 12px;
  border-left: 2px solid var(--sx-teal-soft);
}

/* Payoff curve inside the recommend card (replaces the bar chart) */
.normal-curve {
  display: grid;
  gap: 6px;
  padding: 14px 14px 12px;
  background: var(--sx-bg);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
}
.normal-curve-svg {
  width: 100%;
  height: 130px;
  display: block;
}
.normal-curve-axis {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 6px;
  font-size: 9.5px;
  color: var(--sx-muted);
  letter-spacing: -0.005em;
  text-align: center;
}
.normal-curve-legend {
  display: flex;
  gap: 14px;
  justify-content: center;
  font-size: 10.5px;
  color: var(--sx-navy-muted);
  font-weight: 500;
  padding-top: 4px;
}
.normal-curve-legend > span {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.normal-curve-legend .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.normal-curve-legend .dot.teal { background: var(--sx-teal-dark); }
.normal-curve-legend .dot.red { background: var(--sx-danger); }
.normal-curve-legend .dot.dash {
  background: transparent;
  border: 1.2px dashed var(--sx-border-strong);
  border-radius: 0;
  width: 12px;
  height: 0;
  border-bottom: 1.4px dashed var(--sx-border-strong);
  margin-top: 1px;
}

/* Stepped loading narration */
.normal-loading {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 24px;
  padding: 28px 28px 24px;
  display: grid;
  gap: 18px;
  justify-items: center;
}
.normal-loading-illustration {
  display: flex;
  gap: 6px;
  align-items: flex-end;
  width: 100%;
  max-width: 220px;
  height: 70px;
}
.normal-loading-illustration .normal-empty-bar {
  flex: 1 1 0;
  border-radius: 6px;
  background: linear-gradient(180deg, var(--sx-teal-soft), var(--sx-surface-soft));
  animation: normal-bar-pulse 1.2s ease-in-out infinite;
}
.normal-loading-steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 12px;
  width: 100%;
  max-width: 340px;
}
.normal-loading-step {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  font-size: 14px;
  color: var(--sx-navy-muted);
  letter-spacing: -0.005em;
  transition: color 0.2s ease;
}
.normal-loading-step.state-active {
  color: var(--sx-navy);
  font-weight: 600;
}
.normal-loading-step.state-done {
  color: var(--sx-teal-dark);
  font-weight: 500;
}
.normal-loading-dot {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 999px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  color: var(--sx-teal-dark);
  flex: 0 0 auto;
  transition: background 0.2s ease, border-color 0.2s ease;
}
.normal-loading-step.state-active .normal-loading-dot {
  background: var(--sx-teal-soft);
  border-color: rgba(33, 196, 163, 0.4);
}
.normal-loading-step.state-done .normal-loading-dot {
  background: var(--sx-teal-dark);
  border-color: var(--sx-teal-dark);
  color: var(--sx-surface);
}
.normal-loading-bullet {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--sx-border-strong);
}
.normal-loading-step.state-active .normal-loading-bullet {
  background: var(--sx-teal-dark);
  animation: normal-loading-pulse 0.9s ease-in-out infinite;
}
@keyframes normal-loading-pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.55; transform: scale(0.85); }
}

.normal-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}
.normal-stat {
  display: grid;
  gap: 2px;
  padding: 11px 12px;
  background: var(--sx-bg);
  border: 1px solid var(--sx-border);
  border-radius: 12px;
}
.normal-stat span {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--sx-muted);
  font-weight: 600;
}
.normal-stat strong {
  font-size: 14px;
  color: var(--sx-navy);
  font-weight: 500;
  letter-spacing: -0.005em;
  font-variant-numeric: tabular-nums;
}
.normal-stat.pos strong { color: var(--sx-teal-dark); }
.normal-stat.neg strong { color: var(--sx-danger); }

.normal-cta {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: var(--sx-teal-dark);
  color: var(--sx-surface);
  padding: 13px 22px;
  border-radius: 999px;
  font-weight: 600;
  font-size: 14px;
  text-decoration: none;
  letter-spacing: -0.005em;
  align-self: start;
  justify-self: stretch;
  text-align: center;
  transition: background 0.15s ease, transform 0.06s ease;
  box-shadow: 0 8px 22px rgba(14, 159, 131, 0.22);
}
.normal-cta:hover {
  background: #0c8a72;
}
.normal-cta:hover svg {
  transform: translateX(3px);
}
.normal-cta:active {
  transform: translateY(1px);
}
.normal-cta svg {
  transition: transform 0.15s ease;
}

@media (max-width: 980px) {
  .normal-panel-grid { grid-template-columns: 1fr; }
  .normal-row { grid-template-columns: 1fr 1fr; }
  .normal-stats { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .normal-panel-output { position: static; }
}
@media (max-width: 640px) {
  .normal-row { grid-template-columns: 1fr; }
}

/* ===== Normal Mode — redesigned single-column stage ===== */
.normal-panel {
  display: grid;
  gap: 22px;
}
.normal-stage {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 24px;
  padding: 32px;
  display: grid;
  gap: 24px;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.02);
}
.normal-stage-head {
  display: grid;
  gap: 8px;
  max-width: 640px;
}
.normal-stage-title {
  margin: 0;
  font-size: clamp(24px, 2.6vw, 30px);
  font-weight: 600;
  letter-spacing: -0.022em;
  color: var(--sx-navy);
  line-height: 1.15;
}
.normal-stage-sub {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 14.5px;
  line-height: 1.55;
}

.normal-prompt-block {
  display: grid;
  gap: 14px;
}
.normal-prompt-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: var(--sx-muted);
  font-weight: 600;
}
.normal-prompt-input {
  width: 100%;
  box-sizing: border-box;
  font: inherit;
  font-size: 16px;
  line-height: 1.55;
  padding: 18px 20px;
  border: 1px solid var(--sx-border);
  border-radius: 16px;
  background: var(--sx-bg);
  color: var(--sx-navy);
  outline: none;
  resize: vertical;
  min-height: 96px;
  letter-spacing: -0.005em;
  transition: border-color 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
}
.normal-prompt-input::placeholder {
  color: var(--sx-muted);
}
.normal-prompt-input:focus {
  border-color: var(--sx-teal-dark);
  background: var(--sx-surface);
  box-shadow: 0 0 0 4px rgba(33, 196, 163, 0.16);
}

.normal-suggested {
  display: grid;
  gap: 10px;
}
.normal-suggested-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: var(--sx-muted);
  font-weight: 600;
}
.normal-suggested-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}
.normal-suggested-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  background: var(--sx-bg);
  border: 1px solid var(--sx-border);
  border-radius: 14px;
  text-align: left;
  cursor: pointer;
  color: var(--sx-navy);
  font: inherit;
  transition:
    border-color 0.18s ease,
    background 0.18s ease,
    transform 0.2s ease,
    box-shadow 0.18s ease;
}
.normal-suggested-card:hover {
  border-color: var(--sx-border-strong);
  background: var(--sx-surface);
  transform: translateY(-1px);
  box-shadow: 0 10px 22px rgba(16, 40, 74, 0.06);
}
.normal-suggested-card.active {
  border-color: rgba(33, 196, 163, 0.45);
  background: var(--sx-teal-soft);
}
.normal-suggested-icon {
  display: inline-grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 10px;
  flex: 0 0 32px;
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  color: var(--sx-navy);
}
.normal-suggested-card.tone-bullish .normal-suggested-icon {
  background: var(--sx-teal-soft);
  border-color: rgba(33, 196, 163, 0.35);
  color: var(--sx-teal-dark);
}
.normal-suggested-card.tone-protection .normal-suggested-icon {
  background: var(--sx-blue-soft);
  border-color: rgba(29, 78, 216, 0.25);
  color: #1d4ed8;
}
.normal-suggested-card.tone-range .normal-suggested-icon {
  background: #fef3c7;
  border-color: rgba(180, 83, 9, 0.22);
  color: #b45309;
}
.normal-suggested-card.tone-selector .normal-suggested-icon {
  background: #ede9fe;
  border-color: rgba(109, 40, 217, 0.22);
  color: #6d28d9;
}
.normal-suggested-card.active .normal-suggested-icon {
  background: var(--sx-surface);
  border-color: rgba(33, 196, 163, 0.45);
}
.normal-suggested-body {
  display: grid;
  gap: 4px;
  min-width: 0;
}
.normal-suggested-name {
  font-size: 13.5px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--sx-navy);
  line-height: 1.2;
}
.normal-suggested-text {
  font-size: 12px;
  line-height: 1.5;
  color: var(--sx-navy-muted);
  letter-spacing: -0.003em;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.normal-controls {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 12px;
  padding: 14px;
  background: var(--sx-bg);
  border: 1px solid var(--sx-border);
  border-radius: 16px;
}
.normal-control {
  display: grid;
  gap: 6px;
  min-width: 0;
}
.normal-control-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: var(--sx-muted);
  font-weight: 600;
}
.normal-control-input {
  position: relative;
  display: flex;
  align-items: center;
}
.normal-control-input input,
.normal-control-input select {
  width: 100%;
  box-sizing: border-box;
  font: inherit;
  font-size: 14px;
  padding: 11px 14px;
  border: 1px solid var(--sx-border);
  border-radius: 12px;
  background: var(--sx-surface);
  color: var(--sx-navy);
  outline: none;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}
.normal-control-input input:focus,
.normal-control-input select:focus {
  border-color: var(--sx-navy);
  box-shadow: 0 0 0 3px rgba(16, 40, 74, 0.07);
}
.normal-control-input .sx-select { width: 100%; }
.normal-control-input .sx-select-trigger {
  height: 44px;
  border-radius: 12px;
  padding: 0 12px 0 14px;
  font-size: 14px;
}
.normal-control-input select {
  appearance: none;
  -webkit-appearance: none;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%237c8ba0' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='M6 9l6 6 6-6'/></svg>");
  background-repeat: no-repeat;
  background-position: right 14px center;
  padding-right: 36px;
  cursor: pointer;
}
.normal-control-input.has-suffix input {
  padding-right: 64px;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-variant-numeric: tabular-nums;
}
.normal-control-suffix {
  position: absolute;
  right: 14px;
  font-size: 12px;
  color: var(--sx-muted);
  font-weight: 500;
  pointer-events: none;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  letter-spacing: 0.02em;
}
.normal-control-input.is-locked input {
  padding-right: 38px;
  color: var(--sx-navy-muted);
  cursor: not-allowed;
  background: var(--sx-surface-soft);
}
.normal-control-locked-icon {
  position: absolute;
  right: 14px;
  display: inline-grid;
  place-items: center;
  color: var(--sx-muted);
  pointer-events: none;
}

.normal-cta-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr);
  gap: 16px;
  align-items: center;
}
.normal-cta-row .normal-generate {
  margin: 0;
  width: 100%;
  padding: 15px 26px;
  font-size: 15px;
  border-radius: 999px;
}
.normal-cta-row .normal-disclaimer {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--sx-muted);
}

.normal-output {
  display: grid;
  gap: 12px;
  align-content: start;
}

@media (max-width: 980px) {
  .normal-stage { padding: 24px; gap: 20px; }
  .normal-suggested-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .normal-controls { grid-template-columns: 1fr 1fr; }
  .normal-controls .normal-control:last-child { grid-column: 1 / -1; }
  .normal-cta-row { grid-template-columns: 1fr; }
}
@media (max-width: 640px) {
  .normal-stage { padding: 20px; border-radius: 20px; }
  .normal-suggested-row { grid-template-columns: 1fr; }
  .normal-controls { grid-template-columns: 1fr; }
  .normal-controls .normal-control:last-child { grid-column: auto; }
}

/* ===== Final CTA ===== */
.cta-section {
  padding-top: 40px;
  padding-bottom: 140px;
}
.cta-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-teal-soft);
  border-radius: 32px;
  padding: 72px 40px;
  text-align: center;
  box-shadow: 0 24px 70px rgba(16, 40, 74, 0.08);
}
.cta-card h2 {
  margin: 0 auto 16px;
  max-width: 540px;
}
.cta-card .section-sub {
  margin: 0 auto 28px;
  max-width: 540px;
}

/* ===== Footer ===== */
.landing-footer {
  background: var(--sx-bg);
  padding: 64px 28px 56px;
}
.landing-footer-inner {
  max-width: 1180px;
  margin: 0 auto;
  padding-top: 24px;
  border-top: 1px solid var(--sx-border);
}
.landing-footer-meta {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  flex-wrap: wrap;
}
.landing-footer-brand-block {
  display: grid;
  gap: 14px;
  min-width: 0;
  /* Footer text color is dark navy in the rest of the landing — override the
     attribution component's CSS-var colors so the muted/primary text resolves
     against the light background. */
  --text: var(--sx-navy);
  --text-muted: var(--sx-navy-muted);
}
.landing-footer-brand {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-size: 14px;
}
.landing-footer-attribution a {
  color: var(--sx-navy);
}
.landing-footer-attribution a:hover {
  color: var(--sx-teal-dark);
}
.landing-footer-year {
  color: var(--sx-muted);
  margin-left: 8px;
  font-size: 12px;
}
.landing-footer-links {
  display: inline-flex;
  gap: 22px;
  margin-left: auto;
  font-size: 13px;
  color: var(--sx-navy-muted);
}
.landing-footer-links a:hover { color: var(--sx-navy); }
.landing-disclaimer {
  max-width: 1180px;
  margin: 22px auto 0;
  color: var(--sx-muted);
  font-size: 12px;
  line-height: 1.55;
}

/* ===== Responsive ===== */
@media (max-width: 980px) {
  .hero { padding: 24px 22px 64px; }
  .hero-grid { grid-template-columns: 1fr; gap: 40px; }
  .hero-visual { min-height: 360px; }
  .strategy-cards { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .howto-list { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .split-section { grid-template-columns: 1fr; gap: 40px; }
  .split-section.reverse .split-visual { order: 1; }
  .split-section.reverse .split-text { order: 2; }
  .section { padding: 96px 22px; }
  .strategies-hero { padding: 24px 22px 28px; }
  .detail-shell { padding: 28px 22px 96px; }
  .strategies-toolbar {
    gap: 10px;
  }
  .strategies-toolbar-top {
    flex-wrap: wrap;
  }
  .strategies-toolbar-search { flex: 1 1 100%; order: 1; }
  .strategies-sort { order: 2; margin-left: auto; }
  .strategies-filters {
    -webkit-overflow-scrolling: touch;
  }
}
@media (max-width: 640px) {
  .landing-nav { display: none; }
  .landing-header-inner { padding: 16px 20px; }
  .strategy-cards { grid-template-columns: 1fr; }
  .howto-list { grid-template-columns: 1fr; }
  .cta-card { padding: 48px 20px; }
  .hero-mock-foot { grid-template-columns: 1fr 1fr; }
  .strategies-result-bar { flex-direction: column; align-items: flex-start; gap: 6px; }
  .strategy-card-meta {
    grid-template-columns: 1fr 1fr;
  }
  .strategy-card-meta-cell:nth-child(3) {
    grid-column: 1 / -1;
  }
}
`;