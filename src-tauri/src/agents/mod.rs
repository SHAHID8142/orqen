//! Agent registry: definitions, local CLI discovery, and controlled CLI
//! verification.
//!
//! Boundaries:
//! - `definitions` — static, provider-independent agent metadata (the only
//!   place agent-specific facts live).
//! - `discovery` — locates executables; **never executes anything**.
//! - `verify` — explicit, user-initiated, allowlisted version probe.
//!
//! Execution adapters and orchestration are future features; the
//! capability metadata exists so the UI can distinguish what is real.

pub mod definitions;
pub mod discovery;
pub mod execution;
pub mod runner;
pub mod verify;
