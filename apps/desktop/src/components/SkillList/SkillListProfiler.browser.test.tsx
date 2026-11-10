import { Profiler } from "react";
import { commands } from "vitest/browser";
import { expect, it } from "vitest";
import { cleanup, render } from "vitest-browser-react";
import { skill, deployment } from "../../dev/harness/skill-fixture";
import { SkillListTable } from "./SkillListTable";
import "../../App.css";

it.each([378, 5000])(
  "skills list with %s rows reports median and p95 mount time without a clock assertion",
  async (count) => {
    const skills = Array.from({ length: count }, (_, i) =>
      skill({
        name: `skill-${i}`,
        source_kind: "manual",
        deployments: [
          deployment({
            agent: "codex",
            scope: "global",
            path: `/fixture/.codex/skills/skill-${i}`,
          }),
        ],
      }),
    );
    const samples: number[] = [];
    for (let i = 0; i < 14; i++) {
      await cleanup();
      await render(
        <div style={{ height: 600, overflow: "auto" }}>
          <Profiler
            id="skills"
            onRender={(_id, phase, duration) => {
              if (phase === "mount" && i >= 2) samples.push(duration);
            }}
          >
            <SkillListTable
              skills={skills}
              stats={[]}
              sort="name"
              active={false}
              onSelectSkill={() => {}}
            />
          </Profiler>
        </div>,
      );
    }
    expect(samples).toHaveLength(12);
    const sorted = [...samples].sort((a, b) => a - b);
    await commands.writeFile(
      `.vitest/skills-list-profiler-${count}.json`,
      JSON.stringify({
        rows: count,
        n: 12,
        median_ms: (sorted[5] + sorted[6]) / 2,
        p95_ms: sorted[11],
        samples,
      }),
    );
    await cleanup();
  },
);
