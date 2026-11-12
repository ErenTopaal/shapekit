//! SDK conformance runner, testee protocol client, divergence matrix and issue drafting.
//! The protocol is docs/testee-protocol.md; testees run as child processes, so this crate is
//! unix-only (the memory limit is an rlimit set between fork and exec).

pub mod compare;
pub mod issues;
pub mod known;
pub mod matrix_md;
pub mod plan;
pub mod reproduce;
pub mod runner;
pub mod testee;
pub mod versions;
