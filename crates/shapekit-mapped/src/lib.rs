//! The mapped adapter: one adapter program for PostgreSQL indexers, configured by a mapping
//! file instead of code. `shapekit harness adapter --mapping <file>` speaks the adapter
//! protocol (docs/adapter-interface.md); the file format is in docs/mapping-adapter.md.

pub mod adapter;
pub mod docker;
pub mod lake;
pub mod mapping;
pub mod rules;
pub mod serve;
pub mod source;
pub mod store;
pub mod strkey;
pub mod template;
pub mod value;
