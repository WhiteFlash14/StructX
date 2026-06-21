import {
  LandingHeader,
  LandingFooter,
  LandingStyles,
} from "@/app/_landing-shared";
import { LandingExperience } from "@/app/_landing-experience";

export const metadata = {
  title: "StructX",
  description:
    "Choose how you think BTC will move, review the possible outcomes, and open the strategy from your Sui wallet.",
};

export default function LandingPage() {
  return (
    <main className="landing">
      <LandingHeader showWallet={false} showLaunchApp />
      <LandingExperience />
      <LandingFooter />
      <LandingStyles />
    </main>
  );
}