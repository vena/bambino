#![allow(dead_code)]

//! # Shared Test Utilities & Mock Servers
//!
//! This module contains reusable mock servers, network dummies, and connection
//! primitives utilized across the integration test suite.
//!
//! **Why this resides in `tests/common/mod.rs`:**
//! Cargo treats every top-level file inside the `tests/` directory as a standalone
//! test crate. If we defined our mock server logic directly in the integration test
//! files (or as top-level files in `tests/`), Cargo would recompile the entire mock
//! framework for every single test suite, drastically slowing down compilation and
//! polluting the workspace.
//!
//! By placing these utilities inside a subdirectory with a `mod.rs`, Cargo recognizes
//! it as a standard module rather than a test crate, allowing our test suites to import
//! the shared logic efficiently.

pub mod client;
pub mod io;
pub mod mock_camera;
pub mod mock_ftps;
pub mod mock_mqtt;
