#!/usr/bin/env bash
set -euo pipefail
export PATH="${HOME}/.fuelup/bin:${PATH}"

# Installs Fuel toolchain and builds Sway ABIs for Railway deployments.

if ! command -v fuelup >/dev/null 2>&1; then
  curl -sSf https://install.fuel.network | sh
fi

fuelup toolchain install latest
fuelup default latest

cargo xtask abi
