# shapekit python testee: answers testee protocol requests (docs/testee-protocol.md) with
# stellar-sdk (py-stellar-base), one json object per line on stdin and stdout. run it with the
# python of a venv that holds the sdk version under test; this file's directory is on sys.path
# because python puts a script's directory there.

import os
import sys

from shapekit_testee.coverage_ops import Coverage

if __name__ == "__main__":
    # coverage has to be running before stellar_sdk is imported, or the module bodies the
    # import runs are never counted
    coverage = Coverage.start() if os.environ.get("SHAPEKIT_COVERAGE") == "1" else None

    from shapekit_testee.serve import serve

    sys.exit(serve(sys.stdin.buffer, sys.stdout, coverage))
