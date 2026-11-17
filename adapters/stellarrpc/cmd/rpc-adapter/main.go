// Command rpc-adapter is the shapekit adapter for stellar/stellar-rpc. It
// speaks the adapter protocol on stdin and stdout (docs/adapter-interface.md)
// and logs to stderr.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
	"github.com/ErenTopaal/shapekit/adapters/stellarrpc"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "rpc-adapter: %v\n", err)
		os.Exit(1)
	}
}

func run() error {
	cpus := flag.String("cpus", "", "docker --cpus limit of the stellar-rpc container (default: none)")
	from := flag.String("events-from", stellarrpc.FromEvents,
		"method the export reads events from: events (getEvents), transactions (getTransactions) or ledgers (getLedgers)")
	flag.Parse()
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	a, err := stellarrpc.New(ctx, stellarrpc.Config{CPUs: *cpus, EventsFrom: *from})
	if err != nil {
		return err
	}
	done := make(chan error, 1)
	go func() { done <- wire.Serve(os.Stdin, os.Stdout, a) }()
	select {
	case err := <-done:
		return err
	case <-ctx.Done():
		return a.Close()
	}
}
