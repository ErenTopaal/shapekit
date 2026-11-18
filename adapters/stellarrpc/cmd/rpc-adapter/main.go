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
	image := flag.String("image", "", "stellar-rpc image to run instead of the pinned one, with --version and --commit (upstream drift runs a newer release this way)")
	version := flag.String("version", "", "the version getVersionInfo must answer for --image, <release>-<commit>")
	commit := flag.String("commit", "", "the commit getVersionInfo must answer for --image")
	flag.Usage = func() {
		_, _ = fmt.Fprintf(flag.CommandLine.Output(), "usage: rpc-adapter [flags]\n\n"+
			"rpc-adapter puts stellar-rpc behind the shapekit replay harness. It speaks the adapter\n"+
			"protocol on stdin and stdout (docs/adapter-interface.md) and logs to stderr, so you\n"+
			"run it through the harness, for example:\n\n"+
			"  shapekit check indexer --adapter \"rpc-adapter\"\n\nFlags:\n")
		flag.PrintDefaults()
	}
	flag.Parse()
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	release := stellarrpc.Release{Image: *image, Version: *version, Commit: *commit}
	a, err := stellarrpc.New(ctx, stellarrpc.Config{CPUs: *cpus, EventsFrom: *from, Release: release})
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
