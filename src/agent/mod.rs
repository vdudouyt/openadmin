//! The agentic chat loop.
//!
//! The pieces land bottom-up: the wire protocol, the client, the command
//! runner and the allowlist are all complete and tested before anything in the
//! UI can reach them. Until the worker and `App` wiring land, the compiler
//! cannot see any callers — hence the allow, which goes away with that commit.
#![allow(dead_code)]

pub mod artifacts;
pub mod client;
pub mod exec;
pub mod proto;
pub mod readonly;
