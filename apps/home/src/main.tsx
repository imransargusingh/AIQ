import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";
import { applyTheme, readTheme } from "./theme";

// The saved theme shows before the first paint, so a dark choice never flashes light.
const theme = readTheme();
if (theme !== "system") applyTheme(theme);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
