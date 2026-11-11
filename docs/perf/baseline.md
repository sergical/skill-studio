# Performance baseline

Status: provisional: Cargo-idle samples. The owner will provide the final reference window after wave 1.

Commit: `15c1fbda980cb84d05bd51bd04c9b1d475bc540e`. Dirty: False.

Hardware: Apple M3 Max; 36 GiB RAM.

ProductName: macOS; ProductVersion: 26.7; BuildVersion: 25G229

Fresh runtime and reused runtime; OS cache uncontrolled, not cold disk. MCP persistent after initialize. Empty history.

| Fixture     | Metric                       |   N |        Median |           p95 |           Max |
| ----------- | ---------------------------- | --: | ------------: | ------------: | ------------: |
| small       | core_scan_fresh_runtime_ms   |  10 |         4.532 |        45.779 |        45.779 |
| small       | core_scan_warm_runtime_ms    |  10 |         4.541 |         4.873 |         4.873 |
| small       | cli_scan_ms                  |  10 |        60.255 |       608.386 |       608.386 |
| small       | cli_scan_peak_rss_bytes      |  10 |  37675008.000 |  37748736.000 |  37748736.000 |
| small       | cli_doctor_ms                |  10 |        57.225 |        64.670 |        64.670 |
| small       | cli_doctor_peak_rss_bytes    |  10 |  37699584.000 |  38207488.000 |  38207488.000 |
| small       | cli_history_ms               |  10 |        13.034 |        14.349 |        14.349 |
| small       | cli_history_peak_rss_bytes   |  10 |   9043968.000 |   9093120.000 |   9093120.000 |
| small       | mcp_scan_round_trip_ms       |  10 |         9.299 |        38.812 |        38.812 |
| small       | desktop_first_list_ipc_bytes |  10 |     49171.000 |     49171.000 |     49171.000 |
| real-shaped | core_scan_fresh_runtime_ms   |  10 |        76.724 |       112.326 |       112.326 |
| real-shaped | core_scan_warm_runtime_ms    |  10 |        76.784 |        79.217 |        79.217 |
| real-shaped | cli_scan_ms                  |  10 |       169.252 |       180.437 |       180.437 |
| real-shaped | cli_scan_peak_rss_bytes      |  10 |  41148416.000 |  42221568.000 |  42221568.000 |
| real-shaped | cli_doctor_ms                |  10 |       165.819 |       171.647 |       171.647 |
| real-shaped | cli_doctor_peak_rss_bytes    |  10 |  41074688.000 |  41697280.000 |  41697280.000 |
| real-shaped | cli_history_ms               |  10 |        14.805 |        15.968 |        15.968 |
| real-shaped | cli_history_peak_rss_bytes   |  10 |   9125888.000 |   9158656.000 |   9158656.000 |
| real-shaped | mcp_scan_round_trip_ms       |  10 |       131.605 |       162.324 |       162.324 |
| real-shaped | desktop_first_list_ipc_bytes |  10 |    937689.000 |    937689.000 |    937689.000 |
| large       | core_scan_fresh_runtime_ms   |  10 |      2414.543 |      2530.081 |      2530.081 |
| large       | core_scan_warm_runtime_ms    |  10 |      2437.664 |      2540.055 |      2540.055 |
| large       | cli_scan_ms                  |  10 |      2025.658 |      2106.350 |      2106.350 |
| large       | cli_scan_peak_rss_bytes      |  10 | 101285888.000 | 103645184.000 | 103645184.000 |
| large       | cli_doctor_ms                |  10 |      2029.893 |      2105.659 |      2105.659 |
| large       | cli_doctor_peak_rss_bytes    |  10 |  87162880.000 |  88719360.000 |  88719360.000 |
| large       | cli_history_ms               |  10 |        55.849 |        59.335 |        59.335 |
| large       | cli_history_peak_rss_bytes   |  10 |   9748480.000 |   9830400.000 |   9830400.000 |
| large       | mcp_scan_round_trip_ms       |  10 |      2163.742 |      2179.980 |      2179.980 |
| large       | desktop_first_list_ipc_bytes |  10 |  12456104.000 |  12456104.000 |  12456104.000 |

Before samples, `pgrep -x cargo` returned no PIDs. Other host load is uncontrolled.

small: desktop returned 20 skills; MCP returned partial inventories in 0/10 calls. Partial timings do not measure a complete scan.

real-shaped: desktop returned 378 skills; MCP returned partial inventories in 0/10 calls. Partial timings do not measure a complete scan.

large: desktop returned 5000 skills; MCP returned partial inventories in 7/10 calls. Partial timings do not measure a complete scan.
