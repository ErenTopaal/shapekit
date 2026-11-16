package walletbackend

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/stellar/go-stellar-sdk/xdr"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
)

// store reads back what wallet-backend wrote.
type store struct {
	pool       *pgxpool.Pool
	passphrase string
}

func openStore(ctx context.Context, dsn, passphrase string) (*store, error) {
	cfg, err := pgxpool.ParseConfig(dsn)
	if err != nil {
		return nil, fmt.Errorf("parsing database url: %w", err)
	}
	// export holds one connection for the transaction cursor and uses another
	// for each transaction's rows
	cfg.MaxConns = 4
	pool, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		return nil, fmt.Errorf("opening database pool: %w", err)
	}
	return &store{pool: pool, passphrase: passphrase}, nil
}

func (s *store) close() { s.pool.Close() }

// cursor is wallet-backend's durable ingest cursor, or nil before the
// bootstrap has written it.
func (s *store) cursor(ctx context.Context) (*uint32, error) {
	var v string
	err := s.pool.QueryRow(ctx, `SELECT value FROM ingest_store WHERE key = 'latest_ingest_ledger'`).Scan(&v)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("reading latest_ingest_ledger: %w", err)
	}
	n, err := strconv.ParseUint(v, 10, 32)
	if err != nil {
		return nil, fmt.Errorf("latest_ingest_ledger is %q: %w", v, err)
	}
	if n == 0 {
		return nil, nil
	}
	l := uint32(n)
	return &l, nil
}

// assetIndex maps the asset contract id of every asset wallet-backend knows
// (trustline_assets, SAC rows of contract_tokens, native) to its vocabulary
// name, so token_id values on trustline rows can be named as assets.
type assetIndex struct {
	byContract map[[32]byte]string
}

func (s *store) assets(ctx context.Context) (assetIndex, error) {
	idx := assetIndex{byContract: map[[32]byte]string{}}
	native, err := xdr.MustNewNativeAsset().ContractID(s.passphrase)
	if err != nil {
		return idx, fmt.Errorf("native asset contract id: %w", err)
	}
	idx.byContract[native] = "native"
	add := func(code, issuer string) error {
		a, err := xdr.NewCreditAsset(code, issuer)
		if err != nil {
			return fmt.Errorf("asset %s:%s: %w", code, issuer, err)
		}
		id, err := a.ContractID(s.passphrase)
		if err != nil {
			return fmt.Errorf("asset contract id of %s:%s: %w", code, issuer, err)
		}
		idx.byContract[id] = code + ":" + issuer
		return nil
	}
	rows, err := s.pool.Query(ctx, `SELECT code, issuer FROM trustline_assets
		UNION SELECT code, issuer FROM contract_tokens WHERE type = 'SAC' AND code IS NOT NULL AND issuer IS NOT NULL`)
	if err != nil {
		return idx, fmt.Errorf("reading assets: %w", err)
	}
	defer rows.Close()
	for rows.Next() {
		var code, issuer string
		if err := rows.Scan(&code, &issuer); err != nil {
			return idx, fmt.Errorf("reading assets: %w", err)
		}
		if err := add(code, issuer); err != nil {
			return idx, err
		}
	}
	if err := rows.Err(); err != nil {
		return idx, fmt.Errorf("reading assets: %w", err)
	}
	return idx, nil
}

// export streams one record per stored transaction in ledgers from..to, then
// one unattributed record per ledger holding rows of no stored transaction.
func (s *store) export(ctx context.Context, from, to uint32, emit func(any) error) (uint64, error) {
	idx, err := s.assets(ctx)
	if err != nil {
		return 0, err
	}
	m := &mapper{assets: idx}
	conn, err := s.pool.Acquire(ctx)
	if err != nil {
		return 0, fmt.Errorf("acquiring a connection: %w", err)
	}
	defer conn.Release()
	txs, err := conn.Query(ctx, `SELECT to_id, hash, ledger_number FROM transactions
		WHERE ledger_number BETWEEN $1 AND $2 ORDER BY to_id`, int64(from), int64(to))
	if err != nil {
		return 0, fmt.Errorf("reading transactions: %w", err)
	}
	defer txs.Close()
	var n uint64
	for txs.Next() {
		var (
			toID   int64
			hash   []byte
			ledger int32
		)
		if err := txs.Scan(&toID, &hash, &ledger); err != nil {
			return n, fmt.Errorf("reading transactions: %w", err)
		}
		rec, err := s.transaction(ctx, m, toID, hash, uint32(ledger)) //nolint:gosec // ledger numbers are positive and below 2^31
		if err != nil {
			return n, err
		}
		if err := emit(rec); err != nil {
			return n, err
		}
		n++
	}
	if err := txs.Err(); err != nil {
		return n, fmt.Errorf("reading transactions: %w", err)
	}
	txs.Close()
	k, err := s.unattributed(ctx, from, to, emit)
	return n + k, err
}

func (s *store) transaction(ctx context.Context, m *mapper, toID int64, hash []byte, ledger uint32) (wire.TxRecord, error) {
	rec := wire.TxRecord{Ledger: ledger, Hash: hex.EncodeToString(hash), Participants: []wire.Participant{}, Changes: []wire.Change{}, Unmapped: []wire.Unmapped{}}
	prows, err := s.pool.Query(ctx, `SELECT account_id FROM transactions_accounts WHERE tx_to_id = $1 ORDER BY account_id`, toID)
	if err != nil {
		return rec, fmt.Errorf("reading participants of %d: %w", toID, err)
	}
	for prows.Next() {
		var acct []byte
		if err := prows.Scan(&acct); err != nil {
			prows.Close()
			return rec, fmt.Errorf("reading participants of %d: %w", toID, err)
		}
		ra, addr := rawAddress("account_id", acct, accountColumn)
		p := wire.Participant{Address: addr}
		if addr == nil {
			p.Raw = &ra
		}
		rec.Participants = append(rec.Participants, p)
	}
	prows.Close()
	if err := prows.Err(); err != nil {
		return rec, fmt.Errorf("reading participants of %d: %w", toID, err)
	}
	rows, err := s.rows(ctx, `SELECT * FROM state_changes WHERE to_id = $1 ORDER BY operation_id, state_change_id`, toID)
	if err != nil {
		return rec, fmt.Errorf("reading state changes of %d: %w", toID, err)
	}
	mapped := m.mapTransaction(rows)
	rec.Changes = append(rec.Changes, mapped.changes...)
	rec.Unmapped = append(rec.Unmapped, mapped.unmapped...)
	return rec, nil
}

// unattributed reports state change and participant rows whose transaction
// was not stored, per ledger.
func (s *store) unattributed(ctx context.Context, from, to uint32, emit func(any) error) (uint64, error) {
	orphans, err := s.rows(ctx, `SELECT * FROM state_changes s WHERE s.ledger_number BETWEEN $1 AND $2
		AND NOT EXISTS (SELECT 1 FROM transactions t WHERE t.to_id = s.to_id)
		ORDER BY s.to_id, s.operation_id, s.state_change_id`, int64(from), int64(to))
	if err != nil {
		return 0, fmt.Errorf("reading unattributed state changes: %w", err)
	}
	// a toid keeps the ledger in its upper 32 bits
	parts, err := s.rows(ctx, `SELECT * FROM transactions_accounts a WHERE a.tx_to_id >= $1 AND a.tx_to_id < $2
		AND NOT EXISTS (SELECT 1 FROM transactions t WHERE t.to_id = a.tx_to_id)
		ORDER BY a.tx_to_id`, int64(from)<<32, (int64(to)+1)<<32)
	if err != nil {
		return 0, fmt.Errorf("reading unattributed participants: %w", err)
	}
	byLedger := map[uint32][]wire.Unmapped{}
	var ledgers []uint32
	note := func(l uint32, u wire.Unmapped) {
		if _, ok := byLedger[l]; !ok {
			ledgers = append(ledgers, l)
		}
		byLedger[l] = append(byLedger[l], u)
	}
	for _, r := range orphans {
		l, _ := r.integer("ledger_number")
		note(uint32(l), //nolint:gosec // ledger_number is a postgres integer holding a ledger sequence
			unmappedRow(r, "state change of a transaction wallet-backend did not store"))
	}
	for _, r := range parts {
		id, _ := r.integer("tx_to_id")
		note(uint32(id>>32), //nolint:gosec // the upper half of a positive toid is its ledger
			wire.Unmapped{Table: "transactions_accounts", Row: exportRow(r), Note: "participant of a transaction wallet-backend did not store"})
	}
	slices.Sort(ledgers)
	var n uint64
	for _, l := range ledgers {
		ledger := l
		if err := emit(wire.UnattributedRecord{Ledger: &ledger, Unmapped: byLedger[l]}); err != nil {
			return n, err
		}
		n++
	}
	return n, nil
}

// rows runs a query and returns every row keyed by column name, so the
// columns a commit has are read as they are.
func (s *store) rows(ctx context.Context, sql string, args ...any) ([]row, error) {
	rs, err := s.pool.Query(ctx, sql, args...)
	if err != nil {
		return nil, err
	}
	defer rs.Close()
	fields := rs.FieldDescriptions()
	var out []row
	for rs.Next() {
		vals, err := rs.Values()
		if err != nil {
			return nil, err
		}
		r := make(row, len(vals))
		for i, f := range fields {
			r[f.Name] = vals[i]
		}
		out = append(out, r)
	}
	return out, rs.Err()
}

// jsonValue renders a decoded column value for json: bytes as hex, times in
// RFC 3339, uuids in their text form.
func jsonValue(v any) any {
	switch x := v.(type) {
	case []byte:
		return hex.EncodeToString(x)
	case time.Time:
		return x.UTC().Format(time.RFC3339Nano)
	case [16]byte:
		return fmt.Sprintf("%x-%x-%x-%x-%x", x[0:4], x[4:6], x[6:8], x[8:10], x[10:16])
	case int64:
		// toids and ids pass 2^53; keep them exact
		return strconv.FormatInt(x, 10)
	}
	return v
}

// dump streams every table of the public schema (or only those named), each
// header then its rows ordered by every column, as cell text.
func (s *store) dump(ctx context.Context, only []string, table func(wire.DumpTable) error, row func(wire.DumpRow) error) (wire.DumpResult, error) {
	var res wire.DumpResult
	names, err := s.tableNames(ctx)
	if err != nil {
		return res, err
	}
	if len(only) > 0 {
		for _, n := range only {
			if !slices.Contains(names, n) {
				return res, wire.Errorf(wire.ClassBadRequest, "no table %q in the store", n)
			}
		}
		names = only
	}
	for _, name := range names {
		cols, bytea, err := s.columns(ctx, name)
		if err != nil {
			return res, err
		}
		if err := table(wire.DumpTable{Name: name, Columns: cols}); err != nil {
			return res, err
		}
		res.Tables++
		exprs := make([]string, len(cols))
		order := make([]string, len(cols))
		for i, c := range cols {
			id := pgx.Identifier{c.Name}.Sanitize()
			if bytea[i] {
				exprs[i] = "encode(" + id + ", 'hex')"
			} else {
				exprs[i] = id + "::text"
			}
			order[i] = strconv.Itoa(i + 1)
		}
		sql := "SELECT " + strings.Join(exprs, ", ") + " FROM " + pgx.Identifier{"public", name}.Sanitize()
		if len(cols) > 0 {
			sql += " ORDER BY " + strings.Join(order, ", ")
		}
		n, err := s.dumpRows(ctx, name, sql, len(cols), row)
		res.Rows += n
		if err != nil {
			return res, err
		}
	}
	return res, nil
}

func (s *store) tableNames(ctx context.Context) ([]string, error) {
	rows, err := s.pool.Query(ctx, `SELECT table_name FROM information_schema.tables
		WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name`)
	if err != nil {
		return nil, fmt.Errorf("listing tables: %w", err)
	}
	names, err := pgx.CollectRows(rows, pgx.RowTo[string])
	if err != nil {
		return nil, fmt.Errorf("listing tables: %w", err)
	}
	return names, nil
}

// columns returns a table's columns with format_type's names, and which of
// them are bytea.
func (s *store) columns(ctx context.Context, name string) ([]wire.DumpColumn, []bool, error) {
	rows, err := s.pool.Query(ctx, `SELECT a.attname, format_type(a.atttypid, a.atttypmod), a.atttypid = 'bytea'::regtype
		FROM pg_attribute a WHERE a.attrelid = to_regclass(quote_ident('public') || '.' || quote_ident($1))
		AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum`, name)
	if err != nil {
		return nil, nil, fmt.Errorf("reading the columns of %s: %w", name, err)
	}
	defer rows.Close()
	var (
		cols  []wire.DumpColumn
		bytea []bool
	)
	for rows.Next() {
		var (
			c wire.DumpColumn
			b bool
		)
		if err := rows.Scan(&c.Name, &c.Type, &b); err != nil {
			return nil, nil, fmt.Errorf("reading the columns of %s: %w", name, err)
		}
		cols = append(cols, c)
		bytea = append(bytea, b)
	}
	if err := rows.Err(); err != nil {
		return nil, nil, fmt.Errorf("reading the columns of %s: %w", name, err)
	}
	return cols, bytea, nil
}

func (s *store) dumpRows(ctx context.Context, name, sql string, width int, row func(wire.DumpRow) error) (uint64, error) {
	rows, err := s.pool.Query(ctx, sql)
	if err != nil {
		return 0, fmt.Errorf("reading %s: %w", name, err)
	}
	defer rows.Close()
	var n uint64
	for rows.Next() {
		cells := make([]*string, width)
		dest := make([]any, width)
		for i := range cells {
			dest[i] = &cells[i]
		}
		if err := rows.Scan(dest...); err != nil {
			return n, fmt.Errorf("reading %s: %w", name, err)
		}
		if err := row(wire.DumpRow{Table: name, Cells: cells}); err != nil {
			return n, err
		}
		n++
	}
	if err := rows.Err(); err != nil {
		return n, fmt.Errorf("reading %s: %w", name, err)
	}
	return n, nil
}
