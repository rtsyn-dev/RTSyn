use rtsyn_plugin::{
    DeviceDriver, Plugin, PluginContext, PluginError, PluginId, PluginMeta, Port, PortId,
};
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
    pub struct comedi_range {
        pub min: c_double,
        pub max: c_double,
        pub unit: c_uint,
    }

    pub type LsamplT = c_uint;

    pub const SUBD_AI: c_int = 1;
    pub const SUBD_AO: c_int = 2;

    #[link(name = "comedi")]
    extern "C" {
        pub fn comedi_open(fn_ptr: *const c_char) -> *mut comedi_t;
        pub fn comedi_close(dev: *mut comedi_t) -> c_int;
        pub fn comedi_errno() -> c_int;
        pub fn comedi_strerror(errnum: c_int) -> *const c_char;

        pub fn comedi_get_n_subdevices(dev: *mut comedi_t) -> c_int;
        pub fn comedi_get_subdevice_type(dev: *mut comedi_t, subdevice: c_uint) -> c_int;
        pub fn comedi_get_n_channels(dev: *mut comedi_t, subdevice: c_uint) -> c_int;

        pub fn comedi_get_range(
            dev: *mut comedi_t,
            subdevice: c_uint,
            chan: c_uint,
            range: c_uint,
        ) -> *mut comedi_range;
        pub fn comedi_get_maxdata(dev: *mut comedi_t, subdevice: c_uint, chan: c_uint) -> LsamplT;

        pub fn comedi_to_phys(data: LsamplT, rng: *const comedi_range, maxdata: LsamplT)
            -> c_double;
        pub fn comedi_from_phys(
            data: c_double,
            rng: *const comedi_range,
            maxdata: LsamplT,
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

    pub unsafe fn get_range(dev: *mut comedi_t, subd: u32, chan: u32) -> Result<comedi_range, String> {
        let ptr = comedi_get_range(dev, subd as c_uint, chan as c_uint, 0);
        if ptr.is_null() {
            Err(last_error())
        } else {
            Ok(*ptr)
        }
    }

    pub unsafe fn get_maxdata(dev: *mut comedi_t, subd: u32, chan: u32) -> Result<LsamplT, String> {
        let val = comedi_get_maxdata(dev, subd as c_uint, chan as c_uint);
        if val == 0 {
            let err = comedi_errno();
            if err != 0 {
                Err(last_error())
            } else {
                Ok(val)
            }
        } else {
            Ok(val)
        }
    }

    pub unsafe fn to_phys(data: LsamplT, range: &comedi_range, maxdata: LsamplT) -> f64 {
        comedi_to_phys(data, range as *const comedi_range, maxdata) as f64
    }

    pub unsafe fn from_phys(data: f64, range: &comedi_range, maxdata: LsamplT) -> LsamplT {
        comedi_from_phys(data, range as *const comedi_range, maxdata)
    }

    pub unsafe fn read(dev: *mut comedi_t, subd: u32, chan: u32) -> Result<LsamplT, String> {
        let mut data: LsamplT = 0;
        let res = comedi_data_read(dev, subd as c_uint, chan as c_uint, 0, 0, &mut data);
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
        data: LsamplT,
    ) -> Result<(), String> {
        let res = comedi_data_write(dev, subd as c_uint, chan as c_uint, 0, 0, data);
        if res < 0 {
            Err(last_error())
        } else {
            Ok(())
        }
    }
}

pub struct ComediDaqPlugin {
    id: PluginId,
    meta: PluginMeta,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    input_port_names: Vec<String>,
    output_port_names: Vec<String>,

    device_path: String,
    ai_channels: Vec<(u32, u32)>,
    ao_channels: Vec<(u32, u32)>,

    input_values: HashMap<String, f64>,
    output_values: HashMap<String, f64>,

    is_open: bool,
    last_scan_devices: bool,
    last_scan_nonce: u64,
    active_inputs: Vec<bool>,
    active_outputs: Vec<bool>,
    dev: Option<std::ptr::NonNull<comedilib::comedi_t>>,
}

unsafe impl Send for ComediDaqPlugin {}

impl ComediDaqPlugin {
    pub fn new(id: u64) -> Self {
        let mut plugin = Self {
            id: PluginId(id),
            meta: PluginMeta {
                name: "Comedi DAQ Device Driver".to_string(),
                fixed_vars: vec![],
                default_vars: vec![
                    ("device_path".to_string(), Value::from("/dev/comedi0")),
                    ("scan_devices".to_string(), Value::from(false)),
                    ("scan_nonce".to_string(), Value::from(0_u64)),
                ],
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            input_port_names: Vec::new(),
            output_port_names: Vec::new(),
            device_path: "/dev/comedi0".to_string(),
            ai_channels: Vec::new(),
            ao_channels: Vec::new(),
            input_values: HashMap::new(),
            output_values: HashMap::new(),
            is_open: false,
            last_scan_devices: false,
            last_scan_nonce: 0,
            active_inputs: Vec::new(),
            active_outputs: Vec::new(),
            dev: None,
        };

        plugin.auto_configure();
        plugin
    }

    pub fn set_config(&mut self, device_path: String, scan_devices: bool, scan_nonce: u64) {
        let changed = self.device_path != device_path;
        if changed {
            self.device_path = device_path;
        }
        if changed || (scan_devices && !self.last_scan_devices) || scan_nonce != self.last_scan_nonce
        {
            self.auto_configure();
        }
        self.last_scan_devices = scan_devices;
        self.last_scan_nonce = scan_nonce;
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
            self.active_inputs.resize(self.input_port_names.len(), false);
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

        for (sd, ch) in &self.ao_channels {
            let name = format!("ao{sd}_{ch}");
            self.inputs.push(Port {
                id: PortId(name.clone()),
            });
            self.input_port_names.push(name);
            self.active_inputs.push(false);
        }

        for (sd, ch) in &self.ai_channels {
            let name = format!("ai{sd}_{ch}");
            self.outputs.push(Port {
                id: PortId(name.clone()),
            });
            self.output_port_names.push(name);
            self.active_outputs.push(false);
        }
    }

    fn mock_default_channels(&mut self) {
        if self.ai_channels.is_empty() {
            self.ai_channels = vec![(0, 0), (0, 1)];
        }
        if self.ao_channels.is_empty() {
            self.ao_channels = vec![(1, 0)];
        }
    }

    fn auto_configure(&mut self) {
        let Ok(dev) = (unsafe { comedilib::open(&self.device_path) }) else {
            self.mock_default_channels();
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

        self.ai_channels = ai;
        self.ao_channels = ao;
        if self.ai_channels.is_empty() && self.ao_channels.is_empty() {
            self.mock_default_channels();
        }
        self.update_ports();
        unsafe { comedilib::close(dev) };
    }

    fn comedi_error<E: std::fmt::Display>(_err: E) -> PluginError {
        PluginError::ProcessingFailed
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
            if !self.active_inputs.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let port = format!("ao{sd}_{ch}");
            if let Some(v) = self.input_values.get(&port) {
                let range = unsafe { comedilib::get_range(dev, *sd, *ch) }
                    .map_err(Self::comedi_error)?;
                let max = unsafe { comedilib::get_maxdata(dev, *sd, *ch) }
                    .map_err(Self::comedi_error)?;
                let raw = unsafe { comedilib::from_phys(*v, &range, max) };
                unsafe { comedilib::write(dev, *sd, *ch, raw) }
                    .map_err(Self::comedi_error)?;
            }
        }

        for (idx, (sd, ch)) in self.ai_channels.iter().enumerate() {
            if !self.active_outputs.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let raw = unsafe { comedilib::read(dev, *sd, *ch) }.map_err(Self::comedi_error)?;
            let range = unsafe { comedilib::get_range(dev, *sd, *ch) }
                .map_err(Self::comedi_error)?;
            let max = unsafe { comedilib::get_maxdata(dev, *sd, *ch) }
                .map_err(Self::comedi_error)?;
            let phys = unsafe { comedilib::to_phys(raw, &range, max) };

            let port = format!("ai{sd}_{ch}");
            self.output_values.insert(port, phys);
        }

        Ok(())
    }

}

impl DeviceDriver for ComediDaqPlugin {
    fn open(&mut self) -> Result<(), PluginError> {
        let dev = unsafe { comedilib::open(&self.device_path) }.map_err(Self::comedi_error)?;
        self.dev = std::ptr::NonNull::new(dev);
        self.is_open = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), PluginError> {
        if let Some(dev) = self.dev.take() {
            unsafe { comedilib::close(dev.as_ptr()) };
        }
        self.is_open = false;
        Ok(())
    }

}
