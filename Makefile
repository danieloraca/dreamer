SERVICE := dreamer.service
.DEFAULT_GOAL := help
.PHONY: help build test test-all install-service deploy restart status logs install-mac

help:
	@printf 'Dreamer Pi shortcuts\n  make install-service  Build, install and enable the Pi service\n  make deploy           Rebuild and update the installed service\n  make restart          Restart Dreamer\n  make status           Show service status\n  make logs             Follow service logs\n  make test             Run Rust tests\n'

build:
	cargo build --locked --release

test:
	cargo test --locked
	python3 -m unittest discover -s tests -p 'test_install*.py'

test-all:
	cargo test --locked --features mac-client
	python3 -m unittest discover -s tests -p 'test_install*.py'

install-service: build
	python3 scripts/install_service.py

deploy: install-service

restart:
	sudo systemctl restart $(SERVICE)

status:
	sudo systemctl status $(SERVICE) --no-pager

logs:
	sudo journalctl -u $(SERVICE) -f

# The Mac-only client features are excluded from the ordinary Pi build.
install-mac:
	cargo build --locked --release --features mac-client
	python3 scripts/install_mac.py
