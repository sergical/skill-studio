// ============================================================================
// Skill Studio - skill-stats tests
// ============================================================================

import { describe, expect, it } from "vitest";
import {
  HISTORY_DAYS,
  heatmapDateRangeLocal,
  mondayLead,
  calendarWeeks,
  shortSha,
} from "./skill-stats";

describe("shortSha", () => {
  it("truncates a full sha to 7 characters", () => {
    expect(shortSha("1111111111111111111111111111111111aaaa")).toBe("1111111");
  });

  it("leaves a sha already at or under 7 characters alone", () => {
    expect(shortSha("abc123")).toBe("abc123");
  });
});

describe("mondayLead", () => {
  it("counts the blank slots before the first day in a Monday-first week", () => {
    expect(mondayLead("2026-09-14")).toBe(0);
    expect(mondayLead("2026-09-16")).toBe(2);
    expect(mondayLead("2026-09-13")).toBe(6);
  });
});

describe("heatmapDateRangeLocal", () => {
  it("covers exactly the 30-day history, ending today", () => {
    const { dates, start, end } = heatmapDateRangeLocal(new Date(2026, 8, 16));
    expect(dates).toHaveLength(HISTORY_DAYS);
    expect(start).toBe("2026-08-18");
    expect(end).toBe("2026-09-16");
    // Tuesday 2026-08-18 to Wednesday 2026-09-16: one blank Monday, then 30 days.
    expect(calendarWeeks(dates)).toBe(5);
  });
});
