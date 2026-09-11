#![allow(dead_code)]

//! # Shared Test Utilities & Mock Servers
//!
//! This module contains reusable mock servers, network dummies, and connection
//! primitives utilized across the integration test suite.
//!
//! **Why this resides in a subdirectory with a `mod.rs`:**
//! Cargo treats every top-level file inside the `tests/` directory as a standalone
//! test crate. Defining mock server logic directly in a top-level `tests/` file would
//! make it a test target in its own right rather than a shared module.
//!
//! Since #245 the suite is a single test target rooted at `tests/integration/main.rs`,
//! so this is one module of that binary and is compiled exactly once -- it is no longer
//! recompiled per test suite, because there is only one. Reach these helpers as
//! `crate::common::...` from the sibling test modules; a bare `common::...` path only
//! resolves from a crate root, which these modules no longer are.

pub mod client;
pub mod io;
pub mod mock_camera;
pub mod mock_ftps;
pub mod mock_mqtt;
