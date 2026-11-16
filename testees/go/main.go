// Command testee is the go-stellar-sdk side of the shapekit conformance runner. It speaks the
// testee protocol (docs/testee-protocol.md): one json request per line on stdin, one json
// response per line on stdout.
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"runtime/debug"
)

const protocolVersion = 2

func main() {
	if err := serve(os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "testee:", err)
		os.Exit(1)
	}
}

// serve answers every request line until in is exhausted. A bad request gets an error line
// and the loop goes on, so one malformed vector cannot hide the results of the ones after it.
// Only i/o failures end the loop.
func serve(in io.Reader, out io.Writer) error {
	// ReadBytes grows to any line length; the protocol needs at least 64 MiB lines and a
	// scanner would need a fixed cap
	r := bufio.NewReaderSize(in, 1<<20)
	w := bufio.NewWriter(out)
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)

	for {
		line, readErr := r.ReadBytes('\n')
		if readErr != nil && !errors.Is(readErr, io.EOF) {
			return fmt.Errorf("read request: %w", readErr)
		}
		if len(line) > 0 {
			if err := enc.Encode(handle(bytes.TrimSuffix(line, []byte("\n")))); err != nil {
				return fmt.Errorf("write response: %w", err)
			}
			if err := w.Flush(); err != nil {
				return fmt.Errorf("write response: %w", err)
			}
		}
		if errors.Is(readErr, io.EOF) {
			return nil
		}
	}
}

type response struct {
	ID     string         `json:"id"`
	Status string         `json:"status"`
	Result map[string]any `json:"result,omitempty"`
	Error  *errorBody     `json:"error,omitempty"`
	Reason string         `json:"reason,omitempty"`
}

type errorBody struct {
	Class   string `json:"class"`
	Message string `json:"message"`
}

func handle(line []byte) (resp response) {
	req, err := parseRequest(line)
	resp.ID = req.id
	if err == nil {
		// a panic outside the sdk wrappers is the testee's own fault, so it is reported as
		// internal and never as an sdk crash
		defer func() {
			if p := recover(); p != nil {
				fmt.Fprintf(os.Stderr, "testee: %s request %s panicked: %v\n%s", req.op, req.id, p, debug.Stack())
				resp = response{ID: req.id, Status: "error", Error: &errorBody{Class: classInternal, Message: fmt.Sprintf("testee panicked: %v", p)}}
			}
		}()
		var result map[string]any
		result, err = dispatch(req)
		if err == nil {
			resp.Status = "ok"
			resp.Result = result
			return resp
		}
	}

	if unsup, ok := errors.AsType[*unsupportedError](err); ok {
		resp.Status = "unsupported"
		resp.Reason = unsup.reason
		return resp
	}
	resp.Status = "error"
	if oe, ok := errors.AsType[*opError](err); ok {
		resp.Error = &errorBody{Class: oe.class, Message: oe.message}
	} else {
		resp.Error = &errorBody{Class: classInternal, Message: err.Error()}
	}
	return resp
}

const (
	classRejected   = "rejected"
	classPanic      = "panic"
	classBadRequest = "bad-request"
	classInternal   = "internal"
)

type opError struct {
	class   string
	message string
}

func (e *opError) Error() string { return e.class + ": " + e.message }

func badRequest(format string, args ...any) error {
	return &opError{class: classBadRequest, message: fmt.Sprintf(format, args...)}
}

func internal(format string, args ...any) error {
	return &opError{class: classInternal, message: fmt.Sprintf(format, args...)}
}

type unsupportedError struct{ reason string }

func (e *unsupportedError) Error() string { return "unsupported: " + e.reason }

func unsupported(reason string) error { return &unsupportedError{reason: reason} }

// sdkCall runs one call into the sdk. An error it returns is the sdk refusing the input; a
// panic is the sdk crashing, which the runner counts against the sdk.
func sdkCall(name string, f func() error) (err error) {
	defer func() {
		if p := recover(); p != nil {
			fmt.Fprintf(os.Stderr, "testee: %s panicked: %v\n%s", name, p, debug.Stack())
			err = &opError{class: classPanic, message: fmt.Sprintf("%s panicked: %v", name, p)}
		}
	}()
	if e := f(); e != nil {
		return &opError{class: classRejected, message: fmt.Sprintf("%s: %v", name, e)}
	}
	return nil
}
