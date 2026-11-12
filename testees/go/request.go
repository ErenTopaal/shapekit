package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
)

// request keeps every member as raw json, so numbers reach the sdk exactly as the runner
// wrote them.
type request struct {
	id     string
	op     string
	fields map[string]json.RawMessage
}

// parseRequest reads one line. On failure the returned request still carries the id when one
// could be read, so the error answers the right request.
func parseRequest(line []byte) (request, error) {
	var req request
	if err := json.Unmarshal(line, &req.fields); err != nil || req.fields == nil {
		return req, badRequest("request is not a json object")
	}
	rawID, ok := req.fields["id"]
	if !ok {
		return req, badRequest("request has no id")
	}
	if err := json.Unmarshal(rawID, &req.id); err != nil || bytes.Equal(bytes.TrimSpace(rawID), []byte("null")) {
		return request{}, badRequest("id is not a string")
	}
	rawOp, ok := req.fields["op"]
	if !ok {
		return req, badRequest("request has no op")
	}
	if err := json.Unmarshal(rawOp, &req.op); err != nil || bytes.Equal(bytes.TrimSpace(rawOp), []byte("null")) {
		return req, badRequest("op is not a string")
	}
	return req, nil
}

func (r request) raw(name string) (json.RawMessage, error) {
	v, ok := r.fields[name]
	if !ok {
		return nil, badRequest("%s needs member %s", r.op, name)
	}
	return v, nil
}

// get decodes a member into dst; null is refused because no member of the protocol is
// nullable.
func (r request) get(name, what string, dst any) error {
	raw, err := r.raw(name)
	if err != nil {
		return err
	}
	if bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
		return badRequest("%s must be %s, got null", name, what)
	}
	if err := json.Unmarshal(raw, dst); err != nil {
		return badRequest("%s must be %s: %v", name, what, err)
	}
	return nil
}

func (r request) str(name string) (string, error) {
	var s string
	err := r.get(name, "a string", &s)
	return s, err
}

func (r request) bytes(name string) ([]byte, error) {
	s, err := r.str(name)
	if err != nil {
		return nil, err
	}
	b, err := base64.StdEncoding.Strict().DecodeString(s)
	if err != nil {
		return nil, badRequest("%s is not standard base64: %v", name, err)
	}
	return b, nil
}

func (r request) uint32(name string) (uint32, error) {
	var v uint32
	err := r.get(name, "an unsigned 32-bit integer", &v)
	return v, err
}

func (r request) int32(name string) (int32, error) {
	var v int32
	err := r.get(name, "a signed 32-bit integer", &v)
	return v, err
}

func (r request) nonNegativeInt(name string) (int, error) {
	var v int
	if err := r.get(name, "an unsigned integer", &v); err != nil {
		return 0, err
	}
	if v < 0 {
		return 0, badRequest("%s must be an unsigned integer, got %d", name, v)
	}
	return v, nil
}
