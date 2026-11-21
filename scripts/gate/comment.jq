# The gate-risk PR comment for a stop PR. Input: `gate risk --json`.
# File names come from the PR. Escape Markdown punctuation and show control
# characters and line separators as question marks. The workflow supplies the other text.
def escape_filename:
  gsub("(?<c>[!-/:-@\\[-`{-~])"; "\\\(.c)")
  | gsub("[\\p{Cc}\\p{Cf}\\p{Zl}\\p{Zp}]"; "?");
def names: length as $n | .[:10] | map(escape_filename) | join(", ") + (if $n > 10 then " and \($n - 10) more" else "" end);
$marker + "\n## Needed from you and why\n\n"
+ ([.needs_you[]
    | "**Decision:** \(.ask)\n**Why you:** \(.why)"
      + (if .rule == "manual" then ""
         else "\n**Change:** \(.files | names), matched by rule `\(.rule)` in `.gate/config.toml`." end)]
   | join("\n\n"))
+ "\n\nTo say yes, merge this PR. To say no, tell the agent. If a rule asks about changes that do not need you, change it in `.gate/config.toml`. That change also needs you."
