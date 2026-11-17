"""reads request lines and writes one response line each."""

import json
from typing import IO, Any

from .ops import Ops
from .protocol import BadRequest, parse_line, respond


def handle(ops: Ops, line: bytes) -> dict[str, Any]:
    request_id, request = parse_line(line)

    def run() -> dict[str, Any]:
        if isinstance(request, BadRequest):
            raise request
        return ops.run(request)

    return respond(request_id, run)


def serve(stdin: IO[bytes], stdout: IO[str], coverage: Any = None) -> int:
    ops = Ops(coverage)
    # readline grows to any line length; the protocol needs request lines of at least 64 MiB
    while True:
        line = stdin.readline()
        if not line:
            return 0
        line = line.rstrip(b"\n")
        if not line:
            continue
        stdout.write(json.dumps(handle(ops, line), separators=(",", ":")))
        stdout.write("\n")
        stdout.flush()
