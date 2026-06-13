// Rhanis boot smoke (rhanis-ef8 Step A). The app is wrapped in OnboardingGate
// (src/features/settings/OnboardingGate.tsx): on a FRESH profile it renders
// "読み込み中…" then the wizard heading "Rhanis へようこそ" once settings load.
// A fresh CI runner has no stored settings, so onboarding is incomplete and the
// welcome heading is the deterministic "the app booted" signal — reached
// WITHOUT opening the mic or calling any real API (audio bridge idle until a
// session starts). This is the minimal Step A gate: "boots on Windows".
import { $, expect } from "@wdio/globals";

describe("Rhanis boot smoke", () => {
  it("launches on Windows and shows the onboarding welcome screen", async () => {
    // h1.rhanis-onboarding-heading is rendered by the onboarding wizard branch.
    const heading = await $("h1.rhanis-onboarding-heading");
    await heading.waitForExist({ timeout: 60000 });
    await expect(heading).toHaveText("Rhanis へようこそ");
  });
});
