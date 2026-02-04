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
        cat <<'EOF' > PKGBUILD
pkgname=comedi
pkgver=0.8.0_git
pkgrel=1
pkgdesc="Control and Measurement Device Interface (COMEDI)"
arch=('x86_64')
license=('GPL')
depends=('glibc' 'libusb')
makedepends=('git' 'linux-headers')
provides=('comedi' 'comedilib')
conflicts=('comedi' 'comedilib')
source=(
  'comedi::git+https://github.com/Linux-Comedi/comedi.git'
  'comedilib::git+https://github.com/Linux-Comedi/comedilib.git'
)
sha256sums=('SKIP' 'SKIP')

build() {
  cd "$srcdir/comedi"
  make
  cd "$srcdir/comedilib"
  ./autogen.sh
  ./configure --prefix=/usr
  make
}

package() {
  cd "$srcdir/comedi"
  make DESTDIR="$pkgdir" install

  cd "$srcdir/comedilib"
  make DESTDIR="$pkgdir" install
}
EOF

        makepkg -si --noconfirm
        sudo depmod -a
        sudo modprobe comedi
        cd ..
        rm -rf "$HOME/pkg"
        ;;
    2)
        echo "Debian/Ubuntu install method:"
        echo "1) Packages (comedi-utils, libcomedi-dev)"
        echo "2) DKMS from source (kernel driver)"
        read -r -p "Choice [1-2]: " deb_choice
        case "$deb_choice" in
            1)
                sudo apt update
                sudo apt install -y comedi-utils libcomedi-dev
                sudo modprobe comedi
                ;;
            2)
                sudo apt update
                sudo apt install -y dkms git
                if [ ! -d "$HOME/pkg/comedi" ]; then
                    mkdir -p "$HOME/pkg"
                    git clone https://github.com/Linux-Comedi/comedi.git "$HOME/pkg/comedi"
                fi
                cd "$HOME/pkg/comedi"
                ./autogen.sh
                cd ..
                sudo dkms add ./comedi
                sudo depmod -a
                sudo apt install -y libcomedi0 libcomedi-dev
                sudo modprobe comedi
                ;;
            *)
                echo "Invalid choice."
                exit 1
                ;;
        esac
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
