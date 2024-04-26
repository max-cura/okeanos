// TSOP (in) pin is 21
// TSAL (out) pin is 20

use bcm2835_lpa::GPIO;

fn __init_tsop_tsal(
    gpio: &GPIO
) -> bool {

}

unsafe fn __is_tsop_low_unguarded(
    gpio: &GPIO
) -> bool {
    gpio.gplev0().read()
        .lev21().bit_is_clear()
}