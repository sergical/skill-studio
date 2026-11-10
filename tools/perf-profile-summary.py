"""Summarize Samply main-thread leaf samples and export folded stacks."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("profile", type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    output = args.output_dir or args.profile.parent
    profile = json.loads(gzip.open(args.profile, "rt").read())
    sidecar = json.loads(args.profile.with_suffix(".syms.json").read_text())
    symbols = {row["debug_name"]: row for row in sidecar["data"]}
    addresses = {name: dict(row["known_addresses"]) for name, row in symbols.items()}
    leaves = Counter()
    folded = Counter()
    for thread in profile["threads"]:
        if not thread["isMainThread"]:
            continue
        cache = {}

        def frame_name(frame):
            if frame in cache:
                return cache[frame]
            func = thread["frameTable"]["func"][frame]
            resource = thread["funcTable"]["resource"][func]
            if resource < 0:
                return thread["stringArray"][thread["funcTable"]["name"][func]]
            library = profile["libs"][thread["resourceTable"]["lib"][resource]]
            name = library["debugName"]
            index = addresses.get(name, {}).get(thread["frameTable"]["address"][frame])
            label = (sidecar["string_table"][symbols[name]["symbol_table"][index]["symbol"]]
                     if index is not None else library["name"] + " unresolved")
            cache[frame] = label
            return label

        for stack in thread["samples"]["stack"]:
            if stack is None:
                continue
            frames = []
            while stack is not None:
                frames.append(frame_name(thread["stackTable"]["frame"][stack]))
                stack = thread["stackTable"]["prefix"][stack]
            leaves[frames[0]] += 1
            folded[";".join(name.replace(";", ":") for name in reversed(frames))] += 1
    total = sum(leaves.values())
    if not total:
        raise ValueError("profile has no main-thread stack samples")
    output.mkdir(parents=True, exist_ok=True)
    (output / "real-shaped-profile.folded.txt").write_text(
        "".join(f"{stack} {count}\n" for stack, count in sorted(folded.items())))
    metadata = json.loads((output / "profile-metadata.json").read_text())
    baseline = json.loads((output / "baseline.json").read_text())
    lines = ["# Real-shaped scan profile", "", "Release CLI scan, 378 skills, seed 1, ten complete scans.", "",
             f"Source commit: `{metadata['git_sha']}`.", "",
             f"Hardware: {baseline['cpu']}; {baseline['memory_bytes'] / 1024**3:g} GiB RAM.", "",
             f"Executable SHA-256: `{metadata['binary_sha256']}`.", "",
             f"Recorded: {metadata['recorded_at']}. Load average: `{metadata['load_average']}`.", "",
             f"Cargo processes before/after: {metadata['cargo_processes']}/{metadata['cargo_processes_after']}.", "",
             f"DEVELOPER_DIR: `{metadata['developer_dir']}`.", "",
             "![CLI scan flame graph](real-shaped-flamegraph.svg)", "",
             "The table counts main-thread leaf samples. It does not measure CPU percentages. Unresolved frames stay in the denominator.", "",
             "| Leaf frame | Samples | Share |", "| --- | ---: | ---: |"]
    lines += [f"| `{name}` | {count} | {count / total:.1%} |" for name, count in leaves.most_common(5)]
    lines += ["", f"Total nonempty main-thread stacks: {total}.", "",
              "Filesystem calls dominate the sampled work. A cache needs invalidation and scope-safety proof before it can be a clear win. No scan hot-spot fix is included here.", "",
              "The compressed Samply profile and symbol sidecar preserve the full trace. The SVG uses the same folded stacks and the standard [FlameGraph renderer](https://github.com/brendangregg/FlameGraph).", "",
              "Regenerate the table and folded stacks with:", "", "```sh",
              "python3 tools/perf-profile-summary.py docs/perf/real-shaped-profile.json.gz", "```"]
    (output / "profile.md").write_text("\n".join(lines) + "\n")
    print(f"Summarized {total} main-thread samples")


if __name__ == "__main__":
    main()
