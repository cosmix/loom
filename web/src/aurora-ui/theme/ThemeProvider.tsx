import { useEffect, type ReactNode } from "react";
import { useAtomValue } from "jotai";
import {
  darkVariantAtom,
  lightVariantAtom,
  themeAtom,
} from "../shared/atoms/theme";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const scheme = useAtomValue(themeAtom);
  const lightVariant = useAtomValue(lightVariantAtom);
  const darkVariant = useAtomValue(darkVariantAtom);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", scheme === "dark");
    root.dataset.lightTheme = lightVariant;
    root.dataset.darkTheme = darkVariant;
  }, [scheme, lightVariant, darkVariant]);

  return <>{children}</>;
}
