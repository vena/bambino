# src/mqtt/client — Non-Obvious Type Decisions

- **`src/mqtt/client.rs` is split into `src/mqtt/client/{mod,codec,frame,pending}.rs`**: `codec.rs` holds `encode_*` functions and packet consts, `frame.rs` holds the resumable-frame-read unit (`FrameReadState`/`read_exact_packet`), `pending.rs` holds the pending-message buffer (a second `impl` block). `mod.rs` keeps the struct, `connect()`/`publish_command()`/`poll_wire()`/`send_ping()`/`tick_zombie_check()`.
