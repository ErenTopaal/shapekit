// Package wire is the Go side of the shapekit adapter protocol, version 2
// (docs/adapter-interface.md, schema/adapter-protocol.schema.json): the
// request and response shapes and a line loop that dispatches requests to a
// Handler.
package wire

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"maps"
	"sync"
)

// ProtocolVersion is the newest adapter protocol version this package speaks.
const ProtocolVersion = 2

// MinProtocolVersion is the oldest version it still answers. Version 1 has no
// dump command and no capabilities.
const MinProtocolVersion = 1

// Capabilities an adapter lists in describe (version 2): its handler answers
// dump (Dumper) or restart (Restarter).
const (
	CapabilityDump    = "dump"
	CapabilityRestart = "restart"
)

// Delivery modes.
const (
	DeliverySep54Filesystem = "sep54-filesystem"
	DeliverySep54S3         = "sep54-s3"
	DeliverySep54GCS        = "sep54-gcs"
	DeliveryRPC             = "rpc"
)

// Health values reported by status.
const (
	HealthStarting = "starting"
	HealthRunning  = "running"
	HealthCrashed  = "crashed"
	HealthOOM      = "oom"
)

// ErrorClass is the class of an error response.
type ErrorClass string

// Error classes.
const (
	ClassBadRequest  ErrorClass = "bad-request"
	ClassUnsupported ErrorClass = "unsupported"
	ClassTarget      ErrorClass = "target"
	ClassInternal    ErrorClass = "internal"
)

// Error is an error a handler returns to have it reported with a class.
type Error struct {
	Class   ErrorClass
	Message string
}

func (e *Error) Error() string { return string(e.Class) + ": " + e.Message }

// Errorf builds an *Error.
func Errorf(class ErrorClass, format string, args ...any) *Error {
	return &Error{Class: class, Message: fmt.Sprintf(format, args...)}
}

// Request is one decoded request line. Only the members of its command are set.
type Request struct {
	ID       string       `json:"id"`
	Cmd      string       `json:"cmd"`
	Protocol *int         `json:"protocol,omitempty"`
	Head     *uint32      `json:"head,omitempty"`
	From     *uint32      `json:"from,omitempty"`
	To       *uint32      `json:"to,omitempty"`
	Tables   []string     `json:"tables,omitempty"`
	Start    *StartParams `json:"-"`
}

// StartParams are the members of a start request.
type StartParams struct {
	Chain             string       `json:"chain"`
	Network           string       `json:"network"`
	NetworkPassphrase string       `json:"network_passphrase"`
	Protocol          uint32       `json:"protocol"`
	Delivery          Delivery     `json:"delivery"`
	Ledgers           LedgerWindow `json:"ledgers"`
	Limits            Limits       `json:"limits"`
	DockerNetwork     string       `json:"docker_network,omitempty"`
	Workdir           string       `json:"workdir"`
}

// Delivery says where the replay server serves the chain.
type Delivery struct {
	Mode       string `json:"mode"`
	RPCURL     string `json:"rpc_url"`
	ArchiveURL string `json:"archive_url"`
	Lake       Lake   `json:"lake"`
}

// Lake is the chain's SEP-54 lake.
type Lake struct {
	URL                 string `json:"url"`
	Path                string `json:"path"`
	LedgersPerBatch     uint32 `json:"ledgers_per_batch"`
	BatchesPerPartition uint32 `json:"batches_per_partition"`
}

// LedgerWindow is the chain's ledger range as the harness runs it.
type LedgerWindow struct {
	Bootstrap           uint32 `json:"bootstrap"`
	Last                uint32 `json:"last"`
	CheckpointFrequency uint32 `json:"checkpoint_frequency"`
}

// Limits are the resource limits for the target.
type Limits struct {
	MemoryBytes uint64 `json:"memory_bytes"`
}

// DescribeResult answers describe.
type DescribeResult struct {
	Protocol        int           `json:"protocol"`
	Adapter         string        `json:"adapter"`
	AdapterVersion  string        `json:"adapter_version"`
	Target          string        `json:"target"`
	TargetVersion   string        `json:"target_version"`
	DeliveryModes   []string      `json:"delivery_modes"`
	DefaultDelivery string        `json:"default_delivery"`
	Bootstrap       string        `json:"bootstrap"`
	VocabVersion    int           `json:"vocab_version"`
	Protocols       ProtocolRange `json:"protocols"`
	Claims          Claims        `json:"claims"`
	Capabilities    []string      `json:"capabilities,omitempty"`
}

// ProtocolRange is the ledger protocol range a target ingests.
type ProtocolRange struct {
	Min uint32 `json:"min"`
	Max uint32 `json:"max"`
}

// Claims say which oracle facts the target records.
type Claims struct {
	Participants *ParticipantClaim `json:"participants,omitempty"`
	Changes      []ChangeClaim     `json:"changes"`
}

// ParticipantClaim claims participants of the listed address kinds.
type ParticipantClaim struct {
	AddressKinds []string `json:"address_kinds"`
	MuxIDs       bool     `json:"mux_ids"`
}

// ChangeClaim claims the oracle changes of Kind that satisfy Where, compared on Fields.
type ChangeClaim struct {
	Kind   string           `json:"kind"`
	Where  map[string][]any `json:"where,omitempty"`
	Fields []string         `json:"fields"`
}

// StartResult answers start.
type StartResult struct {
	Components map[string]string `json:"components"`
}

// AdvanceResult answers advance.
type AdvanceResult struct {
	Head uint32 `json:"head"`
}

// StatusResult answers status.
type StatusResult struct {
	LastIngestedLedger *uint32 `json:"last_ingested_ledger"`
	Health             string  `json:"health"`
	LastError          *string `json:"last_error"`
	PeakMemoryBytes    *uint64 `json:"peak_memory_bytes"`
}

// ExportResult answers export after its records.
type ExportResult struct {
	Records uint64 `json:"records"`
}

// DumpTable is a table header line of a dump, sent before the table's rows.
type DumpTable struct {
	Name    string       `json:"name"`
	Columns []DumpColumn `json:"columns"`
}

// DumpColumn is one column of a dumped table, with the store's type name.
type DumpColumn struct {
	Name string `json:"name"`
	Type string `json:"type"`
}

// DumpRow is one stored row. Cells follow the table's columns: byte columns as
// lowercase hex with no prefix, other columns in the store's text form, nil
// for SQL NULL.
type DumpRow struct {
	Table string    `json:"table"`
	Cells []*string `json:"cells"`
}

// DumpResult answers dump after its table and row lines.
type DumpResult struct {
	Tables uint64 `json:"tables"`
	Rows   uint64 `json:"rows"`
}

// Dumper is implemented by handlers that answer dump. Dump streams each
// table's header through table, then its rows through row; tables lists the
// tables asked for, every table when empty.
type Dumper interface {
	Dump(tables []string, table func(DumpTable) error, row func(DumpRow) error) (DumpResult, error)
}

// RestartResult answers restart.
type RestartResult struct{}

// Restarter is implemented by handlers that answer restart: stop the target's
// ingesting process and start it again on the same store.
type Restarter interface {
	Restart() (RestartResult, error)
}

// StopResult answers stop.
type StopResult struct {
	Logs string `json:"logs"`
}

// TxRecord is a record of type transaction.
type TxRecord struct {
	Ledger       uint32        `json:"ledger"`
	Hash         string        `json:"hash"`
	Participants []Participant `json:"participants"`
	Changes      []Change      `json:"changes"`
	Unmapped     []Unmapped    `json:"unmapped"`
}

// UnattributedRecord holds stored rows that belong to no stored transaction.
type UnattributedRecord struct {
	Ledger   *uint32    `json:"ledger,omitempty"`
	Unmapped []Unmapped `json:"unmapped"`
}

// Participant is one stored participant.
type Participant struct {
	Address *string     `json:"address"`
	MuxIDs  []string    `json:"mux_ids,omitempty"`
	Raw     *RawAddress `json:"raw,omitempty"`
}

// RawAddress is an address column's stored bytes and how they decode.
type RawAddress struct {
	Column string `json:"column"`
	Hex    string `json:"hex"`
	Strkey string `json:"strkey,omitempty"`
	Error  string `json:"error,omitempty"`
}

// RowRef names the row a change came from.
type RowRef struct {
	Table    string `json:"table"`
	Category string `json:"category,omitempty"`
	Reason   string `json:"reason,omitempty"`
}

// Change is one exported vocabulary change. Fields holds the vocabulary
// fields (kind excluded); they are flattened next to kind, raw and source.
type Change struct {
	Kind   string
	Fields map[string]any
	Raw    map[string]RawAddress
	Source *RowRef
}

// MarshalJSON flattens Fields next to kind, raw and source.
func (c Change) MarshalJSON() ([]byte, error) {
	m := make(map[string]any, len(c.Fields)+3)
	maps.Copy(m, c.Fields)
	m["kind"] = c.Kind
	if len(c.Raw) > 0 {
		m["raw"] = c.Raw
	}
	if c.Source != nil {
		m["source"] = c.Source
	}
	return json.Marshal(m)
}

// Unmapped is a stored row the adapter has no vocabulary mapping for.
type Unmapped struct {
	Table    string         `json:"table"`
	Category string         `json:"category,omitempty"`
	Reason   string         `json:"reason,omitempty"`
	Row      map[string]any `json:"row"`
	Note     string         `json:"note"`
}

// Handler implements the commands. Export streams records through emit.
type Handler interface {
	Describe(protocol int) (DescribeResult, error)
	Start(p StartParams) (StartResult, error)
	Advance(head uint32) (AdvanceResult, error)
	Status() (StatusResult, error)
	Export(from, to uint32, emit func(record any) error) (ExportResult, error)
	Stop() (StopResult, error)
	// Close tears the target down when stdin ends without a stop.
	Close() error
}

type response struct {
	ID     string         `json:"id"`
	Status string         `json:"status"`
	Result any            `json:"result,omitempty"`
	Record any            `json:"record,omitempty"`
	Table  *DumpTable     `json:"table,omitempty"`
	Row    *DumpRow       `json:"row,omitempty"`
	Error  *responseError `json:"error,omitempty"`
}

type responseError struct {
	Class   ErrorClass `json:"class"`
	Message string     `json:"message"`
}

type typedRecord struct {
	Type string `json:"type"`
	TxRecord
}

type typedUnattributed struct {
	Type string `json:"type"`
	UnattributedRecord
}

// Serve reads requests from in and writes responses to out until stop or end
// of input. It returns after stop has been answered or after Close ran.
func Serve(in io.Reader, out io.Writer, h Handler) error {
	w := &lineWriter{w: bufio.NewWriter(out)}
	sc := bufio.NewScanner(in)
	// a start request carries paths and urls only, but leave room for long ones
	sc.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for sc.Scan() {
		line := sc.Bytes()
		req, perr := parseRequest(line)
		if perr != nil {
			id := ""
			if req != nil {
				id = req.ID
			}
			if err := w.write(response{ID: id, Status: "error", Error: &responseError{Class: ClassBadRequest, Message: perr.Error()}}); err != nil {
				return err
			}
			continue
		}
		done, err := dispatch(req, h, w)
		if err != nil {
			// the harness is gone or the line cannot be answered; the target must not outlive it
			return errors.Join(err, closeAfter(h))
		}
		if done {
			return nil
		}
	}
	if err := sc.Err(); err != nil {
		return errors.Join(fmt.Errorf("reading requests: %w", err), closeAfter(h))
	}
	if err := h.Close(); err != nil {
		return fmt.Errorf("tearing down after end of input: %w", err)
	}
	return nil
}

func closeAfter(h Handler) error {
	if err := h.Close(); err != nil {
		return fmt.Errorf("tearing down: %w", err)
	}
	return nil
}

func parseRequest(line []byte) (*Request, error) {
	var req Request
	if err := json.Unmarshal(line, &req); err != nil {
		return nil, fmt.Errorf("request is not a json object: %w", err)
	}
	if req.ID == "" {
		return &req, errors.New("request has no id")
	}
	switch req.Cmd {
	case "describe":
		if req.Protocol == nil {
			return &req, errors.New("describe needs protocol")
		}
	case "start":
		var p StartParams
		if err := json.Unmarshal(line, &p); err != nil {
			return &req, fmt.Errorf("start members: %w", err)
		}
		req.Start = &p
	case "advance":
		if req.Head == nil {
			return &req, errors.New("advance needs head")
		}
	case "export":
		if req.From == nil || req.To == nil {
			return &req, errors.New("export needs from and to")
		}
		if *req.From > *req.To {
			return &req, fmt.Errorf("export from %d is after to %d", *req.From, *req.To)
		}
	case "dump", "restart", "status", "stop":
	default:
		return &req, fmt.Errorf("unknown cmd %q", req.Cmd)
	}
	return &req, nil
}

func dispatch(req *Request, h Handler, w *lineWriter) (bool, error) {
	var (
		result any
		err    error
		done   bool
	)
	switch req.Cmd {
	case "describe":
		result, err = h.Describe(*req.Protocol)
	case "start":
		result, err = h.Start(*req.Start)
	case "advance":
		result, err = h.Advance(*req.Head)
	case "status":
		result, err = h.Status()
	case "export":
		emit := func(rec any) error {
			switch r := rec.(type) {
			case TxRecord:
				return w.write(response{ID: req.ID, Status: "record", Record: typedRecord{Type: "transaction", TxRecord: r}})
			case UnattributedRecord:
				return w.write(response{ID: req.ID, Status: "record", Record: typedUnattributed{Type: "unattributed", UnattributedRecord: r}})
			default:
				return fmt.Errorf("export emitted %T, not a record", rec)
			}
		}
		result, err = h.Export(*req.From, *req.To, emit)
	case "dump":
		d, ok := h.(Dumper)
		if !ok {
			err = Errorf(ClassUnsupported, "this adapter does not answer dump")
			break
		}
		result, err = d.Dump(req.Tables,
			func(t DumpTable) error {
				return w.write(response{ID: req.ID, Status: "table", Table: &t})
			},
			func(r DumpRow) error {
				return w.write(response{ID: req.ID, Status: "row", Row: &r})
			})
	case "restart":
		r, ok := h.(Restarter)
		if !ok {
			err = Errorf(ClassUnsupported, "this adapter does not answer restart")
			break
		}
		result, err = r.Restart()
	case "stop":
		result, err = h.Stop()
		done = true
	}
	if err != nil {
		class, msg := ClassInternal, err.Error()
		if we, ok := errors.AsType[*Error](err); ok {
			class, msg = we.Class, we.Message
		}
		return done, w.write(response{ID: req.ID, Status: "error", Error: &responseError{Class: class, Message: msg}})
	}
	return done, w.write(response{ID: req.ID, Status: "ok", Result: result})
}

type lineWriter struct {
	mu sync.Mutex
	w  *bufio.Writer
}

func (l *lineWriter) write(v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return fmt.Errorf("encoding response: %w", err)
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if _, err := l.w.Write(append(b, '\n')); err != nil {
		return fmt.Errorf("writing response: %w", err)
	}
	if err := l.w.Flush(); err != nil {
		return fmt.Errorf("writing response: %w", err)
	}
	return nil
}
