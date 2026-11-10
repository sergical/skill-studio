# Launch detection verification plan

| Issue | User flow                   | Expectation                                                              | Proof (file::test or command)                        | Red before fix? | Measured numbers                       |
| ----- | --------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------- | --------------- | -------------------------------------- |
| #247  | Launch with a saved choice  | Detection runs without holding the first-run gate; new agents remain off | useFirstRun tests; detected-agent component test     | Planned         | Release N=10 hit/miss, including shell |
| #247  | Resolve a tool outside PATH | Report fallback provenance and scan app Resources                        | tools.rs fixture tests; core report test             | Planned         | Included in launch samples             |
| #247  | Open editor after detection | Reuse one cached login shell result                                      | tools.rs combined-probe test                         | Planned         | Shell included                         |
| #247  | Run detection               | Runtime creation and probes run in the blocking pool                     | harness_first_run blocking-pool test; spawn mutation | Planned         | No wall-clock assertion                |

Lead default, owner to confirm: detect each launch in the background. New
agents stay off until selected. Detection never writes an agent folder.

The release runner calls the actual desktop detection helper, including runtime construction, project discovery, the login shell, and runtime cleanup. Each sample uses a fresh native process. The input comes from the baseline FixtureBuilder generator.
