#!/usr/bin/env python3
"""TCP listener that aborts a TLS handshake after the client's first flight.

Hardware aid for GitHub issue #386 (the esp32-hw-probe check that used it,
cases 5a/5b, is in that probe's git history): a client
dials this, sends its ClientHello, and the listener closes the connection
before answering. The client's next read fails partway through the handshake,
which is the "peer reset mid-handshake" condition bambino's ESP-IDF backend
must report as SocketError::ConnectionReset rather than Other.

Connections alternate between the two ways a peer can drop, so one probe run
exercises both mbedTLS paths (ESP-IDF v5.5.5 `components/mbedtls/port/
net_sockets.c` and mbedTLS `ssl_msg.c`):

  odd connections   RST (SO_LINGER 0)  -> read() fails ECONNRESET
                                        -> MBEDTLS_ERR_NET_CONN_RESET
  even connections  FIN (plain close)  -> read() returns 0
                                        -> MBEDTLS_ERR_SSL_CONN_EOF

Both map to ConnectionReset. Restart the listener before each probe run so the
first connection is an RST, matching the order the probe logs expect.

Usage (on a machine on the same LAN as the board):

  scripts/tls-reset-listener.py [--port 8884]

then set PROBE_RESET_LISTENER=<this machine's LAN IP>:<port> in
esp32-hw-probe/.env. Stops with Ctrl-C. Nothing is sent to the client and
nothing it sends is stored.
"""

import argparse
import socket
import struct

# How long to wait for the ClientHello before closing anyway. The board sends
# it immediately after connecting; this only bounds a client that never does.
FIRST_FLIGHT_TIMEOUT_S = 10.0


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--port", type=int, default=8884, help="TCP port (default 8884)")
    args = parser.parse_args()

    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("0.0.0.0", args.port))
    server.listen(4)
    print(f"listening on 0.0.0.0:{args.port}; first connection gets RST, then FIN, alternating")

    count = 0
    try:
        while True:
            conn, peer = server.accept()
            count += 1
            rst = count % 2 == 1
            with conn:
                conn.settimeout(FIRST_FLIGHT_TIMEOUT_S)
                try:
                    received = len(conn.recv(4096))
                except socket.timeout:
                    received = 0
                if rst:
                    # l_onoff=1, l_linger=0: close() sends RST instead of FIN.
                    conn.setsockopt(
                        socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0)
                    )
            print(
                f"#{count} from {peer[0]}:{peer[1]}: read {received} byte(s), "
                f"closed with {'RST' if rst else 'FIN'}"
            )
    except KeyboardInterrupt:
        pass
    finally:
        server.close()


if __name__ == "__main__":
    main()
