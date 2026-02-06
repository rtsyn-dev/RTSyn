use rtsyn_plugin::{PluginApi, PluginString};
use serde_json::json;
use std::ffi::c_void;

#[derive(Debug)]
struct MockOut5 {
    out: f64,
}

impl MockOut5 {
    fn new() -> Self {
        Self { out: 5.0 }
    }
}

extern "C" fn create(_id: u64) -> *mut c_void {
    let instance = Box::new(MockOut5::new());
    Box::into_raw(instance) as *mut c_void
}

extern "C" fn destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle as *mut MockOut5));
    }
}

extern "C" fn meta_json(_handle: *mut c_void) -> PluginString {
    let value = json!({
        "name": "Mock Out 5",
        "kind": "mock_out_5_rs_runtime",
        "variables": [
            { "name": "out", "default": 5.0 }
        ]
    });
    PluginString::from_string(value.to_string())
}

extern "C" fn inputs_json(_handle: *mut c_void) -> PluginString {
    PluginString::from_string("[]".to_string())
}

extern "C" fn outputs_json(_handle: *mut c_void) -> PluginString {
    // Optional, but harmless
    PluginString::from_string("[\"out\"]".to_string())
}

extern "C" fn set_config_json(_handle: *mut c_void, _data: *const u8, _len: usize) {
    // no config
}

extern "C" fn set_input(_handle: *mut c_void, _name: *const u8, _len: usize, _value: f64) {
    // no inputs
}

extern "C" fn process(handle: *mut c_void, _tick: u64, _period_seconds: f64) {
    if handle.is_null() {
        return;
    }
    // out stays constant at 5.0
    let instance = unsafe { &mut *(handle as *mut MockOut5) };
    instance.out = 5.0;
}

extern "C" fn get_output(handle: *mut c_void, name: *const u8, len: usize) -> f64 {
    if handle.is_null() || name.is_null() || len == 0 {
        return 0.0;
    }

    let slice = unsafe { std::slice::from_raw_parts(name, len) };
    let Ok(name) = std::str::from_utf8(slice) else {
        return 0.0;
    };

    let instance = unsafe { &*(handle as *mut MockOut5) };
    match name {
        "out" => instance.out,
        _ => 0.0,
    }
}

#[no_mangle]
pub extern "C" fn rtsyn_plugin_api() -> *const PluginApi {
    static API: PluginApi = PluginApi {
        create,
        destroy,
        meta_json,
        inputs_json,
        outputs_json,
        set_config_json,
        set_input,
        process,
        get_output,
        behavior_json: None,
        ui_schema_json: None,
    };
    &API as *const PluginApi
}
