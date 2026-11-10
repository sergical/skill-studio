# Installed-skill acceptance cases

These suites add two tasks each for three installed skills. They use Codex with explicit skill selection. They check native loading evidence and the resulting files or behavior. They do not measure whether a skill improves a model over a no-skill control.

| Skill                          | Cases                                                                                        |
| ------------------------------ | -------------------------------------------------------------------------------------------- |
| `emil-unslop-code`             | Remove dead code without changing invoice totals; propagate fetch failures.                  |
| `emil-forms-and-inputs`        | Repair signup form semantics; block duplicate saves and recover after failure.               |
| `emil-animation-accessibility` | Add a fade-only reduced-motion variant; respect a changing motion preference when scrolling. |

The JSON files use the skill name as `skill_path`. Resolve it against your installed skill root before running. From the repository root, after building `skill-studio-eval`:

```sh
python3 - <<'PY'
import json
import subprocess
import tempfile
from pathlib import Path

skill_root = Path.home() / '.agents/skills'
binary = Path('target/debug/skill-studio-eval').resolve()
output = Path(tempfile.mkdtemp(prefix='installed-skill-checks-'))
for source in sorted(Path('examples/skill-tests/installed').glob('*.json')):
    suite = json.loads(source.read_text())
    suite['skill_path'] = str(skill_root / suite['skill_path'])
    resolved = output / source.name
    resolved.write_text(json.dumps(suite))
    result = subprocess.run(
        [str(binary), '--data-dir', str(output / 'runs'), 'run', str(resolved)],
        capture_output=True, text=True,
    )
    (output / f'{source.stem}-result.json').write_text(result.stdout)
    print(source.stem, 'exit', result.returncode, result.stderr)
print('Evidence:', output)
PY
```

Set `binary` to your shared build output. Each suite makes two real model requests. Acceptance commands use Node.js and run against the generated files. The engine stores source snapshots and raw client evidence separately from the compact result export.

The HTML and CSS checks inspect source contracts. They do not replace rendered accessibility or motion checks. The JavaScript cases execute the returned modules and test success, failure, concurrency, and changing preferences.
