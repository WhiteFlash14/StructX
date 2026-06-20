"use client";

import {
  ConnectButton,
  useCurrentAccount,
  useCurrentWallet,
  useDisconnectWallet,
  useSuiClientContext,
} from "@mysten/dapp-kit";

import { shortAddress } from "@/lib/format";

type Props = {
  onCopyAddress?: (address: string) => void;
};

export function Header({ onCopyAddress }: Props) {
  const account = useCurrentAccount();
  const { currentWallet } = useCurrentWallet();
  const { mutate: disconnectWallet } = useDisconnectWallet();
  const ctx = useSuiClientContext();

  const connected = account?.address ?? null;
  const network = ctx.network;
  const isTestnet = network === "testnet";

  return (
    <header className="hero">
      <div className="hero-text">
        <p className="eyebrow">Sui Testnet</p>
        <h1>Struct X</h1>
        <p className="subtitle">Powered by DeepBook Predict</p>
        <p className="subtitle">
          Structured payoff builder on DeepBook Predict. Compile a strategy,
          preview its legs and payoff, then open it from your own wallet.
          Non-custodial — StructX never holds your funds.
        </p>
      </div>

      <div className="wallet-card">
        <span className={isTestnet ? "network-badge" : "network-badge danger"}>
          <span className="dot" aria-hidden /> {isTestnet ? "Sui Testnet" : `Network: ${network}`}
        </span>

        <ConnectButton />

        {connected && (
          <div className="wallet-meta">
            <span>Connected</span>
            <button
              type="button"
              className="copy-pill"
              onClick={() => onCopyAddress?.(connected)}
              title="Copy wallet address"
            >
              {shortAddress(connected)}
              <span className="copy-glyph" aria-hidden>
                ⧉
              </span>
            </button>
            {currentWallet?.name && <small>{currentWallet.name}</small>}
            <button
              type="button"
              className="disconnect-button"
              onClick={() => disconnectWallet()}
            >
              Disconnect
            </button>
          </div>
        )}
      </div>
    </header>
  );
}
