# See every skill

You'll have a list of installed skills with their scope, source, agents and copies.

## Desktop

Open **Skills**. Pick **Global** or a project to narrow the list. Select a skill to see **Location**, **Agents** and **Source**. Look at its locations to find a shared folder, links to that folder and independent copies. A link points to the same files. Independent copies can have different content.

## CLI

Run with Node.js 18 or later. You need read access to your skills and any projects you want to include.

```sh
npx -y skill-studio@latest library --all
npx -y skill-studio@latest scan
```

The library includes skills outside the shared folder with `--all`. An example row summary from `scan` is `example-review  [manual]  (3 copies)`. Expand the corresponding lines in your output to see each path and ID. For a project that discovery missed, pass its folder with `--project`.

## MCP

Call `library` with `{"all":true}` to include skills outside the shared library. Call `scan` for paths and deployment IDs, or `diagnose_conflict` to find copies with different content. The ID identifies one installed location and is useful when a name matches more than one. The example MCP library call returned `data.count: 3` with three entries in `data.rows`.

These reads leave skills as they are. If the source is blank, check whether the skill was installed by hand or its install record is missing. If a whole project is missing, see [Troubleshooting](/docs/troubleshooting/).
