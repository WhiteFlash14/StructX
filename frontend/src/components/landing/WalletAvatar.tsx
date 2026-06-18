// Polymarket-style identicon: a deterministic, colorful blob derived from
// the wallet address. No image fetching, no IPFS — just two SVG circles
// with hues seeded from the hex digits of the address so the same wallet
// always renders the same avatar across sessions.
//
// Why this shape: a single linear-gradient often looks flat. Two off-center
// translucent circles on a gradient base produce the soft "smeared paint"
// look used by Polymarket / Farcaster / Optimism's profile widgets.

import { useMemo } from "react";

function hueFromHex(hex: string, start: number, len: number): number {
  // parseInt with NaN guard — a too-short address still renders.
  const slice = hex.slice(start, start + len);
  if (!slice) return 200;
  const n = parseInt(slice, 16);
  if (!Number.isFinite(n)) return 200;
  return n % 360;
}

export function WalletAvatar({
  address,
  size = 28,
  className,
}: {
  address: string | null | undefined;
  size?: number;
  className?: string;
}) {
  const palette = useMemo(() => {
    const hex = (address ?? "0x0").replace(/^0x/i, "").toLowerCase();
    const h1 = hueFromHex(hex, 0, 4);
    const h2 = hueFromHex(hex, 4, 4);
    const h3 = hueFromHex(hex, 8, 4);
    return {
      bgFrom: `hsl(${h1}, 78%, 68%)`,
      bgTo: `hsl(${(h2 + 180) % 360}, 70%, 52%)`,
      blob1: `hsl(${h2}, 85%, 64%)`,
      blob2: `hsl(${h3}, 85%, 56%)`,
    };
  }, [address]);

  // Stable gradient id per address so multiple avatars on the same page
  // don't share defs (which would clip the gradient to whichever rendered
  // last).
  const gradId = useMemo(
    () => `wa-${(address ?? "anon").slice(2, 10) || "anon"}`,
    [address],
  );

  return (
    <span
      className={className}
      style={{
        display: "inline-block",
        width: size,
        height: size,
        borderRadius: "50%",
        overflow: "hidden",
        flex: "0 0 auto",
        lineHeight: 0,
      }}
      aria-hidden
    >
      <svg
        viewBox="0 0 32 32"
        width={size}
        height={size}
        xmlns="http://www.w3.org/2000/svg"
      >
        <defs>
          <linearGradient id={gradId} x1="0" y1="0" x2="32" y2="32">
            <stop offset="0%" stopColor={palette.bgFrom} />
            <stop offset="100%" stopColor={palette.bgTo} />
          </linearGradient>
        </defs>
        <rect width="32" height="32" fill={`url(#${gradId})`} />
        <circle
          cx="10"
          cy="11"
          r="9"
          fill={palette.blob1}
          fillOpacity="0.78"
        />
        <circle
          cx="22"
          cy="21"
          r="8"
          fill={palette.blob2}
          fillOpacity="0.78"
        />
      </svg>
    </span>
  );
}
