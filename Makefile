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
SOUNDDIR   = $(DESTDIR)$(PREFIX)/share/ng-term/sounds
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
	@# Sound themes from ./assets/sounds — one directory per theme.
	@# Installing into ~/.local writes straight into the user's data
	@# directory; a system install lands in /usr/..., from where the
	@# program copies them into the data directory on first run.
	@# Only files that are MISSING are installed — same rule the program
	@# applies when seeding at startup. A file already present is the
	@# user's, whatever its contents, so reinstalling never overwrites a
	@# sound they replaced; a deleted one is restored.
	@for d in assets/sounds/*/; do \
		[ -d "$$d" ] || continue; \
		name=$$(basename "$$d"); \
		mkdir -p "$(SOUNDDIR)/$$name"; \
		n=0; kept=0; \
		for f in "$$d"*; do \
			[ -f "$$f" ] || continue; \
			dest="$(SOUNDDIR)/$$name/$$(basename "$$f")"; \
			if [ -e "$$dest" ]; then kept=$$((kept+1)); continue; fi; \
			install -m644 "$$f" "$$dest"; \
			n=$$((n+1)); \
		done; \
		echo "sound theme $$name: $$n installed, $$kept kept"; \
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
	@# $(PREFIX)/share/ng-term is deliberately NOT removed: for a
	@# ~/.local install it IS the user's data directory, holding every
	@# theme they have made. Uninstalling the program must not delete it.
	rm -f "$(APPDIR)/ng-term.desktop"
	@for s in $(ICON_SIZES); do \
		rm -f "$(ICONDIR)/$${s}x$${s}/apps/ng-term.png"; \
	done
	-@update-desktop-database "$(APPDIR)" 2>/dev/null || true
	@echo "ng-term uninstalled from $(PREFIX)"

clean:
	rm -rf target
