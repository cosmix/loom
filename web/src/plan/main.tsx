import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router";

import "@fontsource-variable/literata";
import "@fontsource-variable/piazzolla";
import "@fontsource-variable/source-serif-4";
import "@/index.css";
import "@/plan/plan.css";
import { ThemeProvider } from "@/aurora-ui/theme/ThemeProvider";
import { initClient } from "@/plan/api/client";
import { connectPlanSocket } from "@/plan/api/socket";
import { router } from "@/plan/router";
import { planSocketAtom } from "@/plan/state/atoms";

const store = createStore();
initClient(store);

// Handle held for the app's lifetime: this SPA's root never unmounts, so
// there is no cleanup phase to call `dispose` from. Do not move this into an
// effect, under StrictMode that would connect twice.
store.set(planSocketAtom, connectPlanSocket(store));

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={store}>
      <ThemeProvider>
        <RouterProvider router={router} />
      </ThemeProvider>
    </Provider>
  </StrictMode>,
);
