//! A library for writing assertions on methods, function and closure calls.
//!
//! Racetrack allows for tracking direct and indirect calls to methods. It's inspired by Jest's `fn()` and `spyOn`.
//! The library consists of the tracker, which handles assertions, as well as a proc-macro that allows for automatic tracking injection into methods.
//!
//! # Usage
//!
//! The intended usage is with the proc macro.
//!
//! ```
//! # use std::sync::Arc;
//! use racetrack::{Tracker, track_with};
//!
//! struct TrackedStruct(Arc<Tracker>);
//!
//! #[track_with(0)]
//! impl TrackedStruct {
//!     fn tracked_fn(&self, arg: String) {}
//! }
//!
//! let tracker = Tracker::new();
//! let tracked = TrackedStruct(tracker.clone());
//! tracked.tracked_fn("Test".to_string());
//!
//! tracker
//!     .assert_that("TrackedStruct::tracked_fn")
//!     .was_called_once()
//!     .with(("Test".to_string()));
//! ```
//!
//! However, this has some caviats. All arguments and the return type must implement `ToOwned` and
//! it may not work if you have very specific requirements.
//! So, alternatively, you can use the tracker manually:
//!
//! ```
//! # use std::sync::Arc;
//! use racetrack::{Tracker, CallInfo};
//!
//! struct TrackedStruct(Arc<Tracker>);
//!
//! impl TrackedStruct {
//!     fn tracked_fn(&self, arg: String) {
//!         let call_info = CallInfo {
//!             arguments: Some(Box::new(arg)),
//!             returned: None
//!         };
//!         self.0.log_call("my_fn", call_info);
//!     }
//! }
//!
//! let tracker = Tracker::new();
//! let tracked = TrackedStruct(tracker.clone());
//! tracked.tracked_fn("Test".to_string());
//!
//! tracker
//!     .assert_that("my_fn")
//!     .was_called_once()
//!     .with("Test".to_string());
//! ```

pub mod tracker;

pub use tracker::{Tracker, CallInfo};
pub use racetrack_proc_macro::track_with;