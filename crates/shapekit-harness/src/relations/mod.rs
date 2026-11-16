//! Checks that need no mapping from the target's tables to the oracle vocabulary. Each one
//! reads what the target stored (the adapter's `dump`) next to what the chain gave it.

pub mod carry;
pub mod known;
pub mod replay;
pub mod route;
pub mod state;
