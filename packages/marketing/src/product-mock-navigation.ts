// ============================================================================
// Skill Studio marketing demo - detail navigation labels
// ============================================================================

export type ProductMockRootView =
  | "home"
  | "skills"
  | "plugins"
  | "packs"
  | "listing"
  | "sessions"
  | "activity"
  | "parked";

/** Returns the root-view label that ProductMock's detail breadcrumb shows before the skill name. */
export function productMockDetailBackLabel(from: ProductMockRootView): string {
  switch (from) {
    case "home":
      return "Home";
    case "skills":
      return "Skills";
    case "plugins":
      return "Plugins";
    case "packs":
      return "Packs";
    case "listing":
      return "Model listing";
    case "sessions":
      return "Sessions";
    case "activity":
      return "Activity";
    case "parked":
      return "Parked";
  }
}
