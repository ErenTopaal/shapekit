// Package protocol holds the data layouts that shapekit adapters share with
// the indexers they drive.
package protocol

import (
	"errors"
	"fmt"
	"math"
)

// BatchExtension is the file suffix of a SEP-54 batch object. SEP-54 only
// defines zstd compression, so this is the only suffix in use.
const BatchExtension = ".xdr.zst"

// Layout is the SEP-54 key layout of a ledger data store, as recorded in the
// ledgersPerBatch and batchesPerPartition fields of its .config.json.
//
// Use NewLayout to build one. The zero value has no batch size and every
// method on it panics.
type Layout struct {
	ledgersPerBatch     uint32
	batchesPerPartition uint32
}

// NewLayout checks a ledgersPerBatch and batchesPerPartition pair and returns
// the layout for it.
func NewLayout(ledgersPerBatch, batchesPerPartition uint32) (Layout, error) {
	if ledgersPerBatch == 0 {
		return Layout{}, errors.New("sep-54 layout: ledgersPerBatch must be at least 1")
	}

	// go-stellar-sdk treats 0 like 1, but sep-54 has no meaning for an empty
	// partition, so a config that says 0 is refused instead of guessed at.
	if batchesPerPartition == 0 {
		return Layout{}, errors.New("sep-54 layout: batchesPerPartition must be at least 1")
	}

	// go-stellar-sdk multiplies these as uint32, so a larger partition wraps
	// there and keys would no longer match what galexie writes.
	if batchesPerPartition > 1 && uint64(ledgersPerBatch)*uint64(batchesPerPartition) > math.MaxUint32 {
		return Layout{}, fmt.Errorf(
			"sep-54 layout: partition of %d batches of %d ledgers exceeds %d ledgers",
			batchesPerPartition, ledgersPerBatch, uint64(math.MaxUint32),
		)
	}

	return Layout{ledgersPerBatch: ledgersPerBatch, batchesPerPartition: batchesPerPartition}, nil
}

// LedgersPerBatch returns the number of ledgers in each batch object.
func (l Layout) LedgersPerBatch() uint32 { return l.ledgersPerBatch }

// BatchesPerPartition returns the number of batches in each partition.
func (l Layout) BatchesPerPartition() uint32 { return l.batchesPerPartition }

// Partitioned reports whether keys carry a partition directory. SEP-54 drops
// that level when the partition size is 1.
func (l Layout) Partitioned() bool { return l.batchesPerPartition > 1 }

// BatchRange returns the first and last ledger of the batch holding seq.
func (l Layout) BatchRange(seq uint32) (uint32, uint32) {
	return span(seq, l.ledgersPerBatch)
}

// PartitionRange returns the first and last ledger of the partition holding
// seq. Without partitions it is the batch range.
func (l Layout) PartitionRange(seq uint32) (uint32, uint32) {
	// NewLayout made sure this product fits a uint32
	return span(seq, l.ledgersPerBatch*l.batchesPerPartition)
}

// PartitionName returns the partition directory name for seq, without a
// trailing slash. It returns false when the layout has no partitions.
func (l Layout) PartitionName(seq uint32) (string, bool) {
	if !l.Partitioned() {
		return "", false
	}

	start, end := l.PartitionRange(seq)

	return fmt.Sprintf("%08X--%d-%d", math.MaxUint32-start, start, end), true
}

// BatchFileName returns the batch object name for seq.
//
// SEP-54 uses the short single-ledger form when the batch size is 1.
// go-stellar-sdk uses it whenever the batch starts and ends on the same
// ledger, which also happens for the last batch below MaxUint32 when it is
// cut short. That is what galexie writes, so the sdk rule is followed.
func (l Layout) BatchFileName(seq uint32) string {
	start, end := l.BatchRange(seq)
	if start == end {
		return fmt.Sprintf("%08X--%d%s", math.MaxUint32-start, start, BatchExtension)
	}

	return fmt.Sprintf("%08X--%d-%d%s", math.MaxUint32-start, start, end, BatchExtension)
}

// ObjectKey returns the key of the batch holding seq, relative to the data
// store's ledgers path.
func (l Layout) ObjectKey(seq uint32) string {
	partition, ok := l.PartitionName(seq)
	if !ok {
		return l.BatchFileName(seq)
	}

	return partition + "/" + l.BatchFileName(seq)
}

// span aligns seq down to a multiple of size and caps the end at MaxUint32,
// since the last span of the ledger range can be short.
func span(seq, size uint32) (uint32, uint32) {
	start := seq - seq%size
	if size-1 > math.MaxUint32-start {
		return start, math.MaxUint32
	}

	return start, start + size - 1
}
