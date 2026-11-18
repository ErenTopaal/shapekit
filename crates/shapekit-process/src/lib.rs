//! Process-wide concerns of the `shapekit` binary. A process ended by a signal unwinds
//! nothing, so no `Drop` removes its scratch directories, docker containers or networks;
//! [`cleanup`] keeps a record of them and removes them when SIGINT, SIGTERM or SIGHUP
//! arrives. [`error_text`] prints an error and its causes with each cause once.

pub mod cleanup;
pub mod error_text;
pub mod scratch;
