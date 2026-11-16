package main

// tables_gen.go and tables_gen_<version>.go hold the xdr type and enum tables, generated from
// the xdr schema, the corpus manifest and the sdk source of every version in versions/. every
// schema type the linked sdk version has decodes; the corpus's types and TransactionMeta (the
// converted core test dump, `shapekit conformance run --dataset`) must exist in every version.
//go:generate go run ./internal/gentypes -manifest ../../corpus/manifest.json -schema ../../schema/xdr/v30.0/xdr.json -versions versions -out tables_gen.go -extra TransactionMeta
