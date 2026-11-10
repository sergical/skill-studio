#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

fn handler<'a>(source: &'a str, signature: &str, next: &str) -> &'a str {
    source
        .split_once(signature)
        .unwrap_or_else(|| panic!("missing {signature}"))
        .1
        .split(next)
        .next()
        .unwrap()
}

#[test]
fn every_desktop_flow_names_both_headless_adapters_and_the_shared_op_or_an_explicit_gap() {
    let docs = include_str!("../../../docs/flows.md");
    let cli = include_str!("../src/main.rs");
    let mcp = include_str!("../../mcp/src/lib.rs");
    let mut flows = 0;
    for row in docs
        .split("## Timing")
        .next()
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("| "))
        .skip(2)
    {
        let columns: Vec<_> = row.split('|').map(str::trim).collect();
        flows += 1;
        assert_eq!(
            columns.len(),
            9,
            "flow {} needs CLI, MCP and core/reason columns",
            columns[1]
        );
        let [cli_entry, mcp_entry, op] = [columns[5], columns[6], columns[7]];
        assert!(
            !cli_entry.is_empty() && !mcp_entry.is_empty(),
            "missing adapter for {}",
            columns[1]
        );
        if let Some(reason) = op.strip_prefix("Gap: ") {
            assert!(
                reason.len() > 30,
                "name the missing core operation for {}",
                columns[1]
            );
            assert_eq!(cli_entry, "Unavailable");
            assert_eq!(mcp_entry, "Unavailable");
        } else {
            let home = tempfile::tempdir().unwrap();
            let command = cli_entry.split_whitespace().next().unwrap();
            let help = std::process::Command::new(env!("CARGO_BIN_EXE_skill-studio"))
                .args([command, "--help"])
                .env("HOME", home.path())
                .env("SKILL_STUDIO_TELEMETRY", "0")
                .output()
                .unwrap();
            assert!(
                help.status.success(),
                "CLI command {command} is missing for {}",
                columns[1]
            );
            for tool in mcp_entry.split('/').map(str::trim) {
                assert!(
                    mcp.contains(&format!("async fn {tool}(")),
                    "MCP tool {tool} is missing for {}",
                    columns[1]
                );
            }
            let cli_handlers = cli_entry
                .split(" / ")
                .filter_map(|entry| {
                    let command = entry.split_whitespace().next().unwrap();
                    (!command.starts_with('-')).then(|| {
                        handler(
                            cli,
                            &format!("fn run_{}(", command.replace('-', "_")),
                            "\nfn ",
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mcp_handlers = mcp_entry
                .split('/')
                .map(str::trim)
                .map(|tool| handler(mcp, &format!("async fn {tool}("), "    #[tool("))
                .collect::<Vec<_>>()
                .join("\n");
            for operation in op.split(',').map(str::trim) {
                assert!(
                    cli_handlers.contains(operation),
                    "CLI does not call {operation} for {}",
                    columns[1]
                );
                assert!(
                    mcp_handlers.contains(operation),
                    "MCP does not call {operation} for {}",
                    columns[1]
                );
            }
        }
    }
    assert!(flows > 30, "the desktop flow table was not checked");
}
