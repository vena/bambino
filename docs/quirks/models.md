**bambino > quirks > models**

# Module: quirks::models

## Contents

**Modules**

- [`a1`](#a1) - # A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates
- [`a2`](#a2) - # A2 Series (A2L Bed-Slinger) Quirks & Coordinates
- [`h2`](#h2) - # H2 Series (H2S, H2D, H2D Pro, H2C) Quirks
- [`p1`](#p1) - # P1 Series (P1P & P1S CoreXY) Quirks
- [`p2`](#p2) - # P2 Series (P2S CoreXY) Quirks
- [`x1`](#x1) - # X1 Series (X1C, X1E CoreXY) Quirks
- [`x2`](#x2) - # X2 Series (X2D CoreXY) Quirks

---

## Module: a1

# A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates

Handles the kinematics, safety boundaries, and mechanical constraints of the
A1 bed-slinger family [REF-MOTO-GCODE].

- A1: 256×256×256mm build volume
- A1 Mini: 180×180×180mm build volume



## Module: a2

# A2 Series (A2L Bed-Slinger) Quirks & Coordinates

The A2L is a large-format open-frame bed-slinger with a 330×320×325mm build volume.



## Module: h2

# H2 Series (H2S, H2D, H2D Pro, H2C) Quirks

Manages the properties and kinematic characteristics of the single-nozzle,
IDEX, and tool-changer platforms [REF-MOTO-GCODE].

Z-axis limits vary by model — per `MODEL_MATRIX.csv`'s Build Volume row, Z max does
not vary by active nozzle for these three models:
- H2S: 340mm (single nozzle only)
- H2D/H2D Pro: 325mm
- H2C: 325mm

H2C has 6 Vortek tool-changer hotends + 1 fixed hotend = 7 nozzles.
O1C and O1C2 are hardware revisions with identical quirks.



## Module: p1

# P1 Series (P1P & P1S CoreXY) Quirks

Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.



## Module: p2

# P2 Series (P2S CoreXY) Quirks

Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.



## Module: x1

# X1 Series (X1C, X1E CoreXY) Quirks

Implements hardware safety guidelines and thermal parameters for the premium CoreXY platforms.
X1C and X1E share all behavior except active chamber heater support (X1E only).



## Module: x2

# X2 Series (X2D CoreXY) Quirks

Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.

Build volumes: Main Nozzle 256×256×260mm, Aux/Dual 235.5×256×256mm.
Z-max uses the conservative aux/dual value (256mm).



