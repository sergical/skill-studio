# Performance baseline

Commit: `c205022785ad40db6530dedc99b542d16630a326`. Dirty: False.

Hardware: Apple M3 Max; 36 GiB RAM.

ProductName: macOS; ProductVersion: 26.7; BuildVersion: 25G229

Fresh runtime and reused runtime; OS cache uncontrolled, not cold disk. MCP persistent after initialize. Empty history.

| Fixture     | Metric                       |   N |       Median |           p95 |           Max |
| ----------- | ---------------------------- | --: | -----------: | ------------: | ------------: |
| small       | core_scan_fresh_runtime_ms   |  10 |        7.511 |        43.805 |        43.805 |
| small       | core_scan_warm_runtime_ms    |  10 |        6.166 |         8.005 |         8.005 |
| small       | cli_scan_ms                  |  10 |       60.779 |       507.652 |       507.652 |
| small       | cli_scan_peak_rss_bytes      |  10 | 38232064.000 |  38273024.000 |  38273024.000 |
| small       | cli_doctor_ms                |  10 |       78.763 |       182.786 |       182.786 |
| small       | cli_doctor_peak_rss_bytes    |  10 | 37642240.000 |  38174720.000 |  38174720.000 |
| small       | cli_history_ms               |  10 |       22.713 |        69.793 |        69.793 |
| small       | cli_history_peak_rss_bytes   |  10 |  9084928.000 |   9109504.000 |   9109504.000 |
| small       | mcp_scan_round_trip_ms       |  10 |       33.456 |       109.806 |       109.806 |
| small       | desktop_first_list_ipc_bytes |  10 |    49171.000 |     49171.000 |     49171.000 |
| real-shaped | core_scan_fresh_runtime_ms   |  10 |      130.493 |       284.396 |       284.396 |
| real-shaped | core_scan_warm_runtime_ms    |  10 |      125.570 |       282.836 |       282.836 |
| real-shaped | cli_scan_ms                  |  10 |      187.611 |       237.567 |       237.567 |
| real-shaped | cli_scan_peak_rss_bytes      |  10 | 41426944.000 |  42516480.000 |  42516480.000 |
| real-shaped | cli_doctor_ms                |  10 |      209.178 |       320.028 |       320.028 |
| real-shaped | cli_doctor_peak_rss_bytes    |  10 | 41320448.000 |  41713664.000 |  41713664.000 |
| real-shaped | cli_history_ms               |  10 |       18.273 |        88.728 |        88.728 |
| real-shaped | cli_history_peak_rss_bytes   |  10 |  9158656.000 |   9224192.000 |   9224192.000 |
| real-shaped | mcp_scan_round_trip_ms       |  10 |      167.068 |       176.702 |       176.702 |
| real-shaped | desktop_first_list_ipc_bytes |  10 |   937686.000 |    937686.000 |    937686.000 |
| large       | core_scan_fresh_runtime_ms   |  10 |     3054.489 |      5806.937 |      5806.937 |
| large       | core_scan_warm_runtime_ms    |  10 |     3314.300 |      5945.625 |      5945.625 |
| large       | cli_scan_ms                  |  10 |     6452.130 |     14170.320 |     14170.320 |
| large       | cli_scan_peak_rss_bytes      |  10 | 90390528.000 | 102973440.000 | 102973440.000 |
| large       | cli_doctor_ms                |  10 |     9917.582 |     16423.646 |     16423.646 |
| large       | cli_doctor_peak_rss_bytes    |  10 | 75759616.000 |  88211456.000 |  88211456.000 |
| large       | cli_history_ms               |  10 |      100.402 |       252.822 |       252.822 |
| large       | cli_history_peak_rss_bytes   |  10 |  9846784.000 |   9879552.000 |   9879552.000 |
| large       | mcp_scan_round_trip_ms       |  10 |     2911.085 |      4360.681 |      4360.681 |
| large       | desktop_first_list_ipc_bytes |  10 | 12456104.000 |  12456104.000 |  12456104.000 |

Host load affects these timings. The recorded local baseline ran alongside other builds and tests.

small: desktop returned 20 skills; MCP returned partial inventories in 0/10 calls. Partial timings do not measure a complete scan.

real-shaped: desktop returned 378 skills; MCP returned partial inventories in 0/10 calls. Partial timings do not measure a complete scan.

large: desktop returned 5000 skills; MCP returned partial inventories in 10/10 calls. Partial timings do not measure a complete scan.
