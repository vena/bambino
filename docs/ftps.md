**bambino > ftps**

# Module: ftps

## Contents

**Modules**

- [`client`](#client) - # Implicit FTPS Client Implementation
- [`parser`](#parser) - # UNIX Directory Listing Parsing Engine for FTPS

---

## Module: client

# Implicit FTPS Client Implementation

Implements a secure, platform-agnostic, asynchronous FTPS client designed to execute
over our abstract `AsyncIo` boundaries. This client coordinates implicitly encrypted control channels
on Port 990, Passive port negotiation, TLS session wrapping (with A1-series plaintext bypass),
whitespace-insensitive UNIX listings parsing, and robust chunked uploads [REF-FTPS-CONN] [REF-FTPS-OPS].



## Module: parser

# UNIX Directory Listing Parsing Engine for FTPS

Decodes raw UNIX-style directory listings emitted by the printer's onboard vsFTPd server
over passive data channels. Employs whitespace-insensitive tokenization to handle
variable-width column padding and embeds robust temporal rollover heuristics.



