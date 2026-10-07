// ============================================================================
// Skill Studio - SkillMarkdown link tests
// Flow: open a skill whose SKILL.md links elsewhere and click the link.
// Expect: web links open in the browser; every other link stays inert.
// Failure: a relative, #anchor or mailto: href navigates the app window
// away (#416).
// ============================================================================

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { SkillMarkdown } from "./SkillMarkdown";

const render = (content: string) => renderToStaticMarkup(<SkillMarkdown content={content} />);

describe("SkillMarkdown links", () => {
  it("non_web_links_render_without_an_href_so_a_click_cannot_navigate_the_app_window", () => {
    const html = render("[ref](./references/api.md) [top](#usage) [mail](mailto:a@b.dev)");

    expect(html).not.toContain("href=");
    expect(html).toContain('title="./references/api.md"');
    expect(html).toContain('title="#usage"');
  });

  it("web_links_keep_their_href_and_open_in_a_new_window_the_rust_handler_routes_to_the_browser", () => {
    const html = render("[docs](https://agentskills.io)");

    expect(html).toContain('href="https://agentskills.io"');
    expect(html).toContain('target="_blank"');
  });
});
