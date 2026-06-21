import Link from "next/link";
import { notFound } from "next/navigation";

import {
  LandingFooter,
  LandingHeader,
  LandingStyles,
} from "@/app/_landing-shared";
import { StrategyWorkbench } from "@/components/landing/StrategyWorkbench";
import {
  findCatalogEntryById,
  STRATEGY_CATALOG,
} from "@/lib/strategyCatalog";

export function generateStaticParams() {
  return STRATEGY_CATALOG.map((entry) => ({ id: entry.id }));
}

type Params = { id: string };

export async function generateMetadata({
  params,
}: {
  params: Promise<Params>;
}) {
  const { id } = await params;
  const entry = findCatalogEntryById(id);
  if (!entry) return { title: "StructX" };
  return {
    title: "StructX",
    description: entry.oneLiner,
  };
}

export default async function StrategyDetailPage({
  params,
  searchParams,
}: {
  params: Promise<Params>;
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { id } = await params;
  const entry = findCatalogEntryById(id);
  if (!entry) notFound();

  // `?budget=NN` lets Normal Mode hand the user's chosen budget straight
  // into the Amount field so they don't have to retype it after the
  // recommendation. Defensive: only forward when it's a positive number.
  const sp = (await searchParams) ?? {};
  const rawBudget = Array.isArray(sp.budget) ? sp.budget[0] : sp.budget;
  const initialBudget =
    rawBudget && Number.isFinite(Number(rawBudget)) && Number(rawBudget) > 0
      ? String(rawBudget)
      : undefined;

  return (
    <main className="landing">
      <LandingHeader />
      <section className="detail-shell">
        <Link href="/strategies" className="detail-back">
          ← All strategies
        </Link>

        <header className="detail-head">
          <div className="detail-status-row">
            {entry.categories.map((c) => (
              <span key={c} className="wb-pill">
                {c}
              </span>
            ))}
          </div>
          <h1>{entry.displayName}</h1>
          <p className="detail-summary">
            {entry.oneLiner} {entry.useCase}
          </p>
        </header>

        <StrategyWorkbench
          strategyId={entry.strategyId}
          displayName={entry.displayName}
          initialBudgetDUSDC={initialBudget}
        />
      </section>
      <LandingFooter />
      <LandingStyles />
    </main>
  );
}
