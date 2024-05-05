#
# FILE infra/artefacts/theseus-device/common.mk
# DESC common variables for infra/artefacts/theseus-device
#

# project-level common files
include infra/config-common.mk
include infra/common.mk
include infra/targets.mk

# artefact-specific configuration
ARTEFACT=theseus-upload
ARTEFACT_BUILD_DIR=$(INF_BUILD_DIR)/$(ARTEFACT)
ARTEFACT_INFRA_DIR=$(INF_BASE_DIR)/artefacts/$(ARTEFACT)
CRATE_DIR=$(INF_SRC_DIR)/$(ARTEFACT)

# _RUST_PROFILE is what's passed to `cargo --profile $(_RUST_PROFILE)`, and _RUST_TARGET_PROFILE is the name of the
# directory where the generated files are placed; these are separated since e.g. `--profile=dev` produces files in the
# directory `target/$(TARGET)/debug`
_RUST_PROFILE=release
_RUST_TARGET_PROFILE=release
_RUST_TARGET_DIR=$(INF_CARGO_TARGET_ROOT)/$(_RUST_TARGET_PROFILE)
