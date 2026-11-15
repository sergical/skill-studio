You are in a fresh session on a user's computer. Answer two questions about the user's agent skills.

The user's home folder is `{home}`. Below, `~` means that folder.

Rules: read-only. Change no files and no settings. Do not park, remove, fix, install, update, split or set anything. Today is {today}.

The user installed the skill pack `acme-labs/review-skills` from GitHub with `npx skills`. Yesterday the pack's author shipped a new version, and the user updated the pack. The update changed the copies in the shared skill library `~/.agents/skills`. Before the update, the user had made copies of some skills in the folders of single agents.

U1. How many skills from the `acme-labs/review-skills` pack are in the shared library now?
U2. Which copies of the pack's skills, outside the shared library, do not have the new version? For each, give the skill and the agent whose folder holds the copy: claude-code, codex, opencode, pi or cursor. A link to the library copy has the new version.

Work as fast as you can while still being correct. Then end your answer with one fenced json block in exactly this shape:
{"u1": 0, "u2": [{"skill": "", "agent": ""}],
 "confidence": {"u1": 1, "u2": 1},
 "method": "one sentence",
 "friction": ["each thing that slowed you down or confused you"],
 "missing": ["each tool feature that would have made this easier"]}
Confidence is 1 (a guess) to 5 (certain).
