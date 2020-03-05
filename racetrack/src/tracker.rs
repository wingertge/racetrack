use std::{
    any::Any,
    collections::HashMap,
    sync::Arc
};
use parking_lot::{Mutex, RwLock};

/// Stores call info for the method call.
/// This is usually constructed via the proc-macro, but can be done manually.
#[derive(Debug)]
pub struct CallInfo {
    /// The boxed arguments as a tuple
    pub arguments: Option<Box<dyn Any + Send + Sync>>,
    /// The boxed return value
    pub returned: Option<Box<dyn Any + Send + Sync>>
}

type Calls = Arc<RwLock<Vec<CallInfo>>>;

/// The main tracker class.
/// Construct this in each test if possible, otherwise use a static copy.
/// Any assertions will start with this tracker.
///
/// # Constraints
///
/// * All arguments and return types must implement `ToOwned` to allow the function to be tracked.
///
/// # Example
///
/// ```
/// # use std::sync::Arc;
/// use racetrack::{Tracker, track_with};
///
/// let tracker = Tracker::new();
///
/// struct Tracked(Arc<Tracker>);
///
/// #[track_with(0)]
/// impl Tracked {
///     fn tracked_method(&self, arg: String) {}
/// }
///
/// let tracked = Tracked(tracker.clone());
/// tracked.tracked_method("Test".to_string());
///
/// tracker
///     .assert_that("Tracked::tracked_method")
///     .was_called_once()
///     .with("Test".to_string());
/// ```
///
#[derive(Debug)]
pub struct Tracker {
    calls: Arc<Mutex<HashMap<String, Calls>>>
}

impl Tracker {
    /// Construct a new tracker. This returns an Arc since the library expects one everywhere.
    /// This allows for use of the tracker in multi-threaded/tasked scenarios.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: Arc::new(Mutex::new(HashMap::new()))
        })
    }

    /// Start an assertion chain.
    /// # Arguments
    ///
    /// * `item` - The key of the method for which assertions should be made. e.g. "Tracked::tracked_method"
    pub fn assert_that(&self, item: impl Into<String>) -> Assertion {
        let key = item.into();
        let calls = self.calls.lock();
        let item = if let Some(calls) = calls.get(&key) {
            calls.clone()
        } else {
            Arc::new(RwLock::new(Vec::new()))
        };
        Assertion { item, key }
    }

    /// Log a call to the tracker.
    /// This is usually used by the proc macro but can be called manually if the macro doesn't work for your use case.
    ///
    /// # Arguments
    ///
    /// * `key` - The key for the method. e.g. Tracked::tracked_method
    /// * `call_info` - The call info for the call. May or may not contain arguments and return values.
    pub fn log_call(&self, key: impl Into<String>, call_info: CallInfo) {
        let key = key.into();
        let mut calls = self.calls.lock();
        if let Some(call_infos) = calls.get(&key) {
            let mut call_infos = call_infos.write();
            call_infos.push(call_info);
        } else {
            calls.insert(key, Arc::new(RwLock::new(vec![call_info])));
        }
    }

    /// Clear the tracker completely
    pub fn clear(&self) {
        self.calls.lock().clear();
    }

    /// Print the call info for a specific method. To print the whole tracker, use debug format.
    pub fn print_debug(&self, item: impl Into<String>) {
        let key = item.into();
        let calls = self.calls.lock();
        if let Some(calls) = calls.get(&key) {
            println!("{:?}", calls);
        }
    }
}

/// An assertion object
pub struct Assertion {
    item: Calls,
    key: String
}

impl Assertion {
    /// Require that the method was called exactly once.
    /// Returns an object that lets you assert more detailed metadata.
    pub fn was_called_once(self) -> MetaAssertion {
        {
            let item = self.item.read();
            assert_ne!(item.len(), 0, "{} wasn't called.", self.key);
            assert_eq!(
                item.len(),
                1,
                "{} was called more than once. Was called {} times.",
                self.key,
                item.len()
            );
        }
        MetaAssertion {
            item: self.item,
            key: self.key
        }
    }

    /// Require that the method was called exactly `n` times.
    /// Returns an object that lets you assert more detailed metadata.
    pub fn was_called_times(self, n: usize) -> MetaAssertion {
        {
            let item = self.item.read();
            assert_ne!(
                item.len(),
                0,
                "{} should've been called {} times, but wasn't called.",
                self.key,
                n
            );
            assert!(
                item.len() >= n,
                "{} was called fewer than {} times. Was called {} times.",
                self.key,
                n,
                item.len()
            );
            assert_eq!(
                item.len(),
                n,
                "{} was called more than {} times. Was called {} times.",
                self.key,
                n,
                item.len()
            );
        }
        MetaAssertion {
            item: self.item,
            key: self.key
        }
    }

    /// Require that the method wasn't called. Ends the assertion chain.
    pub fn wasnt_called(self) {
        let item = self.item.read();
        let len = item.len();
        assert_eq!(
            len, 0,
            "{} should not have been called but was called {} times.",
            self.key, len
        );
    }
}

/// A meta assertion object for asserting additional metadata
pub struct MetaAssertion {
    item: Calls,
    key: String
}

impl MetaAssertion {
    /// Require that the method was called at least once with `args`.
    /// T must be a tuple of arguments.
    ///
    /// # Warning
    ///
    /// The argument type must be whatever gets returned by `to_owned`. Usually this is the original type, but things like `&str` become `String`.
    pub fn with<T: PartialEq + 'static>(self, args: T) -> Self {
        {
            let item = self.item.read();
            assert!(item.len() > 0, "{} wasn't called.", self.key);
            assert!(
                item.iter().any(|call_info| {
                    let call_args = call_info.arguments.as_ref().expect(&format!(
                        "You didn't log any arguments for your calls to {}.",
                        self.key
                    ));
                    let cast = call_args.downcast_ref::<T>().expect(&format!(
                        "The arguments logged for {} didn't have that type.",
                        self.key
                    ));
                    cast == &args
                }),
                "{} wasn't called with the arguments specified.",
                self.key
            );
        }
        self
    }

    /// Require that the method was not ever called with `args`.
    /// T must be a tuple of arguments.
    ///
    /// # Warning
    ///
    /// The argument type must be whatever gets returned by `to_owned`. Usually this is the original type, but things like `&str` become `String`.
    pub fn not_with<T: PartialEq + 'static>(self, args: T) -> Self {
        {
            let item = self.item.read();
            if item.len() > 0 {
                assert!(
                    !item.iter().any(|call_info| {
                        let call_args = call_info.arguments.as_ref().expect(&format!(
                            "You didn't log any arguments for your calls to {}.",
                            self.key
                        ));
                        let cast = call_args.downcast_ref::<T>().expect(&format!(
                            "The arguments logged for {} didn't have that type.",
                            self.key
                        ));
                        cast == &args
                    }),
                    "{} was called with the argument when it should'nt have been.",
                    self.key
                );
            }
        }
        self
    }

    /// Require that the method returned `value` at least once.
    /// T must be the return type.
    ///
    /// # Warning
    ///
    /// The return type must be whatever gets returned by `to_owned`. Usually this is the original type, but things like `&str` become `String`.
    pub fn and_returned<T: PartialEq + 'static>(self, value: T) {
        let item = self.item.read();
        assert!(item.len() > 0, "{} wasn't called.", self.key);
        assert!(
            item.iter().any(|call_info| {
                let call_return = call_info.returned.as_ref().expect(&format!(
                    "You didn't log any arguments for your calls to {}.",
                    self.key
                ));
                let cast = call_return.downcast_ref::<T>().expect(&format!(
                    "The arguments logged for {} didn't have that type.",
                    self.key
                ));
                cast == &value
            }),
            "{} wasn't called with the arguments specified.",
            self.key
        );
    }
}
