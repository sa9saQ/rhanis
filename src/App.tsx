// Base-layer styles (theme tokens + shared primitives like `.rhanis-btn`) must load
// BEFORE any feature stylesheet, so feature VARIANTS (.rhanis-btn-approve /
// .rhanis-btn-primary …) win the cascade over the base at equal specificity. Hence
// this import precedes the feature-component imports below (rhanis-iyr).
import "./App.css";

import { ConsoleLayout } from "./features/console/ConsoleLayout";
import { OnboardingGate } from "./features/settings/OnboardingGate";

function App() {
  return (
    <OnboardingGate>
      {/* The glass-box console shell (rhanis-ios.1): sidebar + greeting +
          live activity panel + voice orb. Owns the backend event wiring. */}
      <ConsoleLayout />
    </OnboardingGate>
  );
}

export default App;
