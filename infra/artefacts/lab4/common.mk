#
# FILE infra/artefacts/lab4-send/common.mk
# DESC common variables for infra/artefacts/theseus-device
#

# project-level common files
include infra/config-common.mk
include infra/common.mk
include infra/targets.mk

# artefact-specific configuration
ARTEFACT=lab4
ARTEFACT_BUILD_DIR=$(INF_BUILD_DIR)/$(ARTEFACT)
ARTEFACT_INFRA_DIR=$(INF_BASE_DIR)/artefacts/$(ARTEFACT)
SEND_CRATE_DIR=$(INF_SRC_DIR)/$(ARTEFACT)-send
RECV_CRATE_DIR=$(INF_SRC_DIR)/$(ARTEFACT)-recv
CRATE_DIR=$(INF_SRC_DIR)/$(ARTEFACT)-common

# _RUST_PROFILE is what's passed to `cargo --profile $(_RUST_PROFILE)`, and _RUST_TARGET_PROFILE is the name of the
# directory where the generated files are placed; these are separated since e.g. `--profile=dev` produces files in the
# directory `target/$(TARGET)/debug`
_RUST_PROFILE=release
_RUST_TARGET_PROFILE=release
_RUST_TARGET_DIR=$(INF_CARGO_TARGET_ROOT)/$(TARGET)/$(_RUST_TARGET_PROFILE)
