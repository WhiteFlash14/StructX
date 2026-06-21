import {
  LandingFooter,
  LandingHeader,
  LandingStyles,
} from "@/app/_landing-shared";
import { StrategiesView } from "@/app/strategies/StrategiesView";

export const metadata = {
  title: "StructX",
  description: "Defined-risk BTC payoff strategies built on DeepBook Predict.",
};

export default function StrategiesPage() {
  return (
    <main className="landing">
      <LandingHeader />
      <StrategiesView />
      <LandingFooter />
      <LandingStyles />
    </main>
  );
}
