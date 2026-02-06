#!/usr/bin/env bash
set -euo pipefail

echo "Comedi installation helper"
echo
echo "Select your OS:"
echo "1) Arch"
echo "2) Debian/Ubuntu"
echo "3) Fedora"
echo "4) Quit"
read -r -p "Choice [1-4]: " choice

case "$choice" in
1)
    sudo pacman -S --needed base-devel git linux-headers kmod libusb
    mkdir -p "$HOME/pkg/comedi"
    cd "$HOME/pkg/comedi"
    cat <<'EOF' >PKGBUILD
pkgname=comedi
pkgver=0.8.0_git
pkgrel=1
pkgdesc="Control and Measurement Device Interface (COMEDI)"
arch=('x86_64')
license=('GPL')
depends=('glibc' 'libusb')
makedepends=('git' 'linux-headers' 'autoconf' 'automake' 'libtool')
options=('!lto')
provides=('comedi' 'comedilib')
conflicts=('comedi' 'comedilib')

source=(
  'comedi::git+https://github.com/Linux-Comedi/comedi.git'
  'comedilib::git+https://github.com/Linux-Comedi/comedilib.git'
)
sha256sums=('SKIP' 'SKIP')

build() {
  # Kernel + drivers
  cd "$srcdir/comedi"
  ./autogen.sh
  ./configure --prefix=/usr --sbindir=/usr/bin
  make

  # Userspace library
  cd "$srcdir/comedilib"
  ./autogen.sh
  ./configure --prefix=/usr --sbindir=/usr/bin
  make
}

package() {
  cd "$srcdir/comedi"
  make DESTDIR="$pkgdir" install
  # Move kernel modules to /usr/lib/modules to avoid /lib symlink conflicts on Arch
  if [ -d "$pkgdir/lib/modules" ]; then
    mkdir -p "$pkgdir/usr/lib"
    if [ -d "$pkgdir/usr/lib/modules" ]; then
      cp -a "$pkgdir/lib/modules/." "$pkgdir/usr/lib/modules/"
      rm -rf "$pkgdir/lib/modules"
    else
      mv "$pkgdir/lib/modules" "$pkgdir/usr/lib/modules"
    fi
  fi
  # Avoid packaging host-owned paths/symlinks and kernel metadata files
  rm -rf "$pkgdir/lib"
  if [ -d "$pkgdir/usr/sbin" ]; then
    mkdir -p "$pkgdir/usr/bin"
    cp -a "$pkgdir/usr/sbin/." "$pkgdir/usr/bin/" || true
    rm -rf "$pkgdir/usr/sbin"
  fi
  find "$pkgdir/usr/lib/modules" -maxdepth 2 -type f -name 'modules.*' -delete 2>/dev/null || true

  cd "$srcdir/comedilib"
  # comedilib git tree can miss prebuilt HTML docs; ignore doc install errors
  make -i DESTDIR="$pkgdir" install
}
EOF

    rm -f comedi-*.pkg.tar.* comedi-debug-*.pkg.tar.*
    makepkg -si --noconfirm --cleanbuild
    sudo depmod -a
    sudo modprobe comedi
    cd ..
    rm -rf "$HOME/pkg"
    ;;
2)
    echo "Debian/Ubuntu install method:"
    echo "Installing DKMS from source (kernel driver)"
    sudo apt update
    sudo apt install -y dkms git
    if [ ! -d "$HOME/pkg/comedi" ]; then
        mkdir -p "$HOME/pkg"
        git clone https://github.com/Linux-Comedi/comedi.git "$HOME/pkg/comedi"
    fi
    cd "$HOME/pkg/comedi"
    ./autogen.sh
    ./configure.sh
    cd ..
    sudo dkms add ./comedi
    sudo depmod -a
    sudo apt install -y libcomedi0 libcomedi-dev
    sudo modprobe comedi
    ;;
3)
    sudo dnf install -y comedilib comedi
    sudo modprobe comedi
    ;;
4)
    echo "Aborted."
    exit 0
    ;;
*)
    echo "Invalid choice."
    exit 1
    ;;
esac

echo
echo "COMEDI core loaded. You may need to load your board driver, for example:"
echo "  sudo modprobe ni_usb6501"
echo "  sudo modprobe ni_pcimio"
