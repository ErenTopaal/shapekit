// Command wb-adapter is the shapekit adapter for stellar/wallet-backend. It
// speaks the adapter protocol on stdin and stdout (docs/adapter-interface.md)
// and logs to stderr.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"

	"github.com/ErenTopaal/shapekit/adapters/protocol/wire"
	"github.com/ErenTopaal/shapekit/adapters/walletbackend"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "wb-adapter: %v\n", err)
		os.Exit(1)
	}
}

func run() error {
	commit := flag.String("commit", "", "full wallet-backend commit hash to build and run (required)")
	repo := flag.String("repo", "", "wallet-backend git clone (default: <user cache dir>/shapekit/wallet-backend, cloned on first use)")
	dbMemory := flag.Uint64("db-memory", 1<<30, "memory limit of the timescaledb container in bytes")
	cpus := flag.String("cpus", "", "docker --cpus limit of every container the adapter starts (default: none)")
	builder := flag.String("builder", "", "buildx builder to build the image with (default: the current builder)")
	flag.Usage = func() {
		_, _ = fmt.Fprintf(flag.CommandLine.Output(), "usage: wb-adapter [flags]\n\n"+
			"wb-adapter puts wallet-backend behind the shapekit replay harness. It speaks the adapter\n"+
			"protocol on stdin and stdout (docs/adapter-interface.md) and logs to stderr, so you\n"+
			"run it through the harness, for example:\n\n"+
			"  shapekit check indexer --adapter \"wb-adapter --commit <commit>\"\n\nFlags:\n")
		flag.PrintDefaults()
	}
	flag.Parse()
	if *repo == "" {
		cache, err := os.UserCacheDir()
		if err != nil {
			return fmt.Errorf("no --repo and no user cache directory: %w", err)
		}
		*repo = filepath.Join(cache, "shapekit", "wallet-backend")
	}
	// a terminating signal tears the target down like end of input does
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	a, err := walletbackend.New(ctx, walletbackend.Config{Commit: *commit, RepoDir: *repo, DBMemoryBytes: *dbMemory, CPUs: *cpus, Builder: *builder})
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
