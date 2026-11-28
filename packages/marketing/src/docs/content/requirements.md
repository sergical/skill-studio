# Requirements and install

You'll be able to open Skill Studio or verify that the CLI or MCP server is ready.

## Desktop

Use macOS 13 or later. Check the chip or processor in **Apple menu → About This Mac**. Download the [Apple silicon installer](/download) for an Apple chip or the [Intel installer](/download/intel) for an Intel processor. Open the disk image, move Skill Studio to Applications, then open it. The welcome screen or **Home** confirms it launched. A local scan can run with just the app and readable skill folders.

To remove the app, move Skill Studio from Applications to the Trash. This leaves your skills in their agent folders.

## CLI and MCP

Use an Apple silicon or Intel Mac with Node.js 18 or later. The first `npx` run needs npm access and the optional binary package for your chip. A native CLI binary can run without Node. Agent setup needs a client that can start a stdio server.

```sh
npx -y skill-studio@latest --version
```

The command prints `skill-studio` and the version number. It printed `skill-studio 0.4.1` when this guide was tested. There is no global install to reverse when you use `npx`. For MCP, follow [Set up with an agent](/docs/agent-setup/) and verify the first tool call.

## Tools for each job

| Job                                                                 | What it needs                                                                                                                                                  |
| ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scan, inspect usage or estimate listing budgets                     | Read access to the skill folders, project folders and any local agent history you want included.                                                               |
| Connect MCP from the app                                            | Working `npx` and your client's setup command, or Cursor's install link.                                                                                       |
| Install or update via skills.sh or dotagents                        | Node and working `npx`, npm access or a warm cache, and access to the source. Git sources need Git and any required credentials.                               |
| List a Git pack's skills, check upstream or preview a source switch | Git, source access and a writable temporary folder.                                                                                                            |
| Check which skills have updates                                     | Signed-in GitHub CLI (`gh`) with access to the source repositories.                                                                                            |
| Fork, pull upstream or copy from GitHub in the app                  | Signed-in `gh`, repository access and `tar`. Returning a fork to its installer also needs that installer.                                                      |
| Save, park, remove or restore files                                 | Write access to the relevant skill and data folders. Removing a managed install needs its installer. Parking or unparking a dotagents install needs dotagents. |
| Browse the Skill Store                                              | Network access. No personal sign-in is required.                                                                                                               |

The app finds many tools through your login shell or common install paths. For `gh` it checks the login shell separately. The CLI and MCP use the environment they started with. If an npm binary is missing or a tool works only in your terminal, see [Troubleshooting](/docs/troubleshooting/).
