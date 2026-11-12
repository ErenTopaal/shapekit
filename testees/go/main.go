// Command testee is the go-stellar-sdk side of the shapekit conformance
// runner. It reads one json request per line on stdin and writes one json
// response per line on stdout.
package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"runtime/debug"
)

// the runner sends whole xdr blobs on one line, so allow far more than the
// scanner's 64 KiB default.
const maxLineBytes = 16 << 20

type request struct {
	Op string `json:"op"`
}

type helloResponse struct {
	SDK     string   `json:"sdk"`
	Version string   `json:"version"`
	Ops     []string `json:"ops"`
}

type errorResponse struct {
	Error string `json:"error"`
}

var supportedOps = []string{"hello"}

func main() {
	if err := serve(os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "testee:", err)
		os.Exit(1)
	}
}

// serve answers every request line until in is exhausted. A bad request
// gets an error line and the loop goes on, so one malformed vector cannot
// hide the results of the ones after it. Only i/o failures end the loop.
func serve(in io.Reader, out io.Writer) error {
	sc := bufio.NewScanner(in)
	sc.Buffer(make([]byte, 0, 64<<10), maxLineBytes)

	enc := json.NewEncoder(out)

	for sc.Scan() {
		if err := enc.Encode(handle(sc.Bytes())); err != nil {
			return fmt.Errorf("write response: %w", err)
		}
	}

	if err := sc.Err(); err != nil {
		return fmt.Errorf("read request: %w", err)
	}

	return nil
}

func handle(line []byte) any {
	var req request
	if err := json.Unmarshal(line, &req); err != nil {
		return errorResponse{Error: "invalid request: " + err.Error()}
	}

	switch req.Op {
	case "hello":
		version, err := moduleVersion()
		if err != nil {
			return errorResponse{Error: err.Error()}
		}

		return helloResponse{SDK: "go-stellar-sdk", Version: version, Ops: supportedOps}
	default:
		return errorResponse{Error: "unsupported op"}
	}
}

func moduleVersion() (string, error) {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return "", errors.New("build info not available")
	}

	return info.Main.Version, nil
}
