Simple boot-over-serial protocol.
Designed to be backwards compatible with SU-BOOT (as I am christening the cs140e/cs240lx bootloader).

# Wire Format

The first `0xffaa7755` is a fingerprint for signaling the beginning of the transmission.
```
+----------+--------------------+-------+
| ffaa7755 | content (variable) | CRC32 |
+----------+--------------------+-------+
```
These bytes are then COBS-encoded for transmission.

# Operation (Host)

Initial connection is made at 115200 baud.

Host will wait until it receives:
 - `GET_PROG_INFO` `THESEUSv1` at which point it will flush its receive buffer and switch to THESEUSv1
 - `GET_PROG_INFO`, at which point it will flush its receive buffer and switch to the legacy SU-BOOT protocol

THESEUSv1 (HOST) goes as follows:

> (Note: where it says "Host will send", take it to mean, "Host will repeatedly send ... every 100ms".)

Host will send `SetProtocolVersion { version: 1 }`.
Host will then wait until it receives `RequestProgramInfoRPC`.

Host will then will send `SetBaudRateRPC`.
Host will then wait for the device will reply `BaudRateAck` with either
 - `possible: true`, signifying that the device will switch to the requested baud rate
 - `possible: false`, signifying that the device cannot switch to the requested baud rate
If `possible: true`, Host will then send BaudRateReady, and switch baud rates.
If `possible: false`, Host will print an error and exit.
If Host does not see `RequestProgramInfoRPC` on the new baud rate setting within 50ms, it will return to 115200ms.

Host will then send `ProgramInfo` in response to the `RequestProgramInfoRPC`.

Host will then wait for the device to reply `RequestProgramRPC` with its requested chunk size in BYTES.
Host will then send `ProgramReady`.

Host will then wait for the device to reply `ReadyForChunk`.
Host will then send `ProgramChunk`.

Host will repeat from waiting for `ReadyForChunk` until it receives `ProgramReceived`.

At this point, it will open a dumb echo channel with the device.

# Operation (Device)

Initial connection is made at 115200 baud.

Device will send `GET_PROG_INFO` `THESEUSv1` every 100ms.

Device will then wait until it receives:
 - `PUT_PROG_INFO`, at which point it will switch to the legacy SU-BOOT protocol mode
 - `SetProtocolVersion { version: 1 }`, at which point it will enter THESEUSv1 protocol mode

THESEUSv1 (DEVICE) goes as follows:

### `RequestProgramInfoRPC`

Device will send `RequestProgramInfoRPC`.
Device will wait until it receives:
 - `SetBaudRateRPC` - Device will enter `SetBaudRateRPC`
 - `ProgramInfo`

Device will send `RequestProgramRPC` with the appropriate `chunk_size`.
Device will wait until it receives `ProgramReady`.

Device will send `ReadyForChunk` with the appropriate `chunk_no`.
Device will wait until it receives `ProgramChunk`.

Device will repeat until all chunks received.
Device will broadcast `ProgramReceived`.

### `SetBaudRateRPC`

Device will then send `BaudRateAck`.
Device will then wait until it receives `BaudRateReady` or until 50ms pass.
 - if 50ms pass, then it will return to 115200baud and return to init mode.

If the baud rate was possible, Device will switch rates at this point.
Device will reenter `RequestProgramInfoRPC` mode.

If Device does not receive `ProgramInfo` or `SetBaudRateRPC` within 50ms, it will return to 115200baud.