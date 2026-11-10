# Run skill evaluations through installed clients

The shared Rust evaluation engine owns saved runs, frozen inputs, native client processes, and check evidence; desktop and CLI are adapters. A model SDK would replace the tools, authentication, and skill discovery we need to test, while a screen-owned runner cannot survive navigation or serve the CLI. Client isolation is recorded per trial; only a proven discovery control permits a no-skill comparison. See the [research and alternatives](../research/2026-11-09-skill-assistant.md).
