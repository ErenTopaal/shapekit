//! The recipe explorer (`shapekit explore`): finds recipes nobody wrote by hand. Episodes
//! fill transactions from the XDR schema and the chain's state, run them on a standalone
//! core, learn core's answers, and keep what shows a union arm, a signature path or (on the
//! coverage twin) a core code region not seen before. What it keeps is recipe IR, which
//! `shapekit gen run-ir` and the verifier replay under either network passphrase.

pub mod action;
pub mod arms;
pub mod contract;
pub mod coverage;
pub mod directed;
pub mod episode;
pub mod explore;
pub mod feedback;
pub mod fill;
pub mod gates;
pub mod pairs;
pub mod rng;
pub mod spec;
pub mod verify;
pub mod world;
