"""Serves a local pack for one verification run, until killed.

    python3 serve_pack.py <run-dir> <skill-count>

Builds a bare git repo with <skill-count> skills under <run-dir>/pack, serves
it over smart HTTP on loopback with `git http-backend`, and writes the clone
URL to <run-dir>/pack-url once it answers. Reuses tools/perf-writes.py, so the
benchmark and the verification skill serve the same pack.
"""
import importlib.util
from pathlib import Path
import sys
import threading

REPO = Path(__file__).resolve().parents[4]


def main():
    run_dir = Path(sys.argv[1])
    count = int(sys.argv[2])
    spec = importlib.util.spec_from_file_location("perf_writes", REPO / "tools" / "perf-writes.py")
    perf_writes = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(perf_writes)
    pack = run_dir / "pack"
    pack.mkdir()
    names = perf_writes.make_pack(pack, count)
    _server, base = perf_writes.serve_git(pack / "srv")
    (run_dir / "pack-skills").write_text("\n".join(names) + "\n")
    (run_dir / "pack-url").write_text(f"{base}/pack.git\n")
    threading.Event().wait()


if __name__ == "__main__":
    main()
