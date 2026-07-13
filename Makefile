# cloud-viz dev Makefile
# `make dev`      -> auto-detect platform, run with the right setup
# `make dev-wsl`  -> WSLg wayland fix + run (force-run WSL path anywhere)

# ---- platform detection (parse time) ----
ifeq ($(OS),Windows_NT)
	PLATFORM := windows
else
	UNAME_S := $(shell uname -s)
	ifeq ($(UNAME_S),Darwin)
		PLATFORM := mac
	else ifeq ($(UNAME_S),Linux)
		ifneq (,$(findstring microsoft,$(shell tr A-Z a-z < /proc/sys/kernel/osrelease 2>/dev/null)))
			PLATFORM := wsl
		else
			PLATFORM := linux
		endif
	else
		PLATFORM := unknown
	endif
endif

CARGO ?= cargo
RUN   ?= $(CARGO) run

.PHONY: dev dev-wsl dev-linux dev-mac dev-windows platform

## dev: detect OS and run
dev: dev-$(PLATFORM)

## platform: print detected platform
platform:
	@echo "$(PLATFORM)"

## dev-wsl: fix WSLg wayland socket, then run
dev-wsl:
	@XDG=$${XDG_RUNTIME_DIR:-/run/user/$$(id -u)}; XDG=$${XDG%/}; \
	if [ -S /mnt/wslg/runtime-dir/wayland-0 ]; then \
		[ -e "$$XDG/wayland-0" ]      || ln -sf /mnt/wslg/runtime-dir/wayland-0      "$$XDG/wayland-0"; \
		[ -e "$$XDG/wayland-0.lock" ] || ln -sf /mnt/wslg/runtime-dir/wayland-0.lock "$$XDG/wayland-0.lock"; \
		echo "[dev-wsl] wayland ready: $$XDG/wayland-0"; \
	elif [ -n "$$DISPLAY" ]; then \
		echo "[dev-wsl] no WSLg wayland socket; using X11 (DISPLAY=$$DISPLAY)"; \
		unset WAYLAND_DISPLAY; \
	else \
		echo "[dev-wsl] no wayland socket and no DISPLAY - is WSLg running? (wsl --update)"; \
		exit 1; \
	fi; \
	exec $(RUN)

dev-linux dev-mac dev-windows:
	$(RUN)

dev-unknown:
	@echo "unknown platform; running anyway"; $(RUN)
