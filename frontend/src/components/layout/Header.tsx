"use client";

import {
  ConnectButton,
  useCurrentAccount,
  useCurrentWallet,
  useDisconnectWallet,
  useSuiClientContext,
} from "@mysten/dapp-kit";
import { useEffect, useRef, useState } from "react";

import { CopyButton } from "@/components/common/CopyButton";
import { shortAddress } from "@/lib/format";
import type { WorkspaceView } from "@/types/structx";

type Props = {
  searchValue: string;
  onSearchChange: (value: string) => void;
  onCopied: (label: string) => void;
  currentView: WorkspaceView;
  onViewChange: (view: WorkspaceView) => void;
  managerBalance?: string | null;
  searchPlaceholder?: string;
};

export function Header({
  searchValue,
  onSearchChange,
  onCopied,
  currentView,
  onViewChange,
  managerBalance,
  searchPlaceholder = "Search strategies, payoff types, BTC…",
}: Props) {
  const account = useCurrentAccount();
  const { currentWallet } = useCurrentWallet();
  const { mutate: disconnectWallet } = useDisconnectWallet();
  const ctx = useSuiClientContext();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  const address = account?.address ?? null;
  const network = ctx.network;
  const isTestnet = network === "testnet";

  useEffect(() => {
    function onPointerDown(event: MouseEvent) {
      if (!menuRef.current) return;
      if (!menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }

    function onEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setMenuOpen(false);
    }

    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onEscape);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onEscape);
    };
  }, []);

  return (
    <header className="app-header">
      <div className="app-header-inner">
        <a className="brand" href="#top" aria-label="StructX home">
          <span className="brand-mark" aria-hidden>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
              <path
                d="M4 7l8-4 8 4-8 4-8-4z"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinejoin="round"
              />
              <path
                d="M4 12l8 4 8-4"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinejoin="round"
              />
              <path
                d="M4 17l8 4 8-4"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinejoin="round"
              />
            </svg>
          </span>
          <span className="brand-text">StructX</span>
        </a>

        <div className="search-wrap">
          <svg
            aria-hidden
            className="search-icon"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="11" cy="11" r="7" />
            <path d="M21 21l-4.3-4.3" />
          </svg>
          <input
            type="search"
            placeholder={searchPlaceholder}
            value={searchValue}
            onChange={(e) => onSearchChange(e.target.value)}
            aria-label="Search strategies"
          />
          <kbd className="search-kbd">/</kbd>
        </div>

        <div className="header-right">
          <span
            className={`network-badge ${isTestnet ? "ok" : "danger"}`}
            title={isTestnet ? "Sui Testnet" : `Network: ${network}`}
          >
            <span className="dot" aria-hidden />
            {isTestnet ? "Testnet" : (network ?? "unknown")}
          </span>

          {!address ? (
            <div className="connect-slot">
              <ConnectButton />
            </div>
          ) : (
            <div className="avatar-menu" title={shortAddress(address)} ref={menuRef}>
              <button
                type="button"
                className={`avatar-trigger ${menuOpen ? "open" : ""}`}
                onClick={() => setMenuOpen((open) => !open)}
                aria-haspopup="menu"
                aria-expanded={menuOpen}
              >
                <span className="avatar-orb" aria-hidden />
                <span className="avatar-trigger-label">{shortAddress(address)}</span>
                <svg
                  aria-hidden
                  className={`avatar-chevron ${menuOpen ? "open" : ""}`}
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </button>

              <div className={`avatar-pop ${menuOpen ? "open" : ""}`} role="menu">
                <div className="avatar-pop-head">
                  <span className="avatar-orb small" aria-hidden />
                  <div>
                    <strong>{currentWallet?.name ?? "Wallet"}</strong>
                    <span className="avatar-addr">{shortAddress(address)}</span>
                  </div>
                </div>
                <div className="avatar-balance-line">
                  <span>Selected manager balance</span>
                  <strong>{managerBalance ?? "—"}</strong>
                </div>
                <div className="avatar-pop-nav">
                  <button
                    type="button"
                    className={`avatar-menu-item ${currentView === "strategies" ? "active" : ""}`}
                    onClick={() => {
                      onViewChange("strategies");
                      setMenuOpen(false);
                    }}
                  >
                    Strategy library
                  </button>
                  <button
                    type="button"
                    className={`avatar-menu-item ${currentView === "positions" ? "active" : ""}`}
                    onClick={() => {
                      onViewChange("positions");
                      setMenuOpen(false);
                    }}
                  >
                    Positions & trades
                  </button>
                </div>
                <div className="avatar-pop-actions">
                  <CopyButton
                    value={address}
                    label="Copy address"
                    onCopied={() => onCopied("wallet address")}
                  />
                  <button
                    type="button"
                    className="ghost-button"
                    onClick={() => {
                      setMenuOpen(false);
                      disconnectWallet();
                    }}
                  >
                    Disconnect
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </header>
  );
}
