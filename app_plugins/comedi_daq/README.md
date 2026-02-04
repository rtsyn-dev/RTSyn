# Comedi DAQ Plugin

## Overview

This plugin provides a real-time interface to Comedi-supported DAQ devices. It discovers available analog input/output channels at plugin load and on explicit rescan requests.

By default it builds with the `comedi` feature enabled. Disable it or enable `mock` to run without hardware.

## Install Comedi

Use the helper script:

```bash
bash scripts/install_comedi.bash
```

It prompts for your OS and installs the appropriate packages (or DKMS source install on Debian/Ubuntu), then loads the COMEDI core. You may still need to load your board driver, for example:

```bash
sudo modprobe ni_pcimio
# or e.g.
sudo modprobe ni_usb6501
```
