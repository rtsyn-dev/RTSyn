# NI DAQ Plugin

## Real-Time Optimized National Instruments DAQ Plugin

This plugin provides a real-time interface to National Instruments DAQ devices using the `ni-daqmx-sys` crate for direct C API bindings.

### Key Real-Time Features

1. **Pre-allocated Buffers**: All data buffers are allocated during configuration, not during real-time execution
2. **Persistent Task Handles**: DAQ tasks remain open between process calls to minimize setup overhead
3. **Direct C API**: Uses `ni-daqmx-sys` for minimal overhead compared to higher-level wrappers
4. **Batch Operations**: Reads/writes multiple samples per call to reduce API overhead
5. **Zero-Copy Design**: Minimizes memory allocations in the critical path

### Channel Management

The plugin automatically discovers and configures channels based on the configuration:

- **Analog Input (AI)**: Creates output ports `ai_<channel>` for reading values
- **Analog Output (AO)**: Creates input ports `ao_<channel>` for writing values  
- **Digital Input (DI)**: Creates output ports `di_<channel>` for reading digital states
- **Digital Output (DO)**: Creates input ports `do_<channel>` for writing digital states

### Configuration

- `device_name`: NI DAQ device identifier (e.g., "Dev1")
- `sample_rate`: Sampling frequency in Hz (default: 10kHz)
- `samples_per_channel`: Buffer size for batch operations (default: 1000)
- `ai_channels`: Comma-separated analog input channels (e.g., "ai0,ai1")
- `ao_channels`: Comma-separated analog output channels (e.g., "ao0,ao1")
- `di_channels`: Comma-separated digital input channels
- `do_channels`: Comma-separated digital output channels

### Real-Time Behavior

1. **Open**: Creates and starts DAQ tasks for all configured channels
2. **Process**: Performs batch read/write operations with minimal latency
3. **Close**: Cleanly stops and destroys DAQ tasks

The plugin is designed as a `DeviceDriver` interface, enabling automatic channel management when connections are added/removed in the GUI.

### Dependencies

Requires NI-DAQmx runtime to be installed on the system. The plugin uses `ni-daqmx-sys` for Rust bindings to the native C library.
