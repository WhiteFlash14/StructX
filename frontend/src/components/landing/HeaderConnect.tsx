"use client";

import {
  ConnectButton,
  useCurrentAccount,
  useDisconnectWallet,
} from "@mysten/dapp-kit";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";

import { WalletAvatar } from "@/components/landing/WalletAvatar";
import { shortAddress } from "@/lib/format";

export function HeaderConnect() {
  const account = useCurrentAccount();
  return (
    <div className="landing-connect-wrap">
      {account ? <ProfileDropdown address={account.address} /> : <ConnectButton />}
    </div>
  );
}

function ProfileDropdown({ address }: { address: string }) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const { mutate: disconnect } = useDisconnectWallet();

  // Click-outside close. Also closes on Escape.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const onCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(address);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      // ignore — some browsers reject without user gesture chain
    }
  }, [address]);

  return (
    <div className="profile-dropdown-root" ref={rootRef}>
      <button
        type="button"
        className="profile-pill"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <WalletAvatar address={address} size={22} />
        <span className="profile-pill-addr mono">{shortAddress(address)}</span>
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={open ? "profile-pill-chev open" : "profile-pill-chev"}
          aria-hidden
        >
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>

      {open && (
        <div className="profile-menu" role="menu">
          <div className="profile-menu-head">
            <WalletAvatar address={address} size={36} />
            <div className="profile-menu-head-text">
              <strong className="mono" title={address}>
                {shortAddress(address)}
              </strong>
              <button
                type="button"
                className="profile-copy"
                onClick={onCopy}
                aria-label="Copy address"
              >
                {copied ? "Copied" : "Copy address"}
              </button>
            </div>
          </div>

          <nav className="profile-menu-section" aria-label="Account">
            <Link
              href="/markets"
              className="profile-menu-item"
              onClick={() => setOpen(false)}
              role="menuitem"
            >
              <span className="profile-menu-icon" aria-hidden>
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.7"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M3 17l5-5 4 4 5-7" />
                  <path d="M14 5h6v6" />
                </svg>
              </span>
              Live markets
            </Link>
            <Link
              href="/positions"
              className="profile-menu-item"
              onClick={() => setOpen(false)}
              role="menuitem"
            >
              <span className="profile-menu-icon" aria-hidden>
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.7"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M3 12h4l3-7 4 14 3-7h4" />
                </svg>
              </span>
              Open positions
            </Link>
            <Link
              href="/positions#closed"
              className="profile-menu-item"
              onClick={() => setOpen(false)}
              role="menuitem"
            >
              <span className="profile-menu-icon" aria-hidden>
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.7"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <circle cx="12" cy="12" r="9" />
                  <path d="M12 7v5l3 2" />
                </svg>
              </span>
              Transactions
            </Link>
          </nav>

          <div className="profile-menu-divider" />
          <button
            type="button"
            className="profile-menu-item danger"
            onClick={() => {
              disconnect();
              setOpen(false);
            }}
            role="menuitem"
          >
            <span className="profile-menu-icon" aria-hidden>
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.7"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M15 17l5-5-5-5" />
                <path d="M20 12H9" />
                <path d="M9 21H6a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3" />
              </svg>
            </span>
            Disconnect
          </button>
        </div>
      )}
    </div>
  );
}
