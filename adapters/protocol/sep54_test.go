package protocol

import (
	"fmt"
	"math"
	"testing"
)

// expected keys are worked out by hand from the sep-54 formulas, so a change
// to the formatting code cannot quietly update its own expectations.
func TestObjectKey(t *testing.T) {
	cases := []struct {
		ledgersPerBatch     uint32
		batchesPerPartition uint32
		seq                 uint32
		partition           string
		batch               string
	}{
		// one ledger per file, no partitions
		{1, 1, 2, "", "FFFFFFFD--2.xdr.zst"},
		{1, 1, 3, "", "FFFFFFFC--3.xdr.zst"},
		{1, 1, 63, "", "FFFFFFC0--63.xdr.zst"},
		{1, 1, 64, "", "FFFFFFBF--64.xdr.zst"},
		{1, 1, 64000, "", "FFFF05FF--64000.xdr.zst"},

		// quickstart's galexie layout: one ledger per file, 64000 files per partition
		{1, 64000, 2, "FFFFFFFF--0-63999", "FFFFFFFD--2.xdr.zst"},
		{1, 64000, 3, "FFFFFFFF--0-63999", "FFFFFFFC--3.xdr.zst"},
		{1, 64000, 63, "FFFFFFFF--0-63999", "FFFFFFC0--63.xdr.zst"},
		{1, 64000, 64, "FFFFFFFF--0-63999", "FFFFFFBF--64.xdr.zst"},
		{1, 64000, 64000, "FFFF05FF--64000-127999", "FFFF05FF--64000.xdr.zst"},

		// 64 ledgers per file, no partitions
		{64, 1, 2, "", "FFFFFFFF--0-63.xdr.zst"},
		{64, 1, 3, "", "FFFFFFFF--0-63.xdr.zst"},
		{64, 1, 63, "", "FFFFFFFF--0-63.xdr.zst"},
		{64, 1, 64, "", "FFFFFFBF--64-127.xdr.zst"},
		{64, 1, 64000, "", "FFFF05FF--64000-64063.xdr.zst"},

		// 64 ledgers per file, 8 files per partition
		{64, 8, 2, "FFFFFFFF--0-511", "FFFFFFFF--0-63.xdr.zst"},
		{64, 8, 3, "FFFFFFFF--0-511", "FFFFFFFF--0-63.xdr.zst"},
		{64, 8, 63, "FFFFFFFF--0-511", "FFFFFFFF--0-63.xdr.zst"},
		{64, 8, 64, "FFFFFFFF--0-511", "FFFFFFBF--64-127.xdr.zst"},
		{64, 8, 64000, "FFFF05FF--64000-64511", "FFFF05FF--64000-64063.xdr.zst"},

		// the example data store in sep-54 (2 ledgers per batch, 8 per partition)
		{2, 8, 2, "FFFFFFFF--0-15", "FFFFFFFD--2-3.xdr.zst"},
		{2, 8, 3, "FFFFFFFF--0-15", "FFFFFFFD--2-3.xdr.zst"},
		{2, 8, 18, "FFFFFFEF--16-31", "FFFFFFED--18-19.xdr.zst"},

		// vectors from go-stellar-sdk v0.8.0 support/datastore/schema_test.go
		{100, 2, 10, "FFFFFFFF--0-199", "FFFFFFFF--0-99.xdr.zst"},
		{50, 2, 150, "FFFFFF9B--100-199", "FFFFFF69--150-199.xdr.zst"},
		{200, 2, 300, "FFFFFFFF--0-399", "FFFFFF37--200-399.xdr.zst"},
		{1, 2, 1, "FFFFFFFF--0-1", "FFFFFFFE--1.xdr.zst"},
		{50, 4, 250, "FFFFFF37--200-399", "FFFFFF05--250-299.xdr.zst"},
		{200, 1, 300, "", "FFFFFF37--200-399.xdr.zst"},
		{100, 200, math.MaxUint32, "00001C7F--4294960000-4294967295", "0000005F--4294967200-4294967295.xdr.zst"},
		{1, 200, math.MaxUint32, "0000005F--4294967200-4294967295", "00000000--4294967295.xdr.zst"},
		{200, 1, math.MaxUint32, "", "0000005F--4294967200-4294967295.xdr.zst"},
		{1, 1, math.MaxUint32, "", "00000000--4294967295.xdr.zst"},

		// MaxUint32 is divisible by 3, so the last batch holds one ledger and
		// takes the short name even though the batch size is 3
		{3, 1, math.MaxUint32, "", "00000000--4294967295.xdr.zst"},
		{3, 1, math.MaxUint32 - 1, "", "00000003--4294967292-4294967294.xdr.zst"},
	}

	for _, tc := range cases {
		name := fmt.Sprintf("lpb=%d,bpp=%d,seq=%d", tc.ledgersPerBatch, tc.batchesPerPartition, tc.seq)
		t.Run(name, func(t *testing.T) {
			l, err := NewLayout(tc.ledgersPerBatch, tc.batchesPerPartition)
			if err != nil {
				t.Fatalf("NewLayout: %v", err)
			}

			partition, ok := l.PartitionName(tc.seq)
			if ok != (tc.partition != "") || partition != tc.partition {
				t.Errorf("PartitionName = %q, %v; want %q", partition, ok, tc.partition)
			}

			if got := l.BatchFileName(tc.seq); got != tc.batch {
				t.Errorf("BatchFileName = %q, want %q", got, tc.batch)
			}

			want := tc.batch
			if tc.partition != "" {
				want = tc.partition + "/" + tc.batch
			}

			if got := l.ObjectKey(tc.seq); got != want {
				t.Errorf("ObjectKey = %q, want %q", got, want)
			}
		})
	}
}

func TestRanges(t *testing.T) {
	l, err := NewLayout(64, 8)
	if err != nil {
		t.Fatal(err)
	}

	if s, e := l.BatchRange(64000); s != 64000 || e != 64063 {
		t.Errorf("BatchRange(64000) = %d-%d, want 64000-64063", s, e)
	}

	if s, e := l.PartitionRange(64000); s != 64000 || e != 64511 {
		t.Errorf("PartitionRange(64000) = %d-%d, want 64000-64511", s, e)
	}

	if s, e := l.PartitionRange(math.MaxUint32); s != 4294966784 || e != math.MaxUint32 {
		t.Errorf("PartitionRange(MaxUint32) = %d-%d, want 4294966784-4294967295", s, e)
	}
}

func TestNewLayoutRejects(t *testing.T) {
	cases := []struct {
		ledgersPerBatch     uint32
		batchesPerPartition uint32
	}{
		{0, 1},
		{0, 64000},
		{1, 0},
		{64, 0},
		{65536, 65536},
		{math.MaxUint32, 2},
	}

	for _, tc := range cases {
		if _, err := NewLayout(tc.ledgersPerBatch, tc.batchesPerPartition); err == nil {
			t.Errorf("NewLayout(%d, %d) returned no error", tc.ledgersPerBatch, tc.batchesPerPartition)
		}
	}
}

func TestNewLayoutAcceptsLargest(t *testing.T) {
	// no partition level means the partition size never has to fit a uint32
	if _, err := NewLayout(math.MaxUint32, 1); err != nil {
		t.Errorf("NewLayout(MaxUint32, 1): %v", err)
	}

	// 65535 * 65537 is exactly MaxUint32
	if _, err := NewLayout(65535, 65537); err != nil {
		t.Errorf("NewLayout(65535, 65537): %v", err)
	}
}

// keys sort newest first because the hex prefix counts down from MaxUint32.
func TestKeysSortDescending(t *testing.T) {
	l, err := NewLayout(1, 64000)
	if err != nil {
		t.Fatal(err)
	}

	prev := l.ObjectKey(0)
	for _, seq := range []uint32{1, 2, 63, 64, 63999, 64000, 64001, 1 << 20, math.MaxUint32} {
		cur := l.ObjectKey(seq)
		if cur >= prev {
			t.Errorf("key for %d (%s) does not sort before the previous key (%s)", seq, cur, prev)
		}

		prev = cur
	}
}
