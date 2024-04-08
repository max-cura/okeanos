include infra/artefacts/thalassa/common.mk

#
# CONFIG
#

CPU=arm1176jzf-s
TARGET=armv6zk-none-eabihf
KLIB=lib$(ARTEFACT).rlib

.PHONY: _thalassa_cargo _thalassa_directories

#
# FLAGS
#

FREESTANDING_FLAGS=-nostdlib -ffreestanding -nostartfiles
# mcpu obviates the need for other tuning flags
TUNE_FLAGS=-mcpu=$(CPU)
# KISS.
OPT_FLAGS=-O0
# flags for boot.S
BOOT_ASFLAGS=$(FREESTANDING_FLAGS) $(TUNE_FLAGS:%=-Wa,%) $(OPT_FLAGS) -Wa,--warn -Wa,--fatal-warnings -fPIC
# flags for linking the kernel
KERNEL_LDFLAGS = $(FREESTANDING_FLAGS) $(TUNE_FLAGS) $(OPT_FLAGS)
KERNEL_LDFLAGS += -Wl,--gc-sections
# kept getting an odd warning about missing a .note.GNU-stack section; apparently it means that somehow, the linker
# decided it wanted execstack?
# https://stackoverflow.com/questions/73435637/how-can-i-fix-usr-bin-ld-warning-trap-o-missing-note-gnu-stack-section-imp
# TODO: investigate
KERNEL_LDFLAGS += -z noexecstack

#
# INTERNALS
#

GEN_FILES = $(CRATE_DIR)/$(TARGET).json \
		    $(_RUST_TARGET_DIR)/$(KLIB) \
		    $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).list \
		    $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin \
		    $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).elf \
		    $(ARTEFACT_BUILD_DIR)/boot.o

$(CRATE_DIR)/$(TARGET).json: $(INF_TARGETS_DIR)/$(TARGET).json
	@echo "Installing target file $@"
	cp $< $@

$(_RUST_TARGET_DIR)/$(KLIB): _thalassa_cargo $(CRATE_DIR)/$(TARGET).json
	( cd $(CRATE_DIR) ; cargo build --profile $(_RUST_PROFILE) )

$(ARTEFACT_BUILD_DIR)/boot.o: $(CRATE_DIR)/extern/boot.S
	$(INF_ARM_NONE_EABI_GCC) -c $(BOOT_ASFLAGS) -o $@ $^

# VERY IMPORTANT THING!!!
# ORDER OF OBJECT FILES PASSED TO THE LINKER MATTERS!!!!
# See: https://web.archive.org/web/20180627210132/webpages.charter.net/ppluzhnikov/linker.html
# since boot.o is our entry point, and calls into libthalassa, we need the order to be `ld boot.o libthalassa.???`
$(ARTEFACT_BUILD_DIR)/$(ARTEFACT).elf: $(ARTEFACT_BUILD_DIR)/boot.o \
									   $(_RUST_TARGET_DIR)/$(KLIB)
	$(INF_ARM_NONE_EABI_GCC) \
		-T $(ARTEFACT_INFRA_DIR)/linker.ld \
		$(KERNEL_LDFLAGS) $^ -o $@

$(ARTEFACT_BUILD_DIR)/$(ARTEFACT).list: $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).elf
	$(INF_ARM_NONE_EABI_OBJDUMP) -D $^ > $@

$(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin: $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).elf
	$(INF_ARM_NONE_EABI_OBJCOPY) $^ -O binary $@

#
# PUBLIC INTERFACE
#

.PHONY: all rebuild-all rebuild-cargo rebuild-infra clean-all clean-cargo clean-infra

all: $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin \
	 $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).list

#thalassa-upload: $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin
#	$(INF_PI_INSTALL) $(TTY_PORT) $<

clean-build-all: clean-all all
clean-build-cargo: clean-cargo all
clean-build-infra: clean-infra all

clean-all: clean-cargo clean-infra
clean-cargo:
	( cd $(CRATE_DIR) ; cargo clean )
clean-infra:
	rm -f $(GEN_FILES)