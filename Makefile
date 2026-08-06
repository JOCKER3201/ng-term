# ng-term build system.
#
#   make install       — clean build and install to ~/.local/
#   sudo make install  — clean build and install to /usr/local/
#
# Every install: removes the old build, builds, installs, removes the build.
# The prefix can be overridden: make install PREFIX=/opt/ng-term

ifeq ($(shell id -u),0)
PREFIX ?= /usr/local
else
PREFIX ?= $(HOME)/.local
endif

BINDIR     = $(DESTDIR)$(PREFIX)/bin
FONTDIR    = $(DESTDIR)$(PREFIX)/share/fonts/ng-term
APPDIR     = $(DESTDIR)$(PREFIX)/share/applications
ICONDIR    = $(DESTDIR)$(PREFIX)/share/icons/hicolor
ICON_SIZES = 48 64 128 256 512

.PHONY: all build install uninstall clean

all: build

build:
	cargo build --release

install:
	rm -rf target
	cargo build --release
	install -Dm755 target/release/ng-term "$(BINDIR)/ng-term"
	@# Fonts from ./fonts (optional) — into $(PREFIX)/share/fonts/ng-term,
	@# where the program's font lookup will find them.
	@found=0; \
	for f in fonts/*.ttf fonts/*.otf; do \
		[ -f "$$f" ] || continue; \
		if [ $$found -eq 0 ]; then mkdir -p "$(FONTDIR)"; found=1; fi; \
		install -m644 "$$f" "$(FONTDIR)/"; \
		echo "installed font: $$f"; \
	done; true
	@# Icons (hicolor) + .desktop file with the binary path substituted.
	@for s in $(ICON_SIZES); do \
		install -Dm644 "assets/ng-term-$$s.png" \
			"$(ICONDIR)/$${s}x$${s}/apps/ng-term.png"; \
	done
	@mkdir -p "$(APPDIR)"
	sed "s|@BINDIR@|$(PREFIX)/bin|" assets/ng-term.desktop.in \
		> "$(APPDIR)/ng-term.desktop"
	@chmod 644 "$(APPDIR)/ng-term.desktop"
	-@update-desktop-database "$(APPDIR)" 2>/dev/null || true
	-@gtk-update-icon-cache -f "$(ICONDIR)" 2>/dev/null || true
	rm -rf target
	@echo "ng-term installed at $(BINDIR)/ng-term"

uninstall:
	rm -f "$(BINDIR)/ng-term"
	rm -rf "$(FONTDIR)"
	rm -f "$(APPDIR)/ng-term.desktop"
	@for s in $(ICON_SIZES); do \
		rm -f "$(ICONDIR)/$${s}x$${s}/apps/ng-term.png"; \
	done
	-@update-desktop-database "$(APPDIR)" 2>/dev/null || true
	@echo "ng-term uninstalled from $(PREFIX)"

clean:
	rm -rf target
