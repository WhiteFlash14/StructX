const DEEPBOOK_X_URL = "https://x.com/DeepBookonSui";

function DeepBookPredictMark({ size = 24 }: { size?: number }) {
  return (
    <img
      src="/deepbook-predict.png"
      alt="DeepBook Predict logo"
      width={size}
      height={size}
      style={{ display: "block", borderRadius: size / 4, overflow: "hidden" }}
    />
  );
}

type Props = {
  variant?: "hero" | "footer";
};

export function DeepBookPredictAttribution({ variant = "footer" }: Props) {
  if (variant === "hero") {
    return (
      <div
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 12,
          marginTop: 14,
          padding: "10px 14px",
          borderRadius: 16,
          border: "1px solid rgba(78, 140, 255, 0.22)",
          background:
            "linear-gradient(135deg, rgba(46, 110, 235, 0.14), rgba(11, 20, 44, 0.86))",
          boxShadow: "0 10px 24px rgba(5, 10, 20, 0.18)",
        }}
      >
        <DeepBookPredictMark size={24} />
        <div style={{ display: "grid", gap: 2 }}>
          <span
            style={{
              fontSize: 11,
              letterSpacing: "0.14em",
              textTransform: "uppercase",
              color: "rgba(184, 204, 255, 0.78)",
              fontWeight: 700,
            }}
          >
            Built on
          </span>
          <a
            href={DEEPBOOK_X_URL}
            target="_blank"
            rel="noreferrer"
            style={{
              color: "var(--text)",
              textDecoration: "none",
              fontWeight: 700,
              fontSize: 14,
            }}
          >
            DeepBook Predict
          </a>
        </div>
      </div>
    );
  }

  return (
    <div
      style={{
        display: "grid",
        gap: 10,
        maxWidth: 760,
      }}
    >
      <div
        style={{
          display: "inline-flex",
          alignItems: "flex-start",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        <DeepBookPredictMark size={24} />
        <div style={{ display: "grid", gap: 4 }}>
          <strong style={{ fontSize: 15, color: "var(--text)" }}>
            Built on DeepBook Predict
          </strong>
          <span style={{ fontSize: 13, color: "var(--text-muted)", lineHeight: 1.45 }}>
            StructX compiles wallet-signed BTC payoff baskets on top of DeepBook Predict&apos;s
            expiry-based markets on Sui Testnet.
          </span>
        </div>
      </div>

    </div>
  );
}
