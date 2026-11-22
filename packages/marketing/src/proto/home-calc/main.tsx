// PROTO(home-calc): entry for packages/marketing/proto/home-calc/index.html. Dev server only.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { applySiteTheme, currentSiteTheme } from "../../site-theme";
import { Harness } from "./Harness";
import "../../marketing-site.css";
import "./proto.css";
import "./picker.css";

applySiteTheme(currentSiteTheme());

const root = document.getElementById("root");
if (!root) throw new Error("Prototype root element is missing");

createRoot(root).render(
  <StrictMode>
    <Harness />
  </StrictMode>,
);
