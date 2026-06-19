// Catalog of StructX strategies. Drives the marketplace grid, search and filtering.

import type { StrategyId } from "@/types/structx";

export type StrategyCategory =
  | "Protection"
  | "Breakout"
  | "Downside"
  | "Upside"
  | "Range"
  | "Notes";

export type StrategyStatus = "recommended" | "beta";

export type StrategyCatalogEntry = {
  id: string;
  strategyId: StrategyId;
  name: string;
  displayName: string;
  status: StrategyStatus;
  oneLiner: string;
  useCase: string;
  riskLabel: string;
  fromBudgetDusdc: string;
  categories: StrategyCategory[];
  accent: "blue" | "violet" | "emerald" | "amber";
  // Hint for the builder: which legs the preset will mint, for tooltip/preview copy.
  legHint: string;
};

export function strategyDisplayName(strategyId: StrategyId): string {
  switch (strategyId) {
    case "SMART_BUDGET_SELECTOR":
      return "Smart Budget Selector";
    case "PORTFOLIO_CRASH_SHIELD":
      return "Crash Insurance";
    case "CONVEX_TAIL_LADDER":
      return "Convex Tail Ladder";
    case "EXPIRY_MOVE_NOTE":
      return "Expiry Move Note";
    case "MOONSHOT_UPSIDE":
      return "Moonshot Upside";
    case "UPSIDE_STEP_LADDER":
      return "Upside Step Ladder";
    case "DOWNSIDE_STEP_LADDER":
      return "Downside Step Ladder";
    case "CENTER_BAND_CONDOR":
      return "Center Band Condor";
    case "NEAR_BARRIER_PROXY":
      return "Near-Barrier Proxy";
    case "DOWNSIDE_CONVEXITY":
      return "Downside Convexity";
    default:
      return "Breakout Protection";
  }
}

export const STRATEGY_CATALOG: StrategyCatalogEntry[] = [
  {
    id: "breakout-protection",
    strategyId: "BREAKOUT_PROTECTION",
    name: "Breakout Protection",
    displayName: "Breakout Protection",
    status: "recommended",
    oneLiner: "Two-sided protection for large expiry moves.",
    useCase: "Hedge a position before a high-volatility window.",
    riskLabel: "Defined risk · loss capped at premium",
    fromBudgetDusdc: "from 50 dUSDC",
    categories: ["Breakout", "Protection"],
    accent: "blue",
    legHint: "DOWN tail · lower RANGE · upper RANGE · UP tail",
  },
  {
    id: "smart-budget-selector",
    strategyId: "SMART_BUDGET_SELECTOR",
    name: "Smart Budget Selector",
    displayName: "Smart Budget Selector",
    status: "recommended",
    oneLiner: "Compares available strategies and chooses the best fit for your budget.",
    useCase: "Let StructX rank executable candidates instead of picking one template yourself.",
    riskLabel: "Defined risk · candidate comparison",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Protection", "Breakout"],
    accent: "blue",
    legHint:
      "Selector chooses among Breakout, Crash Shield, Tail Ladder, Expiry Move Note, Moonshot, and staged directionals",
  },
  {
    id: "moonshot-upside",
    strategyId: "MOONSHOT_UPSIDE",
    name: "Moonshot Upside",
    displayName: "Moonshot Upside",
    status: "beta",
    oneLiner: "Defined-risk upside exposure if BTC expires above the upper band.",
    useCase: "Buy cheap asymmetric upside with a range plus a moonshot tail.",
    riskLabel: "Defined risk · upside-only convexity",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Upside", "Breakout"],
    accent: "emerald",
    legHint: "upper RANGE · UP tail",
  },
  {
    id: "upside-step-ladder",
    strategyId: "UPSIDE_STEP_LADDER",
    name: "Upside Step Ladder",
    displayName: "Upside Step Ladder",
    status: "beta",
    oneLiner: "Staged upside payout as BTC expires progressively higher.",
    useCase: "Express a grind-higher, breakout, then continuation-up view in one basket.",
    riskLabel: "Defined risk · staged upside ladder",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Upside", "Breakout"],
    accent: "emerald",
    legHint: "near upside RANGE · upper RANGE · UP tail",
  },
  {
    id: "downside-convexity",
    strategyId: "DOWNSIDE_CONVEXITY",
    name: "Downside Convexity",
    displayName: "Downside Convexity",
    status: "beta",
    oneLiner: "Defined-risk bearish exposure if BTC expires below the lower band.",
    useCase: "Buy cheap asymmetric downside with a range plus a crash tail.",
    riskLabel: "Defined risk · downside-only convexity",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Downside", "Breakout"],
    accent: "violet",
    legHint: "lower RANGE · DOWN tail",
  },
  {
    id: "downside-step-ladder",
    strategyId: "DOWNSIDE_STEP_LADDER",
    name: "Downside Step Ladder",
    displayName: "Downside Step Ladder",
    status: "beta",
    oneLiner: "Staged bearish payout as BTC expires progressively lower.",
    useCase: "Express a grind-lower, breakdown, then continuation-down view in one basket.",
    riskLabel: "Defined risk · staged downside ladder",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Downside", "Breakout"],
    accent: "violet",
    legHint: "near downside RANGE · lower RANGE · DOWN tail",
  },
  {
    id: "center-band-condor",
    strategyId: "CENTER_BAND_CONDOR",
    name: "Center Band Condor",
    displayName: "Center Band Condor",
    status: "beta",
    oneLiner: "Center-band payout with smaller outside wings if BTC expires nearby.",
    useCase: "Express a nearby-expiry view with a stronger center corridor and lighter outer bands.",
    riskLabel: "Defined risk · centered range with wings",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Range", "Notes"],
    accent: "amber",
    legHint: "lower wing RANGE · lower center RANGE · upper center RANGE · upper wing RANGE",
  },
  {
    id: "near-barrier-proxy",
    strategyId: "NEAR_BARRIER_PROXY",
    name: "Near-Barrier Proxy",
    displayName: "Near-Barrier Proxy",
    status: "beta",
    oneLiner: "Terminal-expiry proxy for BTC settling near or beyond an up or down barrier.",
    useCase: "Express a barrier-style expiry view without implying intraperiod touch behavior.",
    riskLabel: "Defined risk · terminal barrier proxy",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Upside", "Downside"],
    accent: "amber",
    legHint: "near-barrier RANGE · beyond-barrier binary tail",
  },
  {
    id: "crash-insurance",
    strategyId: "PORTFOLIO_CRASH_SHIELD",
    name: "Crash Insurance",
    displayName: "Crash Insurance",
    status: "beta",
    oneLiner: "Portfolio-aware downside protection for sharp BTC sell-offs.",
    useCase: "Size a defined-risk hedge against a deeper portfolio drawdown.",
    riskLabel: "Defined risk · portfolio-aware downside",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Protection", "Downside"],
    accent: "violet",
    legHint: "DOWN tail · lower RANGE · mild downside RANGE",
  },
  {
    id: "expiry-move-note",
    strategyId: "EXPIRY_MOVE_NOTE",
    name: "Expiry Move Note",
    displayName: "Expiry Move Note",
    status: "beta",
    oneLiner: "Pays more as BTC expires farther away from the current oracle price.",
    useCase: "Express a terminal move view around the next active expiry.",
    riskLabel: "Defined risk · terminal move",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Range", "Notes"],
    accent: "emerald",
    legHint: "DOWN tail · lower RANGE · upper RANGE · UP tail",
  },
  {
    id: "convex-tail-ladder",
    strategyId: "CONVEX_TAIL_LADDER",
    name: "Convex Tail Ladder",
    displayName: "Convex Tail Ladder",
    status: "beta",
    oneLiner: "Small payout for moderate moves, larger payout for extreme moves.",
    useCase: "Buy convex breakout exposure across downside and upside tails.",
    riskLabel: "Defined risk · convex breakout",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Breakout", "Range"],
    accent: "amber",
    legHint: "DOWN tail · lower RANGE · upper RANGE · UP tail",
  },
];

export const CATEGORY_TABS: { id: "All" | StrategyCategory | "Beta"; label: string }[] =
  [
    { id: "All", label: "All" },
    { id: "Protection", label: "Protection" },
    { id: "Breakout", label: "Breakout" },
    { id: "Downside", label: "Downside" },
    { id: "Upside", label: "Upside" },
    { id: "Range", label: "Range" },
    { id: "Notes", label: "Notes" },
    { id: "Beta", label: "Beta" },
  ];

export type CategoryTab = (typeof CATEGORY_TABS)[number]["id"];

export function filterStrategies(
  entries: StrategyCatalogEntry[],
  tab: CategoryTab,
  query: string,
): StrategyCatalogEntry[] {
  const q = query.trim().toLowerCase();
  return entries.filter((entry) => {
    const matchesTab =
      tab === "All"
        ? true
        : tab === "Beta"
          ? entry.status === "beta"
          : entry.categories.includes(tab);
    if (!matchesTab) return false;
    if (!q) return true;
    const haystack = [
      entry.name,
      entry.displayName,
      entry.oneLiner,
      entry.useCase,
      entry.riskLabel,
      entry.legHint,
      ...entry.categories,
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(q);
  });
}

export function findCatalogEntryById(
  id: string | null,
): StrategyCatalogEntry | null {
  if (!id) return null;
  return STRATEGY_CATALOG.find((entry) => entry.id === id) ?? null;
}

export function findCatalogEntryByStrategyId(
  strategyId: StrategyId | null,
): StrategyCatalogEntry | null {
  if (!strategyId) return null;
  return STRATEGY_CATALOG.find((entry) => entry.strategyId === strategyId) ?? null;
}
