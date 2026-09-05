import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Provider } from "jotai/react";
import { RouterProvider } from "react-router";

import "@/index.css";
import { connectStatusSocket } from "@/api/ws";
import { ThemeProvider } from "@/aurora-ui/theme/ThemeProvider";
import { router } from "@/router";
import { store } from "@/state/store";

// Handle held for the app's lifetime: this SPA's root never unmounts, so
// there is no cleanup phase to call `dispose` from. Do not move this into an
// effect — under StrictMode that would connect twice.
connectStatusSocket(store);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={store}>
      <ThemeProvider>
        <RouterProvider router={router} />
      </ThemeProvider>
    </Provider>
  </StrictMode>,
);
