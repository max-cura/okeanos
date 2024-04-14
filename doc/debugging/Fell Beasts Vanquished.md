A brief list of the thornier bugs I've had to deal with so far, for future reference.
# `ld` order

It happens that the order in which objects are passed to the linker *does* in fact matter—and since I was passing them in the wrong order:
```sh
arm-none-eabi-gcc libthalassa.a boot.o --gc-sections
```
it first passed through `libthalassa.a`, saw no symbols referenced from an entry point, and left; it then went to `boot.o`, found the entry point, and then tried to find the kernel entry point `__tlss_kernel_init` but did not succeed—apparently it won't backtrack into previously passed object files?

> Note: not sure whether `--gc-sections` actually affected anything here
# Parthiv Pi Pinout

The `parthiv-pi` board labels the pins with their GPIO numbers, not the RPi board numbers; this caused my initial efforts at both SPI and PWM to drive the WS2812B to fail as I had plugged DIn into the wrong pin(s).

# Spinlock

I wanted to switch from using `Peripherals::steal()` to `Peripherals::take()`; however, my code simply wasn't getting to the normal boot messages the UART would send—the culprit was obviously `Peripherals::take()` but it was unclear what exactly was going wrong.

I had just written some panic code and suspected that somehow, unwrapping `Peripherals::take()` had panicked silently, going into an infinite loop (the initial panic behavior upon boot).

However, going deeper, it wasn't the unwrap—it was the spinlock, in the critical section. At first, I thought that the RMW loops on the ticket counter were spinning infinitely—but subsequently realized that if I changed the `ldrexh` to `ldrh`, everything suddenly worked.

It turns out that `ldrex` will hang the core if:
- The MMU is not enabled
- The virtual memory area containing the lock is not in cache
and also that the cores will ignore each other's claims if SMP hasn't been enabled (though this last apparently varies by the device).