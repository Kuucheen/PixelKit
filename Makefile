APP_ID := io.github.Kuucheen.PixelKit
PREFIX ?= /usr
DESTDIR ?=
CARGO ?= cargo
PROFILE ?= release
TARGET_DIR ?= target
SYSTEMD_USER_UNIT_DIR ?= $(PREFIX)/lib/systemd/user
BINARY := $(TARGET_DIR)/$(PROFILE)/pixelkit

.PHONY: all build test check install uninstall clean dist vendor flatpak packages

all: build

build:
	$(CARGO) build --locked --profile $(PROFILE)

test:
	$(CARGO) test --locked --all-targets

check:
	$(CARGO) check --locked --all-targets

install: build
	install -Dm755 $(BINARY) $(DESTDIR)$(PREFIX)/bin/pixelkit
	install -Dm644 packaging/linux/$(APP_ID).desktop $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 packaging/linux/$(APP_ID).metainfo.xml $(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	install -Dm644 packaging/linux/$(APP_ID).png $(DESTDIR)$(PREFIX)/share/icons/hicolor/128x128/apps/$(APP_ID).png
	install -Dm644 packaging/linux/512x512/$(APP_ID).png $(DESTDIR)$(PREFIX)/share/icons/hicolor/512x512/apps/$(APP_ID).png
	install -Dm644 packaging/linux/pixelkit.service $(DESTDIR)$(SYSTEMD_USER_UNIT_DIR)/pixelkit.service
	install -Dm644 docs/pixelkit.1 $(DESTDIR)$(PREFIX)/share/man/man1/pixelkit.1
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/pixelkit/LICENSE
	install -Dm644 NOTICE $(DESTDIR)$(PREFIX)/share/doc/pixelkit/NOTICE

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/pixelkit
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop
	rm -f $(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/128x128/apps/$(APP_ID).png
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/512x512/apps/$(APP_ID).png
	rm -f $(DESTDIR)$(SYSTEMD_USER_UNIT_DIR)/pixelkit.service
	rm -f $(DESTDIR)$(PREFIX)/share/man/man1/pixelkit.1

clean:
	$(CARGO) clean

vendor:
	./scripts/vendor.sh

dist:
	./scripts/make-dist.sh

flatpak:
	flatpak-builder --force-clean build-dir packaging/flatpak/io.github.Kuucheen.PixelKit.local.yml

packages:
	./scripts/build-packages.sh $(PACKAGE_ARGS)
