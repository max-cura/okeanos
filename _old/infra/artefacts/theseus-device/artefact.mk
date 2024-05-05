#
# FILE infra/artefacts/theseus-device/artefact.mk
# DESC The actual build system for the `theseus-device` artefact.
#

include infra/artefacts/theseus-device/common.mk

#
# CONFIG
#

# bcm2835 is arm1176jzf-s which has arch `armv6zk`; it also has VFPv2 so we use the hard float (hf) ABI
# note that the `hf` is somewhat overloaded; in our case it simply indicates the presence of VFPv2 and the use of its
# registers in the ABI
CPU=arm1176jzf-s
TARGET=armv6zk-none-eabihf
# need to use a '.a' since '.rlib' doesn't necessarily have all the stuff we need - the fact that `.rlbi' contains any
# object code at all is, or so I am informed, an implementation detail.
KLIB=libtheseus_device.a

.PHONY: _theseus-device_cargo _thalassa_directories

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

CARGO_VARS=THESEUS_DEVICE_GIT_DESC="$(shell git rev-parse --short HEAD)" THESEUS_DEVICE_BUILD_DATE="$(shell date -j "+%Y-%m-%d %H:%M:%S %Z")"

#
# INTERNALS
#

GEN_FILES = $(CRATE_DIR)/$(TARGET).json \
		    $(_RUST_TARGET_DIR)/$(KLIB) \
		    $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).list \
		    $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin \
		    $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).elf \
		    $(ARTEFACT_BUILD_DIR)/boot.o \
		    $(ARTEFACT_BUILD_DIR)/stub.o \
		    $(ARTEFACT_BUILD_DIR)/firmware # TODO add the files here so we can clean properly

$(CRATE_DIR)/$(TARGET).json: $(INF_TARGETS_DIR)/$(TARGET).json
	@echo "Installing target file $@"
	cp $< $@

$(_RUST_TARGET_DIR)/$(KLIB): _theseus-device_cargo $(CRATE_DIR)/$(TARGET).json
	( cd $(CRATE_DIR) ; $(CARGO_VARS) cargo build --profile $(_RUST_PROFILE) )

$(ARTEFACT_BUILD_DIR)/boot.o: $(CRATE_DIR)/extern/boot.S
	$(INF_ARM_NONE_EABI_GCC) -c $(BOOT_ASFLAGS) -o $@ $^
$(ARTEFACT_BUILD_DIR)/stub.o: $(CRATE_DIR)/extern/stub.S
	$(INF_ARM_NONE_EABI_GCC) -c $(BOOT_ASFLAGS) -o $@ $^

# VERY IMPORTANT THING!!!
# ORDER OF OBJECT FILES PASSED TO THE LINKER MATTERS!!!!
# See: https://web.archive.org/web/20180627210132/webpages.charter.net/ppluzhnikov/linker.html
# since boot.o is our entry point, and calls into libtheseus-device, we need the order to be: boot.o libthalassa.(a|rlib|...)
# Without this, was getting linker errors for undefined `_tlss_kernel_init` and `_tlss_fast_reboot`.
$(ARTEFACT_BUILD_DIR)/$(ARTEFACT).elf: $(ARTEFACT_BUILD_DIR)/boot.o \
									   $(ARTEFACT_BUILD_DIR)/stub.o \
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

firmware: $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin
	# config.txt bootcode.bin start.elf kernel.img
	mkdir -p $(ARTEFACT_BUILD_DIR)/firmware
	cp $(INF_CFG_CS240LX_2024_PATH)/firmware/{config.txt,bootcode.bin,start.elf} $(ARTEFACT_BUILD_DIR)/firmware
	cp $(ARTEFACT_BUILD_DIR)/$(ARTEFACT).bin $(ARTEFACT_BUILD_DIR)/firmware/kernel.img

rebuild-all: clean-all all
rebuild-cargo: clean-cargo all
rebuild-infra: clean-infra all

clean-all: clean-cargo clean-infra
clean-cargo:
	( cd $(CRATE_DIR) ; cargo clean )
clean-infra:
	rm -f $(GEN_FILES)