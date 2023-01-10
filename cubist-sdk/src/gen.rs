pub mod backend;
pub mod common;
/// The module for analyzing contracts and generating interfaces
pub mod interface;
/// The name of the shim method that adds the sender to approved callers
pub const APPROVE_CALLER_METHOD_NAME: &str = "approveCaller";
