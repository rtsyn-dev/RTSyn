use rtsyn_plugin::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

mod comedilib {
    use libc::{c_char, c_double, c_int, c_uint};
    use std::ffi::{CStr, CString};

    #[repr(C)]
    pub struct comedi_t {
        _private: [u8; 0],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct comedi_polynomial_t {
        pub coefficients: [c_double; 4],
        pub expansion_origin: c_double,
        pub order: c_uint,
    }

    #[repr(C)]
    pub struct comedi_calibration_t {
        _private: [u8; 0],
    }

    pub type LsamplT = c_uint;

    pub const AREF_GROUND: c_uint = 0;
    pub const AREF_OTHER: c_uint = 3;

    pub const SUBD_AI: c_int = 1;
    pub const SUBD_AO: c_int = 2;
    pub const TO_PHYSICAL: c_int = 0;
    pub const FROM_PHYSICAL: c_int = 1;

    #[link(name = "comedi")]
    extern "C" {
        pub fn comedi_open(fn_ptr: *const c_char) -> *mut comedi_t;
        pub fn comedi_close(dev: *mut comedi_t) -> c_int;
        pub fn comedi_errno() -> c_int;
        pub fn comedi_strerror(errnum: c_int) -> *const c_char;

        pub fn comedi_get_n_subdevices(dev: *mut comedi_t) -> c_int;
        pub fn comedi_get_subdevice_type(dev: *mut comedi_t, subdevice: c_uint) -> c_int;
        pub fn comedi_get_n_channels(dev: *mut comedi_t, subdevice: c_uint) -> c_int;

        pub fn comedi_get_default_calibration_path(dev: *mut comedi_t) -> *mut c_char;
        pub fn comedi_parse_calibration_file(path: *const c_char) -> *mut comedi_calibration_t;
        pub fn comedi_cleanup_calibration(calibration: *mut comedi_calibration_t);
        pub fn comedi_get_softcal_converter(
            subdevice: c_uint,
            channel: c_uint,
            range: c_uint,
            direction: c_int,
            calibration: *const comedi_calibration_t,
            polynomial: *mut comedi_polynomial_t,
        ) -> c_int;
        pub fn comedi_to_physical(
            data: LsamplT,
            polynomial: *const comedi_polynomial_t,
        ) -> c_double;
        pub fn comedi_from_physical(
            data: c_double,
            polynomial: *const comedi_polynomial_t,
        ) -> LsamplT;

        pub fn comedi_data_read(
            dev: *mut comedi_t,
            subd: c_uint,
            chan: c_uint,
            range: c_uint,
            aref: c_uint,
            data: *mut LsamplT,
        ) -> c_int;
        pub fn comedi_data_write(
            dev: *mut comedi_t,
            subd: c_uint,
            chan: c_uint,
            range: c_uint,
            aref: c_uint,
            data: LsamplT,
        ) -> c_int;
    }

    fn last_error() -> String {
        unsafe {
            let err = comedi_errno();
            let msg = comedi_strerror(err);
            if msg.is_null() {
                format!("comedi error {err}")
            } else {
                CStr::from_ptr(msg).to_string_lossy().to_string()
            }
        }
    }

    pub unsafe fn open(path: &str) -> Result<*mut comedi_t, String> {
        let cpath = CString::new(path).map_err(|_| "invalid device path".to_string())?;
        let dev = comedi_open(cpath.as_ptr());
        if dev.is_null() {
            Err(last_error())
        } else {
            Ok(dev)
        }
    }

    pub unsafe fn close(dev: *mut comedi_t) {
        let _ = comedi_close(dev);
    }

    pub unsafe fn get_default_calibration_path(dev: *mut comedi_t) -> Result<String, String> {
        let path = comedi_get_default_calibration_path(dev);
        if path.is_null() {
            Err(last_error())
        } else {
            Ok(CStr::from_ptr(path).to_string_lossy().to_string())
        }
    }

    pub unsafe fn parse_calibration_file(path: &str) -> Result<*mut comedi_calibration_t, String> {
        let cpath = CString::new(path).map_err(|_| "invalid calibration path".to_string())?;
        let calibration = comedi_parse_calibration_file(cpath.as_ptr());
        if calibration.is_null() {
            Err(last_error())
        } else {
            Ok(calibration)
        }
    }

    pub unsafe fn cleanup_calibration(calibration: *mut comedi_calibration_t) {
        comedi_cleanup_calibration(calibration);
    }

    pub unsafe fn get_n_subdevices(dev: *mut comedi_t) -> Result<u32, String> {
        let n = comedi_get_n_subdevices(dev);
        if n < 0 {
            Err(last_error())
        } else {
            Ok(n as u32)
        }
    }

    pub unsafe fn get_subdevice_type(dev: *mut comedi_t, subd: u32) -> Result<i32, String> {
        let t = comedi_get_subdevice_type(dev, subd as c_uint);
        if t < 0 {
            Err(last_error())
        } else {
            Ok(t)
        }
    }

    pub unsafe fn get_n_channels(dev: *mut comedi_t, subd: u32) -> Result<u32, String> {
        let n = comedi_get_n_channels(dev, subd as c_uint);
        if n < 0 {
            Err(last_error())
        } else {
            Ok(n as u32)
        }
    }

    pub unsafe fn get_softcal_converter(
        subd: u32,
        chan: u32,
        range: u32,
        direction: c_int,
        calibration: *const comedi_calibration_t,
    ) -> Result<comedi_polynomial_t, String> {
        let mut polynomial = comedi_polynomial_t {
            coefficients: [0.0; 4],
            expansion_origin: 0.0,
            order: 0,
        };
        let res = comedi_get_softcal_converter(
            subd as c_uint,
            chan as c_uint,
            range as c_uint,
            direction,
            calibration,
            &mut polynomial,
        );
        if res < 0 {
            Err(last_error())
        } else {
            Ok(polynomial)
        }
    }

    pub unsafe fn to_physical(data: LsamplT, polynomial: &comedi_polynomial_t) -> f64 {
        comedi_to_physical(data, polynomial as *const comedi_polynomial_t)
    }

    pub unsafe fn from_physical(data: f64, polynomial: &comedi_polynomial_t) -> LsamplT {
        comedi_from_physical(data, polynomial as *const comedi_polynomial_t)
    }

    pub unsafe fn read(
        dev: *mut comedi_t,
        subd: u32,
        chan: u32,
        range: u32,
        aref: u32,
    ) -> Result<LsamplT, String> {
        let mut data: LsamplT = 0;
        let res = comedi_data_read(
            dev,
            subd as c_uint,
            chan as c_uint,
            range as c_uint,
            aref as c_uint,
            &mut data,
        );
        if res < 0 {
            Err(last_error())
        } else {
            Ok(data)
        }
    }

    pub unsafe fn write(
        dev: *mut comedi_t,
        subd: u32,
        chan: u32,
        range: u32,
        aref: u32,
        data: LsamplT,
    ) -> Result<(), String> {
        let res = comedi_data_write(
            dev,
            subd as c_uint,
            chan as c_uint,
            range as c_uint,
            aref as c_uint,
            data,
        );
        if res < 0 {
            Err(last_error())
        } else {
            Ok(())
        }
    }
}

#[derive(Copy, Clone)]
struct ChannelCalibration {
    to_converter: Option<comedilib::comedi_polynomial_t>,
    from_converter: Option<comedilib::comedi_polynomial_t>,
}

pub struct ComediDaqPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    input_port_names: Vec<String>,
    output_port_names: Vec<String>,

    device_path: String,
    ai_range_index: u32,
    ao_range_index: u32,
    ai_aref: u32,
    ao_aref: u32,
    ai_channels: Vec<(u32, u32)>,
    ao_channels: Vec<(u32, u32)>,

    input_values: HashMap<String, f64>,
    output_values: HashMap<String, f64>,

    is_open: bool,
    last_scan_devices: bool,
    last_scan_nonce: u64,
    active_inputs: Vec<bool>,
    active_outputs: Vec<bool>,
    ao_port_names: Vec<String>,
    ai_port_names: Vec<String>,
    ao_calibration: Vec<Option<ChannelCalibration>>,
    ai_calibration: Vec<Option<ChannelCalibration>>,
    no_device_detected: bool,
    dev: Option<std::ptr::NonNull<comedilib::comedi_t>>,
    calibration: Option<std::ptr::NonNull<comedilib::comedi_calibration_t>>,
}

unsafe impl Send for ComediDaqPlugin {}

impl ComediDaqPlugin {
    fn normalize_device_path(path: &str) -> &str {
        if let Some(idx) = path.find("_subd") {
            &path[..idx]
        } else {
            path
        }
    }

    pub fn new(id: u64) -> Self {
        let mut plugin = Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "Comedi DAQ Device Driver".to_string(),
                fixed_vars: vec![],
                default_vars: vec![
                    ("device_path".to_string(), Value::from("/dev/comedi0")),
                    ("ai_range_index".to_string(), Value::from(0_u64)),
                    ("ao_range_index".to_string(), Value::from(0_u64)),
                    (
                        "ai_aref".to_string(),
                        Value::from(comedilib::AREF_GROUND as u64),
                    ),
                    (
                        "ao_aref".to_string(),
                        Value::from(comedilib::AREF_GROUND as u64),
                    ),
                    ("scan_devices".to_string(), Value::from(false)),
                    ("scan_nonce".to_string(), Value::from(0_u64)),
                ],
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            input_port_names: Vec::new(),
            output_port_names: Vec::new(),
            device_path: "/dev/comedi0".to_string(),
            ai_range_index: 0,
            ao_range_index: 0,
            ai_aref: comedilib::AREF_GROUND,
            ao_aref: comedilib::AREF_GROUND,
            ai_channels: Vec::new(),
            ao_channels: Vec::new(),
            input_values: HashMap::new(),
            output_values: HashMap::new(),
            is_open: false,
            last_scan_devices: false,
            last_scan_nonce: 0,
            active_inputs: Vec::new(),
            active_outputs: Vec::new(),
            ao_port_names: Vec::new(),
            ai_port_names: Vec::new(),
            ao_calibration: Vec::new(),
            ai_calibration: Vec::new(),
            no_device_detected: false,
            dev: None,
            calibration: None,
        };

        plugin.auto_configure();
        plugin
    }

    pub fn set_config(&mut self, device_path: String, scan_devices: bool, scan_nonce: u64) {
        let changed = self.device_path != device_path;
        if changed {
            self.device_path = device_path;
        }
        if changed
            || (scan_devices && !self.last_scan_devices)
            || scan_nonce != self.last_scan_nonce
        {
            self.auto_configure();
        }
        self.last_scan_devices = scan_devices;
        self.last_scan_nonce = scan_nonce;
    }

    pub fn set_data_config(
        &mut self,
        ai_range_index: u32,
        ao_range_index: u32,
        ai_aref: u32,
        ao_aref: u32,
    ) {
        let changed = self.ai_range_index != ai_range_index
            || self.ao_range_index != ao_range_index
            || self.ai_aref != ai_aref
            || self.ao_aref != ao_aref;
        self.ai_range_index = ai_range_index;
        self.ao_range_index = ao_range_index;
        self.ai_aref = ai_aref;
        self.ao_aref = ao_aref;
        if changed && self.is_open {
            let _ = self.rebuild_calibration_cache();
        }
    }

    pub fn set_input(&mut self, port_name: &str, value: f64) {
        self.input_values.insert(port_name.to_string(), value);
    }

    pub fn get_output(&self, port_name: &str) -> f64 {
        self.output_values.get(port_name).copied().unwrap_or(0.0)
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn set_active_ports(
        &mut self,
        input_ports: &std::collections::HashSet<String>,
        output_ports: &std::collections::HashSet<String>,
    ) {
        if self.active_inputs.len() != self.input_port_names.len() {
            self.active_inputs
                .resize(self.input_port_names.len(), false);
        }
        if self.active_outputs.len() != self.output_port_names.len() {
            self.active_outputs
                .resize(self.output_port_names.len(), false);
        }
        for (idx, name) in self.input_port_names.iter().enumerate() {
            self.active_inputs[idx] = input_ports.contains(name);
        }
        for (idx, name) in self.output_port_names.iter().enumerate() {
            self.active_outputs[idx] = output_ports.contains(name);
        }
    }

    pub fn input_port_names(&self) -> &[String] {
        &self.input_port_names
    }

    pub fn output_port_names(&self) -> &[String] {
        &self.output_port_names
    }

    fn update_ports(&mut self) {
        self.inputs.clear();
        self.outputs.clear();
        self.input_port_names.clear();
        self.output_port_names.clear();
        self.active_inputs.clear();
        self.active_outputs.clear();
        self.ao_port_names.clear();
        self.ai_port_names.clear();
        self.ao_calibration.clear();
        self.ai_calibration.clear();
        self.output_values.clear();

        if self.no_device_detected {
            let name = "no device detected".to_string();
            self.outputs.push(Port {
                id: PortId(name.clone()),
            });
            self.output_port_names.push(name.clone());
            self.ai_port_names.push(name);
            self.active_outputs.push(false);
            self.ai_calibration.push(None);
            return;
        }

        for (sd, ch) in &self.ao_channels {
            let name = format!("ao{sd}_{ch}");
            self.inputs.push(Port {
                id: PortId(name.clone()),
            });
            self.input_port_names.push(name);
            self.active_inputs.push(false);
            self.ao_port_names.push(format!("ao{sd}_{ch}"));
            self.ao_calibration.push(None);
        }

        for (sd, ch) in &self.ai_channels {
            let name = format!("ai{sd}_{ch}");
            self.outputs.push(Port {
                id: PortId(name.clone()),
            });
            self.output_port_names.push(name);
            self.active_outputs.push(false);
            self.ai_port_names.push(format!("ai{sd}_{ch}"));
            self.ai_calibration.push(None);
        }
    }

    fn auto_configure(&mut self) {
        let device_path = Self::normalize_device_path(&self.device_path);
        let Ok(dev) = (unsafe { comedilib::open(device_path) }) else {
            self.no_device_detected = true;
            self.ai_channels.clear();
            self.ao_channels.clear();
            self.update_ports();
            return;
        };

        let mut ai = Vec::new();
        let mut ao = Vec::new();
        let n = unsafe { comedilib::get_n_subdevices(dev).unwrap_or(0) };
        for sd in 0..n {
            match unsafe { comedilib::get_subdevice_type(dev, sd) } {
                Ok(t) if t == comedilib::SUBD_AI => {
                    let ch = unsafe { comedilib::get_n_channels(dev, sd).unwrap_or(0) };
                    for c in 0..ch {
                        ai.push((sd, c));
                    }
                }
                Ok(t) if t == comedilib::SUBD_AO => {
                    let ch = unsafe { comedilib::get_n_channels(dev, sd).unwrap_or(0) };
                    for c in 0..ch {
                        ao.push((sd, c));
                    }
                }
                _ => {}
            }
        }

        self.no_device_detected = false;
        self.ai_channels = ai;
        self.ao_channels = ao;
        self.update_ports();
        unsafe { comedilib::close(dev) };
    }

    fn comedi_error<E: std::fmt::Display>(_err: E) -> PluginError {
        PluginError::ProcessingFailed
    }

    fn rebuild_calibration_cache(&mut self) -> Result<(), PluginError> {
        let Some(_dev) = self.dev.as_ref() else {
            if self.no_device_detected {
                if let Some(value) = self.output_values.get_mut("no device detected") {
                    *value = 1.0;
                } else {
                    self.output_values
                        .insert("no device detected".to_string(), 1.0);
                }
            }
            return Ok(());
        };
        let Some(calibration) = self.calibration.as_ref() else {
            return Err(PluginError::ProcessingFailed);
        };
        let calibration = calibration.as_ptr();
        self.ao_calibration.clear();
        self.ao_calibration.reserve(self.ao_channels.len());
        for (sd, ch) in &self.ao_channels {
            let from_converter = unsafe {
                comedilib::get_softcal_converter(
                    *sd,
                    *ch,
                    self.ao_range_index,
                    comedilib::FROM_PHYSICAL,
                    calibration,
                )
            }
            .map_err(Self::comedi_error)?;
            self.ao_calibration.push(Some(ChannelCalibration {
                to_converter: None,
                from_converter: Some(from_converter),
            }));
        }
        self.ai_calibration.clear();
        self.ai_calibration.reserve(self.ai_channels.len());
        for (sd, ch) in &self.ai_channels {
            let to_converter = unsafe {
                comedilib::get_softcal_converter(
                    *sd,
                    *ch,
                    self.ai_range_index,
                    comedilib::TO_PHYSICAL,
                    calibration,
                )
            }
            .map_err(Self::comedi_error)?;
            self.ai_calibration.push(Some(ChannelCalibration {
                to_converter: Some(to_converter),
                from_converter: None,
            }));
        }
        Ok(())
    }
}

impl Plugin for ComediDaqPlugin {
    fn id(&self) -> PluginId {
        self.id
    }

    fn meta(&self) -> &PluginMeta {
        &self.meta
    }

    fn inputs(&self) -> &[Port] {
        &self.inputs
    }

    fn outputs(&self) -> &[Port] {
        &self.outputs
    }

    fn process(&mut self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        if !self.is_open {
            return Ok(());
        }
        let Some(dev) = self.dev.as_ref() else {
            return Ok(());
        };
        let dev = dev.as_ptr();

        for (idx, (sd, ch)) in self.ao_channels.iter().enumerate() {
            let port = self
                .ao_port_names
                .get(idx)
                .map(String::as_str)
                .unwrap_or("");
            let active = self.active_inputs.get(idx).copied().unwrap_or(false);
            let value = if active {
                self.input_values.get(port).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            let Some(calibration) = self.ao_calibration.get(idx).and_then(|v| *v) else {
                continue;
            };
            let Some(from_converter) = calibration.from_converter else {
                continue;
            };
            let raw = unsafe { comedilib::from_physical(value, &from_converter) };
            unsafe { comedilib::write(dev, *sd, *ch, self.ao_range_index, self.ao_aref, raw) }
                .map_err(Self::comedi_error)?;
        }

        for (idx, (sd, ch)) in self.ai_channels.iter().enumerate() {
            if !self.active_outputs.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let raw = unsafe { comedilib::read(dev, *sd, *ch, self.ai_range_index, self.ai_aref) }
                .map_err(Self::comedi_error)?;
            let Some(calibration) = self.ai_calibration.get(idx).and_then(|v| *v) else {
                continue;
            };
            let Some(to_converter) = calibration.to_converter else {
                continue;
            };
            let phys = unsafe { comedilib::to_physical(raw, &to_converter) };

            if let Some(port) = self.ai_port_names.get(idx) {
                self.output_values.insert(port.clone(), phys);
            }
        }

        Ok(())
    }

    fn behavior(&self) -> PluginBehavior {
        PluginBehavior {
            supports_start_stop: true,
            supports_restart: true,
            supports_apply: false,
            extendable_inputs: ExtendableInputs::None,
            loads_started: false,
            external_window: false,
            starts_expanded: true,
            start_requires_connected_inputs: Vec::new(),
            start_requires_connected_outputs: Vec::new(),
        }
    }

    fn connection_behavior(&self) -> ConnectionBehavior {
        ConnectionBehavior { dependent: true }
    }

    fn ui_schema(&self) -> Option<UISchema> {
        Some(
            UISchema::new()
                .field(
                    ConfigField::text("device_path", "Device")
                        .default_value(Value::String("/dev/comedi0".to_string()))
                        .hint("Comedi device node (e.g. /dev/comedi0)"),
                )
                .field(
                    ConfigField::integer("ai_range_index", "AI range index")
                        .default_value(Value::from(0))
                        .min(0),
                )
                .field(
                    ConfigField::integer("ao_range_index", "AO range index")
                        .default_value(Value::from(0))
                        .min(0),
                )
                .field(
                    ConfigField::integer("ai_aref", "AI reference")
                        .default_value(Value::from(comedilib::AREF_GROUND))
                        .min(comedilib::AREF_GROUND as i64)
                        .max(comedilib::AREF_OTHER as i64),
                )
                .field(
                    ConfigField::integer("ao_aref", "AO reference")
                        .default_value(Value::from(comedilib::AREF_GROUND))
                        .min(comedilib::AREF_GROUND as i64)
                        .max(comedilib::AREF_OTHER as i64),
                ),
        )
    }

    fn display_schema(&self) -> Option<DisplaySchema> {
        Some(DisplaySchema {
            inputs: self.input_port_names.clone(),
            outputs: self.output_port_names.clone(),
            variables: Vec::new(),
        })
    }

    fn get_variable(&self, name: &str) -> Option<Value> {
        match name {
            "device_path" => Some(Value::String(if self.no_device_detected {
                "no device detected".to_string()
            } else {
                self.device_path.clone()
            })),
            "ai_range_index" => Some(Value::from(self.ai_range_index)),
            "ao_range_index" => Some(Value::from(self.ao_range_index)),
            "ai_aref" => Some(Value::from(self.ai_aref)),
            "ao_aref" => Some(Value::from(self.ao_aref)),
            "scan_devices" => Some(Value::Bool(self.last_scan_devices)),
            "scan_nonce" => Some(Value::from(self.last_scan_nonce)),
            _ => None,
        }
    }

    fn set_variable(&mut self, name: &str, value: Value) -> Result<(), PluginError> {
        match name {
            "device_path" => {
                if let Value::String(s) = value {
                    if self.device_path != s {
                        self.device_path = s;
                        self.auto_configure();
                    }
                }
            }
            "ai_range_index" => {
                if let Some(n) = value.as_u64() {
                    self.set_data_config(n as u32, self.ao_range_index, self.ai_aref, self.ao_aref);
                }
            }
            "ao_range_index" => {
                if let Some(n) = value.as_u64() {
                    self.set_data_config(self.ai_range_index, n as u32, self.ai_aref, self.ao_aref);
                }
            }
            "ai_aref" => {
                if let Some(n) = value.as_u64() {
                    self.set_data_config(
                        self.ai_range_index,
                        self.ao_range_index,
                        n as u32,
                        self.ao_aref,
                    );
                }
            }
            "ao_aref" => {
                if let Some(n) = value.as_u64() {
                    self.set_data_config(
                        self.ai_range_index,
                        self.ao_range_index,
                        self.ai_aref,
                        n as u32,
                    );
                }
            }
            "scan_devices" => {
                if let Value::Bool(b) = value {
                    if b && !self.last_scan_devices {
                        self.auto_configure();
                    }
                    self.last_scan_devices = b;
                }
            }
            "scan_nonce" => {
                if let Some(n) = value.as_u64() {
                    if self.last_scan_nonce != n {
                        self.last_scan_nonce = n;
                        self.auto_configure();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DeviceDriver for ComediDaqPlugin {
    fn open(&mut self) -> Result<(), PluginError> {
        let device_path = Self::normalize_device_path(&self.device_path);
        let dev = unsafe { comedilib::open(device_path) }.map_err(Self::comedi_error)?;
        let calibration_path =
            unsafe { comedilib::get_default_calibration_path(dev) }.map_err(Self::comedi_error)?;
        let calibration = unsafe { comedilib::parse_calibration_file(&calibration_path) }
            .map_err(Self::comedi_error)?;
        self.dev = std::ptr::NonNull::new(dev);
        self.calibration = std::ptr::NonNull::new(calibration);
        if let Err(err) = self.rebuild_calibration_cache() {
            if let Some(dev) = self.dev.take() {
                unsafe { comedilib::close(dev.as_ptr()) };
            }
            if let Some(calibration) = self.calibration.take() {
                unsafe { comedilib::cleanup_calibration(calibration.as_ptr()) };
            }
            self.ao_calibration.clear();
            self.ai_calibration.clear();
            self.is_open = false;
            return Err(err);
        }
        self.is_open = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), PluginError> {
        if let Some(dev) = self.dev.take() {
            unsafe { comedilib::close(dev.as_ptr()) };
        }
        if let Some(calibration) = self.calibration.take() {
            unsafe { comedilib::cleanup_calibration(calibration.as_ptr()) };
        }
        self.ao_calibration.clear();
        self.ai_calibration.clear();
        self.is_open = false;
        Ok(())
    }
}
