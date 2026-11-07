//! Agent registry: definitions, local CLI discovery, and controlled CLI
//! verification.
//!
//! Boundaries:
//! - `definitions` — static, provider-independent agent metadata (the only
//!   place agent-specific facts live).
//! - `discovery` — locates executables; **never executes anything**.
//! - `verify` — explicit, user-initiated, allowlisted version probe.
//! - `eligibility` — backend-authoritative execution eligibility: a
//!   fresh backend-side verification plus the executable's observed
//!   identity, re-checked by the runner before any spawn.
//!
//! Diagnostic execution is currently disabled for every adapter (no
//! provider offers a per-invocation, verifiable filesystem read
//! confinement — D-022); the machinery stays real and tested so a
//! re-enable cannot bypass the backend policy. Orchestration is a
//! future feature.

pub mod definitions;
pub mod discovery;
pub mod eligibility;
pub mod execution;
pub mod runner;
pub mod verify;
