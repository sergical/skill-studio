# What belongs on Skill Studio’s Home

Someone who has installed a skill still has two questions. Did an agent use it during recent work? Did that agent receive enough of its description to choose it? Skill Studio can answer both across agents from local records. A folder scan, a skill list estimate, a recorded skill list and a recorded use each answer a different part of the question.

| Evidence                                                                                                                                                                                                                      | What it implies for Home                                                                                                                |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| In two fresh Claude Code sessions, a user found that 27 of 36 project skills arrived by name alone. Explicit invocation still worked. [Claude Code listing report](https://github.com/anthropics/claude-code/issues/81081)    | Show what is in each agent's folders beside what it sees now and what the last session recorded. Each agent opens its What it sees tab. |
| A Codex user wanted a way to see which skills and tools ran during a task without rereading the conversation. [Codex task history request](https://github.com/openai/codex/issues/33720)                                      | Put recent skill use on Home. A day opens Activity for that date. An installed skill in the ranking opens its own page.                 |
| In Vercel’s Next.js evaluation, the agent skipped the available skill in 56% of cases. That figure describes their test suite. [Vercel’s experiment](https://vercel.com/blog/agents-md-outperforms-skills-in-our-agent-evals) | Say "uses in session history". An absence of records cannot establish that a skill failed, or that removing it would improve results.   |

The trend and the most-used skills share one date control so the counts describe the same period. Vercel’s analytics screen was a reference for this arrangement. [Analytics reference on Mobbin](https://mobbin.com/screens/f58e584d-9481-4125-961f-b08bde53529b)

## What shipped

Home now opens with Skill use and What your agents see, then keeps Library maintenance (broken, warnings, updates, who can invoke, prompt cost and the grouped rows) below, so every repair still happens on Home. The three rejected layouts (Repairs, Clients, Recent work) and the Home Lab build were removed; they are in this branch's history.

The rest of the app moved to the same words and shapes: one word per idea (agent, use, skill list), the Agents place with What it sees and What it used tabs in place of Client configuration and Sessions, one summary card, and one disclosure marker. The rules are in [apps/desktop/DESIGN.md](../../apps/desktop/DESIGN.md).

A useful usability check is to ask a skill author to find a recently used skill, then check whether another agent received its description. The reports above support the questions. Observing someone try that sequence would test the layout itself.
