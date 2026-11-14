import * as stylex from "@stylexjs/stylex";

// Checked with the dataviz palette validator against each theme's surface (light #ffffff, dark #111111).
export const chartTokens = stylex.defineVars({
  byHand: "#8f8f8f",
  cli: "#3987e5",
  mcp: "#d95926",
  grid: "oklch(1 0 0 / .08)",
});

export const lightChartTheme = stylex.createTheme(chartTokens, {
  byHand: "#8a8a8a",
  cli: "#2a78d6",
  mcp: "#eb6834",
  grid: "oklch(0 0 0 / .08)",
});
