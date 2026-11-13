package main

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"runtime/debug"
	"slices"
	"strings"
	"testing"
)

// roundTrip feeds lines to serve and returns one decoded response per line.
func roundTrip(t *testing.T, input string) []map[string]any {
	t.Helper()
	var out bytes.Buffer
	if err := serve(strings.NewReader(input), &out); err != nil {
		t.Fatalf("serve: %v", err)
	}
	var got []map[string]any
	for line := range strings.SplitSeq(strings.TrimSuffix(out.String(), "\n"), "\n") {
		var m map[string]any
		if err := json.Unmarshal([]byte(line), &m); err != nil {
			t.Fatalf("response %q is not json: %v", line, err)
		}
		got = append(got, m)
	}
	return got
}

func TestHello(t *testing.T) {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		t.Fatal("test binary has no build info")
	}
	want := ""
	for _, d := range info.Deps {
		if d.Path == sdkModule {
			want = d.Version
		}
	}
	if want == "" {
		t.Fatalf("%s is not a dependency of the test binary", sdkModule)
	}

	got := roundTrip(t, `{"id":"0","op":"hello"}`+"\n")
	if len(got) != 1 || got[0]["status"] != "ok" || got[0]["id"] != "0" {
		t.Fatalf("hello: %v", got)
	}
	result := got[0]["result"].(map[string]any)
	if result["protocol"] != float64(1) || result["sdk"] != sdkModule || result["sdk_version"] != want {
		t.Errorf("hello result %v, want protocol 1, %s %s", result, sdkModule, want)
	}
	if result["decode_limits"] != "honoured" { //nolint:misspell // the protocol spells it this way
		t.Errorf("decode_limits %v", result["decode_limits"])
	}
	components := result["components"].(map[string]any)
	if v, ok := components[xdrModule].(string); !ok || v == "" {
		t.Errorf("components %v has no %s version", components, xdrModule)
	}
	var listed []string
	for _, op := range result["ops"].([]any) {
		listed = append(listed, op.(string))
	}
	if !slices.Equal(listed, ops) {
		t.Errorf("ops %v, want %v", listed, ops)
	}
}

func TestRequestErrors(t *testing.T) {
	input := strings.Join([]string{
		`not json`,
		`{"op":"hello"}`,
		`{"id":7,"op":"hello"}`,
		`{"id":"a"}`,
		`{"id":"b","op":"frobnicate"}`,
		`{"id":"c","op":"strkey_decode"}`,
		`{"id":"d","op":"decode_encode","type":"ScVal","xdr":"!!","depth_limit":1000,"len_limit":4}`,
		`{"id":"e","op":"strkey_encode","kind":"account","payload":null}`,
		`{"id":"f","op":"map_sort","entries":[]}`,
		`{"id":"g","op":"decode_encode","type":"NoSuchType","xdr":"AAAAAA==","depth_limit":1000,"len_limit":4}`,
	}, "\n") // no newline after the last line: end of input still ends it

	want := []struct{ id, status, class string }{
		{"", "error", classBadRequest},
		{"", "error", classBadRequest},
		{"", "error", classBadRequest},
		{"a", "error", classBadRequest},
		{"b", "error", classBadRequest},
		{"c", "error", classBadRequest},
		{"d", "error", classBadRequest},
		{"e", "error", classBadRequest},
		{"f", "unsupported", ""},
		{"g", "unsupported", ""},
	}
	got := roundTrip(t, input)
	if len(got) != len(want) {
		t.Fatalf("%d responses for %d requests: %v", len(got), len(want), got)
	}
	for i, w := range want {
		g := got[i]
		if g["id"] != w.id || g["status"] != w.status {
			t.Errorf("request %d: got %v, want id %q status %s", i, g, w.id, w.status)
			continue
		}
		switch w.status {
		case "error":
			e := g["error"].(map[string]any)
			if e["class"] != w.class || e["message"] == "" {
				t.Errorf("request %d: error %v, want class %s", i, e, w.class)
			}
		case "unsupported":
			if r, _ := g["reason"].(string); r == "" {
				t.Errorf("request %d: unsupported without a reason", i)
			}
		}
	}
}

func TestLongLine(t *testing.T) {
	// far past bufio's default sizes; the protocol needs lines of 64 MiB and more
	pad := strings.Repeat(" ", 3<<20)
	line := `{"id":"1",` + pad + `"op":"strkey_decode","text":"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"}` + "\n"
	got := roundTrip(t, line)
	if len(got) != 1 || got[0]["status"] != "ok" {
		t.Fatalf("got %v", got)
	}
}

func TestDecodeEncode(t *testing.T) {
	// ScVal u32 7, then the same with a trailing byte, then a depth limit of zero
	got := roundTrip(t, strings.Join([]string{
		`{"id":"1","op":"decode_encode","type":"ScVal","xdr":"AAAAAwAAAAc=","depth_limit":1000,"len_limit":8}`,
		`{"id":"2","op":"decode_encode","type":"ScVal","xdr":"AAAAAwAAAAcA","depth_limit":1000,"len_limit":9}`,
		`{"id":"3","op":"decode_encode","type":"ScVal","xdr":"AAAAAwAAAAc=","depth_limit":0,"len_limit":8}`,
	}, "\n")+"\n")
	if got[0]["status"] != "ok" || got[0]["result"].(map[string]any)["xdr"] != "AAAAAwAAAAc=" {
		t.Errorf("roundtrip: %v", got[0])
	}
	for _, g := range got[1:] {
		if g["status"] != "error" || g["error"].(map[string]any)["class"] != classRejected {
			t.Errorf("want rejected: %v", g)
		}
	}
}

func TestStrkey(t *testing.T) {
	const account = "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"
	got := roundTrip(t, strings.Join([]string{
		`{"id":"1","op":"strkey_decode","text":"` + account + `"}`,
		`{"id":"2","op":"strkey_encode","kind":"account","payload":"Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo="}`,
		`{"id":"3","op":"strkey_decode","text":" ` + account + `"}`,
	}, "\n")+"\n")
	r := got[0]["result"].(map[string]any)
	if r["kind"] != "account" || r["payload"] != "Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo=" {
		t.Errorf("decode: %v", got[0])
	}
	if got[1]["result"].(map[string]any)["text"] != account {
		t.Errorf("encode: %v", got[1])
	}
	if got[2]["status"] != "error" || got[2]["error"].(map[string]any)["class"] != classRejected {
		t.Errorf("leading space: %v", got[2])
	}
}

// text_base64 carries input that is not utf-8; the bytes must reach the sdk unchanged, which a
// valid account shows without depending on how a given sdk version treats stray bytes
func TestStrkeyTextBase64(t *testing.T) {
	const account = "GA5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DUOR2HI5DVTUJ"
	b64 := base64.StdEncoding.EncodeToString([]byte(account))
	got := roundTrip(t, strings.Join([]string{
		`{"id":"1","op":"strkey_decode","text_base64":"` + b64 + `"}`,
		`{"id":"2","op":"strkey_decode","text":"` + account + `","text_base64":"` + b64 + `"}`,
		`{"id":"3","op":"strkey_decode","text_base64":"!!"}`,
	}, "\n")+"\n")
	if r, _ := got[0]["result"].(map[string]any); r["kind"] != "account" {
		t.Errorf("decode: %v", got[0])
	}
	for _, g := range got[1:] {
		if g["status"] != "error" || g["error"].(map[string]any)["class"] != classBadRequest {
			t.Errorf("want bad request: %v", g)
		}
	}
}

func TestAssetOrderAndPoolID(t *testing.T) {
	const native = "AAAAAA=="
	const usd = "AAAAAVVTRAAAAAAA//////////////////////////////////////////8="
	got := roundTrip(t, strings.Join([]string{
		`{"id":"1","op":"asset_order","a":"` + native + `","b":"` + usd + `"}`,
		`{"id":"2","op":"asset_order","a":"` + usd + `","b":"` + native + `"}`,
		`{"id":"3","op":"asset_order","a":"` + usd + `","b":"` + usd + `"}`,
		`{"id":"4","op":"pool_id","a":"` + native + `","b":"` + usd + `","fee":30}`,
		`{"id":"5","op":"pool_id","a":"` + usd + `","b":"` + native + `","fee":30}`,
	}, "\n")+"\n")
	for i, want := range []float64{-1, 1, 0} {
		if got[i]["result"].(map[string]any)["order"] != want {
			t.Errorf("order %d: %v, want %v", i, got[i], want)
		}
	}
	if id, _ := got[3]["result"].(map[string]any)["pool_id"].(string); len(id) != 64 {
		t.Errorf("pool id: %v", got[3])
	}
	if got[4]["status"] != "error" || got[4]["error"].(map[string]any)["class"] != classRejected {
		t.Errorf("reversed pair: %v", got[4])
	}
}
