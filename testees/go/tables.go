package main

// tables_gen.go holds the xdr type and enum tables, generated from the corpus manifest and
// the sdk source of every version in versions/.
//go:generate go run ./internal/gentypes -manifest ../../corpus/manifest.json -versions versions -out tables_gen.go
