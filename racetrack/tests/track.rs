#![allow(unused)]
#![cfg_attr(feature = "nightly", feature(proc_macro_hygiene))]

use racetrack::Tracker;
use racetrack_proc_macro::track_with;
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref TRACKER: Arc<Tracker> = Tracker::new();
}

#[track_with(TRACKER)]
fn tracked_fn(arg: String) -> String {
    arg.to_lowercase()
}

#[derive(Clone)]
struct TrackedStruct {
    tracker: Arc<Tracker>
}

#[track_with(tracker, exclude = "new, untracked_method")]
impl TrackedStruct {
    #[track_with(TRACKER, namespace = "TrackedStruct")]
    pub fn new(tracker: Arc<Tracker>) -> Self {
        Self { tracker }
    }

    pub fn tracked_method(&self, arg: String) {}

    pub fn untracked_method(&self) {}
}

#[derive(Clone)]
struct StaticTrackedStruct;

#[track_with(TRACKER, include_receiver = false)]
impl StaticTrackedStruct {
    fn new() -> Self {
        StaticTrackedStruct
    }

    fn tracked_method(&self, arg: String) {}
}

#[test]
fn test_track_fn() {
    tracked_fn("TEST".to_string());
    TRACKER
        .assert_that("tracked_fn")
        .was_called_once()
        .with(("TEST".to_string()))
        .and_returned("test".to_string());
}

#[test]
fn test_track_struct() {
    let tracker = Tracker::new();

    let tracked = TrackedStruct::new(tracker.clone());
    tracked.tracked_method("test".to_string());
    tracked.untracked_method();

    tracker.assert_that("TrackedStruct::new").wasnt_called();

    tracker
        .assert_that("TrackedStruct::tracked_method")
        .was_called_once()
        .with(("test".to_string()))
        .and_returned(());

    tracker
        .assert_that("TrackedStruct::untracked_method")
        .wasnt_called();

    TRACKER.assert_that("TrackedStruct::new").was_called_once();
}

#[test]
fn test_track_static_struct() {
    let tracked = StaticTrackedStruct::new();
    tracked.tracked_method("test".to_string());

    TRACKER
        .assert_that("StaticTrackedStruct::new")
        .was_called_once();

    TRACKER
        .assert_that("StaticTrackedStruct::tracked_method")
        .was_called_once()
        .with(("test".to_string()))
        .and_returned(());
}

#[cfg_attr(feature = "nightly", test)]
#[cfg(feature = "nightly")]
fn test_track_closure() {
    let tracker = Tracker::new();

    #[track_with(tracker)]
    let closure = |arg: String| -> String { arg.to_lowercase() };

    closure("TEST".to_string());

    tracker
        .assert_that("closure")
        .was_called_once()
        .with(("TEST".to_string()))
        .and_returned("test".to_string());
}

#[test]
fn test_regression1() {
    #[track_with(TRACKER)]
    fn update(data: String, store: String) {}
}

#[test]
fn test_regression2() {
    let tracker = Tracker::new();

    struct TrackedTupleStruct(Arc<Tracker>);
    #[track_with(0)]
    impl TrackedTupleStruct {
        fn tracked_method(&self, arg: String) {}
    }

    let tracked = TrackedTupleStruct(tracker.clone());
    tracked.tracked_method("Test".to_string());

    tracker
        .assert_that("TrackedTupleStruct::tracked_method")
        .was_called_once()
        .with(("Test".to_owned()));
}
