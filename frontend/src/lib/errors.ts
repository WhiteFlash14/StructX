// Centralized error mapping for the StructX frontend.

import type { ApiErrorBody } from "@/types/structx";
import { ApiError } from "@/lib/api";

export type Severity = "info" | "caution" | "blocking";

export type FriendlyError = {
  title: string;
  message: string;
  action: string;
  severity: Severity;
  debug?: string;
};

const USER_REJECTION_HINTS = [
  "user rejected",
  "user denied",
  "rejected by user",
  "user closed",
  "rejected the request",
  "request rejected",
];

function isUserRejection(text: string): boolean {
  const lower = text.toLowerCase();
  return USER_REJECTION_HINTS.some((hint) => lower.includes(hint));
}

function mapMoveAbort(text: string): FriendlyError | null {
  const lower = text.toLowerCase();
  if (lower.includes("assert_mintable_ask") || lower.includes("easkpriceoutofbounds")) {
    return {
      title: "Quote moved",
      message:
        "The market quote changed before execution. The ask price drifted outside the slippage cap, so the transaction did not go through.",
      action: "Preview the strategy again and retry.",
      severity: "caution",
      debug: text,
    };
  }
  if (lower.includes("quote_spread_from_fair_price")) {
    return {
      title: "Market not currently quotable",
      message:
        "This market is temporarily not quotable within DeepBook Predict's guardrails.",
      action: "Preview again in a moment, or try a smaller budget.",
      severity: "caution",
      debug: text,
    };
  }
  if (lower.includes("invalidusageofpurearg")) {
    return {
      title: "Transaction builder error",
      message: "An invalid argument shape was passed to the Move call.",
      action: "Refresh the page and try again. If it keeps happening, report the digest.",
      severity: "blocking",
      debug: text,
    };
  }
  if (
    /failed to compile strategy after \d+ market attempts/i.test(text) ||
    lower.includes("no mintable size") ||
    (lower.includes("no executable") && lower.includes("market"))
  ) {
    return {
      title: "Budget too small for live market",
      message:
        "The live DeepBook Predict markets do not have a mintable size at this budget. Each leg has a minimum cost on-chain.",
      action: "Try a larger budget. 50 dUSDC is a safe starting point.",
      severity: "blocking",
      debug: text,
    };
  }
  if (lower.includes("insufficient manager balance")) {
    return {
      title: "Insufficient manager balance",
      message: "Your PredictManager does not have enough dUSDC to cover this premium.",
      action: "Reduce the budget, switch managers, or top up dUSDC.",
      severity: "blocking",
      debug: text,
    };
  }
  return null;
}

function mapApiCode(body: ApiErrorBody): FriendlyError | null {
  if (!body.code) return null;
  switch (body.code) {
    case "API_UNAVAILABLE":
      return {
        title: "API unavailable",
        message:
          body.message ?? "The StructX backend did not respond. It may be down or restarting.",
        action: body.action ?? "Restart the backend or wait a few seconds and retry.",
        severity: "blocking",
        debug: body.debug?.stderr ?? body.debug?.stdout,
      };
    case "COMPILE_FAILED":
      return {
        title: body.title ?? "Compile failed",
        message: body.message ?? "Could not compile the strategy with these inputs.",
        action: body.action ?? "Adjust the budget or style and try again.",
        severity: "blocking",
        debug: body.debug?.stderr ?? body.debug?.stdout,
      };
    case "INSUFFICIENT_MANAGER_BALANCE":
      return {
        title: body.title ?? "Insufficient manager balance",
        message:
          body.message ??
          "Your PredictManager does not have enough dUSDC for this premium.",
        action: body.action ?? "Reduce the budget or fund your PredictManager.",
        severity: "blocking",
        debug: body.debug?.stderr,
      };
    case "PREMIUM_EXCEEDS_BUDGET":
      return {
        title: body.title ?? "Premium exceeds budget",
        message:
          body.message ??
          "The premium required for this strategy exceeds your requested budget.",
        action: body.action ?? "Increase the budget or pick a different style.",
        severity: "blocking",
        debug: body.debug?.stderr,
      };
    case "UNSUPPORTED_NETWORK":
      return {
        title: "Wrong network",
        message: "StructX currently supports Sui Testnet only.",
        action: "Switch your wallet network to Sui Testnet.",
        severity: "blocking",
      };
    case "WALLET_NOT_CONNECTED":
      return {
        title: "Wallet not connected",
        message: "Connect a Sui wallet to continue.",
        action: "Click Connect Wallet in the top right.",
        severity: "blocking",
      };
    case "USER_REJECTED_SIGNATURE":
      return {
        title: "Signature rejected",
        message: "No transaction was submitted. You can review the strategy and try again.",
        action: "Click Open Strategy when ready.",
        severity: "caution",
      };
    case "TX_DRY_RUN_FAILED":
      return {
        title: "Dry-run failed",
        message: body.message ?? "Dry-run rejected this transaction.",
        action: body.action ?? "Preview the strategy again and retry.",
        severity: "blocking",
        debug: body.debug?.stderr,
      };
    case "TX_BUILD_FAILED":
      return {
        title: "Transaction build failed",
        message: body.message ?? "Could not build the transaction.",
        action: body.action ?? "Compile again and retry.",
        severity: "blocking",
        debug: body.debug?.stderr,
      };
    case "TX_EXECUTION_FAILED":
      return {
        title: "Transaction failed",
        message: body.message ?? "The transaction did not succeed on-chain.",
        action: body.action ?? "Preview the strategy again and retry.",
        severity: "blocking",
        debug: body.debug?.stderr,
      };
    case "MANAGER_BALANCE_UNAVAILABLE":
      return {
        title: "Manager balance unavailable",
        message:
          body.message ?? "Could not verify the PredictManager's dUSDC balance.",
        action: body.action ?? "Check the manager ID and refresh the balance.",
        severity: "blocking",
        debug: body.debug?.stderr,
      };
    case "AUDIT_FAILED":
      return {
        title: "Audit failed",
        message: body.message ?? "The audit step failed after the transaction executed.",
        action:
          body.action ??
          "The transaction may still be confirmed on-chain. Check the explorer link.",
        severity: "caution",
        debug: body.debug?.stderr,
      };
    case "KNOWN_BINARY_VERIFICATION_MISMATCH":
      return {
        title: "Known binary verification issue",
        message:
          body.message ??
          "Range positions verified, but binary manager-key reads returned 0. This is a known investigation item.",
        action: body.action ?? "Open the explorer link to confirm the on-chain effects.",
        severity: "caution",
      };
    default:
      return null;
  }
}

export function mapError(input: unknown): FriendlyError {
  if (input instanceof ApiError) {
    const mapped = mapApiCode(input.body);
    if (mapped) return mapped;

    const haystack =
      input.body?.message ??
      input.body?.error ??
      input.body?.stderr ??
      input.body?.stdout ??
      input.message;
    const moveMapped = mapMoveAbort(haystack ?? "");
    if (moveMapped) return moveMapped;

    return {
      title: input.body?.title ?? "Backend error",
      message: haystack ?? input.message,
      action: input.body?.action ?? "Try again, or report the digest.",
      severity: "blocking",
      debug: input.body?.debug?.stderr ?? input.body?.debug?.stdout,
    };
  }

  if (input instanceof Error) {
    if (isUserRejection(input.message)) {
      return {
        title: "Signature rejected",
        message: "No transaction was submitted.",
        action: "Review the strategy and try again.",
        severity: "caution",
      };
    }
    const moveMapped = mapMoveAbort(input.message);
    if (moveMapped) return moveMapped;
    return {
      title: "Something went wrong",
      message: input.message,
      action: "Retry, or refresh the page if it keeps happening.",
      severity: "blocking",
    };
  }

  return {
    title: "Unknown error",
    message: String(input ?? "no details"),
    action: "Try again.",
    severity: "blocking",
  };
}

export function mapDryRunFailure(error: string): FriendlyError {
  const moveMapped = mapMoveAbort(error);
  if (moveMapped) return moveMapped;
  return {
    title: "Dry-run failed",
    message: error,
    action: "Preview the strategy again, then retry.",
    severity: "blocking",
    debug: error,
  };
}

// Categorize compiler/preflight warnings into severities.
export function classifyWarning(text: string): Severity {
  const lower = text.toLowerCase();
  if (
    lower.includes("connect a sui wallet") ||
    lower.includes("unsupported wallet") ||
    lower.includes("wrong network") ||
    lower.includes("switch to sui testnet") ||
    lower.includes("insufficient manager balance") ||
    lower.includes("manager balance could not be verified") ||
    lower.includes("manager balance unavailable") ||
    lower.includes("premium exceeds budget") ||
    lower.includes("predictmanager id is required")
  ) {
    return "blocking";
  }
  if (
    lower.includes("quote can change") ||
    lower.includes("slippage") ||
    lower.includes("known issue") ||
    lower.includes("best-effort") ||
    lower.includes("partial")
  ) {
    return "caution";
  }
  return "info";
}
