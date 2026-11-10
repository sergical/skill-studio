import type { RunRecord } from "@skill-studio/lib";

export const recordedRun: RunRecord = {
  schema_version: 1,
  id: "44bff486-bb18-469f-a65d-030104eac370",
  suite: {
    schema_version: 1,
    id: "report-clients",
    name: "Report writer across installed clients",
    skill_path: "/fixtures/report-writer/SKILL.md",
    cases: [
      {
        id: "report",
        name: "Write a project report",
        prompt:
          "Read input.txt and write report.md with a short report of the given facts. Do not use the internet or change any other files.",
        mode: "explicit",
        files: [
          {
            path: "input.txt",
            content: "Project Cedar shipped two fixes on Monday. One issue remains.",
          },
        ],
        checks: [
          {
            id: "exists",
            name: "Report exists",
            assertion: {
              kind: "file_exists",
              path: "report.md",
            },
          },
          {
            id: "facts",
            name: "Includes the shipped fixes",
            assertion: {
              kind: "file_contains",
              path: "report.md",
              text: "two fixes",
            },
          },
          {
            id: "workflow",
            name: "Uses the source verification line",
            assertion: {
              kind: "file_contains",
              path: "report.md",
              text: "Verified from the source facts.",
            },
          },
        ],
      },
    ],
    variants: [
      {
        id: "codex",
        name: "Codex",
        client: "codex",
        model: null,
        skill: {
          kind: "candidate",
        },
      },
      {
        id: "claude-code",
        name: "Claude Code",
        client: "claude-code",
        model: null,
        skill: {
          kind: "candidate",
        },
      },
      {
        id: "open-code",
        name: "OpenCode",
        client: "open-code",
        model: null,
        skill: {
          kind: "candidate",
        },
      },
      {
        id: "pi",
        name: "pi",
        client: "pi",
        model: null,
        skill: {
          kind: "candidate",
        },
      },
    ],
    repetitions: 1,
    timeout_seconds: 120,
    judge: null,
  },
  input_hash: "c3a5fdefca93d106f3add2a39da2c3af2f04973af90806b09c4d9fb636c7feea",
  created_at: "2026-11-10T03:12:46.148927Z",
  finished_at: "2026-11-10T03:13:47.142960Z",
  state: "completed",
  trials: [
    {
      id: "trial-1",
      case_id: "report",
      variant_id: "codex",
      repetition: 1,
      state: "finished",
      started_at: "2026-11-10T03:12:46.186813Z",
      finished_at: "2026-11-10T03:13:02.132150Z",
      client_version: "codex-cli 0.167.0",
      client_binary: "codex",
      isolation:
        "Fresh session and workspace; user config is ignored. Existing login is used. Other user or bundled skills may remain available. Compare as an environment test.",
      skill_hash: "11b9326929a9378ab7400e26441f2713d35132ccde1f8e9377b862bb3282375b",
      result: {
        state: "completed",
        exit_code: 0,
        duration_ms: 15739,
        output:
          "Created [report.md](report.md) with the given facts and the required closing line. No other files were changed.",
        error: null,
        skill_loaded: "yes",
        usage: {
          input_tokens: 86551,
          cached_input_tokens: 68992,
          output_tokens: 201,
          cost_usd: null,
          source: "codex.turn.completed",
          reported_model: null,
        },
        events: [
          {
            kind: "assistant_text",
            text: "I will read the report-writer skill and input.txt, then write report.md from the given facts.",
            is_delta: false,
          },
          {
            kind: "tool_call",
            name: "command_execution",
            summary: "/bin/zsh -lc 'cat .agents/skills/report-writer/SKILL.md; cat input.txt'",
            detail:
              "---\nname: report-writer\ndescription: Write a short project report from facts in input.txt.\n---\nRead input.txt and write report.md. Start with a Report heading. Include only facts from the input. End the report with this exact line:\n\nVerified from the source facts.\nProject Cedar shipped two fixes on Monday. One issue remains.",
          },
          {
            kind: "tool_call",
            name: "command_execution",
            summary: "/bin/zsh -lc 'cat /home/test-user/.agents/skills/pstack-unslop/SKILL.md'",
            detail:
              '---\nname: pstack-unslop\ndescription: Cut AI tells from any writing. Must always apply.\ndisable-model-invocation: true\nmetadata:\n  opencode/autoinvoke: false\n  opencode/slash: true\n---\n\n# Unslop\n\nEdit text to remove AI patterns and add human voice.\n\n## Process\n\n1. Scan for the patterns below.\n2. Rewrite. Preserve meaning, match intended tone.\n3. Add soul (see next section).\n4. Self-audit: "What makes this obviously AI generated?" Fix remaining tells.\n\n## Adding soul\n\nRemoving patterns is half the job. Sterile, voiceless writing is just as obvious.\n\n- **Have opinions.** React to facts instead of neutrally listing pros and cons.\n- **Vary rhythm.** Short sentences. Then longer ones that take their time. Mix it up.\n- **Acknowledge complexity.** "Impressive but also kind of unsettling" beats "impressive."\n- **Use "I" when it fits.** First person isn\'t unprofessional.\n- **Let some mess in.** Perfect structure looks machine-made.\n- **Be specific.** Not "this is concerning" but "there\'s something unsettling about agents churning away at 3am."\n\n## Patterns to detect and fix\n\n### Content\n\n1. **Puffery.** "pivotal moment", "testament to", "evolving landscape", "setting the stage for", "indelible mark", "deeply rooted". Cut puffery, state what happened.\n2. **Name-dropping.** Listing media outlets without context. Pick one, say what was said.\n3. **Superficial -ing phrases.** "highlighting...", "ensuring...", "reflecting...", "showcasing...", "fostering...". Delete or expand with real sources.\n4. **Promotional language.** "nestled", "vibrant", "breathtaking", "groundbreaking", "renowned", "stunning", "must-visit". Use neutral descriptions.\n5. **Vague attributions.** "Experts believe", "Industry reports suggest", "Some critics argue". Name the source or delete.\n6. **Formulaic challenges.** "Despite challenges... continues to thrive." Replace with specific facts.\n\n### Language\n\n7. **AI vocabulary.** Additionally, crucial, delve, enduring, enhance, fostering, garner, interplay, intricate, landscape (abstract), pivotal, showcase, tapestry (abstract), testament, underscore, vibrant. Replace with plain words.\n8. **Fancy ways to say "is".** "serves as", "stands as", "boasts", "features". Just say "is" or "has".\n9. **"Not just X, but Y."** State the point directly instead.\n10. **Rule of three.** Forcing ideas into groups of three. Use the natural number.\n11. **Synonym cycling.** Protagonist, main character, central figure, hero all in one paragraph. Pick one, repeat it.\n12. **False ranges.** "from X to Y" where X and Y aren\'t on a meaningful scale. List topics directly.\n\n### Style\n\n13. **Em dash overuse.** Avoid em dashes entirely. Use periods or commas only (no parentheses, no en dashes, no hyphen-as-dash substitutes). Em dashes are an AI tell, and reaching for parentheses instead just trades one tell for another. If a thought needs separation, end the sentence or use a comma.\n14. **Colon overuse.** Colons are fine before a list or example. Not as mid-sentence connectors. "If you\'re coming from traditional automation: instead of registering event handlers, you describe conditions" adds nothing with the colon. Rewrite to let the point stand on its own without comparison framing. "Describing when the scheduler should fire works best as plain English." Same meaning, no crutch punctuation.\n15. **Boldface overuse.** Don\'t bold every proper noun or acronym.\n16. **Inline-header lists.** The tell is a bold label and colon that restates the line: "**Performance:** Performance improved...". Convert those to prose. A bold lead-in that ends in a period, names the item, and is followed by genuinely new detail ("**Schema in TypeScript.** Tables live in one file.") is fine, not a tell.\n17. **Title case headings.** Use sentence case.\n18. **Decorative emojis.** Remove from headings and bullets.\n19. **Curly quotes.** Replace with straight quotes.\n\n### Communication artifacts\n\n20. **Chatbot phrases.** "I hope this helps!", "Let me know if...", "Of course!", "Certainly!", "Found the smoking gun!" Remove.\n21. **Cutoff disclaimers.** "While specific details are limited..." Find sources or remove.\n22. **Sycophantic tone.** "Great question! You\'re absolutely right!" Respond directly.\n\n### Filler\n\n23. **Filler phrases.** "In order to" becomes "To". "Due to the fact that" becomes "Because". "It is important to note that" gets deleted.\n24. **Excessive hedging.** "could potentially possibly be argued that it might" becomes "may".\n25. **Generic conclusions.** "The future looks bright." State specific plans or facts.\n\n### Jargon\n\n26. **Abstract metaphor nouns.** Substrate, wedge, vector, locus, vantage, nexus, primitive (as noun), harness (as metaphor), surface (as in "API surface"), bedrock, scaffolding (as metaphor), modality, paradigm, gold-plating, ratchet (as metaphor), evacuate (for moving code), endgame, north star, flywheel. These read as technical but usually have a plainer concrete word. "Substrate" becomes "base". "Wedge in" becomes "add". "Vector" becomes "way" or "method". "Gold-plating" becomes "more than the job needs". "Ratchet" becomes the mechanism\'s real name or "a limit that only tightens". "Evacuate" becomes "move out". "Endgame" becomes "the last phase". Pick the concrete word.\n\n### Plain speech\n\n27. **Say what it does, not how it feels.** "the database stays close at hand", "SQL you can read", "types that follow your schema" name a feeling. The fix names the mechanism or a number: "`.toSQL()` returns the exact string sent to the database", "a column rename fails the build". Ask what the sentence tells the reader to do or know, then write that. If you can\'t restate it as a concrete instruction, fact, or number, cut it. One more check: if the sentence could appear unchanged in another project\'s docs, it says nothing about this one. Cut it.\n28. **Shorten or split dense sentences.** If the reader has to backtrack to parse a sentence, break it in two or drop clauses. One idea per sentence.\n29. **Active voice.** Prefer it. Catch "is/are/was/were + past participle" and name the actor: "queries are validated" becomes "the compiler validates queries", "the file is parsed by the loader" becomes "the loader parses the file". Passive is fine only when the actor is unknown or genuinely doesn\'t matter.\n30. **Cut adverbs, or use a stronger verb.** "runs quickly" becomes "is fast" or the number. "significantly improves" becomes the measured delta. An adverb propping up a weak verb means the verb is wrong.\n31. **Prefer the plain word.** "utilize" becomes "use", "leverage" becomes "use", "facilitate" becomes "help", "numerous" becomes "many", "in the event that" becomes "if". The fancier synonym is rarely clearer.\n',
          },
          {
            kind: "tool_call",
            name: "command_execution",
            summary:
              "/bin/zsh -lc \"printf '# Report\\\\n\\\\nProject Cedar shipped two fixes on Monday. One issue remains.\\\\n\\\\nVerified from the source facts.\\\\n' > report.md\ncat report.md\"",
            detail:
              "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
          },
          {
            kind: "assistant_text",
            text: "Created [report.md](report.md) with the given facts and the required closing line. No other files were changed.",
            is_delta: false,
          },
        ],
      },
      judge: null,
      checks: [
        {
          check_id: "exists",
          name: "Report exists",
          status: "passed",
          reason: "Expected file report.md",
          evidence: "report.md",
        },
        {
          check_id: "facts",
          name: "Includes the shipped fixes",
          status: "passed",
          reason: 'Expected report.md to contain "two fixes"',
          evidence:
            "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
        },
        {
          check_id: "workflow",
          name: "Uses the source verification line",
          status: "passed",
          reason: 'Expected report.md to contain "Verified from the source facts."',
          evidence:
            "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
        },
      ],
      outcome: "passed",
      artifacts: [
        {
          path: "input.txt",
          sha256: "afd17ef80494a5d73e7ae2830306070dd5ce7d3f831e7f59b9d8806fdd6ebad9",
          bytes: 61,
          text: "Project Cedar shipped two fixes on Monday. One issue remains.",
          change: "unchanged",
        },
        {
          path: "report.md",
          sha256: "809229bbe7c50d7657343209f09a5defe32e299fabc0cde3909a0b747d0f5012",
          bytes: 105,
          text: "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
          change: "created",
        },
      ],
      reviews: [],
      error: null,
    },
    {
      id: "trial-2",
      case_id: "report",
      variant_id: "claude-code",
      repetition: 1,
      state: "finished",
      started_at: "2026-11-10T03:13:02.152974Z",
      finished_at: "2026-11-10T03:13:15.926538Z",
      client_version: "2.1.319 (Claude Code)",
      client_binary: "claude",
      isolation:
        "Fresh session and workspace; project settings and an empty MCP list are used. Existing login is used. User skills may remain available. Compare as an environment test.",
      skill_hash: "11b9326929a9378ab7400e26441f2713d35132ccde1f8e9377b862bb3282375b",
      result: {
        state: "completed",
        exit_code: 0,
        duration_ms: 13586,
        output:
          'I wrote `report.md` following the report-writer skill. It has a "Report" heading and two facts taken straight from `input.txt`:\n\n- Project Cedar shipped two fixes on Monday.\n- One issue remains.\n\nIt ends with the line the skill requires, "Verified from the source facts." That line is fixed wording from the skill, not a separate check I ran; the report just repeats the two facts as written. I didn\'t change any other files or use the internet.',
        error: null,
        skill_loaded: "unknown",
        usage: {
          input_tokens: 6,
          cached_input_tokens: null,
          output_tokens: 427,
          cost_usd: 0.0619576,
          source: "claude.result",
          reported_model: "claude-opus-5-5",
        },
        events: [
          {
            kind: "tool_call",
            name: "Bash",
            summary:
              '{"command":"cat .claude/skills/report-writer/SKILL.md; echo ----; cat input.txt; ls -la","description":"Read skill and input file"}',
            detail:
              '{"command":"cat .claude/skills/report-writer/SKILL.md; echo ----; cat input.txt; ls -la","description":"Read skill and input file"}',
          },
          {
            kind: "tool_result",
            name: "tool_result",
            summary:
              "---\nname: report-writer\ndescription: Write a short project report from facts in input.txt.\n---\nRead input.txt and write report.md. Start with a Report heading. Include only facts from the input. End the report with this exact line:\n\nVerified from the source facts.\n----\nProject Cedar shipped two fixes on Monday. One issue remains.total 8\ndrwxr-xr-x@ 4 sergiydybskiy  wheel  128 Nov  9 22:13 .\ndrwxr-xr-x@ 5 sergiydybskiy  wheel  160 Nov  9 22:13 ..\ndrwxr-xr-x@ 3 sergiydybskiy  wheel   96 Nov  9 22:13 .claude\n-rw-r--r--@ 1 sergiydybskiy  wheel   61 Nov  9 22:13 input.txt",
          },
          {
            kind: "tool_call",
            name: "Write",
            summary:
              '{"file_path":"/private/test-workspace","content":"# Report\\n\\n- Project Cedar shipped two fixes on Monday.\\n- One issue remains.\\n\\nVerified from the source facts.\\n"}',
            detail:
              '{"file_path":"/private/test-workspace","content":"# Report\\n\\n- Project Cedar shipped two fixes on Monday.\\n- One issue remains.\\n\\nVerified from the source facts.\\n"}',
          },
          {
            kind: "tool_result",
            name: "tool_result",
            summary:
              "File created successfully at: /private/test-workspace (file state is current in your context — no need to Read it back)",
          },
          {
            kind: "assistant_text",
            text: 'I wrote `report.md` following the report-writer skill. It has a "Report" heading and two facts taken straight from `input.txt`:\n\n- Project Cedar shipped two fixes on Monday.\n- One issue remains.\n\nIt ends with the line the skill requires, "Verified from the source facts." That line is fixed wording from the skill, not a separate check I ran; the report just repeats the two facts as written. I didn\'t change any other files or use the internet.',
            is_delta: false,
          },
          {
            kind: "finished",
            ok: true,
            cancelled: false,
            final_text:
              'I wrote `report.md` following the report-writer skill. It has a "Report" heading and two facts taken straight from `input.txt`:\n\n- Project Cedar shipped two fixes on Monday.\n- One issue remains.\n\nIt ends with the line the skill requires, "Verified from the source facts." That line is fixed wording from the skill, not a separate check I ran; the report just repeats the two facts as written. I didn\'t change any other files or use the internet.',
            session_id: "4930d201-cf13-4d82-9167-3a78b51ea7ee",
            cost_usd: 0.0619576,
            duration_ms: 12594,
            skill_loaded: "unknown",
          },
        ],
      },
      judge: null,
      checks: [
        {
          check_id: "exists",
          name: "Report exists",
          status: "passed",
          reason: "Expected file report.md",
          evidence: "report.md",
        },
        {
          check_id: "facts",
          name: "Includes the shipped fixes",
          status: "passed",
          reason: 'Expected report.md to contain "two fixes"',
          evidence:
            "# Report\n\n- Project Cedar shipped two fixes on Monday.\n- One issue remains.\n\nVerified from the source facts.\n",
        },
        {
          check_id: "workflow",
          name: "Uses the source verification line",
          status: "passed",
          reason: 'Expected report.md to contain "Verified from the source facts."',
          evidence:
            "# Report\n\n- Project Cedar shipped two fixes on Monday.\n- One issue remains.\n\nVerified from the source facts.\n",
        },
      ],
      outcome: "passed",
      artifacts: [
        {
          path: "input.txt",
          sha256: "afd17ef80494a5d73e7ae2830306070dd5ce7d3f831e7f59b9d8806fdd6ebad9",
          bytes: 61,
          text: "Project Cedar shipped two fixes on Monday. One issue remains.",
          change: "unchanged",
        },
        {
          path: "report.md",
          sha256: "36822e9a88ab454233c6153a54b76a8fdf08ca961af9a0283e496b0582d241d9",
          bytes: 109,
          text: "# Report\n\n- Project Cedar shipped two fixes on Monday.\n- One issue remains.\n\nVerified from the source facts.\n",
          change: "created",
        },
      ],
      reviews: [],
      error: null,
    },
    {
      id: "trial-3",
      case_id: "report",
      variant_id: "open-code",
      repetition: 1,
      state: "finished",
      started_at: "2026-11-10T03:13:15.959906Z",
      finished_at: "2026-11-10T03:13:32.488557Z",
      client_version: "opencode v2.2.6",
      client_binary: "opencode2",
      isolation:
        "Fresh standalone session and workspace. Existing login and user configuration are used. Other skills may remain available. Compare as an environment test.",
      skill_hash: "11b9326929a9378ab7400e26441f2713d35132ccde1f8e9377b862bb3282375b",
      result: {
        state: "completed",
        exit_code: 0,
        duration_ms: 16012,
        output: "Created `report.md` from the facts in `input.txt`.",
        error: null,
        skill_loaded: "unknown",
        usage: {
          input_tokens: null,
          cached_input_tokens: null,
          output_tokens: null,
          cost_usd: 0.0,
          source: null,
          reported_model: null,
        },
        events: [
          {
            kind: "tool_call",
            name: "read",
            summary: "read",
            detail: null,
          },
          {
            kind: "tool_result",
            name: "read",
            summary:
              "Read file /private/test-workspace lines 1-7\n1: ---\n2: name: report-writer\n3: description: Write a short project report from facts in input.txt.\n4: ---\n5: Read input.txt and write report.md. Start with a Report heading. Include only facts from the input. End the report with this exact line:\n6: \n7: Verified from the source facts.",
          },
          {
            kind: "tool_call",
            name: "read",
            summary: "read",
            detail: null,
          },
          {
            kind: "tool_result",
            name: "read",
            summary:
              "Read file /private/test-workspace lines 1-1\n1: Project Cedar shipped two fixes on Monday. One issue remains.",
          },
          {
            kind: "tool_call",
            name: "patch",
            summary: "patch",
            detail: null,
          },
          {
            kind: "tool_result",
            name: "patch",
            summary: "Success. Updated the following files:\nA report.md",
          },
          {
            kind: "assistant_text",
            text: "Created `report.md` from the facts in `input.txt`.",
            is_delta: false,
          },
        ],
      },
      judge: null,
      checks: [
        {
          check_id: "exists",
          name: "Report exists",
          status: "passed",
          reason: "Expected file report.md",
          evidence: "report.md",
        },
        {
          check_id: "facts",
          name: "Includes the shipped fixes",
          status: "passed",
          reason: 'Expected report.md to contain "two fixes"',
          evidence:
            "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
        },
        {
          check_id: "workflow",
          name: "Uses the source verification line",
          status: "passed",
          reason: 'Expected report.md to contain "Verified from the source facts."',
          evidence:
            "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
        },
      ],
      outcome: "passed",
      artifacts: [
        {
          path: "input.txt",
          sha256: "afd17ef80494a5d73e7ae2830306070dd5ce7d3f831e7f59b9d8806fdd6ebad9",
          bytes: 61,
          text: "Project Cedar shipped two fixes on Monday. One issue remains.",
          change: "unchanged",
        },
        {
          path: "report.md",
          sha256: "809229bbe7c50d7657343209f09a5defe32e299fabc0cde3909a0b747d0f5012",
          bytes: 105,
          text: "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
          change: "created",
        },
      ],
      reviews: [],
      error: null,
    },
    {
      id: "trial-4",
      case_id: "report",
      variant_id: "pi",
      repetition: 1,
      state: "finished",
      started_at: "2026-11-10T03:13:32.518852Z",
      finished_at: "2026-11-10T03:13:47.083403Z",
      client_version: "1.5.0",
      client_binary: "pi",
      isolation:
        "Native skill discovery, context files, extensions and MCP are disabled. Only the selected snapshot is added. This is a local client process, not an operating-system sandbox.",
      skill_hash: "11b9326929a9378ab7400e26441f2713d35132ccde1f8e9377b862bb3282375b",
      result: {
        state: "completed",
        exit_code: 0,
        duration_ms: 13751,
        output:
          "Created `report.md` with the given facts, a Report heading, and the required closing line. No other files were changed.",
        error: null,
        skill_loaded: "yes",
        usage: {
          input_tokens: 3538,
          cached_input_tokens: 2816,
          output_tokens: 179,
          cost_usd: 0.0091476,
          source: "pi.message_end (client cost estimate)",
          reported_model: "gpt-6.1-sol",
        },
        events: [
          {
            kind: "tool_call",
            name: "read",
            summary: '{"path":"/test-workspace","offset":null,"limit":null}',
            detail: null,
          },
          {
            kind: "tool_result",
            name: "read",
            summary:
              '{"content":[{"type":"text","text":"---\\nname: report-writer\\ndescription: Write a short project report from facts in input.txt.\\n---\\nRead input.txt and write report.md. Start with a Report heading. Include only facts from the input. End the report with this exact line:\\n\\nVerified from the source facts.\\n"}],"structuredContent":"---\\nname: report-writer\\ndescription: Write a short project report from facts in input.txt.\\n---\\nRead input.txt and write report.md. Start with a Report heading. Include only facts from the input. End the report with this exact line:\\n\\nVerified from the source facts.\\n"}',
          },
          {
            kind: "tool_call",
            name: "read",
            summary: '{"path":"input.txt","offset":null,"limit":null}',
            detail: null,
          },
          {
            kind: "tool_result",
            name: "read",
            summary:
              '{"content":[{"type":"text","text":"Project Cedar shipped two fixes on Monday. One issue remains."}],"structuredContent":"Project Cedar shipped two fixes on Monday. One issue remains."}',
          },
          {
            kind: "tool_call",
            name: "write",
            summary:
              '{"path":"report.md","content":"# Report\\n\\nProject Cedar shipped two fixes on Monday. One issue remains.\\n\\nVerified from the source facts.\\n"}',
            detail: null,
          },
          {
            kind: "tool_result",
            name: "write",
            summary: '{"content":[{"type":"text","text":"Successfully wrote to report.md"}]}',
          },
          {
            kind: "assistant_text",
            text: "Created",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " `",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: "report",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: ".md",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: "`",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " with",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " the",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " given",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " facts",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: ",",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " a",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " Report",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " heading",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: ",",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " and",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " the",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " required",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " closing",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " line",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: ".",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " No",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " other",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " files",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " were",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: " changed",
            is_delta: true,
          },
          {
            kind: "assistant_text",
            text: ".",
            is_delta: true,
          },
        ],
      },
      judge: null,
      checks: [
        {
          check_id: "exists",
          name: "Report exists",
          status: "passed",
          reason: "Expected file report.md",
          evidence: "report.md",
        },
        {
          check_id: "facts",
          name: "Includes the shipped fixes",
          status: "passed",
          reason: 'Expected report.md to contain "two fixes"',
          evidence:
            "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
        },
        {
          check_id: "workflow",
          name: "Uses the source verification line",
          status: "passed",
          reason: 'Expected report.md to contain "Verified from the source facts."',
          evidence:
            "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
        },
      ],
      outcome: "passed",
      artifacts: [
        {
          path: "input.txt",
          sha256: "afd17ef80494a5d73e7ae2830306070dd5ce7d3f831e7f59b9d8806fdd6ebad9",
          bytes: 61,
          text: "Project Cedar shipped two fixes on Monday. One issue remains.",
          change: "unchanged",
        },
        {
          path: "report.md",
          sha256: "809229bbe7c50d7657343209f09a5defe32e299fabc0cde3909a0b747d0f5012",
          bytes: 105,
          text: "# Report\n\nProject Cedar shipped two fixes on Monday. One issue remains.\n\nVerified from the source facts.\n",
          change: "created",
        },
      ],
      reviews: [],
      error: null,
    },
  ],
  error: null,
};
