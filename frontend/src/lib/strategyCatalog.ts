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
    oneLiner: "Cover both sides when you expect BTC to make a large move by expiry.",
    useCase: "Useful before an event or any period when volatility may pick up.",
    riskLabel: "Defined risk. Your loss is capped at the premium.",
    fromBudgetDusdc: "from 50 dUSDC",
    categories: ["Breakout", "Protection"],
    accent: "blue",
    legHint: "Downside tail, two middle ranges, and an upside tail",
  },
  {
    id: "smart-budget-selector",
    strategyId: "SMART_BUDGET_SELECTOR",
    name: "Smart Budget Selector",
    displayName: "Smart Budget Selector",
    status: "recommended",
    oneLiner: "Compare the available strategies and find a strong fit for your budget.",
    useCase: "A good place to start when you know your budget and want help choosing the structure.",
    riskLabel: "Defined risk with a side-by-side strategy comparison.",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Protection", "Breakout"],
    accent: "blue",
    legHint:
      "Compares Breakout, Crash Insurance, Tail Ladder, Expiry Move Note, Moonshot, and directional ladders",
  },
  {
    id: "moonshot-upside",
    strategyId: "MOONSHOT_UPSIDE",
    name: "Moonshot Upside",
    displayName: "Moonshot Upside",
    status: "beta",
    oneLiner: "Get upside exposure when you think BTC can finish above the upper band.",
    useCase: "Pairs a nearby upside range with a smaller position farther above the market.",
    riskLabel: "Defined risk focused on upside moves.",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Upside", "Breakout"],
    accent: "emerald",
    legHint: "Upper range and upside tail",
  },
  {
    id: "upside-step-ladder",
    strategyId: "UPSIDE_STEP_LADDER",
    name: "Upside Step Ladder",
    displayName: "Upside Step Ladder",
    status: "beta",
    oneLiner: "Build a larger payout as BTC finishes at progressively higher levels.",
    useCase: "Fits a view where BTC may climb, break out, and keep moving higher.",
    riskLabel: "Defined risk across three upside levels.",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Upside", "Breakout"],
    accent: "emerald",
    legHint: "Near upside range, upper range, and upside tail",
  },
  {
    id: "downside-convexity",
    strategyId: "DOWNSIDE_CONVEXITY",
    name: "Downside Convexity",
    displayName: "Downside Convexity",
    status: "beta",
    oneLiner: "Get downside exposure when you think BTC can finish below the lower band.",
    useCase: "Pairs a nearby downside range with a smaller position deeper below the market.",
    riskLabel: "Defined risk focused on downside moves.",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Downside", "Breakout"],
    accent: "violet",
    legHint: "Lower range and downside tail",
  },
  {
    id: "downside-step-ladder",
    strategyId: "DOWNSIDE_STEP_LADDER",
    name: "Downside Step Ladder",
    displayName: "Downside Step Ladder",
    status: "beta",
    oneLiner: "Build a larger payout as BTC finishes at progressively lower levels.",
    useCase: "Fits a view where BTC may drift lower, break down, and keep falling.",
    riskLabel: "Defined risk across three downside levels.",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Downside", "Breakout"],
    accent: "violet",
    legHint: "Near downside range, lower range, and downside tail",
  },
  {
    id: "center-band-condor",
    strategyId: "CENTER_BAND_CONDOR",
    name: "Center Band Condor",
    displayName: "Center Band Condor",
    status: "beta",
    oneLiner: "Put most of the payout near the center with smaller ranges on either side.",
    useCase: "Fits a view that BTC will finish near its current price at the next expiry.",
    riskLabel: "Defined risk across a center range and two wings.",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Range", "Notes"],
    accent: "amber",
    legHint: "Two center ranges with a smaller range on each side",
  },
  {
    id: "near-barrier-proxy",
    strategyId: "NEAR_BARRIER_PROXY",
    name: "Near-Barrier Proxy",
    displayName: "Near-Barrier Proxy",
    status: "beta",
    oneLiner: "Target BTC finishing near or beyond a price level at expiry.",
    useCase: "Use this when the final expiry price matters more than the path BTC takes to get there.",
    riskLabel: "Defined risk based on the final expiry price.",
    fromBudgetDusdc: "from 2 dUSDC",
    categories: ["Upside", "Downside"],
    accent: "amber",
    legHint: "Near-barrier range and a tail beyond the barrier",
  },
  {
    id: "crash-insurance",
    strategyId: "PORTFOLIO_CRASH_SHIELD",
    name: "Crash Insurance",
    displayName: "Crash Insurance",
    status: "beta",
    oneLiner: "Size downside protection around the BTC exposure in your portfolio.",
    useCase: "Useful when a sharp BTC sell-off would cause a meaningful portfolio loss.",
    riskLabel: "Defined risk sized around your portfolio exposure.",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Protection", "Downside"],
    accent: "violet",
    legHint: "Downside tail and two downside ranges",
  },
  {
    id: "expiry-move-note",
    strategyId: "EXPIRY_MOVE_NOTE",
    name: "Expiry Move Note",
    displayName: "Expiry Move Note",
    status: "beta",
    oneLiner: "Pay more when BTC finishes farther from its current price at expiry.",
    useCase: "Fits a view that the next expiry will bring a meaningful move in either direction.",
    riskLabel: "Defined risk across both sides of the market.",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Range", "Notes"],
    accent: "emerald",
    legHint: "Downside tail, two middle ranges, and an upside tail",
  },
  {
    id: "convex-tail-ladder",
    strategyId: "CONVEX_TAIL_LADDER",
    name: "Convex Tail Ladder",
    displayName: "Convex Tail Ladder",
    status: "beta",
    oneLiner: "Receive a smaller payout for moderate moves and more for extreme moves.",
    useCase: "Fits a two-sided breakout view with extra weight in the far tails.",
    riskLabel: "Defined risk with larger payouts in the tails.",
    fromBudgetDusdc: "from 5 dUSDC",
    categories: ["Breakout", "Range"],
    accent: "amber",
    legHint: "Downside tail, two middle ranges, and an upside tail",
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
