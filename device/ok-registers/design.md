Goal: easy registers.

Want to have:

- per field read/write access
- read, write, rmw, local copy, raw values, etc.

Basic element is the register.
Register is under the hood a volatile cell at some address.
Register has various fields

- note that fields may not be the same between read and write--how to handle?
- soln: do like svd2rust and have separate R and W structs that are simultaneously available
- do I want all those callbacks? I'd prefer not...

```rust
registers! {
    IIR ( u32 ) [
        ro PENDING interrupt_pending (inverted) @ 0 : bool,
        ro IID_TXREADY tx_ready @ 1 : bool,
        ro IID_DATAREADY data_ready @ 2 : bool,
        wo CLEAR_RXFIFO @ 1 : bool,
        wo CLEAR_TXFIFO @ 2 : bool,
    ],
    CM_CTL ( u32 ) [
        wo PASSWORD OFFSET(24) NUMBITS(8) 
    ]
}
```