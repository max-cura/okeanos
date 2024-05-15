kernel + trusted drivers (unsafe)
untrusted drivers (no unsafe)

system call interface
- we want a typed interface.

```rust
trait Syscall {
	// A SyscallParams is a collection of integers, and pointers to memory 
	// regions to which we are granting access to the kernel.
	type Params : SyscallParams;
	// A SyscallResult is a collection of integers, and pointers to memory 
	// regions which resulted from a syscall.
	type Result : SyscallResult;

	// Invoke the kernel.
	// KFut can be awaited, or we can attach a callback to it.
	async fn invoke(params: Self::Params) -> KFut<Self::Syscall>;
}
```

exokernel?
- give access to the trusted driver APIs? the whole idea was to eliminate context switches
- so how can we provide protection and security when there's no context switch between trusted peripheral drivers and untrusted userspace code?
- look at how the MIT exokernel did it
- can move the trusted (peripheral) drivers out of the kernel?
	- need to verify accesses to a decent granularity then?

```
EXO_ACCESS: [{ ptr: *mut u32, r_mask: u32, w_mask: u32 }; N_EXO_ACCESS]
```

async
- in the kernel?
- in userspace