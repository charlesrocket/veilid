// mod bump_port;
mod eventual;
mod eventual_base;
mod eventual_value;
mod eventual_value_clone;
mod ip_addr_port;
mod ip_extra;
mod single_future;
mod single_shot_eventual;
mod split_url;
mod tick_task;
mod tools;

pub use cfg_if::*;
pub use log::*;
pub use parking_lot::*;
pub use split_url::*;
pub use static_assertions::*;

pub type PinBox<T> = Pin<Box<T>>;
pub type PinBoxFuture<T> = PinBox<dyn Future<Output = T> + 'static>;
pub type PinBoxFutureLifetime<'a, T> = PinBox<dyn Future<Output = T> + 'a>;
pub type SendPinBoxFuture<T> = PinBox<dyn Future<Output = T> + Send + 'static>;
pub type SendPinBoxFutureLifetime<'a, T> = PinBox<dyn Future<Output = T> + Send + 'a>;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        extern crate alloc;
        pub use alloc::string::String;
        pub use alloc::vec::Vec;
        pub use alloc::collections::btree_map::BTreeMap;
        pub use alloc::collections::btree_set::BTreeSet;
        pub use alloc::boxed::Box;
        pub use alloc::borrow::{Cow, ToOwned};
        pub use wasm_bindgen::prelude::*;
        pub use core::cmp;
        pub use core::mem;
        pub use alloc::rc::Rc;
        pub use core::cell::RefCell;
        pub use core::task;
        pub use core::future::Future;
        pub use core::pin::Pin;
        pub use core::sync::atomic::{Ordering, AtomicBool};
        pub use alloc::sync::{Arc, Weak};
        pub use core::ops::{FnOnce, FnMut, Fn};
        pub use no_std_net::{ SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs, IpAddr, Ipv4Addr, Ipv6Addr };
        pub type SystemPinBoxFuture<T> = PinBox<dyn Future<Output = T> + 'static>;
        pub type SystemPinBoxFutureLifetime<'a, T> = PinBox<dyn Future<Output = T> + 'a>;
    } else {
        pub use std::string::String;
        pub use std::vec::Vec;
        pub use std::collections::btree_map::BTreeMap;
        pub use std::collections::btree_set::BTreeSet;
        pub use std::boxed::Box;
        pub use std::borrow::{Cow, ToOwned};
        pub use std::cmp;
        pub use std::mem;
        pub use std::sync::atomic::{Ordering, AtomicBool};
        pub use std::sync::{Arc, Weak};
        pub use std::rc::Rc;
        pub use std::cell::RefCell;
        pub use std::task;
        pub use std::ops::{FnOnce, FnMut, Fn};
        pub use async_std::future::Future;
        pub use async_std::pin::Pin;
        pub use std::net::{ SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs, IpAddr, Ipv4Addr, Ipv6Addr };
        pub type SystemPinBoxFuture<T> = PinBox<dyn Future<Output = T> + Send + 'static>;
        pub type SystemPinBoxFutureLifetime<'a, T> = PinBox<dyn Future<Output = T> + Send + 'a>;
    }
}

// pub use bump_port::*;
pub use eventual::*;
pub use eventual_base::{EventualCommon, EventualResolvedFuture};
pub use eventual_value::*;
pub use eventual_value_clone::*;
pub use ip_addr_port::*;
pub use ip_extra::*;
pub use single_future::*;
pub use single_shot_eventual::*;
pub use tick_task::*;
pub use tools::*;
