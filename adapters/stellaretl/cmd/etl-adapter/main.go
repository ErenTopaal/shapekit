// Command etl-adapter is the shapekit adapter for stellar/stellar-etl. It
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
	"github.com/ErenTopaal/shapekit/adapters/stellaretl"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "etl-adapter: %v\n", err)
		os.Exit(1)
	}
}

func run() error {
	commit := flag.String("commit", "", "full stellar-etl commit hash to build and run")
	repo := flag.String("repo", "", "stellar-etl git clone (default: <user cache dir>/shapekit/stellar-etl, cloned on first use)")
	cpus := flag.String("cpus", "", "docker --cpus limit of every container the adapter starts (default: none)")
	builder := flag.String("builder", "", "buildx builder to build the image with (default: the current builder)")
	flag.Parse()
	if *repo == "" {
		cache, err := os.UserCacheDir()
		if err != nil {
			return fmt.Errorf("no --repo and no user cache directory: %w", err)
		}
		*repo = filepath.Join(cache, "shapekit", "stellar-etl")
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	a, err := stellaretl.New(ctx, stellaretl.Config{Commit: *commit, RepoDir: *repo, CPUs: *cpus, Builder: *builder})
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
