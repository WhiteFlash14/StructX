// Shared type definitions for the StructX frontend.

export type StrategyStyle = "tail-heavy" | "balanced" | "higher-hit-rate";
export type AppMode = "normal" | "advanced";
export type WorkspaceView = "strategies" | "positions";

export type StrategyId =
  | "BREAKOUT_PROTECTION"
  | "PORTFOLIO_CRASH_SHIELD"
  | "CONVEX_TAIL_LADDER"
  | "MOONSHOT_UPSIDE"
  | "DOWNSIDE_CONVEXITY"
  | "UPSIDE_STEP_LADDER"
  | "DOWNSIDE_STEP_LADDER"
  | "CENTER_BAND_CONDOR"
  | "NEAR_BARRIER_PROXY"
  | "RANGE_CONVICTION"
  | "EXPIRY_MOVE_NOTE"
  | "SMART_BUDGET_SELECTOR";

export type StrategyLeg = {
  kind: "DOWN" | "RANGE" | "UP";
  role: string;
  strike?: string;
  strikeRaw?: string;
  lower?: string;
  upper?: string;
  lowerRaw?: string;
  upperRaw?: string;
  quantityRaw: string;
  quantityDisplay: string;
  askPriceRaw: string;
  premiumRaw: string;
  premiumDisplay: string;
  maxCostRaw?: string;
};

export type PayoffRow = {
  condition: string;
  grossPayoutRaw: string;
  grossPayoutDisplay: string;
  netPnlRaw: string;
  netPnlDisplay: string;
};

export type StrikesBundle = {
  k1: string;
  k2: string;
  k3: string;
  k4: string;
  k1Raw: string;
  k2Raw: string;
  k3Raw: string;
  k4Raw: string;
};

export type SmartSelectorCandidate = {
  strategy: StrategyId;
  scoreE6: string;
  premiumRaw: string;
  maxPayoutRaw: string;
  expectedPayoutRaw: string;
  hitProbabilityBps: number;
  worstCaseImprovementRaw: string;
  complexityPenaltyBps: number;
  scoreBreakdown: {
    maxPayoutScoreE6: string;
    expectedPayoutScoreE6: string;
    hitProbabilityScoreE6: string;
    worstCaseScoreE6: string;
    complexityPenaltyE6: string;
  };
};

export type SmartSelectorInfo = {
  style: string;
  winner: StrategyId;
  winnerScoreE6: string;
  candidateCount: number;
  alternatives: SmartSelectorCandidate[];
};

export type CompileResponse = {
  ok: true;
  compiledStrategyId: string;
  strategy: StrategyId;
  selectedStrategy?: StrategyId;
  smartSelector?: SmartSelectorInfo;
  network: string;
  owner: string;
  oracleId: string;
  expiry: string;
  spot: string;
  style: StrategyStyle;
  styleRatioBps: number;
  budgetRaw: string;
  budgetDisplay: string;
  premiumRequiredRaw: string;
  premiumRequiredDisplay: string;
  maxLossRaw: string;
  maxLossDisplay: string;
  maxGrossPayoutRaw: string;
  maxGrossPayoutDisplay: string;
  maxNetPayoutRaw: string;
  maxNetPayoutDisplay: string;
  strikes: StrikesBundle;
  legs: StrategyLeg[];
  payoffTable: PayoffRow[];
  warnings: string[];
};

export type ParsedIntentSuccess = {
  ok: true;
  intentId: string;
  owner: string;
  rawMessage: string;
  asset: "BTC";
  goal:
    | "downside_protection"
    | "upside_speculation"
    | "two_sided_breakout"
    | "range_income"
    | "unknown";
  budgetDUSDC: string;
  riskPreference: "conservative" | "balanced" | "aggressive";
  timePreference: "nearest_active" | "today" | "this_week";
  recommendedStrategy: StrategyId;
  recommendedStyle: StrategyStyle;
  confidence: number;
  reasoningSummary: string;
  missingFields: string[];
  warnings: string[];
};

export type ParsedIntentFailure = {
  ok: false;
  missingFields: string[];
  clarifyingQuestion: string;
  fallbackIntent?: ParsedIntentSuccess;
};

export type ParsedIntentResponse = ParsedIntentSuccess | ParsedIntentFailure;

export type GuidedCompileResponse = CompileResponse & {
  recommendation?: {
    source: string;
    reasoningSummary: string;
    confidence: number;
    intent: unknown;
  };
};

export type BuildOpenStrategyResponse = {
  ok: true;
  buildKind: "FRONTEND_TRANSACTION_BUILDER";
  network: "sui:testnet";
  compiledStrategyId: string;
  expiryMs: string;
  owner: string;
  managerId: string;
  predictPackageId: string;
  predictObjectId: string;
  clockObjectId: string;
  dusdcCoinType: string;
  oracleId: string;
  slippageBps: number;
  summary: {
    strategy: StrategyId;
    premiumRequiredRaw: string;
    premiumRequiredDisplay: string;
    legs: StrategyLeg[];
  };
  warnings: string[];
};

export type ManagerBalanceResponse = {
  ok: boolean;
  balanceRaw?: string;
  balanceDisplay?: string;
  stdout?: string;
  stderr?: string;
  error?: string;
};

export type AuditMintedLeg = {
  index: number;
  event: "PositionMinted" | "RangeMinted" | string;
  kind: "DOWN" | "UP" | "RANGE" | string;
  direction: "up" | "down" | null;
  strike: string | null;
  lower: string | null;
  upper: string | null;
  quantityRaw: string;
  quantityDisplay: string;
  costRaw: string;
  costDisplay: string;
};

export type PositionVerificationItem = {
  index: number;
  kind: "binary" | "range" | string;
  direction: "up" | "down" | null;
  strike: string | null;
  lower: string | null;
  upper: string | null;
  mintedQty: string;
  managerQty: string;
  check: "ok" | "mismatch" | "unknown" | string;
};

export type PositionVerification = {
  status: "ok" | "partial" | "unknown" | string;
  verifiedCount: number;
  mismatchCount: number;
  items: PositionVerificationItem[];
  knownIssues: string[];
};

export type AuditResponse = {
  ok: boolean;
  digest?: string;
  explorerUrl?: string;
  executionStatus?: string;
  compiledStrategyId?: string;
  artifactPath?: string;
  totalCostRaw?: string;
  totalCostDisplay?: string;
  managerId?: string;
  managerBalanceRaw?: string | null;
  managerBalanceDisplay?: string | null;
  mintedLegs?: AuditMintedLeg[];
  positionVerification?: PositionVerification;
  warnings?: string[];
  debug?: {
    stdout?: string;
    stderr?: string;
  };
  // Legacy / error fallback shape
  stdout?: string;
  stderr?: string;
  error?: string;
};

export type PortfolioTradeRecord = {
  id: string;
  owner: string;
  managerId: string;
  strategy: StrategyId;
  requestedStrategy?: StrategyId;
  displayName: string;
  compiledStrategyId: string;
  digest: string;
  explorerUrl?: string;
  openedAt: string;
  expiry?: string;
  premiumPaidRaw: string;
  premiumPaidDisplay: string;
  maxLossRaw?: string;
  maxLossDisplay?: string;
  maxGrossPayoutRaw?: string;
  maxGrossPayoutDisplay?: string;
  maxNetPayoutRaw?: string;
  maxNetPayoutDisplay?: string;
  managerBalanceRaw?: string | null;
  managerBalanceDisplay?: string | null;
  executionStatus?: string;
  auditOk: boolean;
  legCount: number;
  mintedLegs: AuditMintedLeg[];
  categories?: string[];
  riskLabel?: string;
};

// Structured API error returned by the backend (newer endpoints).
export type ApiErrorBody = {
  ok: false;
  code?: string;
  title?: string;
  message?: string;
  action?: string;
  missingFields?: string[];
  clarifyingQuestion?: string;
  fallbackIntent?: unknown;
  // Legacy error fields kept for compatibility.
  error?: string;
  stdout?: string;
  stderr?: string;
  debug?: {
    stdout?: string;
    stderr?: string;
  };
};

export type CompileInput = {
  owner: string;
  strategy: StrategyId;
  budgetDUSDC: string;
  style: StrategyStyle;
  expiryPreference: string;
  slippageBps: number;
  bucketStepUsd?: number;
  customK1Price?: number;
  customK2Price?: number;
  customK3Price?: number;
  customK4Price?: number;
  portfolioExposureDUSDC?: number;
  overHedgeCapBps?: number;
  deadZoneBps?: number;
  convexGammaBps?: number;
  moonshotRangeWeightBps?: number;
  moonshotTailGammaBps?: number;
  upsideNearRangeWeightBps?: number;
  upsideUpperRangeWeightBps?: number;
  upsideTailGammaBps?: number;
  downsideNearRangeWeightBps?: number;
  downsideLowerRangeWeightBps?: number;
  downsideStepTailGammaBps?: number;
  condorCenterWeightBps?: number;
  barrierSide?: "up" | "down";
  barrierNearRangeWeightBps?: number;
  barrierTailGammaBps?: number;
};

export type ParseIntentInput = {
  owner: string;
  message: string;
  budgetDUSDC?: string;
  riskPreference?: "conservative" | "balanced" | "aggressive";
  timePreference?: "nearest_active" | "today" | "this_week";
};

export type CompileFromIntentInput = {
  owner: string;
  intent: ParsedIntentSuccess;
};

export type BuildOpenInput = {
  owner: string;
  managerId: string;
  compiledStrategyId: string;
  maxPremiumRaw: string;
  slippageBps: number;
};

export type AuditInput = {
  owner: string;
  managerId: string;
  compiledStrategyId: string;
  digest: string;
  effects: unknown;
  events: unknown[];
  objectChanges: unknown[];
};

export type StageStatus = "pending" | "active" | "success" | "failed";

export type ExecutionStage =
  | "configure"
  | "preview"
  | "preflight"
  | "dryRun"
  | "signature"
  | "submitted"
  | "audited";
