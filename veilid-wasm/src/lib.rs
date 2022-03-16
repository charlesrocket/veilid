// wasm-bindgen and clippy don't play well together yet
#![allow(clippy::all)]
#![cfg(target_arch = "wasm32")]
#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::sync::Arc;
use core::any::{Any, TypeId};
use core::cell::RefCell;
use futures_util::FutureExt;
use js_sys::*;
use lazy_static::*;
use log::*;
use send_wrapper::*;
use serde::*;
use veilid_core::xx::*;
use veilid_core::*;
use wasm_bindgen_futures::*;

// Allocator
extern crate wee_alloc;
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

static SETUP_ONCE: Once = Once::new();
pub fn setup() -> () {
    SETUP_ONCE.call_once(|| {});
}

// API Singleton

lazy_static! {
    static ref VEILID_API: SendWrapper<RefCell<Option<veilid_core::VeilidAPI>>> =
        SendWrapper::new(RefCell::new(None));
}

fn get_veilid_api() -> Result<veilid_core::VeilidAPI, veilid_core::VeilidAPIError> {
    (*VEILID_API)
        .borrow()
        .clone()
        .ok_or(veilid_core::VeilidAPIError::NotInitialized)
}

fn take_veilid_api() -> Result<veilid_core::VeilidAPI, veilid_core::VeilidAPIError> {
    (**VEILID_API)
        .take()
        .ok_or(veilid_core::VeilidAPIError::NotInitialized)
}

// JSON Marshalling

pub fn serialize_json<T: Serialize>(val: T) -> String {
    serde_json::to_string(&val).expect("failed to serialize json value")
}

pub fn deserialize_json<T: de::DeserializeOwned>(
    arg: &str,
) -> Result<T, veilid_core::VeilidAPIError> {
    serde_json::from_str(arg).map_err(|e| veilid_core::VeilidAPIError::ParseError {
        message: e.to_string(),
        value: String::new(),
    })
}

pub fn to_json<T: Serialize>(val: T) -> JsValue {
    JsValue::from_str(&serialize_json(val))
}

pub fn from_json<T: de::DeserializeOwned>(val: JsValue) -> Result<T, veilid_core::VeilidAPIError> {
    let s = val
        .as_string()
        .ok_or_else(|| veilid_core::VeilidAPIError::ParseError {
            message: "Value is not String".to_owned(),
            value: String::new(),
        })?;
    deserialize_json(&s)
}

// Utility types for async API results
type APIResult<T> = Result<T, veilid_core::VeilidAPIError>;
const APIRESULT_UNDEFINED: APIResult<()> = APIResult::Ok(());

pub fn wrap_api_future<F, T>(future: F) -> Promise
where
    F: Future<Output = APIResult<T>> + 'static,
    T: Serialize + 'static,
{
    future_to_promise(future.map(|res| {
        res.map(|v| {
            if TypeId::of::<()>() == v.type_id() {
                JsValue::UNDEFINED
            } else {
                to_json(v)
            }
        })
        .map_err(|e| to_json(e))
    }))
}

// WASM Bindings

#[wasm_bindgen()]
pub fn initialize_veilid_wasm() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen()]
pub fn startup_veilid_core(update_callback: Function, json_config: String) -> Promise {
    wrap_api_future(async move {
        let update_callback = Arc::new(move |update: VeilidUpdate| {
            let _ret =
                match Function::call1(&update_callback, &JsValue::UNDEFINED, &to_json(update)) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("calling update callback failed: {:?}", e);
                        return;
                    }
                };
        });

        if VEILID_API.borrow().is_some() {
            return Err(veilid_core::VeilidAPIError::AlreadyInitialized);
        }

        let veilid_api = veilid_core::api_startup_json(update_callback, json_config).await?;
        VEILID_API.replace(Some(veilid_api));
        APIRESULT_UNDEFINED
    })
}

#[wasm_bindgen()]
pub fn get_veilid_state() -> Promise {
    wrap_api_future(async move {
        let veilid_api = get_veilid_api()?;
        let core_state = veilid_api.get_state().await?;
        Ok(core_state)
    })
}

#[wasm_bindgen(js_namespace = veilid)]
pub fn change_log_level(log_level: String) -> Promise {
    wrap_api_future(async move {
        let veilid_api = get_veilid_api()?;
        let log_level: veilid_core::VeilidConfigLogLevel = deserialize_json(&log_level)?;
        veilid_api.change_log_level(log_level).await;
        APIRESULT_UNDEFINED
    })
}

#[wasm_bindgen()]
pub fn shutdown_veilid_core() -> Promise {
    wrap_api_future(async move {
        let veilid_api = take_veilid_api()?;
        veilid_api.shutdown().await;
        APIRESULT_UNDEFINED
    })
}

#[wasm_bindgen()]
pub fn debug(command: String) -> Promise {
    wrap_api_future(async move {
        let veilid_api = get_veilid_api()?;
        let out = veilid_api.debug(command).await?;
        Ok(out)
    })
}

#[wasm_bindgen()]
pub fn veilid_version_string() -> String {
    veilid_core::veilid_version_string()
}

#[derive(Serialize)]
pub struct VeilidVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[wasm_bindgen()]
pub fn veilid_version() -> JsValue {
    let (major, minor, patch) = veilid_core::veilid_version();
    let vv = VeilidVersion {
        major,
        minor,
        patch,
    };
    JsValue::from_serde(&vv).unwrap()
}
