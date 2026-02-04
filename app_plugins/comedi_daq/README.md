# Comedi DAQ Plugin

## Overview

This plugin provides a real-time interface to Comedi-supported DAQ devices. It discovers available analog input/output channels at plugin load and on explicit rescan requests.

By default it builds with the `comedi` feature enabled. Disable it or enable `mock` to run without hardware.

## Install Comedi

### Arch

```bash
sudo pacman -S comedilib comedi
```

- `comedilib`: userspace API (`libcomedi`, headers, tools)
- `comedi`: kernel drivers (`comedi.ko`, board drivers)

Load COMEDI core

```bash
sudo modprobe comedi
```

Load a board driver (example)

```bash
sudo modprobe ni_usb6501
# or e.g.
sudo modprobe ni_pcimio
```

### Debian/Ubuntu

```bash
sudo apt update
sudo apt install comedi-utils libcomedi-dev
```

Load COMEDI core

```bash
sudo modprobe comedi
```

Load a board driver (example)

```bash
sudo modprobe ni_usb6501
# or e.g.
sudo modprobe ni_pcimio
```

### Fedora

```bash
sudo dnf install comedilib comedi
```

Load COMEDI core

```bash
sudo modprobe comedi
```

Load a board driver (example)

```bash
sudo modprobe ni_usb6501
# or e.g.
sudo modprobe ni_pcimio
```
