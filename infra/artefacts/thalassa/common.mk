include infra/config-common.mk
include infra/common.mk
include infra/targets.mk

ARTEFACT=thalassa
ARTEFACT_BUILD_DIR=$(INF_BUILD_DIR)/$(ARTEFACT)
ARTEFACT_INFRA_DIR=$(INF_BASE_DIR)/artefacts/$(ARTEFACT)
CRATE_DIR=$(INF_SRC_DIR)/$(ARTEFACT)

_RUST_PROFILE=release
_RUST_TARGET_PROFILE=release
_RUST_TARGET_DIR=$(CRATE_DIR)/target/$(TARGET)/$(_RUST_TARGET_PROFILE)
